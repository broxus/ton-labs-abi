/*
* Copyright 2018-2020 TON DEV SOLUTIONS LTD.
*
* Licensed under the SOFTWARE EVALUATION License (the "License"); you may not use
* this file except in compliance with the License.
*
* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific TON DEV software governing permissions and
* limitations under the License.
*/

use crate::{
    error::AbiError,
    int::{Int, Uint},
    param_type::ParamType,
    token::{Token, TokenValue, Tokenizer},
};

use num_bigint::{BigInt, Sign};
use std::collections::BTreeMap;
use ton_block::Serializable;
use ton_types::{error, fail, BuilderData, Cell, HashmapE, IBitstring, Result};

impl TokenValue {
    pub fn pack_values_into_chain(
        tokens: &[Token],
        mut cells: Vec<BuilderData>,
        abi_version: u8,
    ) -> Result<BuilderData> {
        for token in tokens {
            cells.append(&mut token.value.write_to_cells(abi_version)?);
        }
        Self::pack_cells_into_chain(cells, abi_version)
    }

    pub fn pack_into_chain(&self, abi_version: u8) -> Result<BuilderData> {
        Self::pack_cells_into_chain(self.write_to_cells(abi_version)?, abi_version)
    }

    // first cell is resulting builder
    // every next cell: put data to root
    fn pack_cells_into_chain(mut cells: Vec<BuilderData>, abi_version: u8) -> Result<BuilderData> {
        cells.reverse();
        let mut packed_cells = match cells.pop() {
            Some(cell) => vec![cell],
            None => fail!(AbiError::InvalidData {
                msg: "No cells".to_owned()
            }),
        };
        while let Some(cell) = cells.pop() {
            let builder = packed_cells.last_mut().unwrap();
            if builder.bits_free() < cell.bits_used()
                || builder.references_free() < cell.references_used()
            {
                // if not enough bits or refs - continue chain
                packed_cells.push(cell);
            } else if cell.references_used() > 0
                && builder.references_free() == cell.references_used()
            {
                // if refs strictly fit into cell we should decide if we can put them into current
                // cell or to the next cell: if all remaining values can fit into current cell,
                // then use current, if not - continue chain
                let (refs, bits) = Self::get_remaining(&cells);
                // in ABI v1 last ref is always used for chaining
                if abi_version != 1 && (refs == 0 && bits + cell.bits_used() <= builder.bits_free())
                {
                    builder.append_builder(&cell)?;
                } else {
                    packed_cells.push(cell);
                }
            } else {
                builder.append_builder(&cell)?;
            }
        }
        while let Some(cell) = packed_cells.pop() {
            match packed_cells.last_mut() {
                Some(builder) => builder.append_reference(cell),
                None => return Ok(cell),
            }
        }
        fail!(AbiError::NotImplemented)
    }

    fn get_remaining(cells: &[BuilderData]) -> (usize, usize) {
        cells.iter().fold((0, 0), |(refs, bits), cell| {
            (refs + cell.references_used(), bits + cell.bits_used())
        })
    }

    fn max_bit_size(&self) -> usize {
        match self {
            TokenValue::Uint(i) => i.size,
            TokenValue::Int(i) => i.size,
            TokenValue::Bool(_) => 1,
            TokenValue::Array(_) => 33,
            TokenValue::FixedArray(_) => 1,
            TokenValue::Cell(_) => 0,
            TokenValue::Map(_, _) => 1,
            TokenValue::Address(_) => 591,
            TokenValue::Bytes(_) | TokenValue::FixedBytes(_) => 0,
            TokenValue::Gram(_) => 128,
            TokenValue::Time(_) => 64,
            TokenValue::Expire(_) => 32,
            TokenValue::PublicKey(_) => 256,
            TokenValue::Tuple(tokens) => tokens
                .iter()
                .fold(0, |acc, token| acc + token.value.max_bit_size()),
        }
    }

