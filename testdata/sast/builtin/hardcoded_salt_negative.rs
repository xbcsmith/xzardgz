use rand::RngCore;
use rand::rngs::OsRng;

fn hash_password(password: &str) -> String {
    let mut salt = [0u8; 32];
    OsRng.fill_bytes(&mut salt);
    String::new()
}
