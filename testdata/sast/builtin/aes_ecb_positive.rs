use aes::Aes256;
use cbc::cipher::block_padding::Pkcs7;

fn encrypt_ecb(key: &[u8], data: &[u8]) -> Vec<u8> {
    let encryptor = ecb::Encryptor::<Aes256>::new(key.into());
    encryptor.encrypt_padded_vec_mut::<Pkcs7>(data)
}
