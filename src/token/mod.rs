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

//! TON ABI params.
use crate::{
    error::AbiError, int::{Int, Uint}, param::Param, param_type::ParamType,
};

use std::collections::BTreeMap;
use std::fmt;
use ton_block::{Grams, MsgAddress};
use ton_types::{Result, Cell, BuilderData};
use num_bigint::{BigInt, BigUint};
use ton_types::error;
use crate::contract::{AbiVersion, ABI_VERSION_2_4};

mod tokenizer;
mod detokenizer;
mod serialize;
mod deserialize;

pub use self::tokenizer::*;
pub use self::detokenizer::*;
pub use self::serialize::*;
pub use self::deserialize::*;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod test_encoding;

pub const STD_ADDRESS_BIT_LENGTH: usize = 267;
pub const MAX_HASH_MAP_INFO_ABOUT_KEY: usize = 12;

/// TON ABI params.
#[derive(Debug, PartialEq, Clone)]
pub struct Token {
    pub name: String,
    pub value: TokenValue,
}

impl Token {
    pub fn new(name: &str, value: TokenValue) -> Self {
        Self { name: name.to_string(), value }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} : {}", self.name, self.value)
    }
}

#[derive(Debug, Clone)]
pub enum MapKeyTokenValue {
    Uint(Uint),
    Int(Int),
    Address(MsgAddress),
}

impl PartialEq for MapKeyTokenValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Uint(a), Self::Uint(b)) => a == b,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Address(a), Self::Address(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for MapKeyTokenValue {}

impl PartialOrd for MapKeyTokenValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MapKeyTokenValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        match (self, other) {
            (Self::Uint(a), Self::Uint(b)) => a.number.cmp(&b.number),
            (Self::Uint(_), _) => Ordering::Less,
            (Self::Int(a), Self::Int(b)) => a.number.cmp(&b.number),
            (Self::Int(_), Self::Uint(_)) => Ordering::Greater,
            (Self::Int(_), Self::Address(_)) => Ordering::Less,
            (Self::Address(a), Self::Address(b)) => a.cmp(b),
            (Self::Address(_), _) => Ordering::Greater,
        }
    }
}

impl From<MapKeyTokenValue> for TokenValue {
    fn from(value: MapKeyTokenValue) -> Self {
        match value {
            MapKeyTokenValue::Uint(uint) => Self::Uint(uint),
            MapKeyTokenValue::Int(int) => Self::Int(int),
            MapKeyTokenValue::Address(address) => Self::Address(address),
        }
    }
}

impl From<&MapKeyTokenValue> for TokenValue {
    fn from(value: &MapKeyTokenValue) -> Self {
        match value {
            MapKeyTokenValue::Uint(uint) => Self::Uint(uint.clone()),
            MapKeyTokenValue::Int(int) => Self::Int(int.clone()),
            MapKeyTokenValue::Address(address) => Self::Address(address.clone()),
        }
    }
}

impl TryFrom<TokenValue> for MapKeyTokenValue {
    type Error = anyhow::Error;

    fn try_from(value: TokenValue) -> std::result::Result<Self, Self::Error> {
        match value {
            TokenValue::Uint(uint) => Ok(Self::Uint(uint)),
            TokenValue::Int(int) => Ok(Self::Int(int)),
            TokenValue::Address(address) => Ok(Self::Address(address)),
            _ => Err(error!(AbiError::InvalidData {
                msg: "Only integer and std address values can be map keys".to_owned()
            }))
        }
    }
}

impl MapKeyTokenValue {
    pub fn type_check(&self, param_type: &ParamType) -> bool {
        match (self, param_type) {
            (Self::Uint(uint), ParamType::Uint(size)) => uint.size == *size,
            (Self::Int(int), ParamType::Int(size)) => int.size == *size,
            (Self::Address(_), ParamType::Address) => true,
            _ => false,
        }
    }
}

impl fmt::Display for MapKeyTokenValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uint(u) => write!(f, "{}", u.number),
            Self::Int(u) => write!(f, "{}", u.number),
            Self::Address(a) => write!(f, "{a}"),
        }
    }
}

