use blowfish::Blowfish;
use cipher::KeyInit;

fn encrypt_blowfish(key: &[u8]) {
    let cipher = Blowfish::new_from_slice(key).unwrap();
}
