use std::{
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
    u32,
};

use crate::ownership::address::{bytes_to_hex_string, Address};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    merkle::MerkleTree,
    transaction::tx::{coinbase_tx, Tx},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    hash: [u8; 32],
    txs: Vec<Tx>,
    prev_hash: [u8; 32],
    nonce: u32,
    height: u32,
    timestamp: u64,
}

impl Block {
    /// Create the genesis block from a coinbase transaction
    pub fn genesis(addr: &Address) -> Self {
        let cbtx = coinbase_tx(addr);
        Self::new(vec![cbtx], [0u8; 32], 0)
    }

    /// Create and mine a new block
    pub fn new(txs: Vec<Tx>, prev_hash: [u8; 32], height: u32) -> Self {
        let mut block = Block {
            hash: [0u8; 32], // Initialize as empty
            txs,
            prev_hash,
            nonce: 0,
            height,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("[Block::new] ERROR: Failed to create timestamp")
                .as_secs(),
        };
        let (nonce, hash) = block.mine();

        // Save mining results to block
        block.hash = hash;
        block.nonce = nonce;
        println!("Hash found: {}", bytes_to_hex_string(&hash));
        println!("Nonce: {}", nonce);

        block
    }

    /// Mines a designated block using proof of work
    pub fn mine(&mut self) -> (u32, [u8; 32]) {
        let target = get_target_difficulty();
        let mut nonce: u32 = 0;
        let mut hash: [u8; 32] = [0; 32];
        let max = u32::MAX;

        while nonce < max {
            hash = self.hash();

            // Print hash repeating over same line
            let hex_str = bytes_to_hex_string(&hash);
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
        (nonce, hash)
    }

    /// Hash the block into a single SHA256 hash
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.prev_hash);
        hasher.update(self.hash_txs());
        // Use little-endian for consitency
        hasher.update(self.nonce.to_le_bytes());
        hasher.update(self.height.to_le_bytes());
        hasher.update(self.timestamp.to_le_bytes());

        let result = hasher.finalize();
        result.into()
    }

    /// Using a Merkle tree, derive the hash of a root block's transactions
    fn hash_txs(&self) -> [u8; 32] {
        let tx_hashes = self.txs.iter().map(|tx| tx.hash().to_vec()).collect();
        let tree = MerkleTree::new(tx_hashes);
        tree.root.hash
    }
}

// Difficulty can be made dynamic in future
const DIFFICULTY: usize = 15;
fn get_target_difficulty() -> [u8; 32] {
    let mut target = [0u8; 32];

    // This PoW algorithm shifts 1 by (256 - Difficulty) to get a target that has zeroes for the first n bits
    // When mining, we will hash while changing the nonce until a hash is found that is less
    // than the target - meaning it has the first n bits set to 0
    let shift = 256 - DIFFICULTY;
    let byte_index = shift / 8;
    let bit_index = shift % 8;

    target[byte_index] = 1 << (7 - bit_index);
    target
}
