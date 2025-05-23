use std::{collections::HashMap, error::Error};

use core_lib::tx::TxOutput;
use rocksdb::IteratorMode;

use crate::{
    blockchain::blocks::block::Block,
    cli::db::{self, utxo_cf, ROCKS_DB},
};

use super::mempool::mempool_contains_txo;

pub type TxOutMap = HashMap<u32, TxOutput>;
pub type UTXOSet = HashMap<[u8; 32], TxOutMap>;

/// Searches through all db entries with the UTXO prefix for utxos with outputs matching the given pub key hash.
///
/// Note that returned utxos *may be in a pending tx within the mempool
pub fn find_utxos_for_addr(pub_key_hash: &[u8; 20]) -> Vec<TxOutput> {
    let mut utxos: Vec<TxOutput> = Vec::new();
    let iter = ROCKS_DB.iterator_cf(utxo_cf(), IteratorMode::Start);

    for res in iter {
        match res {
            Err(_) => {
                panic!("[utxo::find_utxos_for_addr] ERROR: Failed to iterate through db")
            }
            Ok((_, val)) => {
                let tx_out_map: HashMap<u32, TxOutput> = match bincode::deserialize(&val) {
                    Ok(map) => map,
                    Err(e) => {
                        println!("Failed to deserialize TxOutMap: {:?}", e);
                        continue;
                    }
                };

                for (_, tx_out) in tx_out_map {
                    if tx_out.is_locked_with_key(pub_key_hash) {
                        utxos.push(tx_out);
                    }
                }
            }
        }
    }
    utxos
}

/// Creates a hashmap of transaction ids to spendable utxo indexes by searching the db for utxos with spendable
/// outputs that add to the target amount.
///
/// Spendable utxos must not be present in the mempool.
pub fn find_spendable_utxos(
    pub_key_hash: &[u8; 20],
    amount: u32,
) -> Result<UTXOSet, Box<dyn Error>> {
    let mut utxo_map: UTXOSet = HashMap::new();
    let mut accumulated: u32 = 0;
    let iter = ROCKS_DB.iterator_cf(utxo_cf(), IteratorMode::Start);

    for res in iter {
        match res {
            Err(_) => {
                return Err(
                    "[utxo::find_spendable_utxos] ERROR: Failed to iterate through db".into(),
                );
            }
            Ok((key, val)) => {
                let tx_id: [u8; 32] = key.into_vec().try_into().map_err(|e| {
                    format!(
                        "[utxo::find_spendable_utxos] ERROR: Failed to unwrap key {:?}",
                        e
                    )
                })?;
                let txo_map: TxOutMap = bincode::deserialize(&val)?;
                let mut new_txo_map: TxOutMap = HashMap::new();
                for (out_idx, tx_out) in txo_map.iter() {
                    // If we get a match and we have more room to accumulate, add the
                    // index of the utxo to the map, using the tx id as the key
                    if tx_out.is_locked_with_key(&pub_key_hash)
                        && accumulated < amount
                        && !mempool_contains_txo(tx_id, *out_idx)
                    {
                        accumulated += tx_out.value;

                        new_txo_map.insert(*out_idx, tx_out.clone());
                        // Stop iterating once we have enough funds
                        if accumulated >= amount {
                            break;
                        }
                    }
                }
                utxo_map.insert(tx_id, new_txo_map);
            }
        }
        // Stop iterating once we have enough funds
        if accumulated >= amount {
            break;
        }
    }
    // Not enough funds if total spendable is less than new tx value
    if accumulated < amount {
        return Err(
            format!("[tx::new] ERROR: provided address does not have enough funds!!!",).into(),
        );
    }

    Ok(utxo_map)
}

