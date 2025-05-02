use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

use crate::blockchain::chain::{commit_block, get_last_block, get_tx_from_chain};
use crate::blockchain::transaction::tx::TxOutput;
use crate::cli::db::{
    delete_block, delete_utxo, get_all_block_hashes, get_block, get_last_hash, get_orphaned_blocks,
    put_block, put_last_hash, put_mempool, put_utxo, remove_from_orphan_blocks,
    remove_txs_from_mempool, MAX_ORPHAN_CHAIN_AGE,
};
use lazy_static::lazy_static;

use super::block::Block;

// Orphans are an integral part of a p2p blockchain system, as they are the basis of many consensus models.
// Orphans are blocks that may not link to the existing chain, but may be a part of another, longer chain
// existing within the network. If a longer chain exists, all nodes should switch to the longest chain, and revert
// any blocks that may have been mined along a diverging chain.

/// ChainSnapshot defines the chain state before a rollback operation so that the chain can be restored if operations fail
struct ChainSnapshot {
    last_hash: [u8; 32],
    utxo_changes: Vec<UtxoChange>,
    removed_blocks: Vec<Block>,
}

/// Records a change made to the utxo set for use in rollback operations
enum UtxoChange {
    Added {
        tx_id: [u8; 32],
        out_idx: u32,
        utxo: TxOutput,
    },
    Removed {
        tx_id: [u8; 32],
        out_idx: u32,
    },
}

/// ChainManager coordinates rollback operations
struct ChainManager {
    is_locked: Arc<Mutex<bool>>,
    snapshot: Option<ChainSnapshot>,
}

impl ChainManager {
    pub fn new() -> Self {
        ChainManager {
            is_locked: Arc::new(Mutex::new(false)),
            snapshot: None,
        }
    }

    /// Atomic lock operation for the entire chain
    pub fn lock_chain(&self) -> Result<(), Box<dyn Error>> {
        let mut lock = self
            .is_locked
            .lock()
            .map_err(|_| "[ChainManager] ERROR: Failed to acquire lock")?;
        if *lock {
            return Err("[ChainManager] ERROR: Chain is already locked for operations".into());
        }
        *lock = true;
        Ok(())
    }

    pub fn unlock_chain(&self) -> Result<(), Box<dyn Error>> {
        let mut lock = self
            .is_locked
            .lock()
            .map_err(|_| "[ChainManager] ERROR: Failed to acquire lock")?;
        *lock = false;
        Ok(())
    }

    /// create_snapshot takes a snapshot of the current chain state that can be restored
    pub fn create_snapshot(&mut self, base_hash: [u8; 32]) -> Result<(), Box<dyn Error>> {
        let mut snapshot = ChainSnapshot {
            last_hash: get_last_hash()?,
            utxo_changes: Vec::new(),
            removed_blocks: Vec::new(),
        };

        let mut curr_block = get_last_block()?;

        // Store all blocks that would be removed in the rollback
        while curr_block.hash != base_hash {
            snapshot.removed_blocks.push(curr_block.clone());

            if let Some(prev_block) = get_block(&curr_block.prev_hash)? {
                curr_block = prev_block;
            } else {
                return Err("Failed to get previous block during snapshot creation".into());
            }
        }

        self.snapshot = Some(snapshot);
        Ok(())
    }

    // Restore chain to previous state if rollback fails
    pub fn restore_snapshot(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(snapshot) = &self.snapshot {
            // Restore last hash
            put_last_hash(&snapshot.last_hash);

            // Restore UTXOs
            for change in &snapshot.utxo_changes {
                match change {
                    UtxoChange::Added {
                        tx_id,
                        out_idx,
                        utxo,
                    } => {
                        put_utxo(tx_id, *out_idx, utxo)?;
                    }
                    UtxoChange::Removed { tx_id, out_idx } => {
                        delete_utxo(tx_id, *out_idx)?;
                    }
                }
            }

            // Re-add removed blocks if any
            for block in snapshot.removed_blocks.iter().rev() {
                put_block(block);
            }

            self.snapshot = None;
            Ok(())
        } else {
            Err("[ChainManager] ERROR: No snapshot available to restore".into())
        }
    }

