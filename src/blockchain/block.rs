struct Block {
    hash: [u8; 32],
    prev_hash: [u8; 32],
    nonce: i32,
    height: i32,
    timestamp: i64,
}
