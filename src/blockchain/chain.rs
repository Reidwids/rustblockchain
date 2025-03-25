use rocksdb::IteratorMode;
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::{
    cli::db::{self, blockchain_exists, get_block, get_last_hash, ROCKS_DB},
    networking::node::NODE_KEY,
    wallets::address::{bytes_to_hex_string, Address},
};

use super::block::Block;

/// Initializes the blockchain, and fails if a blockchain already exists
pub fn create_blockchain(addr: &Address) -> Result<(), Box<dyn Error>> {
    if blockchain_exists() {
        panic!("[chain::create_blockchain] ERROR: Blockchain already exists");
    }

    let mut genesis_block = Block::genesis(addr)?;
    genesis_block.mine()?;
    Ok(())
}

/// Clears the existing chain. Retains the node id
pub fn clear_blockchain() {
    let mut batch = rocksdb::WriteBatch::default();

    for item in ROCKS_DB.iterator(IteratorMode::Start).flatten() {
        let (key, _) = item;
        if key.as_ref() != NODE_KEY.as_bytes() {
            batch.delete(key.as_ref()); // Convert Box<[u8]> to &[u8]
        }
    }

    ROCKS_DB
        .write(batch)
        .expect("[chain::clear_blockchain] ERROR: Failed to delete blockchain");
}

pub fn get_last_block() -> Result<Block, Box<dyn Error>> {
    let lh: [u8; 32] = get_last_hash()?;
    let block = db::get_block(&lh)
        .map_err(|e| {
            format!(
                "[block::get_last_block] ERROR: Could not get last block {:?}",
                e
            )
        })?
        .ok_or_else(|| "[block::get_last_block] ERROR: Last block not found")?;

    Ok(block)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BlockJson {
    height: u32,
    hash: String,
    prev_hash: String,
    timestamp: u64,
    nonce: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    txs: Option<Vec<TxJson>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TxJson {
    id: String,
    inputs: Vec<TxInputJson>,
    outputs: Vec<TxOutputJson>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TxInputJson {
    prev_tx_id: String,
    out: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TxOutputJson {
    value: u32,
    pub_key_hash: String,
}

pub fn get_blockchain_json(include_txs: bool) -> Result<Vec<BlockJson>, Box<dyn Error>> {
    let mut blocks = Vec::new();
    let mut current_block = get_last_block()?;

    loop {
        let block_json = BlockJson {
            height: current_block.height,
            hash: bytes_to_hex_string(&current_block.hash),
            prev_hash: bytes_to_hex_string(&current_block.prev_hash),
            timestamp: current_block.timestamp,
            nonce: current_block.nonce,
            txs: if include_txs {
                Some(
                    current_block
                        .txs
                        .iter()
                        .map(|tx| TxJson {
                            id: bytes_to_hex_string(&tx.id),
                            inputs: tx
                                .inputs
                                .iter()
                                .map(|input| TxInputJson {
                                    prev_tx_id: bytes_to_hex_string(&input.prev_tx_id),
                                    out: input.out,
                                })
                                .collect(),
                            outputs: tx
                                .outputs
                                .iter()
                                .map(|output| TxOutputJson {
                                    value: output.value,
                                    pub_key_hash: bytes_to_hex_string(&output.pub_key_hash),
                                })
                                .collect(),
                        })
                        .collect(),
                )
            } else {
                None
            },
        };

        blocks.push(block_json);

        if current_block.is_genesis() {
            break;
        }

        current_block = get_block(&current_block.prev_hash)
            .map_err(|e| {
                format!(
                    "[handlers::handle_print_blockchain] ERROR: Failed to fetch previous block {}",
                    e
                )
            })?
            .ok_or_else(|| "[block::get_last_block] ERROR: Last block not found")?;
    }

    Ok(blocks)
}
