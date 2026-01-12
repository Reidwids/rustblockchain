use serde::{Deserialize, Serialize};

use crate::json_types::{BlockJson, UTXOSetJson};

#[derive(Serialize, Deserialize, Debug)]
pub struct GetUTXORes {
    pub address: String,
    pub utxos: UTXOSetJson,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetWalletBalanceRes {
    pub address: String,
    pub balance: u32,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(transparent)]
pub struct PrintBlockchainRes {
    pub chain: Vec<BlockJson>,
}
