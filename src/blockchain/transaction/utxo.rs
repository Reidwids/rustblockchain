use std::collections::HashMap;

use rocksdb::{Direction, IteratorMode};

use crate::{blockchain::block::Block, cli::db};

use super::tx::TxOutput;

const UTXO_PREFIX: &[u8] = b"utxo-";

/// Searches through all db entries with the UTXO prefix for utxos with outputs matching the given pub key hash
pub fn find_utxos(pub_key_hash: &[u8; 20]) -> Vec<TxOutput> {
    // Need to add mempool checks******************************************
    let mut utxos: Vec<TxOutput> = Vec::new();
    let db = db::open_db();
    let iter = db.iterator(IteratorMode::From(UTXO_PREFIX, Direction::Forward));

    for res in iter {
        match res {
            Err(_) => {
                panic!("[utxo::find_utxos] ERROR: Failed to iterate through db")
            }
            Ok((_, val)) => {
                let outputs: Vec<TxOutput> = bincode::deserialize(&val).unwrap();
                for output in outputs {
                    if output.is_locked_with_key(pub_key_hash) {
                        utxos.push(output);
                    }
                }
            }
        }
    }

    utxos
}

/// Creates a hashmap of transaction ids to spendable utxo indexes by searching the db for utxos with spendable
/// outputs that add to the target amount
pub fn find_spendable_utxos(
    pub_key_hash: [u8; 20],
    amount: u32,
) -> (u32, HashMap<[u8; 32], Vec<usize>>) {
    let mut utxo_map: HashMap<[u8; 32], Vec<usize>> = HashMap::new();
    let mut accumulated: u32 = 0;
    let db = db::open_db();
    let iter = db.iterator(IteratorMode::From(UTXO_PREFIX, Direction::Forward));

    for res in iter {
        match res {
            Err(_) => {
                panic!("[utxo::find_utxos] ERROR: Failed to iterate through db")
            }
            Ok((key, val)) => {
                let outputs: Vec<TxOutput> = bincode::deserialize(&val).unwrap();
                let tx_id: [u8; 32] = key
                    .strip_prefix(UTXO_PREFIX)
                    .expect("[utxo::find_spendable_utxos] ERROR: Failed to trim prefix")
                    .try_into()
                    .expect("[utxo::find_spendable_utxos] ERROR: Failed to parse transaction ID");

                for (out_idx, output) in outputs.iter().enumerate() {
                    // If we get a match and we have more room to accumulate, add the
                    // index of the utxo to the map, using the tx id as the key
                    if output.is_locked_with_key(&pub_key_hash) && accumulated < amount {
                        accumulated += output.value;
                        utxo_map.entry(tx_id).or_insert_with(Vec::new).push(out_idx);
                        // Stop iterating once we have enough funds
                        if accumulated >= amount {
                            break;
                        }
                    }
                }
            }
        }
        // Stop iterating once we have enough funds
        if accumulated >= amount {
            break;
        }
    }
    (accumulated, utxo_map)
}

/// Builds a hashmap containing the UTXO set from the chain found in the database.
fn get_utxos_from_chain() -> HashMap<[u8; 32], Vec<TxOutput>> {
    // Map of tx ids to tx outputs
    let mut utxo_map: HashMap<[u8; 32], Vec<TxOutput>> = HashMap::new();
    // Map of spent tx out indexes to their respective tx ids
    let mut spent_txo_idx_map: HashMap<[u8; 32], Vec<usize>> = HashMap::new();

    // Get most recent block
    let last_hash = db::get_last_hash();
    let mut current_block = db::get_block(&last_hash);

    loop {
        for tx in &current_block.txs {
            // Loop through all tx outputs in the current block txs
            'outputs: for (out_idx, out) in tx.outputs.iter().enumerate() {
                // If any entries in the spent txo map for this tx contain the out current out idx,
                // the out must be spent and therefore shouldn't be added to the utxo map.
                if let Some(spent_outs) = spent_txo_idx_map.get(&tx.id) {
                    if spent_outs.contains(&out_idx) {
                        continue 'outputs;
                    }
                }
                utxo_map
                    .entry(tx.id)
                    .or_insert_with(Vec::new)
                    .push(out.clone());
            }

            // Tx inputs spend outputs from previous txs. By adding the outs to the
            // spent txo map, we are able to check above in the next iteration if we
            // should skip adding an out to the utxo set.
            if !tx.is_coinbase() {
                for tx_in in &tx.inputs {
                    spent_txo_idx_map
                        .entry(tx_in.prev_tx_id)
                        .or_insert_with(Vec::new)
                        .push(tx_in.out);
                }
            }
        }
        // Break if we have reached the first block
        if current_block.is_genesis() {
            break;
        }
        // Otherwise, get the next block
        current_block = db::get_block(&current_block.prev_hash);
    }
    utxo_map
}

/// Delete all utxos stored in the db
fn delete_all_utxos() {
    let db = db::open_db();
    let iter = db.iterator(IteratorMode::From(UTXO_PREFIX, Direction::Forward));

    for res in iter {
        match res {
            Err(_) => {
                panic!("[utxo::delete_all_utxos] ERROR: Failed to iterate through db")
            }
            Ok((key, _)) => db
                .delete(key)
                .expect("[utxo::delete_all_utxos] ERROR: Failed to delete key"),
        }
    }
}

/// Reindexes utxos in db. Deletes all existing and uses the chain from the db
/// to rebuild all utxos in the db.
pub fn reindex_utxos() {
    delete_all_utxos();
    let utxos = get_utxos_from_chain();

    // Loop through all retrieved utxos and add them to the db with utxo prefix
    for (tx_id, tx_outs) in utxos.iter() {
        let serialized = bincode::serialize(&tx_outs)
            .expect("[utxo::reindex_utxos] ERROR: Serialization failed");
        db::put_db(&get_utxo_key(&tx_id), &serialized);
    }
}

/// Helper fn to append the utxo prefix to a given tx id
pub fn get_utxo_key(tx_id: &[u8; 32]) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend_from_slice(UTXO_PREFIX);
    key.extend_from_slice(tx_id);
    key
}

/// Update utxos with a new block
pub fn update_utxos(block: &Block) {
    for tx in &block.txs {
        if !tx.is_coinbase() {
            // Loop through tx in's in new block to check if they're spent
            for tx_in in &tx.inputs {
                // Fetch existing utxos and update by removing any outputs now spent by
                // a given tx in
                let utxo_key = get_utxo_key(&tx_in.prev_tx_id);
                let utxo_data = db::get_db(&utxo_key).unwrap();
                let utxos: Vec<TxOutput> = bincode::deserialize(&utxo_data).unwrap();
                let mut new_outs: Vec<TxOutput> = Vec::new();

                for (out_idx, out) in utxos.iter().enumerate() {
                    if out_idx != tx_in.out {
                        new_outs.push(out.clone());
                    }
                }

                // If no outputs are left, delete the key,
                // Otherwise persist the updated utxo set
                if new_outs.len() == 0 {
                    db::delete(&utxo_key);
                } else {
                    let serialized = bincode::serialize(&new_outs)
                        .expect("[utxo::update_utxos] ERROR: Serialization failed");
                    db::put_db(&utxo_key, &serialized);
                }
            }
        }
        // Add the new outputs as utxos for future txs
        let new_serialized_outs = bincode::serialize(&tx.outputs)
            .expect("[utxo::update_utxos] ERROR: Serialization failed");
        db::put_db(&get_utxo_key(&tx.id), &new_serialized_outs);
    }
}
