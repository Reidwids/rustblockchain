use std::collections::HashMap;

use crate::wallet::address::Address;
use rocksdb::Transaction;
use secp256k1::ecdsa::{SerializedSignature, Signature};
use secp256k1::rand::RngCore;
use secp256k1::{rand, Message, PublicKey, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};

/** Constants **/
const COINBASE_REWARD: i64 = 20;

/** Transaction **/
#[derive(Serialize, Deserialize, Debug)]
struct Tx {
    id: [u8; 32], // ID of the transaction
    inputs: Vec<TxInput>,
    outputs: Vec<TxOutput>,
}

impl Tx {
    /// Returns the sha256 hash of the transaction, to be used as the tx ID
    pub fn hash(&self) -> [u8; 32] {
        let mut tx_copy = self.clone();
        tx_copy.id = [0u8; 32]; // Id field should be empty, since we set the tx id field with the resolved hash

        let serialized = bincode::serialize(&tx_copy).expect("Serialization failed");
        let hash = Sha256::digest(&serialized);

        hash.into() // Convert to [u8; 32]
    }

    /// Returns a human readable string of the transaction
    pub fn to_string(&self) -> String {
        let mut lines: Vec<String> = Vec::new();

        lines.push(format!("--- Transaction {:x}:", hex::encode(self.id)));

        for (i, input) in self.inputs.iter().enumerate() {
            lines.push(format!("Input # {}:", i));
            lines.push(format!("  Input TxID: {:x}", hex::encode(input.id)));
            lines.push(format!("  Out: {}", input.out));
            lines.push(format!("  Signature: {:x}", hex::encode(&input.signature)));
            lines.push(format!("  PubKey: {:x}", hex::encode(&input.pub_key)));
        }

        for (i, output) in self.outputs.iter().enumerate() {
            lines.push(format!("Output {}:", i));
            lines.push(format!("  Value: {}", output.value));
            lines.push(format!(
                "  PubKeyHash: {:x}",
                hex::encode(&output.pub_key_hash)
            ));
        }

        lines.join("\n")
    }

    /// Returns a copy of the given Tx without a pub key or signature
    pub fn trimmed_copy(&self) -> Tx {
        let inputs: Vec<TxInput>;
        let outputs: Vec<TxInput>;

        for (i, input) in self.inputs.iter().enumerate() {
            inputs.push(TxInput {
                tx_id: input.tx_id,
                out: input.out,
                signature: vec![],
                pub_key: vec![],
            })
        }

        for (i, output) in self.outputs.iter().enumerate() {
            outputs.push(TxOutput {
                value: output.value,
                pub_key_hash: output.pub_key_hash,
            })
        }

        Tx {
            id: self.id,
            inputs,
            outputs,
        }
    }

    /// Checks if this is the coinbase tx
    pub fn is_coinbase(&self) -> bool {
        self.inputs.len() == 1 && self.inputs[0].tx_id == [0; 32] && self.inputs[0].out == u32::MAX
    }

    /// Sign a tx with a given private key and
    pub fn sign(&mut self, priv_key: &SecretKey, prev_txs: &HashMap<[u8; 32], Tx>) {
        if self.is_coinbase() {
            return; // Coinbase txs don't need to be signed
        }

        let secp = Secp256k1::new();

        // Loop through inputs from original tx so we can append a signature
        for (i, input) in &mut self.inputs.iter().enumerate() {
            // Build a copy for hashing that does not include the pubkey or signature
            let mut tx_copy = self.trimmed_copy();

            // Find the prev tx corresponding to the tx input
            let prev_tx = prev_txs
                .get(&input.tx_id) // tx id of the input represents a previous output
                .expect("[Tx::sign] ERROR: Previous tx missing!");

            // Resolve the output for the found tx
            let prev_output = &prev_tx.outputs[input.out as usize];

            // Set pubkey, so our hash includes the sender pubkey
            tx_copy.inputs[i as usize].pub_key = prev_output.pub_key_hash.to_vec();

            // Set the ID to the hash of the tx. When we verify, this will be used for pubkey comparison
            tx_copy.id = tx_copy.hash();
            let msg = Message::try_from(&tx_copy.id).expect("[Tx::sign] Invalid hash!");
            let sig = secp.sign_ecdsa(&msg, priv_key);

            // Set the sig of the original input
            input.signature = sig.serialize_compact().to_vec();
        }
    }

