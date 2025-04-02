use std::{
    error::Error,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
    u32,
};

use crate::{
    blockchain::{chain::get_last_block, transaction::mempool::is_output_spent_in_mempool},
    cli::db,
    wallets::address::Address,
};
use hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    merkle::MerkleTree,
    transaction::tx::{coinbase_tx, Tx},
};

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

            // Ensure no txs are double spent
            for tx_input in &tx.inputs {
                if is_output_spent_in_mempool(tx_input.prev_tx_id, tx_input.out) {
                    return Err("[block::mine] ERROR: tx contains outputs spent in mempool".into());
                }
            }
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
