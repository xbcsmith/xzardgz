use aes_gcm::{Aes256Gcm, KeyInit, Nonce};

fn encrypt_gcm(key: &[u8], nonce_bytes: &[u8], data: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new_from_slice(key).unwrap();
    vec![]
}