    pub fn verify(&self, prev_txs: &HashMap<[u8; 32], Tx>) -> bool {
        if self.is_coinbase() {
            return true;
        }

        for (i, input) in &mut self.inputs.iter().enumerate() {
            let mut tx_copy = self.trimmed_copy();

            // Use the same tx build pattern as signing
            let prev_tx = prev_txs
                .get(&input.tx_id)
                .expect("[Tx::sign] ERROR: Previous tx missing!");
            let prev_output = &prev_tx.outputs[input.out as usize];
            tx_copy.inputs[i as usize].pub_key = prev_output.pub_key_hash.to_vec();
            tx_copy.id = tx_copy.hash();

            // Reconstruct sig
            let sig = Signature::from_compact(&input.signature)
                .expect("[Tx::verify] Invalid signature format!");

            // Deserialize the public key
            let pub_key = PublicKey::from_slice(&input.pub_key)
                .expect("[Tx::verify] Invalid public key format!");

            // Verify the signature
            let msg = Message::try_from(&tx_copy.id).expect("[Tx::sign] Invalid hash!");
            if secp.verify_ecdsa(&msg, &sig, &pub_key).is_err() {
                return false;
            }
        }
        true
    }

    /// Create a new tx
    pub fn new(&self) {} // TODO
}

/** Inputs and Outputs **/
struct TxOutputs {
    outputs: Vec<TxOutput>,
}

#[derive(Serialize, Deserialize, Debug)]
struct TxOutput {
    value: u64,             // Value of output tokens in the tx. Outputs cannot be split
    pub_key_hash: [u8; 20], // Recipient pub key (Sha256 + Ripemd160). Locks the output so it can only be included in a future input by the output author.
}

impl TxOutput {
    /// Creates a new tx output given a value and a recipient address.
    pub fn new(value: u64, addr: &Address) -> TxOutput {
        let mut txo = TxOutput {
            value,
            pub_key_hash: [0; 20],
        };
        txo.lock(addr);
        txo
    }

    /// Locks a `txOutput` with the given address
    pub fn lock(&mut self, addr: &Address) {
        self.pub_key_hash.copy_from_slice(addr.pub_key_hash());
    }

    /// Returns a boolean representing the comparison of the pub_key_hash to an incoming hash
    pub fn is_locked_with_key(&self, pub_key_hash: [u8; 20]) -> bool {
        pub_key_hash == self.pub_key_hash
    }
}

struct TxInput {
    tx_id: [u8; 32],      // ID of the transaction the output is inside of
    out: u32,             // Index that the output appears within the referenced transaction
    signature: Signature, // Signature created with the senders priv_key proving that they can spend the prev transaction output.
    pub_key: PublicKey, // The spender's public key - used to verify the signature against the pubkeyhash of the last transaction
}

impl TxInput {
    /// Test if a given address matches the locking pub key hash of the tx input
    pub fn uses_key(&self, addr: &Address) -> bool {
        let locking_hash = self.pub_key;
        return locking_hash == addr.pub_key_hash();
    }

    /// Create a pub key hash from a given public key
    fn public_key_hash(pub_key: &PublicKey) -> [u8; 20] {
        let sha256_hash = Sha256::digest(pub_key.serialize());
        let ripemd160_hash = Ripemd160::digest(sha256_hash);
        ripemd160_hash.try_into().expect("Hash should be 20 bytes")
    }
}

/// Create the coinbase tx
pub fn coinbase_tx(to: Address) -> Tx {
    // Create random data for the tx
    let mut rand_data = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut rand_data);

    let data = hex::encode(rand_data);

    // There are no prev tx ins, so make a random instance
    let tx_in = vec![TxInput {
        tx_id: [0u8; 32],
        out: u32::MAX,
        signature: rand_data.to_vec(),
        pub_key: vec![],
    }];

    // Create the tx out with the creators pub key hash
    let tx_out = vec![TxOutput {
        value: COINBASE_REWARD, // Reward for coinbase tx is static
        pub_key_hash: to.pub_key_hash(),
    }];

    // Create the tx with empty id
    let mut tx = Tx {
        id: [0u8; 32],
        inputs: tx_in,
        outputs: tx_out,
    };

    // Create the id from the hash
    tx.id = tx.hash();

    // Return the tx
    tx
}
