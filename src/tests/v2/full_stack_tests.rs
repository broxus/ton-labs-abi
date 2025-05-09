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
use anyhow::Context;
use ed25519_dalek::{Signature, Signer};

use ton_types::{BuilderData, SliceData, IBitstring, deserialize_tree_of_cells};
use ton_types::dictionary::HashmapE;
use ton_block::{Deserializable, MsgAddress, MsgAddressInt, Serializable, Transaction};
use smallvec::smallvec;
use crate::contract::ABI_VERSION_2_2;
use crate::{Function, Param, ParamType, Token, Uint};
use crate::json_abi::*;

const WALLET_ABI: &str = r#"{
    "ABI version": 2,
    "header": [
        "expire",
        "pubkey"
    ],
    "functions": [
        {
            "name": "sendTransaction",
            "inputs": [
                {"name":"dest","type":"address"},
                {"name":"value","type":"uint128"},
                {"name":"bounce","type":"bool"}
            ],
            "outputs": [
            ]
        },
        {
            "name": "setSubscriptionAccount",
            "inputs": [
                {"name":"addr","type":"address"}
            ],
            "outputs": [
            ]
        },
        {
            "name": "getSubscriptionAccount",
            "inputs": [
            ],
            "outputs": [
                {"name":"value0","type":"address"}
            ]
        },
        {
            "name": "createOperationLimit",
            "inputs": [
                {"name":"value","type":"uint256"}
            ],
            "outputs": [
                {"name":"value0","type":"uint256"}
            ]
        },
        {
            "name": "createArbitraryLimit",
            "inputs": [
                {"name":"value","type":"uint128"},
                {"name":"period","type":"uint32"}
            ],
            "outputs": [
                {"name":"value0","type":"uint64"}
            ]
        },
        {
            "name": "changeLimit",
            "inputs": [
                {"name":"limitId","type":"uint64"},
                {"name":"value","type":"uint256"},
                {"name":"period","type":"uint32"}
            ],
            "outputs": [
            ]
        },
        {
            "name": "deleteLimit",
            "inputs": [
                {"name":"limitId","type":"uint64"}
            ],
            "outputs": [
            ]
        },
        {
            "name": "getLimit",
            "inputs": [
                {"name":"limitId","type":"uint64"}
            ],
            "outputs": [
                {"components":[{"name":"value","type":"uint256"},{"name":"period","type":"uint32"},{"name":"ltype","type":"uint8"},{"name":"spent","type":"uint256"},{"name":"start","type":"uint32"}],"name":"value0","type":"tuple"}
            ]
        },
        {
            "name": "getLimitCount",
            "inputs": [
            ],
            "outputs": [
                {"name":"value0","type":"uint64"}
            ]
        },
        {
            "name": "getLimits",
            "inputs": [
            ],
            "outputs": [
                {"name":"value0","type":"uint64[]"}
            ]
        },
        {
            "name": "constructor",
            "inputs": [
            ],
            "outputs": [
            ]
        }
    ],
    "events": [{
        "name": "event",
        "inputs": [
            {"name":"param","type":"uint8"}
        ]
    }
    ],
    "data": [
        {"key":101,"name":"subscription","type":"address"},
        {"key":100,"name":"owner","type":"uint256"}
    ]
}
"#;

#[test]
fn test_constructor_call() {
    let params = r#"{}"#;

    let test_tree = encode_function_call(
        WALLET_ABI,
        "constructor",
        None,
        params,
        false,
        None,
        None,
    ).unwrap();

    let mut expected_tree = BuilderData::new();
    expected_tree.append_bit_zero().unwrap();       // None for signature
    expected_tree.append_u32(0xffffffff).unwrap();  // max u32 for expire
    expected_tree.append_bit_zero().unwrap();       // None for public key
    expected_tree.append_u32(0x68B55F3F).unwrap();  // function id

    let test_tree = test_tree.into_cell().and_then(SliceData::load_cell).unwrap();
    let expected_tree = expected_tree.into_cell().and_then(SliceData::load_cell).unwrap();
    assert_eq!(test_tree, expected_tree);

    let response = decode_unknown_function_call(
        WALLET_ABI,
        test_tree,
        false,
        false,
    ).unwrap();

    assert_eq!(response.params, params);
    assert_eq!(response.function_name, "constructor");


    let test_tree = SliceData::from_raw(vec![0xE8, 0xB5, 0x5F, 0x3F], 32);

    let response = decode_unknown_function_response(
        WALLET_ABI,
        test_tree.clone(),
        false
    )
    .unwrap();

    assert_eq!(response.params, params);
    assert_eq!(response.function_name, "constructor");


    let response = decode_function_response(
        WALLET_ABI,
        "constructor",
        test_tree,
        false
    )
    .unwrap();

    assert_eq!(response, params);
}

