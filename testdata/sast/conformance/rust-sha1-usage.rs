// ruleid: rust-sha1-usage
let _ = sha1::Sha1::new();
// ruleid: rust-sha1-usage
let _ = Sha1::new();
// ok: SHA-256 is a safe alternative
let _ = sha2::Sha256::new();
