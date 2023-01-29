use std::borrow::Cow;

use ed25519::signature::Signer;

pub fn sign_with_signature_id(key: &ed25519_dalek::Keypair, data: &[u8], signature_id: Option<i32>) -> ed25519_dalek::Signature {
    let data = extend_signature_with_id(data, signature_id);
    key.sign(&data)
}

pub fn extend_signature_with_id(data: &[u8], signature_id: Option<i32>) -> Cow<'_, [u8]> {
    match signature_id {
        Some(signature_id) => {
            let mut result = Vec::with_capacity(4 + data.len());
            result.extend_from_slice(&signature_id.to_be_bytes());
            result.extend_from_slice(data);
            Cow::Owned(result)
        }
        None => Cow::Borrowed(data),
    }
}
