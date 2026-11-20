fn verify_integrity(data: &[u8], expected: u32) -> bool {
    let checksum = crc32fast::hash(data);
    checksum == expected
}
