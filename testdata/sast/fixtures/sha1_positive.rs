// Positive fixture for rust-sha1-usage rule.
// This file intentionally uses the cryptographically broken SHA-1.
use sha1::Digest;
use sha1::Sha1;

/// Creates a new SHA-1 hasher.
pub fn new_sha1_hasher() -> Sha1 {
    sha1::Sha1::new()
}

/// Also matches the pattern: Sha1::new()
pub fn new_sha1_direct() -> Sha1 {
    Sha1::new()
}
