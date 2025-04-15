use rocksdb::IteratorMode;
use serde::{Deserialize, Serialize};
use std::error::Error;

use super::block::Block;
use crate::{
    cli::db::{
        self, blockchain_exists, delete_all_blocks, delete_all_orphan_blocks, delete_all_utxos,
        delete_last_hash, delete_mempool, get_block, get_last_hash, ROCKS_DB,
    },
    networking::node::NODE_KEY,
    wallets::address::Address,
};
use hex;

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
    delete_all_blocks();
    delete_all_utxos();
    delete_all_orphan_blocks();
    delete_mempool();
    delete_last_hash();
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

pub fn get_chain_height() -> Result<u32, Box<dyn Error>> {
    let lb = get_last_block()?;
    Ok(lb.height)
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
            hash: hex::encode(&current_block.hash),
            prev_hash: hex::encode(&current_block.prev_hash),
            timestamp: current_block.timestamp,
            nonce: current_block.nonce,
            txs: if include_txs {
                Some(
                    current_block
                        .txs
                        .iter()
                        .map(|tx| TxJson {
                            id: hex::encode(&tx.id),
                            inputs: tx
                                .inputs
                                .iter()
                                .map(|input| TxInputJson {
                                    prev_tx_id: hex::encode(&input.prev_tx_id),
                                    out: input.out,
                                })
                                .collect(),
                            outputs: tx
                                .outputs
                                .iter()
                                .map(|output| TxOutputJson {
                                    value: output.value,
                                    pub_key_hash: hex::encode(&output.pub_key_hash),
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
                    "[chain::get_blockchain_json] ERROR: Failed to fetch previous block {}",
                    e
                )
            })?
            .ok_or_else(|| "[chain::get_blockchain_json] ERROR: Last block not found")?;
    }

    Ok(blocks)
}
