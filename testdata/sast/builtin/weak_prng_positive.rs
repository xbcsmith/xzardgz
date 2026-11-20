use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

fn generate_token() -> u64 {
    let mut rng = SmallRng::from_entropy();
    rng.gen()
}
