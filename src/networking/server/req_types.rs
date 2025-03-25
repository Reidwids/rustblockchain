use crate::blockchain::transaction::tx::{Tx, TxInput, TxOutput};
use hex::decode;
use secp256k1::{ecdsa::Signature, PublicKey};
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Serialize, Deserialize)]
pub struct TxJson {
    pub id: String, // Hex-encoded
    pub inputs: Vec<TxInputJson>,
    pub outputs: Vec<TxOutputJson>,
}

impl TxJson {
    pub fn to_tx(self) -> Result<Tx, Box<dyn Error>> {
        Ok(Tx {
            id: decode_hex(&self.id)?,
            inputs: self
                .inputs
                .iter()
                .map(|input| {
                    Ok(TxInput::new(
                        decode_hex(&input.prev_tx_id)?,
                        input.out,
                        decode_sig(&input.signature)?,
                        decode_pubkey(&input.pub_key)?,
                    ))
                })
                .collect::<Result<Vec<TxInput>, Box<dyn Error>>>()?,
            outputs: self
                .outputs
                .iter()
                .map(|output| {
                    Ok(TxOutput {
                        value: output.value,
                        pub_key_hash: decode_hex(&output.pub_key_hash)?,
                    })
                })
                .collect::<Result<Vec<TxOutput>, Box<dyn Error>>>()?,
        })
    }
}
fn decode_hex<T: TryFrom<Vec<u8>>>(hex: &str) -> Result<T, Box<dyn Error>> {
    decode(hex)?
        .try_into()
        .map_err(|_| "[tx_json::decode_hex] ERROR: Decoding error".into())
}

fn decode_sig(sig: &str) -> Result<Signature, Box<dyn Error>> {
    Signature::from_der(&decode(sig)?)
        .map_err(|_| "[tx_json::decode_sig] ERROR: Invalid signature".into())
}

fn decode_pubkey(pubkey: &str) -> Result<PublicKey, Box<dyn Error>> {
    PublicKey::from_slice(&decode(pubkey)?)
        .map_err(|_| "[tx_json::decode_pub_key] ERROR: Invalid public key".into())
}

#[derive(Serialize, Deserialize)]
pub struct TxInputJson {
    pub prev_tx_id: String, // Hex-encoded
    pub out: u32,
    pub signature: String, // Base64-encoded
    pub pub_key: String,   // Hex-encoded
}

#[derive(Serialize, Deserialize)]
pub struct TxOutputJson {
    pub value: u32,
    pub pub_key_hash: String, // Hex-encoded
}