/// Builds a hashmap containing the UTXO set from the chain found in the database.
fn get_utxos_from_chain() -> Result<UTXOSet, Box<dyn Error>> {
    let mut utxo_map: UTXOSet = HashMap::new();
    // Map of spent tx out indexes to their respective tx ids
    let mut spent_txo_map: HashMap<[u8; 32], Vec<u32>> = HashMap::new();

    // Get most recent block
    let last_hash = db::get_last_hash()?;
    let mut current_block = db::get_block(&last_hash)?.ok_or_else(|| {
        format!(
            "[utxo::get_utxos_from_chain] ERROR: Could not find block from last hash {:?}",
            last_hash
        )
    })?;

    loop {
        for tx in &current_block.txs {
            // Loop through all tx outputs in the current block txs
            'outputs: for (out_idx, tx_out) in tx.outputs.iter().enumerate() {
                // If any entries in the spent txo map for this tx contain the out current out idx,
                // the out must be spent and therefore shouldn't be added to the utxo map.
                let out_idx = out_idx
                    .try_into()
                    .expect("[utxo::get_utxos_from_chain] ERROR: Index too large for u32");
                if let Some(spent_outs) = spent_txo_map.get(&tx.id) {
                    if spent_outs.contains(&out_idx) {
                        continue 'outputs;
                    }
                }
                utxo_map
                    .entry(tx.id)
                    .or_insert_with(HashMap::new) // If `tx_id` isn't found, insert an empty HashMap
                    .insert(out_idx, tx_out.clone());
            }

            // Tx inputs spend outputs from previous txs. By adding the outs to the
            // spent txo map, we are able to check above in the next iteration if we
            // should skip adding an out to the utxo set.
            if !tx.is_coinbase() {
                for tx_in in &tx.inputs {
                    spent_txo_map
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
        current_block = db::get_block(&current_block.prev_hash)?.ok_or_else(|| {
            format!(
                "[utxo::get_utxos_from_chain] ERROR: Could not find next block {:?}",
                current_block.prev_hash
            )
        })?;
    }
    Ok(utxo_map)
}

/// Delete all utxos stored in the db
fn delete_all_utxos() -> Result<(), Box<dyn Error>> {
    let iter = ROCKS_DB.iterator_cf(utxo_cf(), IteratorMode::Start);

    for res in iter {
        let (key, _) =
            res.map_err(|_| "[utxo::delete_all_utxos] ERROR: Failed to iterate through db")?;

        if let Err(e) = ROCKS_DB.delete(key) {
            return Err(format!(
                "[utxo::delete_all_utxos] ERROR: Failed to delete key: {}",
                e
            )
            .into());
        }
    }

    Ok(())
}

/// Reindexes utxos in db. Deletes all existing and uses the chain from the db
/// to rebuild all utxos in the db.
pub fn reindex_utxos() -> Result<(), Box<dyn Error>> {
    delete_all_utxos()?;
    let utxos = get_utxos_from_chain()?;

    // Loop through all retrieved utxos and add them to the db with utxo prefix
    for (tx_id, txo_map) in utxos {
        for (out_idx, txo) in txo_map {
            db::put_utxo(&tx_id, out_idx, &txo)?;
        }
    }

    Ok(())
}

/// Update utxos with a new block
pub fn update_utxos(block: &Block) -> Result<(), Box<dyn Error>> {
    for tx in &block.txs {
        if !tx.is_coinbase() {
            for tx_in in &tx.inputs {
                // Remove any outputs now spent by a given tx input
                db::delete_utxo(&tx_in.prev_tx_id, tx_in.out)?;
            }
        }

        // Add the new outputs as utxos for future txs
        for (out_idx, tx_out) in tx.outputs.iter().enumerate() {
            let out_idx = out_idx
                .try_into()
                .expect("[utxo::update_utxos] ERROR: Index too large for u32");
            db::put_utxo(&tx.id, out_idx, tx_out)?;
        }
    }
    Ok(())
}

// /// Fetch all utxos from the db. Does not reindex, simply builds a map from the existing utxos in the db.
// pub fn get_all_utxos() -> Result<UTXOSet, Box<dyn Error>> {
//     let mut utxo_map: UTXOSet = HashMap::new();
//     let iter = ROCKS_DB.iterator_cf(utxo_cf(), IteratorMode::Start);
//     for res in iter {
//         match res {
//             Err(_) => {
//                 return Err("[db::get_all_utxos] ERROR: Failed to iterate through db".into());
//             }
//             Ok((key, val)) => {
//                 let tx_id: [u8; 32] = key.into_vec().try_into().map_err(|e| {
//                     format!(
//                         "[utxo::find_spendable_utxos] ERROR: Failed to unwrap key {:?}",
//                         e
//                     )
//                 })?;
//                 let txo_map: TxOutMap = bincode::deserialize(&val)?;
//                 utxo_map.insert(tx_id, txo_map);
//             }
//         }
//     }
//     Ok(utxo_map)
// }
