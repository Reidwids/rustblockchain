use crate::{
    cli::db::{blockchain_exists, put_db, LAST_HASH_KEY},
    ownership::address::Address,
};

use super::block::Block;

pub fn init_blockchain(addr: &Address) {
    if blockchain_exists() {
        panic!("[chain::init_blockchain] ERROR: Blockchain already exists");
    }

    let genesis_block = Block::genesis(addr);

    let block_hash = &genesis_block.hash();
    let block_data = bincode::serialize(&genesis_block)
        .expect("[chain::init_blockchain] ERROR: Failed to serialize genesis block");

    // Store block ref and last hash
    put_db(&genesis_block.hash(), &block_data);
    put_db(LAST_HASH_KEY.as_bytes(), block_hash);
}
