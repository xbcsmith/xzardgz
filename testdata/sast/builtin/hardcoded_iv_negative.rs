use rand::RngCore;
use rand::rngs::OsRng;

fn encrypt(plaintext: &[u8]) -> Vec<u8> {
    let mut iv = [0u8; 16];
    OsRng.fill_bytes(&mut iv);
    vec![]
}
