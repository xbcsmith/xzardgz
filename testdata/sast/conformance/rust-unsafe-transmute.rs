// ruleid: rust-unsafe-transmute
let _ = unsafe { std::mem::transmute(val) };
// ruleid: rust-unsafe-transmute
let _ = unsafe { mem::transmute::<u32, f32>(val) };
// ok: safe type conversion using From trait
let _ = u64::from(val);
