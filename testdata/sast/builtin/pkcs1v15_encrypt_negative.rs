use rand::rngs::OsRng;
use rsa::{Oaep, RsaPublicKey};
use sha2::Sha256;

fn encrypt_message(pub_key: &RsaPublicKey, message: &[u8]) -> Vec<u8> {
    pub_key
        .encrypt(&mut OsRng, Oaep::new::<Sha256>(), message)
        .unwrap()
}
