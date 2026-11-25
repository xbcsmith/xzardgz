// ruleid: rust-des-cipher
let _ = Des::new(key);
// ok: AES-GCM is safe
let _ = Aes256Gcm::new(key);
