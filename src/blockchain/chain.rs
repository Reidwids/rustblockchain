use rocksdb::IteratorMode;

use crate::{
    cli::db::{blockchain_exists, get_db, get_last_hash, open_db, put_db, LAST_HASH_KEY},
    ownership::{address::Address, node::NODE_KEY},
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
    let db = open_db();

    let mut batch = rocksdb::WriteBatch::default();

    for item in db.iterator(IteratorMode::Start).flatten() {
        let (key, _) = item;
        if key.as_ref() != NODE_KEY.as_bytes() {
            batch.delete(key.as_ref()); // Convert Box<[u8]> to &[u8]
        }
    }

    db.write(batch)
        .expect("[chain::clear_blockchain] ERROR: Failed to delete blockchain");
}

pub fn get_last_block() -> Block {
    let lh = get_last_hash();
    let block_serialized =
        get_db(&lh).expect("[block::get_last_block] ERROR: Could not get last block");
    let block: Block = bincode::deserialize(&block_serialized)
        .expect("[block::get_last_block] ERROR: Failed to deserialize last block");
    block
}
