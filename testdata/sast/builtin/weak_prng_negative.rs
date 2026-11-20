use rand::rngs::OsRng;
use rand::Rng;

fn generate_token() -> u64 {
    OsRng.gen()
}
