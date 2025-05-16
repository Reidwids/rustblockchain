use core_lib::address::{hash_pub_key, Address};
use core_lib::tx::{Tx, TxInput, TxOutput};
use secp256k1::rand::RngCore;
use secp256k1::{rand, Message, PublicKey, Secp256k1, SecretKey};
use std::error::Error;

use crate::cli::db::get_utxo;

/** Constants **/
pub const COINBASE_REWARD: u32 = 100;
pub trait TxVerify {
    fn verify(&self) -> Result<bool, Box<dyn std::error::Error>>;
}

impl TxVerify for Tx {
    fn verify(&self) -> Result<bool, Box<dyn Error>> {
        // Coinbase txs do not need standard verification
        if self.is_coinbase() {
            return Ok(true);
        }

        for input in &self.inputs {
            let mut tx_copy = self.trimmed_copy();

            // Verify that the prev output pub key hash matches the pub key of the input
            let prev_tx_out = if let Some(tx) = get_utxo(&input.prev_tx_id, input.out)? {
                tx
            } else {
                return Ok(false);
            };

            // Recompute the pub key hash from the input's public key
            let computed_pub_key_hash = hash_pub_key(&input.pub_key);

            // Check if the computed pub key hash matches the expected one
            if computed_pub_key_hash != prev_tx_out.pub_key_hash {
                return Ok(false);
            }

            // Recompute the tx id from the trimmed copy. If the id differs from
            // the signed tx id, the signature verification will fail
            tx_copy.id = tx_copy.hash()?;

            // Verify the signature was created by signing the tx is with the given pub key
            let msg = Message::from_digest(tx_copy.id);
            if Secp256k1::new()
                .verify_ecdsa(&msg, &input.signature, &input.pub_key)
                .is_err()
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// Create the coinbase tx
pub fn coinbase_tx(reward_addr: &Address) -> Result<Tx, Box<dyn Error>> {
    // Coinbase txs will contain an arbitrary in, since there is no previous out
    let mut rand_data = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut rand_data);

    // Create a random ephemeral pubkey and signature
    let secp = Secp256k1::new();
    let secret_key = SecretKey::new(&mut rand::thread_rng());
    let msg = Message::from_digest(rand_data);
    let signature = secp.sign_ecdsa(&msg, &secret_key);

    // Create the dummy in tx
    let tx_in = vec![TxInput::new(
        [0u8; 32],
        u32::MAX,
        signature,
        PublicKey::from_secret_key(&secp, &secret_key),
    )];

    // Create the tx out with the creator's pub key hash
    let tx_out = vec![TxOutput {
        value: COINBASE_REWARD, // Reward for coinbase tx is static
        pub_key_hash: *reward_addr.pub_key_hash(),
    }];

    // Create the tx with an empty id, and fill it with the tx hash
    let mut tx = Tx {
        id: [0u8; 32],
        inputs: tx_in,
        outputs: tx_out,
    };
    // Note that the coinbase tx hash is irrelevant, since we don't verify the coinbase tx.
    tx.id = tx.hash()?;
    Ok(tx)
}
