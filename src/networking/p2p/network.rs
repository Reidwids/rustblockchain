use libp2p::{
    futures::StreamExt,
    gossipsub::{self, IdentTopic},
    identity::Keypair,
    noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::sync::mpsc;

use crate::cli::db;

// Inventory enum matching your existing type
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum Inventory {
    Transaction([u8; 32]),
    Block([u8; 32]),
}

pub enum P2PMessage {
    BroadcastInv(Inventory),
    HealthCheck(),
}

pub async fn start_p2p_network(
    local_key: Keypair,
    mut rx: mpsc::Receiver<P2PMessage>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // let port = port.unwrap_or(4001);
    let port = 4001;
    let p2p_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port).parse().unwrap();
    // Build swarm with blockchain behaviour
    let mut swarm = SwarmBuilder::with_existing_identity(local_key.clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(|key| BlockchainBehaviour::create(key))
        .unwrap()
        .build();

    // Listen on a specific port
    swarm.listen_on(p2p_addr.clone()).unwrap();

    // Load and connect to bootstrap nodes
    let bootstrap_nodes = get_seed_nodes();
    for node in bootstrap_nodes {
        let _ = swarm.dial(node);
    }

    loop {
        tokio::select! {
            // Handle network events
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::Behaviour(BlockchainBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message { message, .. }
                    )) => {
                        // Attempt to deserialize received message
                        if let Ok(inv) = serde_json::from_slice::<Inventory>(&message.data) {
                            // Handle the received inventory
                            swarm.behaviour_mut().handle_inventory_message(inv);
                        }
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("Listening on {}", address);
                    }
                    SwarmEvent::ConnectionEstablished {
                        peer_id, endpoint, ..
                    } => {
                        println!("Connection established with {peer_id} at {:?}", endpoint);
                        db::put_peer(peer_id, endpoint.get_remote_address().clone());
                    }
                    _ => {}
                }
            }

            // Handle local broadcast requests
            Some(message) = rx.recv() => {
                match message {
                    P2PMessage::BroadcastInv(inv) => {
                        // Publish inventory to gossipsub topic
                        if let Err(e) = swarm.behaviour_mut().publish_inventory(&inv) {
                            eprintln!("Failed to broadcast inventory: {}", e);
                        }
                    }
                    P2PMessage::HealthCheck() => {
                        println!("P2P Channel received msg")
                    }
                }
            }
        }
    }
}

// Custom network behavior with composition
#[derive(NetworkBehaviour)]
pub struct BlockchainBehaviour {
    pub gossipsub: gossipsub::Behaviour,
}

impl BlockchainBehaviour {
    pub fn create(local_key: &Keypair) -> Self {
        // Configure gossipsub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .max_transmit_size(10 * 1024 * 1024) // 10MB max message size
            .validation_mode(gossipsub::ValidationMode::Strict)
            .build()
            .expect("Valid gossipsub config");

        let mut behaviour = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
        .expect("Valid gossipsub behaviour");

        // Create and subscribe to blockchain inventory topic
        let blockchain_topic = IdentTopic::new("blockchain_inventory");
        behaviour
            .subscribe(&blockchain_topic)
            .expect("Can subscribe to topic");

        Self {
            gossipsub: behaviour,
        }
    }

    // Method to publish inventory
    pub fn publish_inventory(&mut self, inv: &Inventory) -> Result<(), Box<dyn std::error::Error>> {
        let topic = IdentTopic::new("blockchain_inventory");

        // Serialize inventory
        let serialized_inv = serde_json::to_vec(inv)?;

        // Publish to topic
        self.gossipsub.publish(topic, serialized_inv)?;

        Ok(())
    }

    // Handle received inventory message
    pub fn handle_inventory_message(&mut self, inv: Inventory) -> bool {
        // Check if this inventory item is new
        // Process the new inventory item
        match inv {
            Inventory::Transaction(tx_hash) => {
                // Example: Check if transaction exists in mempool/database
                if !self.transaction_exists(&tx_hash) {
                    println!("New transaction inventory received: {:?}", tx_hash);

                    // TODO: check if tx exists already

                    // Trigger transaction retrieval
                    self.request_transaction_data(tx_hash);

                    return true;
                }
            }
            Inventory::Block(block_hash) => {
                // Similar logic for blocks
                if !self.block_exists(&block_hash) {
                    println!("New block inventory received: {:?}", block_hash);

                    // TODO: check if tx exists already

                    // Trigger block retrieval
                    self.request_block_data(block_hash);

                    return true;
                }
            }
        }
        false
    }

    // Mock methods - replace with actual database/mempool checks
    fn transaction_exists(&self, tx_hash: &[u8; 32]) -> bool {
        // Placeholder - replace with actual implementation
        false
    }

    fn block_exists(&self, block_hash: &[u8; 32]) -> bool {
        // Placeholder - replace with actual implementation
        false
    }

    // Mock methods for data retrieval
    fn request_transaction_data(&self, tx_hash: [u8; 32]) {
        // Trigger transaction data retrieval
        println!("Requesting transaction data for {:?}", tx_hash);
    }

    fn request_block_data(&self, block_hash: [u8; 32]) {
        // Trigger block data retrieval
        println!("Requesting block data for {:?}", block_hash);
    }
}

// Once deployed, introduce seed nodes
const SEED_NODES: [&str; 2] = ["/ip4/127.0.0.1/tcp/4001", "/ip4/127.0.0.1/tcp/4002"];
fn get_seed_nodes() -> Vec<Multiaddr> {
    SEED_NODES
        .iter()
        .map(|addr| addr.parse().expect("Invalid Multiaddr"))
        .collect()
}
