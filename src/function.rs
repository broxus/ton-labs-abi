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

//! Contract function call builder.

use crate::{contract::{ABI_VERSION_1_0, ABI_VERSION_2_3}, error::AbiError, param::Param, param_type::ParamType, token::{SerializedValue, Token, TokenValue}};

use crate::contract::{AbiVersion, SerdeFunction};
use crate::signature::sign_with_signature_id;
use ed25519_dalek::{Keypair, SIGNATURE_LENGTH};
use sha2::{Digest, Sha256};
use smallvec::SmallVec;
use std::collections::HashMap;
use ton_block::{Serializable, MsgAddressInt};
use ton_types::{BuilderData, fail, IBitstring, Result, SliceData};
use crate::token::Cursor;

/// Contract function specification.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Function {
    /// ABI version
    pub abi_version: AbiVersion,
    /// Function name.
    pub name: String,
    /// Function header parameters.
    pub header: Vec<Param>,
    /// Function input.
    pub inputs: Vec<Param>,
    /// Function output.
    pub outputs: Vec<Param>,
    /// Function ID for inbound messages
    pub input_id: u32,
    /// Function ID for outbound messages
    pub output_id: u32,
}

impl Function {
    /// Creates `Function` struct from parsed JSON struct `SerdeFunction`
    pub fn from_serde(abi_version: AbiVersion, serde_function: SerdeFunction, header: Vec<Param>) -> Self {
        let mut function = Function {
            abi_version,
            name: serde_function.name,
            header,
            inputs: serde_function.inputs,
            outputs: serde_function.outputs,
            input_id: 0,
            output_id: 0,
        };
        if let Some(id) = serde_function.id {
            function.input_id = id;
            function.output_id = id
        } else {
            let id = function.get_function_id();
            function.input_id = id & 0x7FFFFFFF;
            function.output_id = id | 0x80000000;
        };
        function
    }

    /// Returns all header params of given function.
    pub fn header_params(&self) -> &Vec<Param> {
        &self.header
    }

    /// Returns all input params of given function.
    pub fn input_params(&self) -> &Vec<Param> {
        &self.inputs
    }

    /// Returns all output params of given function.
    pub fn output_params(&self) -> &Vec<Param> {
        &self.outputs
    }

    /// Returns true if function has input parameters, false in not
    pub fn has_input(&self) -> bool {
        !self.inputs.is_empty()
    }

    /// Returns true if function has output parameters, false in not
    pub fn has_output(&self) -> bool {
        !self.outputs.is_empty()
    }

    /// Retruns ABI function signature
    pub fn get_function_signature(&self) -> String {
        let mut input_types = vec![];
        if self.abi_version.major == 1 {
            input_types.append(&mut self.header.iter()
                .map(|param| param.kind.type_signature())
                .collect::<Vec<String>>())
        }

        input_types.append(
            &mut self
                .inputs
                .iter()
                .map(|param| param.kind.type_signature())
                .collect::<Vec<String>>(),
        );

        let input_types = input_types.join(",");

        let output_types = self
            .outputs
            .iter()
            .map(|param| param.kind.type_signature())
            .collect::<Vec<String>>()
            .join(",");

        format!("{}({})({})v{}", self.name, input_types, output_types, self.abi_version.major)
    }

    pub fn calc_function_id(signature: &str) -> u32 {
        // Sha256 hash of signature
        let mut hasher = Sha256::new();

        hasher.update(signature.as_bytes());

        let function_hash = hasher.finalize();

        let mut bytes: [u8; 4] = [0; 4];
        bytes.copy_from_slice(&function_hash[..4]);
        //println!("{}: {:X}", signature, u32::from_be_bytes(bytes));

        u32::from_be_bytes(bytes)
    }

    /// Computes function ID for contract function
    pub fn get_function_id(&self) -> u32 {
        let signature = self.get_function_signature();

        Self::calc_function_id(&signature)
    }

    /// Returns ID for call message
    pub fn get_input_id(&self) -> u32 {
        self.input_id
    }

    /// Returns ID for response message
    pub fn get_output_id(&self) -> u32 {
        self.output_id
    }

    /// Parses the ABI function output to list of tokens.
    pub fn decode_output(&self, mut data: SliceData, _internal: bool) -> Result<Vec<Token>> {
        let id = data.get_next_u32()?;
        if id != self.get_output_id() { Err(AbiError::WrongId { id } )? }
        TokenValue::decode_params(self.output_params(), data, &self.abi_version, false)
    }

