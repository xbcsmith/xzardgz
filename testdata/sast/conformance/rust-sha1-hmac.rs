// ruleid: rust-sha1-hmac
let _ = Hmac::<Sha1>::new_from_slice(key);
// ruleid: rust-sha1-hmac
let _ = HmacSha1::new_from_slice(key);
// ok: HMAC-SHA256 is safe
let _ = Hmac::<Sha256>::new_from_slice(key);