    pub fn write_to_cells(&self, abi_version: u8) -> Result<Vec<BuilderData>> {
        match self {
            TokenValue::Uint(uint) => Self::write_uint(uint),
            TokenValue::Int(int) => Self::write_int(int),
            TokenValue::Bool(b) => Self::write_bool(b),
            TokenValue::Tuple(ref tokens) => {
                let mut vec = vec![];
                for token in tokens.iter() {
                    vec.append(&mut token.value.write_to_cells(abi_version)?);
                }
                Ok(vec)
            }
            TokenValue::Array(ref tokens) => Self::write_array(tokens, abi_version),
            TokenValue::FixedArray(ref tokens) => Self::write_fixed_array(tokens, abi_version),
            TokenValue::Cell(cell) => Self::write_cell(cell),
            TokenValue::Map(key_type, value) => Self::write_map(key_type, value, abi_version),
            TokenValue::Address(address) => Ok(vec![address.write_to_new_cell()?]),
            TokenValue::Bytes(ref arr) | TokenValue::FixedBytes(ref arr) => {
                Self::write_bytes(arr, abi_version)
            }
            TokenValue::Gram(gram) => Ok(vec![gram.write_to_new_cell()?]),
            TokenValue::Time(time) => Ok(vec![time.write_to_new_cell()?]),
            TokenValue::Expire(expire) => Ok(vec![expire.write_to_new_cell()?]),
            TokenValue::PublicKey(key) => Self::write_public_key(key),
        }
    }

    fn write_int(value: &Int) -> Result<Vec<BuilderData>> {
        let vec = value.number.to_signed_bytes_be();
        let vec_bits_length = vec.len() * 8;

        let mut builder = BuilderData::new();

        if value.size > vec_bits_length {
            let padding = if value.number.sign() == num_bigint::Sign::Minus {
                0xFFu8
            } else {
                0u8
            };

            let dif = value.size - vec_bits_length;

            let mut vec_padding = Vec::new();
            vec_padding.resize(dif / 8 + 1, padding);

            builder.append_raw(&vec_padding, dif)?;
            builder.append_raw(&vec, value.size - dif)?;
        } else {
            let offset = vec_bits_length - value.size;
            let first_byte = vec[offset / 8] << (offset % 8);

            builder.append_raw(&[first_byte], 8 - offset % 8)?;
            builder.append_raw(&vec[offset / 8 + 1..], vec[offset / 8 + 1..].len() * 8)?;
        };

        Ok(vec![builder])
    }

    fn write_uint(value: &Uint) -> Result<Vec<BuilderData>> {
        let int = Int {
            number: BigInt::from_biguint(Sign::Plus, value.number.clone()),
            size: value.size,
        };

        Self::write_int(&int)
    }

    fn write_bool(value: &bool) -> Result<Vec<BuilderData>> {
        let mut builder = BuilderData::new();
        builder.append_bit_bool(value.clone())?;
        Ok(vec![builder])
    }

    fn write_cell(cell: &Cell) -> Result<Vec<BuilderData>> {
        let mut builder = BuilderData::new();
        builder.append_reference_cell(cell.clone());
        Ok(vec![builder])
    }

    // creates dictionary with indexes of an array items as keys and items as values
    // and prepends dictionary to cell
    fn put_array_into_dictionary(array: &[TokenValue], abi_version: u8) -> Result<HashmapE> {
        let mut map = HashmapE::with_bit_len(32);

        let value_len = array
            .get(0)
            .map(|value| value.max_bit_size())
            .unwrap_or_default();
        let value_in_ref = Self::value_in_ref(32, value_len);

        for i in 0..array.len() {
            let index = (i as u32).write_to_new_cell()?;

            let data =
                Self::pack_cells_into_chain(array[i].write_to_cells(abi_version)?, abi_version)?;

            if value_in_ref {
                map.set_builder(index.into(), &data)?;
            } else {
                map.setref(index.into(), &data.into_cell()?)?;
            }
        }

        Ok(map)
    }

    fn write_array(value: &Vec<TokenValue>, abi_version: u8) -> Result<Vec<BuilderData>> {
        let map = Self::put_array_into_dictionary(value, abi_version)?;

        let mut builder = BuilderData::new();
        builder.append_u32(value.len() as u32)?;

        map.write_to(&mut builder)?;

        Ok(vec![builder])
    }

    fn write_fixed_array(value: &Vec<TokenValue>, abi_version: u8) -> Result<Vec<BuilderData>> {
        let map = Self::put_array_into_dictionary(value, abi_version)?;

        Ok(vec![map.write_to_new_cell()?])
    }

