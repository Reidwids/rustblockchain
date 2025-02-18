use rocksdb::DB;

const DB_PATH: &str = "./data/db";
pub const LAST_HASH_KEY: &str = "lh";

// Our db will hold 2 types of kv pairs - an "lh" / hash pair to store our last hash,
// And hash / block pairs to store and retrieve each block
pub fn open_db() -> DB {
    DB::open_default(DB_PATH).expect("[get_db] ERROR: Failed to open RocksDB")
}

pub fn put_db(key: &[u8], value: &[u8]) {
    let db = DB::open_default(DB_PATH).expect("[put_db] ERROR: Failed to open RocksDB");
    db.put(key, value)
        .expect("[put] ERROR: Failed to write to DB");
}

pub fn get_db(key: &[u8]) -> Option<Vec<u8>> {
    let db = DB::open_default(DB_PATH).expect("[get_db] ERROR: Failed to open RocksDB");
    db.get(key)
        .expect("[get] ERROR: Failed to read from DB")
        .map(|v| v.to_vec())
}

pub fn blockchain_exists() -> bool {
    let db = DB::open_default(DB_PATH).expect("Failed to open RocksDB");
    db.get(LAST_HASH_KEY.as_bytes()).unwrap_or(None).is_some()
}
