// Positive fixture for rust-md5-usage rule.
// This file intentionally uses the broken MD5 hash function.
use md5::Digest;
use md5::Md5;

/// Computes an MD5 digest of the given data.
/// This is intentionally using the broken MD5 algorithm.
pub fn compute_md5_digest(data: &[u8]) -> Vec<u8> {
    let result = md5::compute(data);
    result.0.to_vec()
}

pub fn new_md5_hasher() -> Md5 {
    Md5::new()
}