    fn write_bytes(data: &[u8], abi_version: u8) -> Result<Vec<BuilderData>> {
        let cell_len = BuilderData::bits_capacity() / 8;
        let mut len = data.len();
        let mut cell_capacity = if abi_version == 1 {
            std::cmp::min(cell_len, len)
        } else {
            match len % cell_len {
                0 => cell_len,
                x => x,
            }
        };
        let mut builder = BuilderData::new();
        while len > 0 {
            len -= cell_capacity;
            builder.append_raw(&data[len..len + cell_capacity], cell_capacity * 8)?;
            let mut new_builder = BuilderData::new();
            new_builder.append_reference(builder);
            builder = new_builder;
            cell_capacity = std::cmp::min(cell_len, len);
        }
        // if bytes are empty then we need builder with ref to empty cell
        if builder.references_used() == 0 {
            builder.append_reference(BuilderData::new());
        }
        Ok(vec![builder])
    }

    fn value_in_ref(key_len: usize, value_len: usize) -> bool {
        super::MAX_HASH_MAP_INFO_ABOUT_KEY + key_len + value_len <= 1023
    }

    fn write_map(
        key_type: &ParamType,
        value: &BTreeMap<String, TokenValue>,
        abi_version: u8,
    ) -> Result<Vec<BuilderData>> {
        let key_len = key_type.get_map_key_size()?;
        let value_len = value
            .values()
            .next()
            .map(|value| value.max_bit_size())
            .unwrap_or_default();
        let value_in_ref = Self::value_in_ref(key_len, value_len);

        let mut hashmap = HashmapE::with_bit_len(key_len);

        for (key, value) in value.iter() {
            let key = Tokenizer::tokenize_parameter(key_type, &key.as_str().into())?;

            let mut key_vec = key.write_to_cells(abi_version)?;
            if key_vec.len() != 1 {
                fail!(AbiError::InvalidData {
                    msg: "Map key must be 1-cell length".to_owned()
                })
            };
            if &ParamType::Address == key_type
                && key_vec[0].length_in_bits() != super::STD_ADDRESS_BIT_LENGTH
            {
                fail!(AbiError::InvalidData {
                    msg: "Only std non-anycast address can be used as map key".to_owned()
                })
            }

            let data =
                Self::pack_cells_into_chain(value.write_to_cells(abi_version)?, abi_version)?;

            let slice_key = key_vec.pop().unwrap().into();
            if value_in_ref {
                hashmap.set_builder(slice_key, &data)?;
            } else {
                hashmap.setref(slice_key, &data.into_cell()?)?;
            }
        }

        let mut builder = BuilderData::new();
        hashmap.write_to(&mut builder)?;

        Ok(vec![builder])
    }

    fn write_public_key(data: &Option<ed25519_dalek::PublicKey>) -> Result<Vec<BuilderData>> {
        let mut builder = BuilderData::new();
        if let Some(key) = data {
            builder.append_bit_one()?;
            let bytes = &key.to_bytes()[..];
            let length = bytes.len() * 8;
            builder.append_raw(bytes, length)?;
        } else {
            builder.append_bit_zero()?;
        }
        Ok(vec![builder])
    }
}

#[test]
fn test_pack_cells() {
    let cells = vec![
        BuilderData::with_bitstring(vec![1, 2, 0x80]).unwrap(),
        BuilderData::with_bitstring(vec![3, 4, 0x80]).unwrap(),
    ];
    let builder = BuilderData::with_bitstring(vec![1, 2, 3, 4, 0x80]).unwrap();
    assert_eq!(
        TokenValue::pack_cells_into_chain(cells, 1).unwrap(),
        builder
    );

    let cells = vec![
        BuilderData::with_raw(vec![0x55; 100], 100 * 8).unwrap(),
        BuilderData::with_raw(vec![0x55; 127], 127 * 8).unwrap(),
        BuilderData::with_raw(vec![0x55; 127], 127 * 8).unwrap(),
    ];

    let builder = BuilderData::with_raw(vec![0x55; 127], 127 * 8).unwrap();
    let builder =
        BuilderData::with_raw_and_refs(vec![0x55; 127], 127 * 8, vec![builder.into()]).unwrap();
    let builder =
        BuilderData::with_raw_and_refs(vec![0x55; 100], 100 * 8, vec![builder.into()]).unwrap();
    let tree = TokenValue::pack_cells_into_chain(cells, 1).unwrap();
    assert_eq!(tree, builder);
}
