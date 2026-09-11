// Negative fixture for rust-md5-usage rule.
// This file uses SHA-256 (a strong hash function) instead of MD5.
use sha2::Digest;
use sha2::Sha256;

/// Computes a SHA-256 digest of the given data.
pub fn compute_sha256_digest(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}
