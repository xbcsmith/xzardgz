// Positive fixture for rust-des-cipher rule.
// This file intentionally uses the broken DES block cipher.
use des::Des;
use des::KeyInit;

/// Encrypts data using the broken DES cipher.
pub fn des_encrypt(key: &[u8]) -> Des {
    Des::new(key.into())
}

/// Also uses Triple-DES.
pub fn triple_des_encrypt(key: &[u8]) {
    let _cipher = TdesEde3::new(key.into());
}
