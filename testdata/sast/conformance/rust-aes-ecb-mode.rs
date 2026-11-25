// ruleid: rust-aes-ecb-mode
let _ = Ecb::<Aes128, _>::new(key);
// ruleid: rust-aes-ecb-mode
let _ = ecb::Encryptor::<Aes256>::new(key);
// ok: AES-GCM is safe
let _ = Aes256Gcm::new(key);
