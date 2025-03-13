use crate::cli::db::ROCKS_DB;
use libp2p::{identity, PeerId};

pub const NODE_KEY: &str = "node_id";

pub struct Node {
    private_key: identity::Keypair,
    public_key: PeerId,
}

impl Node {
    /// Get or create the local node ID.
    pub fn get_or_create_peer_id() -> Self {
        // Try to fetch existing node id
        if let Ok(Some(peer_id_privk_bytes)) = ROCKS_DB.get(NODE_KEY) {
            if let Ok(private_key) = identity::Keypair::ed25519_from_bytes(peer_id_privk_bytes) {
                return Self {
                    public_key: PeerId::from_public_key(&private_key.public()),
                    private_key,
                };
            }
        }

        // Else create the node id
        let private_key = identity::Keypair::generate_ed25519();
        let encoded = private_key
            .to_protobuf_encoding()
            .expect("[node::get_or_create_peer_id] ERROR: Failed to encode node private key");

        // Store it in RocksDB
        ROCKS_DB
            .put(NODE_KEY, encoded)
            .expect("[node::get_or_create_peer_id] ERROR: Failed to store node id in RocksDB");

        Self {
            public_key: PeerId::from_public_key(&private_key.public()),
            private_key,
        }
    }

    pub fn get_peer_id(&self) -> &PeerId {
        &self.public_key
    }

    pub fn get_priv_key(&self) -> &identity::Keypair {
        &self.private_key
    }
}
