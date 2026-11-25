// ruleid: rust-hardcoded-salt
let salt = [0u8; 16];
// ruleid: rust-hardcoded-salt
let password_salt = [1u8, 2u8, 3u8, 4u8];
// ok: generating random salt is safe
let random_value = OsRng.gen::<[u8; 16]>();
