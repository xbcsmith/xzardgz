// ruleid: rust-crc32-for-security
let _ = crc32fast::hash(data);
// ok: HMAC-SHA256 is a correct integrity check
let _ = hmac_sha256::hash(key, data);
