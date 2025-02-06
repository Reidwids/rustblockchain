use base58::FromBase58;
use base58::ToBase58;

pub struct Address {
    pub_key_hash: [u8; 20],
    version: u8,
    checksum: [u8; 4],
}

impl Address {
    pub fn pub_key_hash(&self) -> &[u8; 20] {
        &self.pub_key_hash
    }

    /// Create a new Address instance. Provided address must be a string slice of a base58 encoded 25 byte address.
    /// Bytes should take the format: `[[0 version], [1-21 pub key hash], [21-24 checksum]]`
    fn new(addr: &str) -> Address {
        let decoded_addr = addr.from_base58().expect("Invalid Base58 address");

        if decoded_addr.len() != 25 {
            panic!("Invalid address length")
        }

        // Extract version byte (first byte)
        let version = decoded_addr[0];

        // Extract public key hash (next 20 bytes)
        let pub_key_hash = &decoded_addr[1..21]; // The public key hash is 20 bytes
        let mut pub_key_hash_arr = [0u8; 20];
        pub_key_hash_arr.copy_from_slice(pub_key_hash);

        // Extract checksum (last 4 bytes)
        let checksum = &decoded_addr[decoded_addr.len() - 4..];
        let mut checksum_arr = [0u8; 4];
        checksum_arr.copy_from_slice(checksum);

        Address {
            pub_key_hash: pub_key_hash_arr,
            version,
            checksum: checksum_arr,
        }
    }

    /// Concat the address components into a full base58 encoded address
    fn get_address(&self) -> String {
        let full_addr = [
            vec![self.version],
            self.pub_key_hash.to_vec(),
            self.checksum.to_vec(),
        ]
        .concat();

        full_addr.to_base58()
    }
}
