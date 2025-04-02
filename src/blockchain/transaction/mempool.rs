use crate::cli::db::{self, get_mempool};

use super::tx::Tx;
use std::{collections::HashMap, error::Error};

pub type Mempool = HashMap<[u8; 32], Tx>;

/// Returns a bool representing if the output exists in any txs stored in the mempool
pub fn mempool_contains_txo(tx_id: [u8; 32], out_idx: u32) -> bool {
    let mempool = get_mempool();
    for (_, tx) in mempool {
        for tx_in in tx.inputs {
            if tx_in.prev_tx_id == tx_id && tx_in.out == out_idx {
                return true;
            }
        }
    }
    return false;
}

/// Returns the tx from the mempool if found
pub fn get_tx_from_mempool(tx_id: [u8; 32]) -> Option<Tx> {
    let mempool = get_mempool();
    mempool.get(&tx_id).cloned()
}

/// Check if the mempool contains a given tx
pub fn mempool_contains_tx(tx_id: [u8; 32]) -> bool {
    let mempool = get_mempool();
    match mempool.get(&tx_id) {
        Some(_) => true,
        None => false,
    }
}

pub fn add_tx_to_mempool(tx: &Tx) -> Result<(), Box<dyn Error>> {
    for tx_input in &tx.inputs {
        if mempool_contains_txo(tx_input.prev_tx_id, tx_input.out) {
            return Err(
                "[mempool::add_tx_to_mempool] ERROR: tx contains outputs spent in mempool".into(),
            );
        }
    }

    db::put_mempool(&tx);
    Ok(())
}
