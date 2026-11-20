use aes_gcm::{Aes256Gcm, KeyInit};

fn encrypt_data(key: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new_from_slice(key).unwrap();
    vec![]
}
