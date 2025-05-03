use core_lib::wallet::Wallet;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::to_value;
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[derive(Serialize, Deserialize)]
pub struct JsWallet {
    pub private_key: String,
    pub public_key: String,
}

#[wasm_bindgen]
pub fn create_wallet() -> JsValue {
    let wallet = Wallet::new();

    let js_wallet = JsWallet {
        private_key: wallet.private_key().display_secret().to_string(),
        public_key: wallet.pub_key().to_string(),
    };

    to_value(&js_wallet).unwrap()
}
