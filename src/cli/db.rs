use rocksdb::DB;

use crate::blockchain::block::Block;

pub const DB_PATH: &str = "./data/db";
pub const LAST_HASH_KEY: &str = "lh";

// Our db will hold 2 types of kv pairs - an "lh" / hash pair to store our last hash,
// And hash / block pairs to store and retrieve each block
pub fn open_db() -> DB {
    DB::open_default(DB_PATH).expect("[open_db] ERROR: Failed to open RocksDB")
}

pub fn put_db(key: &[u8], value: &[u8]) {
    let db = open_db();
    db.put(key, value)
        .expect("[put] ERROR: Failed to write to DB");
}

pub fn delete(key: &[u8]) {
    let db = open_db();
    db.delete(key)
        .expect("[delete] ERROR: Failed to delete from DB")
}

pub fn get_db(key: &[u8]) -> Option<Vec<u8>> {
    let db = open_db();
    db.get(key)
        .expect("[get] ERROR: Failed to read from DB")
        .map(|v| v.to_vec())
}

pub fn blockchain_exists() -> bool {
    let db = open_db();
    db.get(LAST_HASH_KEY.as_bytes()).unwrap_or(None).is_some()
}

pub fn get_block(block_hash: &[u8; 32]) -> Block {
    let block_data =
        get_db(block_hash).expect("[chain::iterator] ERROR: Failed find block hash in chain");
    let block: Block = bincode::deserialize(&block_data).unwrap();
    block
}

pub fn get_last_hash() -> [u8; 32] {
    let last_hash = get_db(LAST_HASH_KEY.as_bytes())
        .expect("[db::get_last_hash] ERROR: Failed to get last hash from the db");
    last_hash
        .try_into()
        .expect("[db::get_last_hash] ERROR: Failed to parse last hash")
}