    /// Parses the ABI function output to list of tokens. Allows partial decoding.
    pub fn decode_output_partial(&self, mut data: SliceData, _internal: bool) -> Result<Vec<Token>> {
        let id = data.get_next_u32()?;
        if id != self.get_output_id() { Err(AbiError::WrongId { id } )? }
        TokenValue::decode_params(self.output_params(), data, &self.abi_version, true)
    }

    /// Parses the ABI function call to list of tokens.
    pub fn decode_input(
        &self,
        data: SliceData,
        internal: bool,
        allow_partial: bool,
    ) -> Result<Vec<Token>> {
        let (_, id, cursor) = Self::decode_header(&self.abi_version, data, &self.header, internal)?;

        if id != self.get_input_id() {
            Err(AbiError::WrongId { id })?
        }

        TokenValue::decode_params_with_cursor(
            self.input_params(),
            cursor,
            &self.abi_version,
            allow_partial,
            true,
        )
            .map(|(tokens, _)| tokens)
    }

    /// Decodes function id from contract answer
    pub fn decode_input_id(
        abi_version: &AbiVersion,
        cursor: SliceData,
        header: &Vec<Param>,
        internal: bool,
    ) -> Result<u32> {
        let (_, id, _) = Self::decode_header(abi_version, cursor, header, internal)?;
        Ok(id)
    }

    /// Decodes function id from contract answer
    pub fn decode_output_id(mut data: SliceData) -> Result<u32> {
        data.get_next_u32()
    }

    /// Encodes provided function parameters into `BuilderData` containing ABI contract call
    pub fn encode_input(
        &self,
        header: &HashMap<String, TokenValue>,
        input: &[Token],
        internal: bool,
        pair: Option<(&Keypair, Option<i32>)>,
        address: Option<MsgAddressInt>,
    ) -> Result<BuilderData> {

        let (mut builder, hash) =
            self.create_unsigned_call(header, input, internal, pair.is_some(), address)?;

        if !internal {
            builder = match pair {
                Some((pair, signature_id)) => {
                    let signature = sign_with_signature_id(pair, hash.as_slice(), signature_id);
                    Self::fill_sign(
                        &self.abi_version,
                        Some(&signature.to_bytes()),
                        Some(&pair.public.to_bytes()),
                        builder)?
                },
                None => Self::fill_sign(&self.abi_version, None, None, builder)?
            }
        }

        Ok(builder)
    }

    /// Encodes provided function return values into `BuilderData`
    pub fn encode_internal_output(&self, answer_id: u32, input: &[Token]) -> Result<BuilderData> {
        let vec = vec![answer_id.write_to_new_cell()?.into()];
        let builder = TokenValue::pack_values_into_chain(input, vec, &self.abi_version)?;
        Ok(builder)
    }

    /// Encodes function header with provided header parameters
    fn encode_header(
        &self,
        header_tokens: &HashMap<String, TokenValue>,
        internal: bool
    ) -> Result<Vec<SerializedValue>> {
        let mut vec = vec![];
        if !internal {
            for param in &self.header {
                if let Some(token) = header_tokens.get(&param.name) {
                    if !token.type_check(&param.kind) {
                        return Err(AbiError::WrongParameterType.into());
                    }
                    vec.append(&mut token.write_to_cells(&self.abi_version)?);
                } else {
                    vec.append(&mut TokenValue::get_default_value_for_header(&param.kind)?.write_to_cells(&self.abi_version)?);
                }
            }
        }
        if self.abi_version.major == 1 {
            vec.insert(0, self.get_input_id().write_to_new_cell()?.into());
        } else {
            vec.push(self.get_input_id().write_to_new_cell()?.into());
        }
        Ok(vec)
    }

    /// Encodes function header with provided header parameters
    fn encode_default_header(
        &self,
        time: u64,
        internal: bool
    ) -> Result<Vec<SerializedValue>> {
        let mut vec = vec![];
        if !internal {
            for param in &self.header {
                let header_value = match &param.kind {
                    ParamType::Time => TokenValue::Time(time),
                    ParamType::Expire => TokenValue::Expire(u32::MAX),
                    ParamType::PublicKey => TokenValue::PublicKey(None),
                    any_type => return Err(
                        AbiError::InvalidInputData {
                            msg: format!(
                                "Type {} doesn't have default value and must be explicitly defined",
                                any_type)}.into()
                    )
                };

                vec.append(&mut header_value.write_to_cells(&self.abi_version)?);
            }
        }
        if self.abi_version.major == 1 {
            vec.insert(0, self.get_input_id().write_to_new_cell()?.into());
        } else {
            vec.push(self.get_input_id().write_to_new_cell()?.into());
        }
        Ok(vec)
    }

