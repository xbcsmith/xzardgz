// ruleid: rust-rc4-cipher
let _ = Rc4::new(key);
// ok: ChaCha20Poly1305 is safe
let _ = ChaCha20Poly1305::new(key);
