use crate::cli::db::ROCKS_DB;
use libp2p::{identity, PeerId};

pub const NODE_KEY: &str = "node_id";

pub struct Node {
    private_key: identity::Keypair,
    public_key: PeerId,
}

impl Node {
    /// Get or create the local node ID.
    pub fn get_or_create_keys() -> Self {
        // Try to fetch existing node id
        match ROCKS_DB.get(NODE_KEY) {
            Ok(Some(peer_id_privk_bytes)) => {
                // Try to decode using protobuf (matching encoding method)
                match identity::Keypair::from_protobuf_encoding(&peer_id_privk_bytes) {
                    Ok(private_key) => {
                        let public_key = PeerId::from_public_key(&private_key.public());
                        return Self {
                            public_key,
                            private_key,
                        };
                    }
                    Err(_) => {
                        // Continue to keychain creation
                    }
                }
            }
            // Continue to keychain creation
            Ok(None) => {}
            Err(_) => {}
        }

        // Create new key
        let private_key = identity::Keypair::generate_ed25519();
        let public_key = PeerId::from_public_key(&private_key.public());

        // Store using protobuf encoding
        if let Ok(encoded) = private_key.to_protobuf_encoding() {
            let _ = ROCKS_DB.put(NODE_KEY, encoded);
        }

        println!(
            "No local node keys found. Created new peer id: {:?}",
            public_key
        );

        Self {
            public_key,
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
