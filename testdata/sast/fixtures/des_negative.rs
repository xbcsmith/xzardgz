// Negative fixture for rust-des-cipher rule.
// This file uses AES-256-GCM (a strong cipher) instead of DES.
use aes_gcm::Aes256Gcm;
use aes_gcm::KeyInit;

/// Encrypts data using the strong AES-256-GCM cipher.
pub fn aes_encrypt(key: &[u8]) -> Aes256Gcm {
    Aes256Gcm::new(key.into())
}
