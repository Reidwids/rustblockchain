use secp256k1::ecdsa::Signature;
use secp256k1::{Message, PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::{error::Error, fmt::Debug};

use crate::address::Address;
use crate::wallet::Wallet;

pub type TxOutMap = HashMap<u32, TxOutput>;
pub type UTXOSet = HashMap<[u8; 32], TxOutMap>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tx {
    pub id: [u8; 32], // ID of the transaction
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
}

impl Tx {
    /// Returns the sha256 hash of the transaction, to be used as the tx ID
    pub fn hash(&self) -> Result<[u8; 32], Box<dyn Error>> {
        let mut tx_copy = self.clone();
        tx_copy.id = [0u8; 32]; // Id field should be empty, since we set the tx id field with the resolved hash

        let serialized =
            bincode::serialize(&tx_copy).map_err(|e| format!("Serialization failed, {:?}", e))?;
        let hash = Sha256::digest(&serialized);

        Ok(hash.into()) // Convert to [u8; 32]
    }

    /// Returns a copy of the given Tx without input pub keys and signatures.
    /// This ensures standardization when signing and validating - so that the tx
    /// has the same format when on either side of the tx.
    /// Note that removing the pub key isn't necessary - but is done simply to shave off
    /// extra data.
    pub fn trimmed_copy(&self) -> Tx {
        let mut trimmed_inputs: Vec<TxInput> = vec![];

        let secp = Secp256k1::new();

        for input in &self.inputs {
            trimmed_inputs.push(TxInput::new(
                input.prev_tx_id,
                input.out,
                // Set the sig to an empty byte array
                empty_signature(),
                // Set the pubkey to a standardized dummy key
                PublicKey::from_secret_key(&secp, &empty_priv_key()),
            ))
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
            && self.inputs[0].out == u32::MAX
    }

    /// Sign a tx with a given private key
    pub fn sign(&mut self, priv_key: &SecretKey) -> Result<(), Box<dyn Error>> {
        if self.is_coinbase() {
            return Ok(()); // Coinbase txs don't need to be signed
        }
        let secp = Secp256k1::new();
        let tx_copy_base = self.trimmed_copy();

        // Loop through inputs from original tx so we can append a signature.
        for input in &mut self.inputs {
            // Build a copy for hashing that does not include the pubkey or signature
            let mut tx_copy: Tx = tx_copy_base.trimmed_copy();

            // Set the ID to the hash of the tx. When we verify, this will be used for pubkey comparison
            tx_copy.id = tx_copy.hash()?;
            let msg = Message::from_digest(tx_copy.id);
            let sig = secp.sign_ecdsa(&msg, priv_key);

            // Set the sig of the original input
            input.signature = Signature::from_compact(&sig.serialize_compact())
                .map_err(|e| format!("[Tx::sign] ERROR: Failed to serialize signature {:?}", e))?;
            // Note we assume here that the public key has already been added to the tx
        }

        Ok(())
    }

    /// Create a new tx
    pub fn new(
        from_wallet: &Wallet,
        to_address: &Address,
        value: u32,
        spendable_txos: UTXOSet,
    ) -> Result<Tx, Box<dyn Error>> {
        let mut inputs: Vec<TxInput> = Vec::new();
        let mut outputs: Vec<TxOutput> = Vec::new();
        let mut sum = 0;

        // Create a new input from each spendable txo contributing to the sum
        for (tx_id, txo_map) in spendable_txos {
            for (out_idx, txo) in txo_map {
                inputs.push(TxInput::new(
                    tx_id,
                    out_idx,
                    empty_signature(),
                    *from_wallet.pub_key(),
                ));
                sum += txo.value;
            }
        }

        // Create a new output of the to address receiving the value
        outputs.push(TxOutput {
            value,
            pub_key_hash: *to_address.pub_key_hash(),
        });

        // Any leftover sum should be retained by the sender
        if sum > value {
            outputs.push(TxOutput {
                value: sum - value,
                pub_key_hash: *from_wallet.get_wallet_address().pub_key_hash(),
            });
        }

        // Sign the tx
        let mut new_tx = Tx {
            id: [0; 32],
            inputs,
            outputs,
        };
        new_tx.id = new_tx.hash()?;
        new_tx.sign(from_wallet.private_key())?;

        Ok(new_tx)
    }
}

/** Inputs and Outputs **/

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct TxOutput {
    pub value: u32, // Value of output tokens in the tx. Outputs cannot be split
    pub pub_key_hash: [u8; 20], // Recipient pub key (Sha256 + Ripemd160). Locks the output so it can only be included in a future input by the output author.
}

impl TxOutput {
    /// Returns a boolean representing the comparison of the pub_key_hash to an incoming hash
    pub fn is_locked_with_key(&self, pub_key_hash: &[u8; 20]) -> bool {
        self.pub_key_hash == *pub_key_hash
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct TxInput {
    /// ID of the transaction the output is inside of
    pub prev_tx_id: [u8; 32],
    /// Index that the output appears within the referenced transaction
    pub out: u32,
    /// Signature created with the senders priv_key proving that they can spend the prev transaction output.
    pub signature: Signature,
    /// The spender's public key - used to verify the signature against the pubkeyhash of the last transaction
    pub pub_key: PublicKey,
}
impl TxInput {
    pub fn new(prev_tx_id: [u8; 32], out: u32, signature: Signature, pub_key: PublicKey) -> Self {
        Self {
            prev_tx_id,
            out,
            signature,
            pub_key,
        }
    }
}

// TODO: Factor these out in future with options
fn empty_priv_key() -> SecretKey {
    SecretKey::from_slice(&[1u8; 32]).unwrap()
}

fn empty_signature() -> Signature {
    Signature::from_compact(&[0u8; 64]).unwrap()
}
