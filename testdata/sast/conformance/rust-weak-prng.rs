// ruleid: rust-weak-prng
let _ = SmallRng::seed_from_u64(42);
// ruleid: rust-weak-prng
let _ = SmallRng::from_entropy();
// ok: OsRng is cryptographically secure
let _ = OsRng;
