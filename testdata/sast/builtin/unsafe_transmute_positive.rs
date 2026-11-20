fn reinterpret_bytes(val: u32) -> f32 {
    unsafe { std::mem::transmute(val) }
}
