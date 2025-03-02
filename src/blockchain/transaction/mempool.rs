use crate::cli::db;

use super::tx::Tx;

const MEMPOOL_KEY: &[u8] = b"mempool";

pub fn put_mempool(tx: &Tx) {
    let mut mempool = get_mempool();
    mempool.push(tx.clone());
    let serialized =
        bincode::serialize(&mempool).expect("[mempool::put_mempool] ERROR: Failed to serialize tx");
    db::put_db(MEMPOOL_KEY, &serialized);
}

pub fn get_mempool() -> Vec<Tx> {
    // Try to get the serialized mempool
    match db::get_db(MEMPOOL_KEY) {
        Some(serialized) => {
            // Deserialize the mempool and return it
            bincode::deserialize(&serialized)
                .expect("[mempool::get_mempool] ERROR: Failed to deserialize mempool")
        }
        // If no pool exists, return an empty vec
        _ => Vec::new(),
    }
}
/// Delete all mempool entries by deleting the mempool key
pub fn reset_mempool() {
    let db = db::open_db();

    // Delete the mempool key, effectively resetting the entire mempool. No error on failure
    db.delete(MEMPOOL_KEY);
}
