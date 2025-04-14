use std::{
    collections::HashMap,
    error::Error,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
    u32,
};

use crate::{
    blockchain::chain::get_last_block,
    cli::db::{self, get_block},
    wallets::address::Address,
};
use hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    merkle::MerkleTree,
    transaction::tx::{coinbase_tx, Tx, COINBASE_REWARD},
};

pub type OrphanBlocks = HashMap<[u8; 32], Block>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    pub txs: Vec<Tx>,
    pub prev_hash: [u8; 32],
    pub hash: [u8; 32],
    pub nonce: u32,
    pub height: u32,
    pub timestamp: u64,
}

impl Block {
    /// Create the genesis block from a coinbase transaction
    pub fn genesis(addr: &Address) -> Result<Self, Box<dyn Error>> {
        let cbtx = coinbase_tx(addr)?;
        Ok(Block {
            hash: [0u8; 32], // Initialize as empty
            txs: vec![cbtx],
            prev_hash: [0u8; 32],
            nonce: 0,
            height: 0,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("[Block::new] ERROR: Failed to create timestamp")
                .as_secs(),
        })
    }

    pub fn is_genesis(&self) -> bool {
        self.prev_hash == [0u8; 32] && self.height == 0
    }

    /// Create and mine a new block
    pub fn new(reward_addr: &Address) -> Result<Self, Box<dyn Error>> {
        let cbtx = coinbase_tx(reward_addr)?;
        let prev_block = get_last_block()?;
        let txs: Vec<Tx> = db::get_mempool().values().cloned().collect();
        let mut all_txs = Vec::with_capacity(txs.len() + 1);
        all_txs.push(cbtx); // Add coinbase first
        all_txs.extend_from_slice(&txs); // Add the rest of the transactions

        Ok(Block {
            hash: [0u8; 32], // Initialize as empty
            txs: all_txs,
            prev_hash: prev_block.hash,
            nonce: 0,
            height: prev_block.height + 1,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("[Block::new] ERROR: Failed to create timestamp")
                .as_secs(),
        })
    }

    /// Mines a designated block using proof of work
    pub fn mine(&mut self) -> Result<(), Box<dyn Error>> {
        let target = get_target_difficulty();
        let mut nonce: u32 = 0;
        let mut hash: [u8; 32] = [0; 32];
        let max = u32::MAX;

        println!("Validating block...");
        for tx in &self.txs {
            tx.verify()
                .map_err(|e| format!("[block::mine] ERROR: Cannot mine block - {:?}", e))?;
        }
        println!("Validation successful!");
        println!("Mining block:");
        while nonce < max {
            self.nonce = nonce;
            hash = self.hash()?;

            // Print hash repeating over same line
            let hex_str = hex::encode(&hash);
            print!("\r{}", hex_str);
            std::io::stdout().flush().unwrap();

            // If hash is less than target, it meets our PoW criteria
            if hash < target {
                break;
            } else {
                // Increasing our nonce changes the block so the next hash will be different
                nonce += 1
            }
        }
        // Leave an empty line after the hash is found
        println!();

        self.hash = hash;
        self.nonce = nonce;
        println!("Hash found: {}", hex::encode(&hash));
        println!("Nonce: {}", nonce);

        // Prepare block for db
        let block_hash = self.hash()?;
        // Store block ref and last hash
        db::put_block(&block_hash, self);
        db::put_last_hash(&block_hash);
        Ok(())
    }

    /// Hash the block into a single SHA256 hash
    pub fn hash(&self) -> Result<[u8; 32], Box<dyn Error>> {
        let mut hasher = Sha256::new();
        hasher.update(self.prev_hash);
        hasher.update(self.hash_txs()?);
        // Use little-endian for consitency
        hasher.update(self.nonce.to_le_bytes());
        hasher.update(self.height.to_le_bytes());
        hasher.update(self.timestamp.to_le_bytes());

        let result = hasher.finalize();
        Ok(result.into())
    }

    /// Using a Merkle tree, derive the hash of a root block's transactions
    fn hash_txs(&self) -> Result<[u8; 32], Box<dyn Error>> {
        let tx_hashes: Result<Vec<Vec<u8>>, Box<dyn Error>> = self
            .txs
            .iter()
            .map(|tx| tx.hash().map(|h| h.to_vec()))
            .collect();

        let tx_hashes = tx_hashes?;

        let tree = MerkleTree::new(tx_hashes);

        Ok(tree.root.hash)
    }

    pub fn verify(&self) -> Result<bool, Box<dyn Error>> {
        if self.txs.is_empty() {
            return Ok(false);
        }

        // Verify txs
        for tx in &self.txs {
            if !tx.verify()? {
                return Ok(false);
            }
        }

        // Verify coinbase tx
        let coinbase = &self.txs[0];
        if !coinbase.is_coinbase() || coinbase.outputs[0].value != COINBASE_REWARD {
            return Ok(false);
        }

        // Verify PoW
        let target = get_target_difficulty();
        let hash = self.hash()?;
        if hash >= target || hash != self.hash {
            return Ok(false);
        }

        Ok(true)
    }

    /// Verifies a block without checking tx validity. Txs will be checked
    /// if/when the orphan is added to the chain.
    pub fn verify_orphan(&self) -> Result<bool, Box<dyn Error>> {
        if self.txs.is_empty() {
            return Ok(false);
        }

        // Verify coinbase tx
        let coinbase = &self.txs[0];
        if !coinbase.is_coinbase() || coinbase.outputs[0].value != COINBASE_REWARD {
            return Ok(false);
        }

        // Verify PoW
        let target = get_target_difficulty();
        let hash = self.hash()?;
        if hash >= target || hash != self.hash {
            return Ok(false);
        }

        return Ok(true);
    }
}

// Difficulty can be made dynamic in future
const DIFFICULTY: usize = 16;
fn get_target_difficulty() -> [u8; 32] {
    let mut target = [0u8; 32];

    // This PoW algorithm shifts 1 by (256 - Difficulty) to get a target that has zeroes for the first *Difficulty bits
    // When mining, we will hash while changing the nonce until a hash is found that is less
    // than the target - meaning it has the first n bits set to 0
    let byte_index = DIFFICULTY / 8;
    let bit_index = DIFFICULTY % 8;

    target[byte_index] = 1 << (7 - bit_index);
    target
}

pub fn get_blocks_since_height(height: u32) -> Result<Vec<Block>, Box<dyn Error>> {
    let mut current_block = if let Ok(b) = get_last_block() {
        b
    } else {
        return Err(
            "[block::get_blocks_since_height] ERROR: Could not find blocks since last height"
                .into(),
        );
    };

    let mut res: Vec<Block> = Vec::new();
    let mut block_height = current_block.height;
    // Trace back blocks until we reach the block height matching the height we have requested
    // which would be the last height our requesting node has. If we are requesting with height 0,
    // The genesis block is included in the request
    while height < block_height || height == 0 {
        res.push(current_block.clone());

        if current_block.is_genesis() {
            break;
        }

        current_block = get_block(&current_block.prev_hash)
            .map_err(|e| {
                format!(
                    "[block::get_blocks_since_height] ERROR: Failed to fetch previous block {}",
                    e
                )
            })?
            .ok_or_else(|| "[block::get_blocks_since_height] ERROR: Last block not found")?;

        block_height = current_block.height;
    }

    Ok(res)
}
