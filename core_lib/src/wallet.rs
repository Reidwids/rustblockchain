use std::str::FromStr;

use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use std::error::Error;

use crate::address::Address;

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
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

    pub fn from_keys(pub_key: String, priv_key: String) -> Result<Self, Box<dyn Error>> {
        let private_key = SecretKey::from_str(&priv_key)?;
        let public_key = PublicKey::from_str(&pub_key)?;

        Ok(Wallet {
            private_key,
            public_key,
        })
    }
}
