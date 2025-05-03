use crate::{
    blockchain::blocks::block::Block,
    cli::db::{self, get_mempool},
};

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

/// Update mempool with a new block
pub fn update_mempool(block: &Block) -> Result<(), Box<dyn Error>> {
    let mempool = get_mempool();

    // Use mempool id/out hashmap for faster lookup
    let mut input_map: HashMap<([u8; 32], u32), [u8; 32]> = HashMap::new();
    for (mem_tx_id, mem_tx) in &mempool {
        for input in &mem_tx.inputs {
            input_map.insert((input.prev_tx_id, input.out), *mem_tx_id);
        }
    }

    // Track all mempool txs that got spent in the block
    let mut tx_ids_to_remove = Vec::new();
    for block_tx in &block.txs {
        if !block_tx.is_coinbase() {
            for input in &block_tx.inputs {
                if let Some(mem_tx_id) = input_map.get(&(input.prev_tx_id, input.out)) {
                    tx_ids_to_remove.push(*mem_tx_id);
                }
            }
        }
    }

    db::remove_txs_from_mempool(tx_ids_to_remove);
    Ok(())
}
