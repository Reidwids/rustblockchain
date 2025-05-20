use core_lib::{
    address::Address,
    constants::SEED_API_NODE,
    req_types::{GetUTXORes, TxJson, convert_json_to_utxoset},
    tx::{Tx, UTXOSet},
    wallet::Wallet,
};
use reqwest::Client;
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[wasm_bindgen]
pub struct JsWallet {
    inner: Wallet,
}

#[wasm_bindgen]
impl JsWallet {
    #[wasm_bindgen(constructor)]
    pub fn new() -> JsWallet {
        JsWallet {
            inner: Wallet::new(),
        }
    }

    #[wasm_bindgen]
    pub fn get_wallet_address(&self) -> String {
        self.inner.get_wallet_address().get_full_address()
    }

    #[wasm_bindgen]
    pub fn get_public_key(&self) -> String {
        self.inner.pub_key().to_string()
    }

    #[wasm_bindgen]
    pub fn get_priv_key(&self) -> String {
        self.inner.private_key().display_secret().to_string()
    }

    #[wasm_bindgen]
    pub fn from_keys(pub_key: String, priv_key: String) -> Result<JsWallet, JsValue> {
        match Wallet::from_keys(pub_key, priv_key) {
            Ok(wallet) => Ok(JsWallet { inner: wallet }),
            Err(e) => Err(JsValue::from_str(&format!(
                "[wallet::from_keys] ERROR: Unable to parse wallet from given keys: {e}"
            ))),
        }
    }
}

#[wasm_bindgen]
pub async fn send_tx(to: &str, from_wallet: &JsWallet, value: u32) -> Result<JsValue, JsValue> {
    let from_address = from_wallet.get_wallet_address();

    let url = format!(
        "{}/utxo?address={}&amount={}",
        SEED_API_NODE, from_address, value
    );

    let utxos: UTXOSet;

    let client = Client::new();
    match client.get(url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<GetUTXORes>().await {
                    Ok(data) => match convert_json_to_utxoset(&data.utxos) {
                        Ok(set) => {
                            utxos = set;
                        }
                        Err(e) => {
                            return Err(JsValue::from_str(&format!(
                                "[wasm::send_tx] ERROR: Failed to convert UTXO JSON to UTXOSet: {}",
                                e
                            )));
                        }
                    },
                    Err(e) => {
                        return Err(JsValue::from_str(&format!(
                            "[wasm::send_tx] ERROR: Failed to parse UTXO response: {}",
                            e
                        )));
                    }
                }
            } else {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                return Err(JsValue::from_str(&format!(
                    "[wasm::send_tx] ERROR: Failed to fetch UTXOs from node: {} - {}",
                    status, error_text
                )));
            }
        }
        Err(e) => {
            return Err(JsValue::from_str(&format!(
                "[wasm::send_tx] ERROR: Failed to connect to node: {}",
                e
            )));
        }
    }

    let to_address = match Address::new_from_str(to) {
        Ok(a) => a,
        Err(e) => {
            return Err(JsValue::from_str(&format!(
                "[wasm::send_tx] ERROR: Invalid destination address: {}",
                e
            )));
        }
    };

    let tx = match Tx::new(&from_wallet.inner, &to_address, value, utxos) {
        Ok(tx) => tx,
        Err(e) => {
            return Err(JsValue::from_str(&format!(
                "[wasm::send_tx] ERROR: Failed to create tx: {}",
                e
            )));
        }
    };

    let url = format!("{}/tx/send", SEED_API_NODE);

    let tx_json = match TxJson::from_tx(&tx) {
        Ok(tx) => tx,
        Err(e) => {
            return Err(JsValue::from_str(&format!(
                "[wasm::send_tx] ERROR: Failed to serialize tx: {}",
                e
            )));
        }
    };

    match client.post(&url).json(&tx_json).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                return Ok(JsValue::from_str("Transaction successfully sent to node"));
            } else {
                let status = resp.status();
                let error_text = resp.text().await.unwrap_or_default();
                return Err(JsValue::from_str(&format!(
                    "[wasm::send_tx] ERROR: Failed to send transaction: {} - {}",
                    status, error_text
                )));
            }
        }
        Err(e) => {
            return Err(JsValue::from_str(&format!(
                "[wasm::send_tx] ERROR: Error sending request: {}",
                e
            )));
        }
    }
}
