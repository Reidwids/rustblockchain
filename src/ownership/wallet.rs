use std::{
    collections::HashMap,
    error::Error,
    fs::{File, OpenOptions},
    io::{Read, Write},
};

use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::address::Address;

#[derive(Serialize, Deserialize, Debug)]
struct Wallet {
    private_key: SecretKey,
    public_key: PublicKey,
}

impl Wallet {
    /// Create new wallet - Creates new pub key and private key
    fn new() -> Self {
        let secp = Secp256k1::new();
        let (private_key, public_key) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());

        Wallet {
            private_key,
            public_key,
        }
    }

    /// Get wallet address
    pub fn get_wallet_address(&self) -> Address {
        Address::new_from_key(self.public_key)
    }
}
#[derive(Serialize, Deserialize, Debug)]
struct WalletStore {
    wallets: HashMap<String, Wallet>,
}

impl WalletStore {
    pub fn save_to_file(&self, node_id: &Uuid) -> Result<(), Box<dyn Error>> {
        let encoded: Vec<u8> = bincode::serialize(self)?;
        let mut file = File::create(get_wallet_path(node_id))?;
        file.write_all(&encoded)?;
        Ok(())
    }

    pub fn get_wallets(node_id: &Uuid) -> HashMap<String, Wallet> {
        // In the future, could handle multiple local wallets or a get or create paradigm here
        let wallet_store = Self::load_from_file(node_id)
            .expect("[WalletStore::load_from_file] ERROR: Could not load wallet file");
        wallet_store.wallets
    }

    fn load_from_file(node_id: &Uuid) -> Result<Self, Box<dyn Error>> {
        // Load file
        let mut file = OpenOptions::new()
            .read(true)
            .open(get_wallet_path(node_id))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        // Cast to wallets object
        let wallets: WalletStore = bincode::deserialize(&buffer)?;
        Ok(wallets)
    }

    pub fn add_wallet(&mut self) -> Address {
        let new_wallet = Wallet::new();
        let address = new_wallet.get_wallet_address();
        self.wallets.insert(address.get_full_address(), new_wallet);
        address
    }
}

fn get_wallet_path(node_id: &Uuid) -> String {
    format!("./data/wallet_{}.data", node_id.to_string())
}