    /// Encodes function header with provided header parameters
    pub fn decode_header(
        abi_version: &AbiVersion,
        cursor: SliceData,
        header: &Vec<Param>,
        internal: bool,
    ) -> Result<(Vec<Token>, u32, Cursor)> {
        let mut tokens = vec![];
        let mut id = 0;
        let mut cursor: Cursor = cursor.into();
        if abi_version == &ABI_VERSION_1_0 {
            id = cursor.slice.get_next_u32()?;
            cursor.used_bits += 32;
        }
        if !internal {
            // skip signature
            if abi_version == &ABI_VERSION_1_0 {
                cursor.slice.checked_drain_reference()?;
                cursor.used_refs += 1;
            } else {
                if cursor.slice.get_next_bit()? {
                    cursor.slice.get_next_bytes(SIGNATURE_LENGTH)?;
                }
                cursor.used_bits += if abi_version >= &ABI_VERSION_2_3 {
                    TokenValue::max_bit_size(&ParamType::Address, abi_version)
                } else {
                    1 + SIGNATURE_LENGTH * 8
                };
            }


            (tokens, cursor) =
                TokenValue::decode_params_with_cursor(header, cursor, abi_version, true, false)?;
        }
        if abi_version != &ABI_VERSION_1_0 {
            id = cursor.slice.get_next_u32()?;
            cursor.used_bits += 32;
        }
        Ok((tokens, id, cursor))
    }

    /// Encodes provided function parameters into `BuilderData` containing ABI contract call.
    /// `BuilderData` is prepared for signing. Sign should be the added by `add_sign_to_function_call` function
    pub fn create_unsigned_call(
        &self,
        header: &HashMap<String, TokenValue>,
        input: &[Token],
        internal: bool,
        reserve_sign: bool,
        address: Option<MsgAddressInt>,
    ) -> Result<(BuilderData, ton_types::UInt256)> {
        let params = self.input_params();

        if !Token::types_check(input, params.as_slice()) {
            fail!(AbiError::WrongParameterType);
        }

        // prepare standard message
        let mut cells = self.encode_header(header, internal)?;

        let mut remove_ref = false;
        let mut remove_bits = 0;
        if !internal {
            let mut sign_builder = BuilderData::new();
            if self.abi_version.major == 1 {
                // reserve reference for sign
                sign_builder.checked_append_reference(ton_types::Cell::default())?;
                remove_ref = true;
            } else {
                // reserve in-cell data
                if reserve_sign {
                    if self.abi_version >= ABI_VERSION_2_3 {
                        sign_builder.append_raw(
                            &[0u8; 128], //MAX DATA BYTES
                            TokenValue::max_bit_size(&ParamType::Address, &self.abi_version),
                        )?;
                        remove_bits =
                            TokenValue::max_bit_size(&ParamType::Address, &self.abi_version);
                    } else {
                        sign_builder.append_bit_one()?;
                        sign_builder.append_raw(
                            &[0u8; SIGNATURE_LENGTH],
                            SIGNATURE_LENGTH * 8,
                        )?;
                        remove_bits = 1 + SIGNATURE_LENGTH * 8;
                    }
                } else {
                    sign_builder.append_bit_zero()?;
                    remove_bits = 1;
                }
            }
            cells.insert(
                0,
                SerializedValue {
                    data: sign_builder,
                    max_bits: if self.abi_version >= ABI_VERSION_2_3 {
                        TokenValue::max_bit_size(&ParamType::Address, &self.abi_version)
                    } else {
                        1 + SIGNATURE_LENGTH * 8
                    },
                    max_refs: if remove_ref { 1 } else { 0 },
                },
            );
        }

        // encoding itself
        let mut builder = TokenValue::pack_values_into_chain(input, cells, &self.abi_version)?;

        if !internal {
            // delete reserved sign before hash
            let mut slice = builder.into_cell().and_then(SliceData::load_cell)?;
            if remove_ref {
                slice.checked_drain_reference()?;
            }
            if remove_bits != 0 {
                slice.get_next_bits(remove_bits)?;
            }
            builder = BuilderData::from_slice(&slice);
        }

        let hash = if self.abi_version >= ABI_VERSION_2_3 && reserve_sign {
            let address = address.ok_or(AbiError::AddressRequired)?;
            let mut address_builder = address.write_to_new_cell()?;
            address_builder.append_builder(&builder)?;
            address_builder.into_cell()?.repr_hash()
        } else {
            builder.clone().into_cell()?.repr_hash()
        };

        Ok((builder, hash))
    }

