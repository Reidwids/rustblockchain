use std::{collections::HashMap, error::Error, sync::Arc};

use once_cell::sync::Lazy;
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, Options, DB};

use crate::blockchain::{
    block::{Block, OrphanBlocks},
    chain::get_chain_height,
    transaction::{
        mempool::{update_mempool, Mempool},
        tx::{Tx, TxOutput},
        utxo::{update_utxos, TxOutMap},
    },
};

/// LAST_HASH_KEY holds the key to discover the last block hash
pub const LAST_HASH_KEY: &str = "lh";
/// MEMPOOL_KEY holds the key to retrieve the mempool
const MEMPOOL_KEY: &str = "mempool";
/// Orphan key is used to retrieve the orphaned block set
const ORPHAN_KEY: &str = "orphan";

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

/// Returns an option representing a utxo. the utxo will be deserialized if found.
pub fn get_utxo(tx_id: &[u8; 32], out_idx: u32) -> Result<Option<TxOutput>, Box<dyn Error>> {
    let txo_data = ROCKS_DB
        .get_cf(utxo_cf(), tx_id)
        .map_err(|e| format!("[db::get_utxo] ERROR: Failed to read from DB {:?}", e))?;

    match txo_data {
        None => Ok(None),
        Some(data) => {
            let txo_map: TxOutMap = bincode::deserialize(&data)?;
            Ok(txo_map.get(&out_idx).cloned())
        }
    }
}

/// Returns a bool representing if a tx exists in the utxo set
pub fn utxo_set_contains_tx(tx_id: [u8; 32]) -> Result<bool, Box<dyn Error>> {
    let txo_data = ROCKS_DB
        .get_cf(utxo_cf(), tx_id)
        .map_err(|e| format!("[db::get_utxo] ERROR: Failed to read from DB {:?}", e))?;

    match txo_data {
        None => Ok(false),
        Some(_) => Ok(true),
    }
}

pub fn put_utxo(tx_id: &[u8; 32], out_idx: u32, tx_out: &TxOutput) -> Result<(), Box<dyn Error>> {
    // Try to get the existing TxOutMap for this transaction ID
    let mut txo_map = match ROCKS_DB.get_cf(utxo_cf(), tx_id)? {
        Some(data) => bincode::deserialize::<TxOutMap>(&data)?,
        None => HashMap::new(), // If no existing map, create a new one
    };

    txo_map.insert(out_idx, tx_out.clone());

    let serialized = bincode::serialize(&txo_map)
        .map_err(|e| format!("[db::put_utxo] ERROR: Serialization failed {:?}", e))?;

    ROCKS_DB
        .put_cf(utxo_cf(), tx_id, serialized)
        .map_err(|e| format!("[db::put_utxo] ERROR: Failed to write to DB {:?}", e))?;

    Ok(())
}

pub fn delete_utxo(tx_id: &[u8; 32], out_idx: u32) -> Result<(), Box<dyn Error>> {
    // Try to get the existing TxOutMap for this transaction ID
    let mut txo_map = match ROCKS_DB.get_cf(utxo_cf(), tx_id)? {
        Some(data) => bincode::deserialize::<TxOutMap>(&data)?,
        None => return Ok(()), // No entry found, nothing to delete
    };

    // Remove the specific UTXO if it exists
    if txo_map.remove(&out_idx).is_some() {
        if txo_map.is_empty() {
            // If no more outputs remain, remove the entire tx_id entry
            ROCKS_DB.delete_cf(utxo_cf(), tx_id).map_err(|e| {
                format!("[db::delete_utxo] ERROR: Failed to delete from DB {:?}", e)
            })?;
        } else {
            // Otherwise, update DB with the modified map
            let serialized = bincode::serialize(&txo_map)
                .map_err(|e| format!("[db::delete_utxo] ERROR: Serialization failed {:?}", e))?;

            ROCKS_DB
                .put_cf(utxo_cf(), tx_id, serialized)
                .map_err(|e| format!("[db::delete_utxo] ERROR: Failed to update DB {:?}", e))?;
        }
    }

    Ok(())
}

