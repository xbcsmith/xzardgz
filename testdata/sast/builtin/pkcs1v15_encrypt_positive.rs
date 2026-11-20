use rand::rngs::OsRng;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};

fn encrypt_message(pub_key: &RsaPublicKey, message: &[u8]) -> Vec<u8> {
    pub_key
        .encrypt(&mut OsRng, Pkcs1v15Encrypt, message)
        .unwrap()
}