    pub fn encode_internal_input(&self, input: &[Token]) -> Result<BuilderData> {
        let params = self.input_params();

        if !Token::types_check(input, params.as_slice()) {
            fail!(AbiError::WrongParameterType);
        }

        TokenValue::pack_values_into_chain(
            input,
            vec![self.get_input_id().write_to_new_cell()?.into()],
            &self.abi_version
        )
    }

    /// Encodes provided function parameters into `BuilderData` containing ABI contract call.
    pub fn encode_run_local_input(&self, time: u64, input: &[Token]) -> Result<BuilderData> {
        let params = self.input_params();

        if !Token::types_check(input, params) {
            fail!(AbiError::WrongParameterType);
        }

        // prepare standard message
        let mut cells = self.encode_default_header(time, false)?;

        let mut remove_ref = false;
        let mut remove_bits = 0;

        let mut sign_builder = BuilderData::new();
        if self.abi_version.major == 1 {
            // reserve reference for sign
            sign_builder.checked_append_reference(ton_types::Cell::default())?;
            remove_ref = true;
        } else {
            sign_builder.append_bit_zero()?;
            remove_bits = 1;
        }

        cells.insert(
            0,
            SerializedValue {
                data: sign_builder,
                max_bits: if self.abi_version >= ABI_VERSION_2_3 {
                    TokenValue::max_bit_size(&ParamType::Address, &self.abi_version)
                } else {
                    1 + SIGNATURE_LENGTH * 8
                },
                max_refs: if remove_ref { 1 } else { 0 },
            },
        );

        // encoding itself
        let mut builder = TokenValue::pack_values_into_chain(input, cells, &self.abi_version)?;

        // delete reserved sign before hash
        let mut slice = SliceData::load_builder(builder)?;
        if remove_ref {
            slice.checked_drain_reference()?;
        }
        if remove_bits != 0 {
            slice.get_next_bits(remove_bits)?;
        }
        builder = BuilderData::from_slice(&slice);

        if self.abi_version == ABI_VERSION_1_0 {
            // sign in reference
            if builder.references_free() == 0 {
                fail!(AbiError::InvalidInputData {
                    msg: "No free reference for signature".to_owned()
                });
            }
            builder.checked_append_reference(ton_types::Cell::default())?;
        } else {
            // sign in cell body
            let mut sign_builder = BuilderData::new();
            sign_builder.append_bit_zero()?;
            builder.prepend_builder(&sign_builder)?;
        }

        Ok(builder)
    }

    /// Add sign to messsage body returned by `prepare_input_for_sign` function
    pub fn fill_sign(
        abi_version: &AbiVersion,
        signature: Option<&[u8]>,
        public_key: Option<&[u8]>,
        mut builder: BuilderData,
    ) -> Result<BuilderData> {

        if abi_version == &ABI_VERSION_1_0 {
            // sign in reference
            if builder.references_free() == 0 {
                fail!(AbiError::InvalidInputData {
                    msg: "No free reference for signature".to_owned()
                });
            }
            if let Some(signature) = signature {
                let mut signature = SmallVec::from_slice(signature);
                if let Some(public_key) = public_key {
                    signature.extend_from_slice(public_key);
                }

                let len = signature.len() * 8;
                builder.checked_prepend_reference(BuilderData::with_raw(signature, len)?.into_cell()?)?;
            } else {
                builder.checked_prepend_reference(ton_types::Cell::default())?;
            }
        } else {
            // sign in cell body
            let mut sign_builder = BuilderData::new();
            if let Some(signature) = signature {
                let len = signature.len() * 8;
                sign_builder.append_bit_one()?;
                sign_builder.append_raw(signature, len)?;
            } else {
                sign_builder.append_bit_zero()?;
            }
            builder.prepend_builder(&sign_builder)?;
        }

        Ok(builder)
    }

    /// Add sign to messsage body returned by `prepare_input_for_sign` function
    pub fn add_sign_to_encoded_input(
        abi_version: &AbiVersion,
        signature: &[u8],
        public_key: Option<&[u8]>,
        function_call: SliceData,
    ) -> Result<BuilderData> {
        let builder = BuilderData::from_slice(&function_call);

        Self::fill_sign(abi_version, Some(signature), public_key, builder)
    }

    /// Check if message body is related to this function
    pub fn is_my_input_message(&self, data: SliceData, internal: bool) -> Result<bool> {
        let decoded_id = Self::decode_input_id(&self.abi_version, data, &self.header, internal)?;
        Ok(self.get_input_id() == decoded_id)
    }

    /// Check if message body is related to this function
    pub fn is_my_output_message(&self, data: SliceData, _internal: bool) -> Result<bool> {
        let decoded_id = Self::decode_output_id(data)?;
        Ok(self.get_output_id() == decoded_id)
    }
}