#[test]
fn test_signed_call() {
    let params = r#"
    {
        "value": 12,
        "period": 30
    }"#;

    let expected_params = r#"{"value":"12","period":"30"}"#;

    let pair = Keypair::generate(&mut rand::thread_rng());

    let test_tree = encode_function_call(
        WALLET_ABI,
        "createArbitraryLimit",
        None,
        params,
        false,
        Some((&pair, None)),
        None,
    )
    .unwrap();

    let mut test_tree = test_tree.into_cell().and_then(SliceData::load_cell).unwrap();

    let response = decode_unknown_function_call(
        WALLET_ABI,
        test_tree.clone(),
        false,
        false
    )
    .unwrap();

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&response.params).unwrap(),
        serde_json::from_str::<serde_json::Value>(expected_params).unwrap());
    assert_eq!(response.function_name, "createArbitraryLimit");

    let mut expected_tree = BuilderData::new();
    expected_tree.append_u32(0xffffffff).unwrap();          // expire
    expected_tree.append_bit_one().unwrap();                // Some for public key
    expected_tree.append_raw(&pair.public.to_bytes(), ed25519_dalek::PUBLIC_KEY_LENGTH * 8).unwrap();
    expected_tree.append_u32(0x2238B58A).unwrap();          // function id
    expected_tree.append_raw(&[0; 15], 15 * 8).unwrap();    // value
    expected_tree.append_u8(12).unwrap();                   // value
    expected_tree.append_u32(30).unwrap();                  // period

    assert!(test_tree.get_next_bit().unwrap());
    let sign = &test_tree.get_next_bytes(ed25519_dalek::SIGNATURE_LENGTH).unwrap();
    let sign = Signature::from_bytes(sign.as_slice()).unwrap();

    assert_eq!(test_tree, expected_tree.into_cell().and_then(SliceData::load_cell).unwrap());

    let hash = test_tree.into_cell().repr_hash();
    pair.verify(hash.as_slice(), &sign).unwrap();

    let expected_response = r#"{"value0":"0"}"#;

    let response_tree = SliceData::load_builder(
        BuilderData::with_bitstring(
            smallvec![0xA2, 0x38, 0xB5, 0x8A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80])
            .unwrap()
        ).unwrap();

    let response = decode_function_response(
        WALLET_ABI,
        "createArbitraryLimit",
        response_tree.clone(),
        false
    )
    .unwrap();

    assert_eq!(response, expected_response);


    let response = decode_unknown_function_response(
        WALLET_ABI,
        response_tree,
        false
    )
    .unwrap();

    assert_eq!(response.params, expected_response);
    assert_eq!(response.function_name, "createArbitraryLimit");
}

#[test]
fn test_not_signed_call() {
    let params = r#"{
        "limitId": "0x2"
    }"#;
    let header = r#"{
        "pubkey": "11c0a428b6768562df09db05326595337dbb5f8dde0e128224d4df48df760f17",
        "expire": 123
    }"#;

    let test_tree = encode_function_call(
        WALLET_ABI,
        "getLimit",
        Some(header),
        params,
        false,
        None,
        None,
    )
    .unwrap();

    let mut expected_tree = BuilderData::new();
    expected_tree.append_bit_zero().unwrap();        // None for signature
    expected_tree.append_u32(123).unwrap();          // expire
    expected_tree.append_bit_one().unwrap();         // Some for public key
    expected_tree.append_raw(
        &hex::decode("11c0a428b6768562df09db05326595337dbb5f8dde0e128224d4df48df760f17").unwrap(),
        32 * 8).unwrap();                            // pubkey
    expected_tree.append_u32(0x4B774C98).unwrap();   // function id
    expected_tree.append_u64(2).unwrap();            // limitId

    assert_eq!(test_tree, expected_tree);
}

