use std::{
    collections::HashMap,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};

use super::address::Address;

const WALLET_PATH: &str = "./data/wallet_store.data";

#[derive(Serialize, Deserialize, Debug)]
pub struct Wallet {
    private_key: SecretKey,
    public_key: PublicKey,
}

impl Wallet {
    /// Create new wallet - Creates new pub key and private key
    pub fn new() -> Self {
        let secp = Secp256k1::new();
        let (private_key, public_key) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());

        Wallet {
            private_key,
            public_key,
        }
    }

    /// Gets the full wallet address from a given wallet using the public key
    pub fn get_wallet_address(&self) -> Address {
        Address::new_from_key(self.public_key)
    }

    pub fn pub_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn private_key(&self) -> &SecretKey {
        &self.private_key
    }
}
#[derive(Serialize, Deserialize, Debug)]
pub struct WalletStore {
    pub wallets: HashMap<String, Wallet>,
}

impl WalletStore {
    pub fn save_to_file(&self) -> Result<(), Box<dyn Error>> {
        let path = Path::new(WALLET_PATH);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let encoded: Vec<u8> = bincode::serialize(self)?;
        let mut file = File::create(path)?;
        file.write_all(&encoded)?;
        Ok(())
    }

    /// Get or create an existing wallet store
    pub fn init_wallet_store() -> Result<WalletStore, String> {
        if Path::new(WALLET_PATH).exists() {
            Self::load_from_file().map_err(|e| {
                format!(
                    "[WalletStore::load_from_file] ERROR: Could not load wallet file: {}",
                    e
                )
            })
        } else {
            Ok(WalletStore {
                wallets: HashMap::new(),
            })
        }
    }

    fn load_from_file() -> Result<Self, Box<dyn Error>> {
        // Load file
        let mut file = OpenOptions::new().read(true).open(WALLET_PATH)?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        // Cast to wallets object
        let wallets: WalletStore = bincode::deserialize(&buffer)?;
        Ok(wallets)
    }

    pub fn add_wallet(&mut self) -> Result<Address, String> {
        let new_wallet = Wallet::new();
        let address = new_wallet.get_wallet_address();
        self.wallets.insert(address.get_full_address(), new_wallet);
        self.save_to_file().map_err(|e| {
            format!(
                "[wallet::add_wallet] ERROR: Failed to save new wallet: {}",
                e
            )
        })?;

        Ok(address)
    }

    pub fn get_local_wallet(&self, addr: &Address) -> Result<&Wallet, String> {
        self.wallets.get(&addr.get_full_address()).ok_or_else(|| {
            format!(
                "[wallet::get_local_wallet] ERROR: Wallet not found for address: {}",
                addr.get_full_address()
            )
        })
    }
}
