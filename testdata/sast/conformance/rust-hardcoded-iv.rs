// ruleid: rust-hardcoded-iv
let iv = [0u8; 16];
// ruleid: rust-hardcoded-iv
let nonce = [1u8, 2u8, 3u8, 4u8, 5u8, 6u8, 7u8, 8u8, 9u8, 10u8, 11u8, 12u8];
// ok: generating random IV is safe
let iv = OsRng.fill_bytes(&mut iv_buf);
