use std::{collections::HashMap, error::Error};

use rocksdb::IteratorMode;

use crate::{
    blockchain::block::Block,
    cli::db::{self, utxo_cf, ROCKS_DB},
};

use super::{mempool::is_output_spent_in_mempool, tx::TxOutput};

pub type UTXOSet = HashMap<([u8; 32], u32), TxOutput>;

/// Searches through all db entries with the UTXO prefix for utxos with outputs matching the given pub key hash
pub fn find_utxos(pub_key_hash: &[u8; 20]) -> Vec<TxOutput> {
    // Need to add mempool checks******************************************
    let mut utxos: Vec<TxOutput> = Vec::new();
    let iter = ROCKS_DB.iterator_cf(utxo_cf(), IteratorMode::Start);

    for res in iter {
        match res {
            Err(_) => {
                panic!("[utxo::find_utxos] ERROR: Failed to iterate through db")
            }
            Ok((_, val)) => {
                let tx_out: TxOutput = bincode::deserialize(&val).unwrap();
                if tx_out.is_locked_with_key(pub_key_hash) {
                    utxos.push(tx_out);
                }
            }
        }
    }
    utxos
}

/// Creates a hashmap of transaction ids to spendable utxo indexes by searching the db for utxos with spendable
/// outputs that add to the target amount
pub fn find_spendable_utxos(pub_key_hash: [u8; 20], amount: u32) -> (u32, UTXOSet) {
    let mut utxo_map: UTXOSet = HashMap::new();
    let mut accumulated: u32 = 0;
    let iter = ROCKS_DB.iterator_cf(utxo_cf(), IteratorMode::Start);

    for res in iter {
        match res {
            Err(_) => {
                panic!("[utxo::find_utxos] ERROR: Failed to iterate through db")
            }
            Ok((key, val)) => {
                let tx_out: TxOutput = bincode::deserialize(&val).unwrap();
                let (tx_id, out_idx) = db::from_utxo_db_key(&key);
                // If we get a match and we have more room to accumulate, add the
                // index of the utxo to the map, using the tx id as the key
                if tx_out.is_locked_with_key(&pub_key_hash)
                    && accumulated < amount
                    && !is_output_spent_in_mempool(tx_id, out_idx)
                {
                    accumulated += tx_out.value;
                    utxo_map.insert((tx_id, out_idx), tx_out);
                    // Stop iterating once we have enough funds
                    if accumulated >= amount {
                        break;
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
fn get_utxos_from_chain() -> Result<UTXOSet, Box<dyn Error>> {
    let mut utxo_map: UTXOSet = HashMap::new();
    // Map of spent tx out indexes to their respective tx ids
    let mut spent_txo_idx_map: HashMap<[u8; 32], Vec<u32>> = HashMap::new();

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
                if let Some(spent_outs) = spent_txo_idx_map.get(&tx.id) {
                    if spent_outs.contains(&out_idx) {
                        continue 'outputs;
                    }
                }
                utxo_map.insert((tx.id, out_idx), tx_out.clone());
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
    for ((tx_id, out_idx), tx_out) in utxos {
        db::put_utxo(&tx_id, out_idx, &tx_out)?;
    }

    Ok(())
}

/// Update utxos with a new block
pub fn update_utxos(block: &Block) -> Result<(), Box<dyn Error>> {
    for tx in &block.txs {
        if !tx.is_coinbase() {
            // Loop through tx in's in new block to check if they're spent
            for tx_in in &tx.inputs {
                // Fetch existing utxos and update by removing any outputs now spent by
                // a given tx in
                db::delete_utxo(&tx_in.prev_tx_id, tx_in.out);
            }

            // Add the new outputs as utxos for future txs
            for (out_idx, tx_out) in tx.outputs.iter().enumerate() {
                let out_idx = out_idx
                    .try_into()
                    .expect("[utxo::update_utxos] ERROR: Index too large for u32");
                db::put_utxo(&tx.id, out_idx, tx_out)?;
            }
        }
    }
    Ok(())
}
