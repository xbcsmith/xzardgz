use hmac::{Hmac, Mac};
use sha2::Sha256;

fn verify_integrity(key: &[u8], data: &[u8], expected: &[u8]) -> bool {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(data);
    mac.verify_slice(expected).is_ok()
}
