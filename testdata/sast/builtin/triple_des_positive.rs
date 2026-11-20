use cipher::KeyInit;
use des::TdesEde3;

fn encrypt_triple_des(key: &[u8]) -> Vec<u8> {
    let cipher = TdesEde3::new_from_slice(key).unwrap();
    vec![]
}
