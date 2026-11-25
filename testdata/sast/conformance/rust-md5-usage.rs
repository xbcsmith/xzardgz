// ruleid: rust-md5-usage
let _ = md5::compute(b"hello");
// ruleid: rust-md5-usage
let _ = Md5::new();
// ok: SHA-256 is a safe alternative
let _ = sha2::Sha256::new();
