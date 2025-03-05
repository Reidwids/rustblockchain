use crate::cli::db::get_mempool;

use super::tx::Tx;

// Could be optimized into a hashmap and persisted to db
pub type Mempool = Vec<Tx>;

pub fn is_output_spent_in_mempool(tx_id: [u8; 32], out_idx: u32) -> bool {
    let mempool = get_mempool();
    for tx in mempool {
        for tx_in in tx.inputs {
            if tx_in.prev_tx_id == tx_id && tx_in.out == out_idx {
                return true;
            }
        }
    }
    return false;
}
