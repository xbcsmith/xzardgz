// ruleid: rust-triple-des-cipher
let _ = TdesEde3::new_from_slice(key);
// ruleid: rust-triple-des-cipher
let _ = TdesEde2::new_from_slice(key);
// ok: AES-GCM is safe
let _ = Aes256Gcm::new_from_slice(key);