/// TON ABI param values.
#[derive(Debug, PartialEq, Clone)]
pub enum TokenValue {
    /// uint<M>: unsigned integer type of bits.
    ///
    /// Encoded as M bits of big-endian number representation put into cell data.
    Uint(Uint),
    /// int<M>: signed integer type of bits.
    ///
    /// Encoded as M bits of big-endian number representation put into cell data.
    Int(Int),
    /// Variable length integer
    ///
    /// Encoded according to blockchain specification
    VarInt(usize, BigInt),
    /// Variable length unsigned integer
    ///
    /// Encoded according to blockchain specification
    VarUint(usize, BigUint),
    /// bool: boolean value.
    ///
    /// Encoded as one bit put into cell data.
    Bool(bool),
    /// Tuple: several values combinde into tuple.
    ///
    /// Encoded as all tuple elements encodings put into cell data one by one.
    Tuple(Vec<Token>),
    /// T[]: dynamic array of elements of the type T.
    ///
    /// Encoded as all array elements encodings put to separate cell.
    Array(ParamType, Vec<TokenValue>),
    /// T[k]: dynamic array of elements of the type T.
    ///
    /// Encoded as all array elements encodings put to separate cell.
    FixedArray(ParamType, Vec<TokenValue>),
    /// TVM Cell
    ///
    Cell(Cell),
    /// Dictionary of values
    ///
    Map(ParamType, ParamType, BTreeMap<MapKeyTokenValue, TokenValue>),
    /// MsgAddress
    ///
    Address(MsgAddress),
    /// AddrStd or AddrNone
    AddressStd(MsgAddress),
    /// Raw byte array
    ///
    /// Encoded as separate cells chain
    Bytes(Vec<u8>),
    /// Fixed sized raw byte array
    ///
    /// Encoded as separate cells chain
    FixedBytes(Vec<u8>),
    /// UTF8 string
    ///
    /// Encoded similar to `Bytes`
    String(String),
    /// Nanograms
    ///
    Token(Grams),
    /// Timestamp
    Time(u64),
    /// Message expiration time
    Expire(u32),
    /// Public key
    PublicKey(Option<ed25519_dalek::PublicKey>),
    /// Optional parameter
    Optional(ParamType, Option<Box<TokenValue>>),
    /// Parameter stored in reference
    Ref(Box<TokenValue>),
}

impl fmt::Display for TokenValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TokenValue::Uint(u) => write!(f, "{}", u.number),
            TokenValue::Int(u) => write!(f, "{}", u.number),
            TokenValue::VarUint(_, u) => write!(f, "{u}"),
            TokenValue::VarInt(_, u) => write!(f, "{u}"),
            TokenValue::Bool(b) => write!(f, "{b}"),
            TokenValue::Tuple(tokens) => {
                let mut first = true;
                for token in tokens {
                    if first {
                        write!(f, "({token}")?;
                    } else {
                        write!(f, ",{token}")?;
                    }
                    first = false;
                }
                write!(f, ")")
            }
            TokenValue::Array(_, tokens) | TokenValue::FixedArray(_, tokens) => {
                let mut first = true;
                for token in tokens {
                    if first {
                        write!(f, "[{token}")?;
                    } else {
                        write!(f, ",{token}")?;
                    }
                    first = false;
                }
                write!(f, "]")
            }
            TokenValue::Cell(c) => write!(f, "{c:?}"),
            TokenValue::Map(_key_type, _value_type, map) => {
                let s = map
                    .iter()
                    .map(|(k, v)| format!("{k}:{v}"))
                    .collect::<Vec<String>>()
                    .join(",");

                write!(f, "{{{}}}", s)
            }
            TokenValue::Address(a) | TokenValue::AddressStd(a) => write!(f, "{}", a),
            TokenValue::Bytes(bytes) | TokenValue::FixedBytes(bytes) => write!(f, "{bytes:?}"),
            TokenValue::String(string) => write!(f, "{string}"),
            TokenValue::Token(g) => write!(f, "{g}"),
            TokenValue::Time(time) => write!(f, "{time}"),
            TokenValue::Expire(expire) => write!(f, "{expire}"),
            TokenValue::Ref(value) => write!(f, "{value}"),
            TokenValue::PublicKey(key) => if let Some(key) = key {
                write!(f, "{}", hex::encode(&key.to_bytes()))
            } else {
                write!(f, "None")
            },
            TokenValue::Optional(_, value) => if let Some(value) = value {
                write!(f, "{value}")
            } else {
                write!(f, "None")
            }
        }
    }
}

