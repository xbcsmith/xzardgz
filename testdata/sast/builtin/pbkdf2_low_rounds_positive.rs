use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

fn derive_key(password: &[u8], salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password, salt, 1000, &mut key);
    key
}
