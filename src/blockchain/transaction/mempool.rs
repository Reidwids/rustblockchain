use crate::cli::db::{self, get_mempool};

use super::tx::Tx;
use std::{collections::HashMap, error::Error};

pub type Mempool = HashMap<[u8; 32], Tx>;

pub fn is_output_spent_in_mempool(tx_id: [u8; 32], out_idx: u32) -> bool {
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

pub fn add_tx_to_mempool(tx: &Tx) -> Result<(), Box<dyn Error>> {
    for tx_input in &tx.inputs {
        if is_output_spent_in_mempool(tx_input.prev_tx_id, tx_input.out) {
            return Err(
                "[mempool::add_tx_to_mempool] ERROR: tx contains outputs spent in mempool".into(),
            );
        }
    }

    db::put_mempool(&tx);
    Ok(())
}