impl TokenValue {
    /// Check whether the type of the token matches the given parameter type.
    ///
    /// Numeric types (`Int` and `Uint`) type check if the size of the token
    /// type is of equal size with the provided parameter type.
    pub fn type_check(&self, param_type: &ParamType) -> bool {
        match self {
            TokenValue::Uint(uint) => *param_type == ParamType::Uint(uint.size),
            TokenValue::Int(int) => *param_type == ParamType::Int(int.size),
            TokenValue::VarUint(size, _) => *param_type == ParamType::VarUint(*size),
            TokenValue::VarInt(size, _) => *param_type == ParamType::VarInt(*size),
            TokenValue::Bool(_) => *param_type == ParamType::Bool,
            TokenValue::Tuple(ref arr) => {
                if let ParamType::Tuple(params) = param_type {
                    Token::types_check(arr, &params)
                } else {
                    false
                }
            }
            TokenValue::Array(inner_type, ref tokens) => {
                if let ParamType::Array(ref param_type) = *param_type {
                    inner_type == param_type.as_ref()
                        && tokens.iter().all(|t| t.type_check(param_type))
                } else {
                    false
                }
            }
            TokenValue::FixedArray(inner_type, ref tokens) => {
                if let ParamType::FixedArray(ref param_type, size) = *param_type {
                    size == tokens.len()
                        && inner_type == param_type.as_ref()
                        && tokens.iter().all(|t| t.type_check(param_type))
                } else {
                    false
                }
            }
            TokenValue::Cell(_) => *param_type == ParamType::Cell,
            TokenValue::Map(map_key_type, map_value_type, ref values) => {
                if let ParamType::Map(ref key_type, ref value_type) = *param_type {
                    map_key_type == key_type.as_ref()
                        && map_value_type == value_type.as_ref()
                        && values.iter().all(|t| t.1.type_check(value_type))
                } else {
                    false
                }
            }
            TokenValue::Address(_) => *param_type == ParamType::Address,
            TokenValue::AddressStd(_) => *param_type == ParamType::AddressStd,
            TokenValue::Bytes(_) => *param_type == ParamType::Bytes,
            TokenValue::FixedBytes(ref arr) => *param_type == ParamType::FixedBytes(arr.len()),
            TokenValue::String(_) => *param_type == ParamType::String,
            TokenValue::Token(_) => *param_type == ParamType::Token,
            TokenValue::Time(_) => *param_type == ParamType::Time,
            TokenValue::Expire(_) => *param_type == ParamType::Expire,
            TokenValue::PublicKey(_) => *param_type == ParamType::PublicKey,
            TokenValue::Optional(opt_type, opt_value) => {
                if let ParamType::Optional(ref param_type) = *param_type {
                    param_type.as_ref() == opt_type
                        && opt_value
                        .as_ref()
                        .map(|val| val.type_check(param_type))
                        .unwrap_or(true)
                } else {
                    false
                }
            }
            TokenValue::Ref(value) => {
                if let ParamType::Ref(ref param_type) = *param_type {
                    value.type_check(param_type)
                } else {
                    false
                }
            }
        }
    }

