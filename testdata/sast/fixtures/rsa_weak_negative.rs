// Negative fixture for rust-weak-rsa-key rule.
// This file generates a compliant RSA key with 2048 bits.
fn generate_compliant_key() {
    let key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
    let _ = key;
}
