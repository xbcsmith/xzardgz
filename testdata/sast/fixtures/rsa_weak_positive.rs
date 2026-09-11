// Positive fixture for rust-weak-rsa-key rule.
// This file intentionally generates an RSA key with fewer than 2048 bits.
fn generate_weak_key() {
    let key = RsaPrivateKey::new(&mut rng, 1024).unwrap();
    let _ = key;
}
