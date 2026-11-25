// ruleid: rust-blowfish-cipher
let _ = Blowfish::new_from_slice(key);
// ok: AES-GCM is safe
let _ = Aes256Gcm::new_from_slice(key);
