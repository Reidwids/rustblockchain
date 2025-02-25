use rocksdb::IteratorMode;

use crate::{
    cli::db::{blockchain_exists, open_db, put_db, LAST_HASH_KEY},
    ownership::{address::Address, node::NODE_KEY},
};

use super::block::Block;

/// Initializes the blockchain, and fails if a blockchain already exists
pub fn create_blockchain(addr: &Address) {
    if blockchain_exists() {
        panic!("[chain::create_blockchain] ERROR: Blockchain already exists");
    }

    let genesis_block = Block::genesis(addr);

    let block_hash = &genesis_block.hash();
    let block_data = bincode::serialize(&genesis_block)
        .expect("[chain::create_blockchain] ERROR: Failed to serialize genesis block");

    // Store block ref and last hash
    put_db(&genesis_block.hash(), &block_data);
    put_db(LAST_HASH_KEY.as_bytes(), block_hash);
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
