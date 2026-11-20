use chacha20poly1305::{ChaCha20Poly1305, KeyInit};

fn encrypt_data(key: &[u8]) {
    let cipher = ChaCha20Poly1305::new_from_slice(key).unwrap();
}
