use ton_block::{Deserializable, StateInit};
use ton_types::{deserialize_cells_tree, Result, SliceData};

use crate::Contract;

const DEPOOL_TVC: &[u8] = include_bytes!("data/DePool.tvc");
const PUB_KEY: [u8; ed25519_dalek::PUBLIC_KEY_LENGTH] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
    17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32
];

#[test]
fn test_pubkey() -> Result<()> {
    let mut si_roots = deserialize_cells_tree(&mut DEPOOL_TVC.as_ref())?;
    assert_eq!(si_roots.len(), 1);

    let state_init = StateInit::construct_from(&mut SliceData::load_cell(si_roots.remove(0))?)?;
    let data = SliceData::load_cell(state_init.data.unwrap())?;

    let pub_key = Contract::get_pubkey(&data)?.unwrap();
    assert_eq!(pub_key, [0; ed25519_dalek::PUBLIC_KEY_LENGTH]);

    let data = Contract::insert_pubkey(data, &PUB_KEY)?;
    let pub_key = Contract::get_pubkey(&data)?.unwrap();

    assert_eq!(pub_key, PUB_KEY);

    Ok(())
}
