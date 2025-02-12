use secp256k1::{PublicKey, Secp256k1, SecretKey};

use super::address::Address;

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
    pub fn get_address(&self) -> Address {
        Address::new_from_key(self.public_key)
    }
}
