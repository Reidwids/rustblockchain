use crate::wallet::address::Address;
use secp256k1::ecdsa::Signature;
use secp256k1::PublicKey;

struct TxOutputs {
    outputs: Vec<TxOutput>,
}

struct TxOutput {
    value: u64,             // Value of output tokens in the tx. Outputs cannot be split
    pub_key_hash: [u8; 20], // Recipient pub key (Sha256 + Ripemd160)
}

impl TxOutput {
    /// Creates a new tx output given a value and a recipient address.
    pub fn new(value: u64, addr: Address) -> TxOutput {
        let mut txo = TxOutput {
            value,
            pub_key_hash: [0; 20],
        };
        txo.lock(addr);
        txo
    }

    /// Locks a `txOutput` with the given address
    pub fn lock(&mut self, addr: Address) {
        self.pub_key_hash.copy_from_slice(addr.pub_key_hash());
    }

    /// Returns a boolean representing the comparison of the pub_key_hash to an incoming hash
    pub fn isLockedWithKey(&self, pub_key_hash: [u8; 20]) -> bool {
        pub_key_hash == self.pub_key_hash
    }
}

struct TxInput {
    id: [u8; 32],         // ID of the transaction the output is inside of
    out: u32,             // Index that the output appears within the referenced transaction
    signature: Signature, // Signature created with the senders priv_key proving that they can spend the prev transaction output.
    pub_key: PublicKey, // The spender's public key - used to verify the signature against the pubkeyhash of the last transaction
}