#[test]
fn test_add_signature_full() {
    let params = r#"{"limitId":"2"}"#;
    let header = "{}";

    let (msg, data_to_sign) = prepare_function_call_for_sign(
        WALLET_ABI,
        "getLimit",
        Some(header),
        params,
        None,
    )
    .unwrap();

    let pair = Keypair::generate(&mut rand::thread_rng());
    let signature = pair.sign(data_to_sign.as_slice()).to_bytes().to_vec();

    let msg = add_sign_to_function_call(
        WALLET_ABI,
        &signature,
        Some(&pair.public.to_bytes()),
        msg.into_cell().and_then(SliceData::load_cell).unwrap()).unwrap();

    let decoded = decode_unknown_function_call(
        WALLET_ABI,
        msg.into_cell().and_then(SliceData::load_cell).unwrap(),
        false,
        false
    ).unwrap();

    assert_eq!(decoded.params, params);
}

#[test]
fn test_find_event() {
    let event_tree = SliceData::load_builder(
        BuilderData::with_bitstring(
            smallvec![0x0C, 0xAF, 0x24, 0xBE, 0xFF, 0x80])
        .unwrap()).unwrap();

    let decoded = decode_unknown_function_response(WALLET_ABI, event_tree, false).unwrap();

    assert_eq!(decoded.function_name, "event");
    assert_eq!(decoded.params, r#"{"param":"255"}"#);
}

#[test]
fn test_store_pubkey() {
    let mut test_map = HashmapE::with_bit_len(Contract::DATA_MAP_KEYLEN);
    let test_pubkey = [11u8; 32];
    test_map.set_builder(
        0u64.serialize().and_then(SliceData::load_cell).unwrap(),
        &BuilderData::with_raw(smallvec![0u8; 32], 256).unwrap(),
    ).unwrap();

    let data = test_map.serialize().unwrap();



    let new_data = Contract::insert_pubkey(SliceData::load_cell(data).unwrap(), &test_pubkey).unwrap();

    let new_map = HashmapE::with_hashmap(Contract::DATA_MAP_KEYLEN, new_data.reference_opt(0));
    let key_slice = new_map.get(
        0u64.serialize().and_then(SliceData::load_cell).unwrap(),
    )
    .unwrap()
    .unwrap();

    assert_eq!(key_slice.get_bytestring(0), test_pubkey);
}

#[test]
fn decode_params() {

    let abi = r#"{"ABI version":2,"data":[{"key":1,"name":"_randomNonce","type":"uint256"}],"events":[{"inputs":[{"name":"previousOwner","type":"uint256"},{"name":"newOwner","type":"uint256"}],"name":"OwnershipTransferred","outputs":[]}],"fields":[{"name":"_pubkey","type":"uint256"},{"name":"_timestamp","type":"uint64"},{"name":"_constructorFlag","type":"bool"},{"name":"owner","type":"uint256"},{"name":"_randomNonce","type":"uint256"}],"functions":[{"inputs":[{"name":"dest","type":"address"},{"name":"value","type":"uint128"},{"name":"bounce","type":"bool"},{"name":"flags","type":"uint8"},{"name":"payload","type":"cell"}],"name":"sendTransaction","outputs":[]},{"inputs":[{"name":"newOwner","type":"uint256"}],"name":"transferOwnership","outputs":[]},{"inputs":[],"name":"constructor","outputs":[]},{"inputs":[],"name":"owner","outputs":[{"name":"owner","type":"uint256"}]},{"inputs":[],"name":"_randomNonce","outputs":[{"name":"_randomNonce","type":"uint256"}]}],"header":["time"],"version":"2.2"}"#;
    let abi = Contract::load(abi.as_bytes()).unwrap();


    let tx = "te6ccgECCwEAAm8AA7V++NnCdgsS7iubg2YKljkWMK+Nl4rodhkGdA6ME3X1l2AAAVf+pN2AHEqw3VLPrqWO4rwpNsyQj5WeGXAg+bV8rOllzOC4e0FwAAFX/qPpXBYglxvwADRw9u7oBQQBAg8MQEYbHIJEQAMCAG/JiqxsTBx2WAAAAAAAAgAAAAAAAh+1bvDWnLCgRmTLFrApyvKnoCvN5oGbiFWPDRqpjy0EQJAfZACdQy+jE4gAAAAAAAAAACSAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIACCcj7pfRHC7sHIea84Hs/hukiSefxdzPwsH4Ne5GCMU2vSK9sdrhKJoUW6Md9cS31zzC4xsPaU5ULRh/WiRm/kBvoCAeAIBgEB3wcBsWgB3xs4TsFiXcVzcGzBUscixhXxsvFdDsMgzoHRgm6+su0ADXqbt4aynxEK1cyy5QQ+J7v5F3aeGaWPbVJjYUSyrwFR3IDsoAYcdoQAACr/1JuwBMQS437ACgHdiAHfGzhOwWJdxXNwbMFSxyLGFfGy8V0OwyDOgdGCbr6y7AV48xwh0kVSa89wWq/VOK3fnIvDljptTJeqcB10t2RV0KCx4Vq1k5kzaY0RhHBKxqT0tiyp/nC0e+1/jx19p/AwAAAF+9ORPq0zuZGyCQFlgAa9TdvDWU+IhWrmWXKCHxPd/Iu7TwzSx7apMbCiWVeAoAAAAAAAAAAAAAAADuaygBAICgBLDgTSnoAdQT3MtmUbh60b5nBYLKOECUJ+GUmxfw3N+6y3zjrx6BA=";
    let tx = Transaction::construct_from_base64(tx).unwrap();
    let message = tx.in_msg.unwrap().read_struct().context("Failed reading in msg").unwrap();
    let body = message.body().unwrap();

    let (_, _, body) =
        Function::decode_header(&abi.abi_version, body, &vec![Param::new("time", ParamType::Time)], false).unwrap();


    let input_params = vec![
        Param::new("dest", ParamType::Address),
        Param::new("value", ParamType::Uint(128)),
        Param::new( "bounce", ParamType::Bool),
        Param::new("flags", ParamType::Uint(8)),
        Param::new("payload", ParamType::Cell)
    ];

    let tokens = TokenValue::decode_params(
        input_params.as_slice(),
        body.slice,
        &ABI_VERSION_2_2,
        false,
    ).unwrap();

    let cell_bytes = base64::decode("te6ccgEBAQEAKAAASw4E0p6AHUE9zLZlG4etG+ZwWCyjhAlCfhlJsX8Nzfust8468egQ").unwrap();
    let mut cell_slice = cell_bytes.as_slice();

    let result_tokens = vec![
        Token::new("dest", TokenValue::Address(MsgAddress::from_str("0:35ea6ede1aca7c442b5732cb9410f89eefe45dda7866963db5498d8512cabc05").unwrap())),
        Token::new("value", TokenValue::Uint(Uint::new(2000000000u128, 128))),
        Token::new("bounce", TokenValue::Bool(true)),
        Token::new("flags", TokenValue::Uint(Uint::new(0u128, 8))),
        Token::new("payload", TokenValue::Cell(deserialize_tree_of_cells(&mut cell_slice ).unwrap())),
    ];

    assert_eq!(tokens, result_tokens);
}

#[test]
fn test_update_decode_contract_data() {
    let mut test_map = HashmapE::with_bit_len(Contract::DATA_MAP_KEYLEN);
    test_map
        .set_builder(
            SliceData::load_builder(0u64.write_to_new_cell().unwrap()).unwrap(),
            &BuilderData::with_raw(smallvec![0u8; 32], 256).unwrap(),
        )
        .unwrap();

    let params = r#"{
        "subscription": "0:1111111111111111111111111111111111111111111111111111111111111111",
        "owner": "15438945231642159389809464667825054380435997955418741871927677867721750618658"
     }
    "#;

    let data = SliceData::load_cell(test_map.serialize().unwrap()).unwrap();
    let new_data = update_contract_data(WALLET_ABI, params, data).unwrap();
    let new_map = HashmapE::with_hashmap(Contract::DATA_MAP_KEYLEN, new_data.reference_opt(0));

    let key_slice = new_map
        .get(SliceData::load_builder(0u64.write_to_new_cell().unwrap()).unwrap())
        .unwrap()
        .unwrap();

    assert_eq!(key_slice.get_bytestring(0), vec![0u8; 32]);

    let subscription_slice = new_map
        .get(SliceData::load_builder(101u64.write_to_new_cell().unwrap()).unwrap())
        .unwrap()
        .unwrap();

    assert_eq!(
        subscription_slice,
        SliceData::load_cell(
            MsgAddressInt::with_standart(None, 0, [0x11; 32].into())
                .unwrap()
                .serialize()
                .unwrap()
        )
            .unwrap()
    );

    let owner_slice = new_map
        .get(SliceData::load_builder(100u64.write_to_new_cell().unwrap()).unwrap())
        .unwrap()
        .unwrap();

    assert_eq!(owner_slice.get_bytestring(0), vec![0x22; 32]);

    let decoded = decode_contract_data(WALLET_ABI, new_data, false).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(params).unwrap(),
        serde_json::from_str::<Value>(&decoded).unwrap()
    );
}