    // Record UTXO changes for possible restore
    pub fn record_utxo_change(&mut self, change: UtxoChange) {
        if let Some(snapshot) = &mut self.snapshot {
            snapshot.utxo_changes.push(change);
        }
    }
}

// Global chain manager instance
lazy_static! {
    static ref CHAIN_MANAGER: Mutex<ChainManager> = Mutex::new(ChainManager::new());
}

pub fn check_for_valid_orphan_blocks() -> Result<(), Box<dyn Error>> {
    let orphan_map = get_orphaned_blocks();
    let last_hash = get_last_hash()?;
    for (_, block) in orphan_map.iter() {
        if block.prev_hash == last_hash {
            println!("Valid orphan block found! Attempting to commit...");
            commit_block(&block.clone())?;
        }
    }

    Ok(())
}

pub fn check_orphans_for_longest_chain() -> Result<(), Box<dyn Error>> {
    let block_hashes = get_all_block_hashes()?;
    let orphan_map = get_orphaned_blocks();
    let mut manager = CHAIN_MANAGER.lock()?;

    // Process each orphan that could connect to main chain
    for (_, orphan) in orphan_map.iter() {
        if block_hashes.contains(&orphan.prev_hash) {
            // Identify potential orphan chain
            let orphan_chain = build_orphan_chain(orphan, &orphan_map)?;

            // Get base block where the orphan chain would connect
            let base_block = get_block(&orphan.prev_hash)?.ok_or_else(|| {
                "[orphan::check_orphans_for_longest_chain] ERROR: Failed to fetch base block"
                    .to_string()
            })?;

            let orphan_chain_height = base_block.height as usize + orphan_chain.len();
            let last_chain_block = get_last_block()?;

            if orphan_chain_height > last_chain_block.height as usize {
                // Found longer chain - attempt adoption with safety measures
                if let Err(e) = adopt_orphan_chain(&base_block, &orphan_chain, &mut manager) {
                    println!("Failed to adopt orphan chain: {}", e);
                    // Ensure chain is unlocked even if adoption fails
                    let _ = manager.unlock_chain();
                }
            } else {
                let height_diff = last_chain_block.height as usize - orphan_chain_height;

                // Remove orphan chain if it's too old
                if height_diff > MAX_ORPHAN_CHAIN_AGE as usize {
                    prune_orphan_chain(&orphan_chain);
                    break;
                }
            }
        }
    }

    Ok(())
}

fn build_orphan_chain(
    starting_orphan: &Block,
    orphan_map: &HashMap<[u8; 32], Block>,
) -> Result<Vec<Block>, Box<dyn Error>> {
    let mut chain = vec![starting_orphan.clone()];
    let mut curr_hash = starting_orphan.hash;

    // Build continuous chain from orphans
    loop {
        let next_orphan = orphan_map
            .iter()
            .find(|(_, b)| b.prev_hash == curr_hash)
            .map(|(_, b)| b.clone());

        if let Some(next_block) = next_orphan {
            chain.push(next_block.clone());
            curr_hash = next_block.hash;
        } else {
            break;
        }
    }

    Ok(chain)
}

fn adopt_orphan_chain(
    base_block: &Block,
    orphan_chain: &[Block],
    manager: &mut ChainManager,
) -> Result<(), Box<dyn Error>> {
    // Lock chain during the entire operation
    manager.lock_chain()?;

    // Create restore point before changes
    manager.create_snapshot(base_block.hash)?;

    // Rollback to the base block
    if let Err(e) = rollback_chain_to_block(base_block.hash, manager) {
        println!(
            "[orphan::adopt_orphan_chain] ERROR: Failed to rollback chain: {}",
            e
        );
        // Restore previous state
        manager.restore_snapshot()?;
        manager.unlock_chain()?;
        return Err(e);
    }

    // Apply orphan blocks one by one with validation
    for orphan_block in orphan_chain {
        if let Err(e) = validate_and_apply_block(orphan_block, manager) {
            println!(
                "[orphan::adopt_orphan_chain] ERROR: Failed to apply orphan block: {}",
                e
            );
            // Restore previous state
            manager.restore_snapshot()?;
            manager.unlock_chain()?;
            return Err(e);
        }
    }

    // Remove applied orphans from orphan pool
    let orphan_hashes: Vec<[u8; 32]> = orphan_chain.iter().map(|b| b.hash).collect();
    remove_from_orphan_blocks(orphan_hashes);

    manager.unlock_chain()?;
    Ok(())
}

