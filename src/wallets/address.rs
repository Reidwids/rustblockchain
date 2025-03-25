use base58::FromBase58;
use base58::ToBase58;
use ripemd::Ripemd160;
use secp256k1::PublicKey;
use sha2::{Digest, Sha256};
use std::error::Error;

const VERSION: u8 = 0;

#[derive(Debug)]
pub struct Address {
    pub_key_hash: [u8; 20],
    version: u8,
    checksum: [u8; 4], // Checksum length of 4 bytes
}

impl Address {
    /// Create a new Address instance. Provided address must be a string slice of a base58 encoded 25 byte address.
    /// Bytes should take the format: `[[0 version], [1-21 pub key hash], [21-24 checksum]]`
    pub fn new_from_str(addr: &str) -> Result<Self, Box<dyn Error>> {
        let decoded_addr = addr.from_base58().map_err(|e| {
            format!(
                "[Address::new_from_str] ERROR: Failed to decode address: {:?}",
                e
            )
        })?;

        if decoded_addr.len() != 25 {
            return Err("[Address::new_from_str] ERROR: Invalid address length".into());
        }

        // Extract version byte (first byte)
        let version = decoded_addr[0];

        // Extract public key hash (next 20 bytes)
        let pub_key_hash: [u8; 20] = decoded_addr[1..21].try_into()?; // The public key hash is 20 bytes

        // Extract checksum (last 4 bytes)
        let checksum: [u8; 4] = decoded_addr[decoded_addr.len() - 4..].try_into()?;

        let target_checksum = Address::calculate_checksum(version, &pub_key_hash);
        if target_checksum != checksum {
            return Err("[Address::new_from_str] ERROR: Checksum is invalid".into());
        }

        Ok(Address {
            pub_key_hash,
            version,
            checksum,
        })
    }

    pub fn new_from_key(pub_key: PublicKey) -> Self {
        let pub_key_hash = hash_pub_key(&pub_key);
        let checksum = Address::calculate_checksum(VERSION, &pub_key_hash);

        Address {
            pub_key_hash,
            version: VERSION,
            checksum,
        }
    }

    pub fn pub_key_hash(&self) -> &[u8; 20] {
        &self.pub_key_hash
    }

    /// Calculates the checksum - first 4 bytes of SHA-256(SHA-256(version + pub_key_hash))
    fn calculate_checksum(version: u8, pub_key_hash: &[u8; 20]) -> [u8; 4] {
        // The checksum helps prevent typos or address corruption.
        // When decoding an address, we recompute the checksum and compare it with the stored one
        // to ensure address integrity

        // Hash the version & pub key hash together
        let mut hasher = Sha256::new();
        hasher.update(&[version]);
        hasher.update(pub_key_hash);
        let hash1 = hasher.finalize();

        // Hash an extra time to improve security, to avoid length extension attacks.
        let mut hasher = Sha256::new();
        hasher.update(&hash1);
        let hash2 = hasher.finalize();

        let mut checksum = [0u8; 4];
        checksum.copy_from_slice(&hash2[..4]);
        checksum
    }

    /// Concat the address components into a full base58 encoded address
    pub fn get_full_address(&self) -> String {
        let full_addr = [
            vec![self.version],
            self.pub_key_hash.to_vec(),
            self.checksum.to_vec(),
        ]
        .concat();

        full_addr.to_base58()
    }
}

/// Hashes a public key using SHA-256 followed by RIPEMD-160
pub fn hash_pub_key(pub_key: &PublicKey) -> [u8; 20] {
    let sha256_hash = Sha256::digest(&pub_key.serialize());
    let ripemd160_hash = Ripemd160::digest(&sha256_hash);
    ripemd160_hash
        .try_into()
        .expect("[Address::hash_pub_key] ERROR: Hash should be 20 bytes")
}
