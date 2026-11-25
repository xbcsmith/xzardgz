// ruleid: rust-pbkdf2-low-rounds
pbkdf2::pbkdf2_hmac::<sha2::Sha256>(&pass, &salt, 1000, &mut out);
// ok: 600000 rounds exceeds the threshold
pbkdf2::pbkdf2_hmac::<sha2::Sha256>(&pass, &salt, 600000, &mut out);
