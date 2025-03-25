use std::{error::Error, sync::Arc};

use libp2p::PeerId;
use once_cell::sync::Lazy;
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, DB};

use crate::{
    blockchain::{
        block::Block,
        transaction::{
            mempool::Mempool,
            tx::{Tx, TxOutput},
        },
    },
    networking::p2p::network::PeerCollection,
};

pub const LAST_HASH_KEY: &str = "lh";
const MEMPOOL_KEY: &str = "mempool";
const PEERS_KEY: &str = "peers";

const UTXO_CF: &str = "utxo";
const BLOCK_CF: &str = "block";

pub const DB_PATH: &str = "./data/db";

// Our db will hold 3 types of kv pairs - an "lh" / hash pair to store our last hash,
// hash / block pairs to store and retrieve each block, and utxos
pub static ROCKS_DB: Lazy<Arc<DB>> = Lazy::new(|| {
    let mut opts = Options::default();
    opts.create_if_missing(true);
    opts.create_missing_column_families(true);

    let cf_descriptors = vec![
        ColumnFamilyDescriptor::new(BLOCK_CF, Options::default()),
        ColumnFamilyDescriptor::new(UTXO_CF, Options::default()),
    ];

    let db =
        DB::open_cf_descriptors(&opts, DB_PATH, cf_descriptors).expect("Failed to open RocksDB");

    Arc::new(db) // Wrap DB in Arc to share it safely
});

/*** UTXO DB handlers ***/
pub fn utxo_cf() -> &'static ColumnFamily {
    ROCKS_DB
        .cf_handle(UTXO_CF)
        .expect("Column family not found")
}

fn to_utxo_db_key(tx_id: &[u8; 32], out_idx: u32) -> Vec<u8> {
    let mut key = Vec::with_capacity(36); // 32 bytes for tx_id + 4 bytes for out_idx
    key.extend_from_slice(tx_id);
    key.extend_from_slice(&out_idx.to_be_bytes());
    key
}

// TODO: make all db interactions have Result return types
pub fn from_utxo_db_key(key: &[u8]) -> ([u8; 32], u32) {
    // Ensure the key has the expected length (36 bytes: 32 for tx_id, 8 for out_idx)
    assert!(key.len() == 36, "Key length should be 36 bytes");

    let mut tx_id = [0u8; 32];
    tx_id.copy_from_slice(&key[0..32]); // Copy first 32 bytes into tx_id

    let out_idx = u32::from_be_bytes(key[32..36].try_into().expect("Failed to convert out_idx"));

    (tx_id, out_idx)
}

/// Returns an option representing a utxo. the utxo will be deserialized if found.
pub fn get_utxo(tx_id: &[u8; 32], out_idx: u32) -> Result<Option<TxOutput>, Box<dyn Error>> {
    let utxo_data = ROCKS_DB
        .get_cf(utxo_cf(), to_utxo_db_key(tx_id, out_idx))
        .map_err(|e| format!("[db::get_utxo] ERROR: Failed to read from DB {:?}", e))?;

    Ok(utxo_data.and_then(|data| bincode::deserialize(&data).ok()))
}

pub fn put_utxo(tx_id: &[u8; 32], out_idx: u32, tx_out: &TxOutput) -> Result<(), Box<dyn Error>> {
    let serialized = bincode::serialize(&tx_out)
        .map_err(|e| format!("[db::put_utxo] ERROR: Serialization failed {:?}", e))?;

    ROCKS_DB
        .put_cf(utxo_cf(), to_utxo_db_key(tx_id, out_idx), serialized)
        .map_err(|e| format!("[db::put_utxo] ERROR: Failed to write to DB {:?}", e))?;

    Ok(())
}

pub fn delete_utxo(tx_id: &[u8; 32], out_idx: u32) {
    ROCKS_DB
        .delete_cf(utxo_cf(), to_utxo_db_key(tx_id, out_idx))
        .expect("[delete] ERROR: Failed to delete from DB")
}

/*** Block DB handlers ***/

