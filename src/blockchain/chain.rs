use rocksdb::IteratorMode;

use crate::{
    cli::db::{self, blockchain_exists, get_last_hash, ROCKS_DB},
    networking::node::NODE_KEY,
    wallets::address::Address,
};

use super::block::Block;

/// Initializes the blockchain, and fails if a blockchain already exists
pub fn create_blockchain(addr: &Address) {
    if blockchain_exists() {
        panic!("[chain::create_blockchain] ERROR: Blockchain already exists");
    }

    let mut genesis_block = Block::genesis(addr);

    genesis_block.mine();
}

/// Clears the existing chain. Retains the node id
pub fn clear_blockchain() {
    let mut batch = rocksdb::WriteBatch::default();

    for item in ROCKS_DB.iterator(IteratorMode::Start).flatten() {
        let (key, _) = item;
        if key.as_ref() != NODE_KEY.as_bytes() {
            batch.delete(key.as_ref()); // Convert Box<[u8]> to &[u8]
        }
    }

    ROCKS_DB
        .write(batch)
        .expect("[chain::clear_blockchain] ERROR: Failed to delete blockchain");
}

pub fn get_last_block() -> Block {
    let lh: [u8; 32] = get_last_hash();
    db::get_block(&lh).expect("[block::get_last_block] ERROR: Could not get last block")
}
