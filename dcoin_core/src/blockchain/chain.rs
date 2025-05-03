use core_lib::address::Address;
use serde::{Deserialize, Serialize};
use std::error::Error;

use super::{blocks::block::Block, transaction::tx::Tx};
use crate::{
    blockchain::{
        blocks::orphan::{check_for_valid_orphan_blocks, check_orphans_for_longest_chain},
        transaction::{mempool::update_mempool, utxo::update_utxos},
    },
    cli::db::{
        self, blockchain_exists, delete_all_blocks, delete_all_orphan_blocks, delete_all_utxos,
        delete_last_hash, delete_mempool, get_block, get_last_hash, put_block, put_last_hash,
        put_orphan_block, remove_from_orphan_blocks,
    },
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

pub fn get_tx_from_chain(tx_id: [u8; 32]) -> Result<Tx, Box<dyn Error>> {
    let last_hash = db::get_last_hash()?;
    let mut current_block = db::get_block(&last_hash)?.ok_or_else(|| {
        format!(
            "[chain::find_tx_in_chain] ERROR: Could not find block from last hash {:?}",
            last_hash
        )
    })?;

    loop {
        for tx in &current_block.txs {
            if tx.id == tx_id {
                return Ok(tx.clone());
            }
        }
        // Break if we have reached the first block
        if current_block.is_genesis() {
            break;
        }
        // Otherwise, get the next block
        current_block = db::get_block(&current_block.prev_hash)?.ok_or_else(|| {
            format!(
                "[chain::find_tx_in_chain] ERROR: Could not find next block {:?}",
                current_block.prev_hash
            )
        })?;
    }

    Err("[chain::find_tx_in_chain] ERROR: Could not find tx in chain".into())
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

    put_block(&block);
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

    // Check if new block allows other orphaned blocks to be committed
    check_for_valid_orphan_blocks()?;
    check_orphans_for_longest_chain()?;

    println!("Block was successfully committed to the blockchain");
    Ok(())
}