fn validate_and_apply_block(
    block: &Block,
    manager: &mut ChainManager,
) -> Result<(), Box<dyn Error>> {
    block.verify()?;

    // Apply block to the chain
    apply_block_to_chain(block, manager)?;

    Ok(())
}

fn rollback_chain_to_block(
    target_hash: [u8; 32],
    manager: &mut ChainManager,
) -> Result<(), Box<dyn Error>> {
    let mut curr_block = get_last_block()?;

    // Verify the target block exists
    get_block(&target_hash)?.ok_or_else(|| {
        "[orphan::rollback_chain_to_block] ERROR: Failed to get target block for rollback"
            .to_string()
    })?;

    // Track affected UTXOs during rollback for potential recovery
    loop {
        for tx in &curr_block.txs {
            // Remove UTXOs created by this transaction
            for (i, _) in tx.outputs.iter().enumerate() {
                delete_utxo(&tx.id, i as u32)?;
                manager.record_utxo_change(UtxoChange::Removed {
                    tx_id: tx.id,
                    out_idx: i as u32,
                });
            }

            // Restore UTXOs consumed by this transaction
            for input in &tx.inputs {
                let prev_tx = get_tx_from_chain(input.prev_tx_id)?;
                let tx_out = prev_tx.outputs[input.out as usize].clone();
                put_utxo(&input.prev_tx_id, input.out, &tx_out)?;
                manager.record_utxo_change(UtxoChange::Added {
                    tx_id: input.prev_tx_id,
                    out_idx: input.out,
                    utxo: tx_out,
                });
            }

            // Return transaction to mempool
            put_mempool(tx);
        }

        // Break if we've reached the target block
        if curr_block.prev_hash == target_hash {
            break;
        }

        // Delete current block and move to previous block
        delete_block(&curr_block.hash);
        curr_block = get_block(&curr_block.prev_hash)?
            .ok_or_else(|| "[orphan::rollback_chain_to_block] ERROR: Failed to get previous block during rollback".to_string())?;
    }

    // Update the chain tip
    put_last_hash(&target_hash);

    Ok(())
}

// Apply a block to the chain with proper UTXO management
fn apply_block_to_chain(block: &Block, manager: &mut ChainManager) -> Result<(), Box<dyn Error>> {
    // Process all transactions in the block
    for tx in &block.txs {
        // Remove inputs from UTXO set
        for input in &tx.inputs {
            delete_utxo(&input.prev_tx_id, input.out)?;
            manager.record_utxo_change(UtxoChange::Added {
                tx_id: input.prev_tx_id,
                out_idx: input.out,
                utxo: get_tx_from_chain(input.prev_tx_id)?.outputs[input.out as usize].clone(),
            });
        }

        // Add outputs to UTXO set
        for (i, output) in tx.outputs.iter().enumerate() {
            put_utxo(&tx.id, i as u32, output)?;
            manager.record_utxo_change(UtxoChange::Removed {
                tx_id: tx.id,
                out_idx: i as u32,
            });
        }

        // Remove from mempool if it was there
        remove_txs_from_mempool(vec![tx.id]);
    }

    put_block(block);
    put_last_hash(&block.hash);
    Ok(())
}

fn prune_orphan_chain(orphan_chain: &[Block]) {
    let orphan_hashes: Vec<[u8; 32]> = orphan_chain.iter().map(|b| b.hash).collect();
    remove_from_orphan_blocks(orphan_hashes);
}
