use crate::blockchain::transaction::{
    tx::{Tx, TxInput, TxOutput},
    utxo::UTXOSet,
};
use hex::decode;
use secp256k1::{ecdsa::Signature, PublicKey};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error};

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

#[derive(Serialize, Deserialize, Debug)]
pub struct TxOutputJson {
    pub value: u32,
    pub pub_key_hash: String, // Hex-encoded
}

pub type UTXOSetJson = HashMap<String, HashMap<u32, TxOutputJson>>;

pub fn convert_utxoset_to_json(utxoset: &UTXOSet) -> UTXOSetJson {
    utxoset
        .iter()
        .map(|(tx_id, tx_out_map)| {
            let tx_id_hex = hex::encode(tx_id);
            let out_map_json: HashMap<u32, TxOutputJson> = tx_out_map
                .iter()
                .map(|(idx, txo)| {
                    (
                        *idx,
                        TxOutputJson {
                            value: txo.value,
                            pub_key_hash: hex::encode(&txo.pub_key_hash),
                        },
                    )
                })
                .collect();
            (tx_id_hex, out_map_json)
        })
        .collect()
}

pub fn convert_json_to_utxoset(json: &UTXOSetJson) -> Result<UTXOSet, Box<dyn std::error::Error>> {
    let mut utxoset = UTXOSet::new();

    for (tx_id_hex, out_map_json) in json {
        let tx_id_bytes = hex::decode(tx_id_hex)?;
        let tx_id_array: [u8; 32] = tx_id_bytes
            .try_into()
            .map_err(|_| "Failed to convert tx ID")?;

        let mut out_map = HashMap::new();
        for (idx, txo_json) in out_map_json {
            let pub_key_hash = hex::decode(&txo_json.pub_key_hash)?;
            out_map.insert(
                *idx,
                TxOutput {
                    value: txo_json.value,
                    pub_key_hash: pub_key_hash
                        .try_into()
                        .map_err(|_| "Failed to convert pub_key_hash")?,
                },
            );
        }

        utxoset.insert(tx_id_array, out_map);
    }

    Ok(utxoset)
}
