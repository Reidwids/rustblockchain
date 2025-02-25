use crate::cli::db::DB_PATH;
use rocksdb::DB;
use uuid::Uuid;

pub const NODE_KEY: &str = "node_id";

/// Get or create the local node ID.
pub fn get_node_id() -> Uuid {
    let db = DB::open_default(DB_PATH).expect("[node::get_node_id] ERROR: Failed to open RocksDB");

    // Try to fetch existing node id
    if let Ok(Some(uuid_bytes)) = db.get(NODE_KEY) {
        let uuid_str = String::from_utf8(uuid_bytes)
            .expect("[node::get_node_id] ERROR: Invalid UUID format in DB");
        if let Ok(uuid) = Uuid::parse_str(&uuid_str) {
            return uuid;
        }
    }

    // Else create the Uuid
    let new_uuid = Uuid::new_v4();
    // Store it in RocksDB
    db.put(NODE_KEY, new_uuid.to_string().as_bytes())
        .expect("[node::get_node_id] ERROR: Failed to store node UUID in RocksDB");

    new_uuid
}
