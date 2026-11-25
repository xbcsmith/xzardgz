// ruleid: rust-weak-rsa-key
let _ = RsaPrivateKey::new(&mut rng, 1024);
// ok: 4096 bits is strong
let _ = RsaPrivateKey::new(&mut rng, 4096);
