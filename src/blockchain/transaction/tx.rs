use std::collections::HashMap;

use secp256k1::ecdsa::Signature;
use secp256k1::rand::RngCore;
use secp256k1::{rand, Message, PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Debug;

use crate::ownership::address::{hash_pub_key, Address};

/** Constants **/
const COINBASE_REWARD: u32 = 100;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tx {
    pub id: [u8; 32], // ID of the transaction
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
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

        lines.push(format!("--- Transaction {}:", hex::encode(self.id)));

        for (i, input) in self.inputs.iter().enumerate() {
            lines.push(format!("Input # {}:", i));
            lines.push(format!("  Input TxID: {}", hex::encode(input.prev_tx_id)));
            lines.push(format!("  Out: {}", input.out));
            lines.push(format!(
                "  Signature: {}",
                hex::encode(input.signature.serialize_compact())
            ));
            lines.push(format!(
                "  PubKey: {}",
                hex::encode(input.pub_key.serialize())
            ));
        }

        for (i, output) in self.outputs.iter().enumerate() {
            lines.push(format!("Output {}:", i));
            lines.push(format!("  Value: {}", output.value));
            lines.push(format!(
                "  PubKeyHash: {}",
                hex::encode(&output.pub_key_hash)
            ));
        }

        lines.join("\n")
    }

    /// Returns a copy of the given Tx without input pub keys and signatures.
    /// This ensures standardization when signing and validating - so that the tx
    /// has the same format when on either side of the tx.
    /// Note that removing the pub key isn't necessary - but is done simply to shave off
    /// extra data.
    fn trimmed_copy(&self) -> Tx {
        let mut trimmed_inputs: Vec<TxInput> = vec![];

        let secp = Secp256k1::new();
        let dummy_priv_key = SecretKey::from_slice(&[1u8; 32])
            .expect("[Tx::trimmed_copy] ERROR: Failed to trim pub key");

        for input in &self.inputs {
            trimmed_inputs.push(TxInput {
                prev_tx_id: input.prev_tx_id,
                out: input.out,
                // Set the sig to an empty byte array
                signature: Signature::from_compact(&[0u8; 64])
                    .expect("[Tx::trimmed_copy] ERROR: Failed to trim signature"),
                // Set the pubkey to a standardized dummy key
                pub_key: PublicKey::from_secret_key(&secp, &dummy_priv_key),
            })
        }

        Tx {
            id: [0u8; 32], // Empty ID to be filled after hashing
            inputs: trimmed_inputs,
            outputs: self.outputs.clone(),
        }
    }

    /// Checks if this is the coinbase tx
    pub fn is_coinbase(&self) -> bool {
        self.inputs.len() == 1
            && self.inputs[0].prev_tx_id == [0; 32]
            && self.inputs[0].out == usize::MAX
    }

    /// Sign a tx with a given private key
    pub fn sign(&mut self, priv_key: &SecretKey) {
        if self.is_coinbase() {
            return; // Coinbase txs don't need to be signed
        }
        let secp = Secp256k1::new();
        let tx_copy_base = self.trimmed_copy();

        // Loop through inputs from original tx so we can append a signature.
        for input in &mut self.inputs {
            // Build a copy for hashing that does not include the pubkey or signature
            let mut tx_copy: Tx = tx_copy_base.trimmed_copy();

            // Set the ID to the hash of the tx. When we verify, this will be used for pubkey comparison
            tx_copy.id = tx_copy.hash();
            let msg = Message::from_digest(tx_copy.id);
            let sig = secp.sign_ecdsa(&msg, priv_key);

            // Set the sig of the original input
            input.signature = Signature::from_compact(&sig.serialize_compact())
                .expect("[Tx::sign] ERROR: Failed to serialize signature");
            // Note we assume here that the public key has already been added to the tx
        }
    }

    pub fn verify(&self, prev_txs: &HashMap<[u8; 32], Tx>) -> bool {
        // Coinbase txs do not need standard verification
        if self.is_coinbase() {
            return true;
        }

        for input in &self.inputs {
            let mut tx_copy = self.trimmed_copy();

            // Verify that the prev output pub key hash matches the pub key of the input
            let prev_tx = prev_txs
                .get(&input.prev_tx_id)
                .expect("[Tx::verify] ERROR: Previous tx missing!");
            let prev_output = &prev_tx.outputs[input.out as usize];
            // Recompute the pub key hash from the input's public key
            let computed_pub_key_hash = hash_pub_key(&input.pub_key);
            // Check if the computed pub key hash matches the expected one
            if computed_pub_key_hash != prev_output.pub_key_hash {
                println!("[Tx::verify] ERROR: PubKey does not match PubKeyHash!");
                return false;
            }

            // Recompute the tx id from the trimmed copy. If the id differs from
            // the signed tx id, the signature verification will fail
            tx_copy.id = tx_copy.hash();

            // Verify the signature was created by signing the tx is with the given pub key
            let msg = Message::from_digest(tx_copy.id);
            if Secp256k1::new()
                .verify_ecdsa(&msg, &input.signature, &input.pub_key)
                .is_err()
            {
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxOutput {
    pub value: u32, // Value of output tokens in the tx. Outputs cannot be split
    pub pub_key_hash: [u8; 20], // Recipient pub key (Sha256 + Ripemd160). Locks the output so it can only be included in a future input by the output author.
}

impl TxOutput {
    /// Creates a new tx output given a value and a recipient address.
    pub fn new(value: u32, addr: &Address) -> Self {
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
    pub fn is_locked_with_key(&self, pub_key_hash: &[u8; 20]) -> bool {
        self.pub_key_hash == *pub_key_hash
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxInput {
    pub prev_tx_id: [u8; 32], // ID of the transaction the output is inside of
    pub out: usize,           // Index that the output appears within the referenced transaction
    signature: Signature, // Signature created with the senders priv_key proving that they can spend the prev transaction output.
    pub_key: PublicKey, // The spender's public key - used to verify the signature against the pubkeyhash of the last transaction
}

impl TxInput {
    /// Test if a given address matches the locking pub key hash of the tx input
    pub fn uses_key(&self, addr: &Address) -> bool {
        let locking_hash = hash_pub_key(&self.pub_key);
        return locking_hash == *addr.pub_key_hash();
    }
}

/// Create the coinbase tx
pub fn coinbase_tx(to: &Address) -> Tx {
    // Coinbase txs will contain an arbitrary in, since there is no previous out
    let mut rand_data = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut rand_data);

    // Create a random ephemeral pubkey and signature
    let secp = Secp256k1::new();
    let secret_key = SecretKey::new(&mut rand::thread_rng());
    let msg = Message::from_digest(rand_data);
    let signature = secp.sign_ecdsa(&msg, &secret_key);

    // Create the dummy in tx
    let tx_in = vec![TxInput {
        prev_tx_id: [0u8; 32],
        out: usize::MAX,
        signature,
        pub_key: PublicKey::from_secret_key(&secp, &secret_key),
    }];

    // Create the tx out with the creator's pub key hash
    let tx_out = vec![TxOutput {
        value: COINBASE_REWARD, // Reward for coinbase tx is static
        pub_key_hash: *to.pub_key_hash(),
    }];

    // Create the tx with an empty id, and fill it with the tx hash
    let mut tx = Tx {
        id: [0u8; 32],
        inputs: tx_in,
        outputs: tx_out,
    };
    // Note that the coinbase tx hash is irrelevant, since we don't verify the coinbase tx.
    tx.id = tx.hash();
    tx
}