    /// Returns `ParamType` the token value represents
    pub(crate) fn get_param_type(&self) -> ParamType {
        match self {
            TokenValue::Uint(uint) => ParamType::Uint(uint.size),
            TokenValue::Int(int) => ParamType::Int(int.size),
            TokenValue::VarUint(size, _) => ParamType::VarUint(*size),
            TokenValue::VarInt(size, _) => ParamType::VarInt(*size),
            TokenValue::Bool(_) => ParamType::Bool,
            TokenValue::Tuple(ref arr) => {
                ParamType::Tuple(arr.iter().map(|token| token.get_param()).collect())
            }
            TokenValue::Array(param_type, _) => ParamType::Array(Box::new(param_type.clone())),
            TokenValue::FixedArray(param_type, tokens) => {
                ParamType::FixedArray(Box::new(param_type.clone()), tokens.len())
            }
            TokenValue::Cell(_) => ParamType::Cell,
            TokenValue::Map(key_type, value_type, _) => {
                ParamType::Map(Box::new(key_type.clone()), Box::new(value_type.clone()))
            }
            TokenValue::Address(_) => ParamType::Address,
            TokenValue::AddressStd(_) => ParamType::AddressStd,
            TokenValue::Bytes(_) => ParamType::Bytes,
            TokenValue::FixedBytes(ref arr) => ParamType::FixedBytes(arr.len()),
            TokenValue::String(_) => ParamType::String,
            TokenValue::Token(_) => ParamType::Token,
            TokenValue::Time(_) => ParamType::Time,
            TokenValue::Expire(_) => ParamType::Expire,
            TokenValue::PublicKey(_) => ParamType::PublicKey,
            TokenValue::Optional(ref param_type, _) => {
                ParamType::Optional(Box::new(param_type.clone()))
            }
            TokenValue::Ref(value) => ParamType::Ref(Box::new(value.get_param_type())),
        }
    }

    pub fn get_default_value_for_header(param_type: &ParamType) -> Result<Self> {
        match param_type {
            ParamType::Time => Ok(TokenValue::Time(now_ms_u64())),
            ParamType::Expire => Ok(TokenValue::Expire(u32::MAX)),
            ParamType::PublicKey => Ok(TokenValue::PublicKey(None)),
            any_type => Err(
                AbiError::InvalidInputData {
                    msg: format!(
                        "Type {} doesn't have default value and must be explicitly defined",
                        any_type)
                }.into())
        }
    }

    pub fn get_map_key_size(param_type: &ParamType) -> Result<usize> {
        match param_type {
            ParamType::Int(size) | ParamType::Uint(size) => Ok(*size),
            ParamType::Address | ParamType::AddressStd => Ok(crate::token::STD_ADDRESS_BIT_LENGTH),
            _ => Err(error!(AbiError::InvalidData {
                msg: "Only integer and std address values can be map keys".to_owned()
            })),
        }
    }

    pub(crate) fn varint_size_len(size: usize) -> usize {
        8 - ((size - 1) as u8).leading_zeros() as usize
    }

    pub(crate) fn is_large_optional(param_type: &ParamType, abi_version: &AbiVersion) -> bool {
        Self::max_bit_size(param_type, abi_version) >= BuilderData::bits_capacity()
            || Self::max_refs_count(param_type, abi_version) >= BuilderData::references_capacity()
    }

    pub(crate) fn max_refs_count(param_type: &ParamType, abi_version: &AbiVersion) -> usize {
        match param_type {
            // in-cell serialized types
            ParamType::Uint(_)
            | ParamType::Int(_)
            | ParamType::VarUint(_)
            | ParamType::VarInt(_)
            | ParamType::Bool
            | ParamType::Address
            | ParamType::AddressStd
            | ParamType::Token
            | ParamType::Time
            | ParamType::Expire
            | ParamType::PublicKey => 0,
            ParamType::FixedBytes(_) if abi_version >= &ABI_VERSION_2_4 => 0,
            // reference serialized types
            ParamType::Array(_)
            | ParamType::FixedArray(_, _)
            | ParamType::Cell
            | ParamType::String
            | ParamType::Map(_, _)
            | ParamType::Bytes
            | ParamType::FixedBytes(_)
            | ParamType::Ref(_) => 1,
            // tuple refs is sum of inner types refs
            ParamType::Tuple(params) => params.iter().fold(0, |acc, param| {
                acc + Self::max_refs_count(&param.kind, abi_version)
            }),
            // large optional is serialized into reference
            ParamType::Optional(param_type) => {
                if Self::is_large_optional(param_type, abi_version) {
                    1
                } else {
                    Self::max_refs_count(param_type, abi_version)
                }
            }
        }
    }

