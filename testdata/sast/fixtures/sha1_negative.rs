// Negative fixture for rust-sha1-usage rule.
// This file uses SHA-256 which is not flagged by the rule.
use sha2::Digest;
use sha2::Sha256;

pub fn hash_data(data: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().to_vec()
}