const ABI_WITH_FIELDS: &str = r#"{
    "version": "2.1",
    "functions": [],
    "fields": [
        {"name":"__pubkey","type":"uint256"},
        {"name":"__timestamp","type":"uint64"},
        {"name":"ok","type":"bool"},
        {"name":"value","type":"uint32"}
    ]
}"#;

#[test]
fn test_decode_storage_fields() {
    let mut storage = BuilderData::new();
    storage
        .append_bitstring(&[vec![0x55; 32], vec![0x80]].join(&[][..]))
        .unwrap();
    storage.append_u64(123).unwrap();
    storage.append_bit_one().unwrap();
    storage.append_u32(456).unwrap();
    let storage = SliceData::load_builder(storage).unwrap();

    let decoded = decode_storage_fields(ABI_WITH_FIELDS, storage, false).unwrap();

    assert_eq!(
        decoded,
        serde_json::json!({
            "__pubkey": "38597363079105398474523661669562635951089994888546854679819194669304376546645",
            "__timestamp":"123",
            "ok": true,
            "value": "456"
        })
            .to_string()
    );
}

fn value_helper(abi_type: &str, value: &str) -> Result<BuilderData> {
    let abi = serde_json::json!({
        "ABI version": 2,
        "version": "2.3",
        "functions": [
          {"name": "test","inputs": [{"name":"value","type":abi_type}],"outputs": []}
        ],
        "events": [],
        "data": []
    }).to_string();
    let params = serde_json::json!({"value": value}).to_string();
    encode_function_call(
        &abi,
        "test",
        None,
        &params,
        false,
        None,
        None,
    )
}