    pub(crate) fn max_bit_size(param_type: &ParamType, abi_version: &AbiVersion) -> usize {
        match param_type {
            ParamType::Uint(size) => *size,
            ParamType::Int(size) => *size,
            ParamType::VarUint(size) => Self::varint_size_len(*size) + (size - 1) * 8,
            ParamType::VarInt(size) => Self::varint_size_len(*size) + (size - 1) * 8,
            ParamType::Bool => 1,
            ParamType::Array(_) => 33,
            ParamType::FixedArray(_, _) => 1,
            ParamType::Cell => 0,
            ParamType::Map(_, _) => 1,
            ParamType::Address => 591,
            ParamType::AddressStd => 2 + (1 + 5 + 30) + 8 + 256,
            ParamType::FixedBytes(size) if  abi_version >= &ABI_VERSION_2_4 => size * 8,
            ParamType::Bytes | ParamType::FixedBytes(_) => 0,
            ParamType::String => 0,
            ParamType::Token => 124,
            ParamType::Time => 64,
            ParamType::Expire => 32,
            ParamType::PublicKey => 257,
            ParamType::Ref(_) => 0,
            ParamType::Tuple(params) => params.iter().fold(0, |acc, param| {
                acc + Self::max_bit_size(&param.kind, abi_version)
            }),
            ParamType::Optional(param_type) => {
                if Self::is_large_optional(&param_type, abi_version) {
                    1
                } else {
                    1 + Self::max_bit_size(&param_type, abi_version)
                }
            }
        }
    }

    pub(crate) fn default_value(param_type: &ParamType) -> TokenValue {
        match param_type {
            ParamType::Uint(size) => TokenValue::Uint(Uint::new(0, *size)),
            ParamType::Int(size) => TokenValue::Int(Int::new(0, *size)),
            ParamType::VarUint(size) => TokenValue::VarUint(*size, 0u32.into()),
            ParamType::VarInt(size) => TokenValue::VarInt(*size, 0.into()),
            ParamType::Bool => TokenValue::Bool(false),
            ParamType::Array(inner) => TokenValue::Array(inner.as_ref().clone(), vec![]),
            ParamType::FixedArray(inner, size) => TokenValue::FixedArray(
                inner.as_ref().clone(),
                std::iter::repeat(Self::default_value(inner))
                    .take(*size)
                    .collect(),
            ),
            ParamType::Cell => TokenValue::Cell(Default::default()),
            ParamType::Map(key, value) => TokenValue::Map(
                key.as_ref().clone(),
                value.as_ref().clone(),
                Default::default(),
            ),
            ParamType::Address => TokenValue::Address(MsgAddress::AddrNone),
            ParamType::AddressStd => TokenValue::AddressStd(MsgAddress::AddrNone),
            ParamType::Bytes => TokenValue::Bytes(vec![]),
            ParamType::FixedBytes(size) => TokenValue::FixedBytes(vec![0; *size]),
            ParamType::String => TokenValue::String(Default::default()),
            ParamType::Token => TokenValue::Token(Default::default()),
            ParamType::Time => TokenValue::Time(0),
            ParamType::Expire => TokenValue::Expire(0),
            ParamType::PublicKey => TokenValue::PublicKey(None),
            ParamType::Ref(inner) => TokenValue::Ref(Box::new(Self::default_value(inner))),
            ParamType::Tuple(params) => TokenValue::Tuple(
                params
                    .iter()
                    .map(|inner| Token {
                        name: inner.name.clone(),
                        value: Self::default_value(&inner.kind),
                    })
                    .collect(),
            ),
            ParamType::Optional(inner) => TokenValue::Optional(inner.as_ref().clone(), None),

        }
    }
}

impl Token {
    /// Check if all the types of the tokens match the given parameter types.
    pub fn types_check(tokens: &[Token], params: &[Param]) -> bool {
        params.len() == tokens.len() && {
            params.iter().zip(tokens).all(|(param, token)| {
                // println!("{} {} {}", token.name, token.value, param.kind);
                token.value.type_check(&param.kind) && token.name == param.name
            })
        }
    }

    /// Returns `Param` the token represents
    pub(crate) fn get_param(&self) -> Param {
        Param {
            name: self.name.clone(),
            kind: self.value.get_param_type(),
        }
    }
}

#[cfg(all(target_arch = "wasm32", feature = "web"))]
fn now_ms_u64() -> u64 {
    js_sys::Date::now() as u64
}

#[cfg(not(all(target_arch = "wasm32", feature = "web")))]
fn now_ms_u64() -> u64 {
    use std::time::SystemTime;

    let duration = (SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)).expect("Shouldn't fail");
    duration.as_secs() * 1000 + duration.subsec_millis() as u64
}