pub fn delete_all_utxos() {
    let _ = ROCKS_DB.delete_range_cf(utxo_cf(), b"", b"");
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

pub fn delete_all_blocks() {
    let _ = ROCKS_DB.delete_range_cf(block_cf(), b"", b"");
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

pub fn delete_last_hash() {
    let _ = ROCKS_DB.delete(LAST_HASH_KEY);
}

/*** Mempool DB handlers ***/
pub fn get_mempool() -> Mempool {
    let mempool_data = ROCKS_DB.get(MEMPOOL_KEY.as_bytes()).unwrap();
    mempool_data
        .and_then(|data| bincode::deserialize(&data).ok()) // Try to deserialize
        .unwrap_or_else(HashMap::new)
}

pub fn put_mempool(tx: &Tx) {
    let mut mempool = get_mempool();

    // Insert each output of the transaction into the mempool UTXOSet
    mempool.insert(tx.id, tx.clone());

    let serialized =
        bincode::serialize(&mempool).expect("[db::put_mempool] ERROR: Failed to serialize mempool");

    ROCKS_DB
        .put(MEMPOOL_KEY, serialized)
        .expect("[db::put_mempool] ERROR: Failed to write to DB");
}

pub fn remove_txs_from_mempool(tx_ids: Vec<[u8; 32]>) {
    let mut mempool = get_mempool();

    for tx_id in tx_ids {
        mempool.remove(&tx_id);
    }

    let serialized =
        bincode::serialize(&mempool).expect("[db::put_mempool] ERROR: Failed to serialize mempool");

    ROCKS_DB
        .put(MEMPOOL_KEY, serialized)
        .expect("[db::remove_txs_from_mempool] ERROR: Failed to write to DB");
}

/// Delete all mempool entries by deleting the mempool key
pub fn delete_mempool() {
    // Delete the mempool key, effectively resetting the entire mempool. No error on failure
    let _ = ROCKS_DB.delete(MEMPOOL_KEY);
}

/*** Orphan DB handlers ***/
pub fn get_orphaned_blocks() -> OrphanBlocks {
    let block_data = ROCKS_DB.get(ORPHAN_KEY.as_bytes()).unwrap();
    block_data
        .and_then(|data| bincode::deserialize(&data).ok()) // Try to deserialize
        .unwrap_or_else(HashMap::new)
}

pub fn put_orphan_block(block: &Block) {
    // TODO: Put cap on map size, use LRU evictions
    let mut block_map = get_orphaned_blocks();

    // Insert each output of the transaction into the mempool UTXOSet
    block_map.insert(block.hash, block.clone());

    let serialized = bincode::serialize(&block_map)
        .expect("[db::put_orphan_block] ERROR: Failed to serialize orphan blocks");

    ROCKS_DB
        .put(ORPHAN_KEY, serialized)
        .expect("[db::put_orphan_block] ERROR: Failed to write to DB");
}

pub fn remove_from_orphan_blocks(block_hashes: Vec<[u8; 32]>) {
    let mut block_map = get_orphaned_blocks();

    for hash in block_hashes {
        block_map.remove(&hash);
    }

    let serialized = bincode::serialize(&block_map)
        .expect("[db::remove_from_orphan_blocks] ERROR: Failed to serialize mempool");

    ROCKS_DB
        .put(ORPHAN_KEY, serialized)
        .expect("[db::remove_blocks_from_orphan_blocks] ERROR: Failed to write to DB");
}

pub fn check_for_valid_orphan_blocks() -> Result<(), Box<dyn Error>> {
    let block_map = get_orphaned_blocks();
    let last_hash = get_last_hash()?;
    for (_, block) in block_map.iter() {
        if block.prev_hash == last_hash {
            println!("Valid orphan block found! Attempting to commit...");
            commit_block(&block.clone())?;
        }
    }

    Ok(())
}

pub fn delete_all_orphan_blocks() {
    // Delete the mempool key, effectively resetting the entire mempool. No error on failure
    let _ = ROCKS_DB.delete(ORPHAN_KEY);
}

pub fn commit_block(block: &Block) -> Result<(), Box<dyn Error>> {
    match block.verify() {
        Ok(v) => {
            if !v {
                println!("Verification failed for given block!");
                println!("Checking if block is a valid orphan block...");
                match block.verify_orphan() {
                    Ok(v) => {
                        if !v {
                            println!("Block is not a valid orphan block and will be discarded");
                            return Ok(());
                        }
                        put_orphan_block(&block);
                        println!("Block is a valid orphan and has been persisted for future consideration");
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(
                            format!("[network::handle_inventory_res] ERROR: {:?}", e).into()
                        );
                    }
                }
            }
        }
        Err(e) => {
            return Err(format!(
                "[network::handle_inventory_res] ERROR: failed to verify block: {:?}",
                e
            )
            .into());
        }
    }

    // TODO: Should send a signal to cancel mining
    if let Err(e) = update_utxos(&block) {
        return Err(format!(
            "[miner::handle_mine] ERROR: Failed to update utxos: {:?}",
            e
        )
        .into());
    };

    if let Err(e) = update_mempool(&block) {
        return Err(format!(
            "[miner::handle_mine] ERROR: Failed to update mempool: {:?}",
            e
        )
        .into());
    };

    put_block(&block.hash, &block);
    remove_from_orphan_blocks(vec![block.hash]);

    let current_height = if let Ok(h) = get_chain_height() {
        h
    } else {
        // Chain is empty, therefore set curr height to 0
        0
    };
    if block.height >= current_height {
        put_last_hash(&block.hash);
    }

    // Check if new block allows orphaned blocks to be committed
    check_for_valid_orphan_blocks()?;

    println!("Block was successfully committed to the blockchain");
    Ok(())
}