#[test]
fn test_max_varuint32() {
    // value max bit size (2 ** log2(32) - 1) * 8
    let value = "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let encoded = value_helper("varuint32", value).unwrap();

    assert_eq!(
        encoded.data(),
        &hex::decode("1869a0307ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc").unwrap()
    );
    assert_eq!(encoded.length_in_bits(), 286);
    assert_eq!(encoded.references().len(), 0);
}

#[test]
fn test_max_varint32() {
    // value max bit size (2 ** log2(32) - 1) * 8
    let value = "0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let encoded = value_helper("varint32", value).unwrap();

    assert_eq!(
        encoded.data(),
        &hex::decode("30d82fc87dfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc").unwrap()
    );
    assert_eq!(encoded.length_in_bits(), 286);
    assert_eq!(encoded.references().len(), 0);
}

#[test]
fn test_max_uint() {
    let value = "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let encoded = value_helper("uint256", value).unwrap();

    assert_eq!(
        encoded.data(),
        &hex::decode("3a8707b37fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff80").unwrap()
    );
    assert_eq!(encoded.length_in_bits(), 289);
    assert_eq!(encoded.references().len(), 0);
}

#[test]
fn test_max_int() {
    let value = "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let encoded = value_helper("int257", value).unwrap();

    assert_eq!(
        encoded.data(),
        &hex::decode("088fb044bfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffc0").unwrap()
    );
    assert_eq!(encoded.length_in_bits(), 290);
    assert_eq!(encoded.references().len(), 0);
}