pub fn block_cf() -> &'static ColumnFamily {
    ROCKS_DB
        .cf_handle(BLOCK_CF)
        .expect("Column family not found")
}

pub fn get_block(block_hash: &[u8; 32]) -> Result<Option<Block>, Box<dyn Error>> {
    let block_data = ROCKS_DB
        .get_cf(block_cf(), block_hash)
        .map_err(|e| format!("[db::get_block] ERROR: Failed to read from DB {:?}", e))?;

    match block_data {
        Some(data) => {
            let block: Block = bincode::deserialize(&data).map_err(|e| {
                format!("[db::get_block] ERROR: Failed to deserialize block {:?}", e)
            })?;
            Ok(Some(block))
        }
        None => Ok(None),
    }
}

pub fn put_block(block_hash: &[u8; 32], block_data: &Block) {
    let serialized =
        bincode::serialize(&block_data).expect("[db::put_block] ERROR: Serialization failed");
    ROCKS_DB
        .put_cf(block_cf(), block_hash, serialized)
        .expect("[db::put_block] ERROR: Failed to write to DB");
}

/*** Last Hash DB handlers ***/

pub fn blockchain_exists() -> bool {
    ROCKS_DB
        .get(LAST_HASH_KEY.as_bytes())
        .unwrap_or(None)
        .is_some()
}

pub fn get_last_hash() -> Result<[u8; 32], Box<dyn Error>> {
    let last_hash: [u8; 32] = ROCKS_DB
        .get(LAST_HASH_KEY.as_bytes())?
        .ok_or_else(|| "[db::get_last_hash] ERROR: No last hash found in the db")?
        .try_into()
        .map_err(|e| {
            format!(
                "[db::get_last_hash] ERROR: Failed to parse last hash: {:?}",
                e
            )
        })?;

    Ok(last_hash)
}

pub fn put_last_hash(last_hash: &[u8; 32]) {
    ROCKS_DB
        .put(LAST_HASH_KEY, last_hash)
        .expect("[db::put_last_hash] ERROR: Failed to write to DB");
}

/*** Mempool DB handlers ***/

pub fn get_mempool() -> Mempool {
    let mempool_data = ROCKS_DB.get(MEMPOOL_KEY.as_bytes()).unwrap();
    mempool_data
        .and_then(|data| bincode::deserialize(&data).ok())
        .unwrap_or_else(Vec::new)
}

pub fn put_mempool(tx: &Tx) {
    let mut mempool = get_mempool();
    mempool.push(tx.clone());
    let serialized =
        bincode::serialize(&mempool).expect("[db::put_mempool] ERROR: Failed to serialize tx");
    ROCKS_DB
        .put(MEMPOOL_KEY, serialized)
        .expect("[db::put_mempool] ERROR: Failed to write to DB");
}

/// Delete all mempool entries by deleting the mempool key
pub fn reset_mempool() {
    // Delete the mempool key, effectively resetting the entire mempool. No error on failure
    let _ = ROCKS_DB.delete(MEMPOOL_KEY);
}

/*** Peer handlers ***/

pub fn get_peers() -> PeerCollection {
    let peers_data = ROCKS_DB.get(PEERS_KEY.as_bytes()).unwrap();
    peers_data
        .and_then(|data| bincode::deserialize::<PeerCollection>(&data).ok())
        .unwrap_or_else(PeerCollection::new)
}

pub fn put_peer(peer_id: PeerId, addr: libp2p::Multiaddr) {
    let mut peers = get_peers();

    if !peers.get(&peer_id).unwrap_or(&vec![]).contains(&addr) {
        peers
            .entry(peer_id)
            // If the entry doesn't exist, create a new Vec<Multiaddr>
            .or_insert_with(Vec::new)
            .push(addr); // Add the new address to the vector
    }

    ROCKS_DB
        .put(
            PEERS_KEY,
            bincode::serialize(&peers).expect("[db::put_peer] ERROR: Failed to serialize peers"),
        )
        .expect("[db::put_peer] ERROR: Failed to write to DB");
}
