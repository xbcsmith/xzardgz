// ruleid: rust-pkcs1v15-encrypt
let _ = key.encrypt(&mut rng, Pkcs1v15Encrypt, msg);
// ruleid: rust-pkcs1v15-encrypt
let _ = rsa::pkcs1v15::Pkcs1v15Encrypt;
// ok: OAEP is safe
let _ = key.encrypt(&mut rng, Oaep::new::<sha2::Sha256>(), msg);