const ABI_WITH_FIELDS_V24: &str = r#"{
    "version": "2.4",
    "functions": [],
    "fields": [
        {"name":"__pubkey","type":"uint256","init":true},
        {"name":"__timestamp","type":"uint64"},
        {"name":"ok","type":"bool", "init": true},
        {"name":"value","type":"address"}
    ]
}"#;

#[test]
fn test_encode_storage_fields() {
    let test_tree = encode_storage_fields(
        ABI_WITH_FIELDS_V24,
        Some(
            r#"{
            "__pubkey": "0x11c0a428b6768562df09db05326595337dbb5f8dde0e128224d4df48df760f17",
            "ok": true
        }"#,
        ),
    )
        .unwrap();

    let mut expected_tree = BuilderData::new();
    expected_tree
        .append_raw(
            &hex::decode("11c0a428b6768562df09db05326595337dbb5f8dde0e128224d4df48df760f17")
                .unwrap(),
            32 * 8,
        )
        .unwrap();
    expected_tree.append_u64(0).unwrap();
    expected_tree.append_bit_one().unwrap();
    expected_tree.append_bits(0, 2).unwrap();

    assert_eq!(test_tree, expected_tree);

    assert!(dbg!(encode_storage_fields(
        ABI_WITH_FIELDS_V24,
        Some(
            r#"{
            "ok": true
        }"#
        ),
    ))
        .is_err());

    assert!(dbg!(encode_storage_fields(
        ABI_WITH_FIELDS_V24,
        Some(
            r#"{
            "__pubkey": "0x11c0a428b6768562df09db05326595337dbb5f8dde0e128224d4df48df760f17",
            "__timestamp": 123,
            "ok": true
        }"#
        ),
    ))
        .is_err());
}

const ABI_WRONG_STORAGE_LAYOUT: &str = r#"{
	"ABI version": 2,
	"version": "2.3",
	"header": ["pubkey", "time", "expire"],
	"functions": [],
	"data": [
		{"key":1,"name":"_collectionName","type":"bytes"}
	],
	"events": [
	],
	"fields": [
		{"name":"_pubkey","type":"uint256"},
		{"name":"_timestamp","type":"uint64"},
		{"name":"_constructorFlag","type":"bool"},
		{"components":[{"name":"dtCreated","type":"uint32"},{"name":"ownerAddress","type":"address"},{"name":"kekAddress","type":"address"}],"name":"_info","type":"tuple"},
		{"components":[{"name":"contents","type":"bytes"},{"name":"extension","type":"bytes"},{"name":"name","type":"bytes"},{"name":"comment","type":"bytes"}],"name":"_media","type":"tuple"},
		{"name":"_collectionName","type":"bytes"},
		{"name":"_tokensIssued","type":"uint128"},
		{"name":"_externalMedia","type":"address"}
	]
}
"#;

#[test]
fn test_wrong_storage_layout() {
    let image = include_bytes!("FairNFTCollection.tvc");
    let image = ton_block::StateInit::construct_from_bytes(image).unwrap();

    assert!(decode_storage_fields(
        ABI_WRONG_STORAGE_LAYOUT,
        SliceData::load_cell(image.data.unwrap()).unwrap(),
        false
    )
        .is_ok());
}
