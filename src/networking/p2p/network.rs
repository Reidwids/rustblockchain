use libp2p::{
    futures::StreamExt,
    gossipsub::{self, IdentTopic, Message},
    kad::{self, store::MemoryStore},
    noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use std::{error::Error, str::FromStr};
use tokio::sync::mpsc;

use crate::{
    blockchain::{
        block::Block,
        transaction::{
            mempool::{add_tx_to_mempool, get_tx_from_mempool, mempool_contains_tx},
            tx::Tx,
        },
    },
    cli::db::utxo_set_contains_tx,
    networking::node::Node,
};

// Inventory enum matching your existing type
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum NewInventory {
    Transaction([u8; 32]),
    Block([u8; 32]),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Inventory {
    Transaction(Tx),
    Block(Block),
}

pub enum P2Prx {
    BroadcastNewInv(NewInventory),
    HealthCheck(),
}

pub async fn start_p2p_network(
    mut rx: mpsc::Receiver<P2Prx>,
    port: u16,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let node = Node::get_or_create_keys();
    println!("Local peer id: {}", node.get_peer_id());

    let p2p_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port).parse().unwrap();

    // Build swarm with blockchain behaviour
    let mut swarm = SwarmBuilder::with_existing_identity(node.get_priv_key().clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )
        .unwrap()
        .with_behaviour(|_| BlockchainBehaviour::create())
        .unwrap()
        .build();

    // Listen on a specific port
    swarm.listen_on(p2p_addr.clone()).unwrap();

    // Load and connect to bootstrap nodes
    bootstrap_kademlia(&mut swarm).await;

    // Main event loop
    loop {
        tokio::select! {
            // Handle network events
            event = swarm.select_next_some() => {
                match event {
                    // Handle gossipsub messages (original functionality)
                    SwarmEvent::Behaviour(BlockchainBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message { message, .. }
                    )) => {
                        let topic_str = message.topic.to_string();
                        if topic_str.starts_with("direct:") {
                            let parts: Vec<&str> = topic_str.split(':').collect();

                            if parts.len() < 3 {
                                return Err("[gossipsub::direct] ERROR: Received invalid direct message".into())
                            }
                                let target_peer_id = parts[1];

                                // Check if this message is meant for us
                                if PeerId::from_str(target_peer_id)? == node.get_peer_id().clone() {
                                    match parts[2] {
                                        INV_REQ_TOPIC => {
                                            swarm.behaviour_mut().handle_inventory_req(message)
                                        }
                                        INV_RES_TOPIC => {
                                            swarm.behaviour_mut().handle_inventory_res(message)
                                        }
                                        _ => {}
                                    }
                                }
                        } else {
                            match topic_str.as_str() {
                                NEW_INV_TOPIC => {
                                    swarm.behaviour_mut().handle_new_inventory(message);
                                }
                                _ => {}
                        }
                    }
                }

                    // Handle Kademlia events
                    SwarmEvent::Behaviour(BlockchainBehaviourEvent::Kademlia(event)) => {
                        match event {
                            kad::Event::RoutingUpdated { peer, .. } => {
                                println!("Kademlia routing updated for peer: {}", peer);
                            }
                            _ => {}
                        }
                    }

                    // Listen address events (original functionality)
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("Listening on {}", address);
                    }

                    // Connection established events - add peer to Kademlia
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                        println!("Connected to peer: {}", peer_id);

                        // Add connected peer to Kademlia routing table
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, endpoint.get_remote_address().clone());
                    }
                    _ => {}
                }
            }

            // Handle local broadcast requests
            Some(message) = rx.recv() => {
                match message {
                    P2Prx::BroadcastNewInv(inv) => {
                        // Publish inventory to gossipsub topic (original functionality)
                        if let Err(e) = swarm.behaviour_mut().publish_new_inventory(&inv) {
                            eprintln!("Failed to broadcast inventory: {}", e);
                        }
                    }
                    P2Prx::HealthCheck() => {
                        println!("P2P Channel received health check")
                    }
                }
            }
        }
    }
}

// Custom network behavior with Kademlia added
#[derive(NetworkBehaviour)]
struct BlockchainBehaviour {
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<MemoryStore>,
}

impl BlockchainBehaviour {
    fn create() -> Self {
        let node = Node::get_or_create_keys();
        let peer_id = *node.get_peer_id();

        // Configure gossipsub for gossip msgs between peers
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .max_transmit_size(10 * 1024 * 1024) // 10MB max message size
            .validation_mode(gossipsub::ValidationMode::Strict)
            .build()
            .expect("[network::blockchain_behavior] ERROR: invalid gossipsub config");

        let mut gossipsub_behaviour = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(node.get_priv_key().clone()),
            gossipsub_config,
        )
        .expect("[network::blockchain_behavior] ERROR: invalid gossipsub behavior");

        let topics = get_all_topics(&peer_id);

        for t in topics {
            gossipsub_behaviour
                .subscribe(&t)
                .expect("[network::blockchain_behavior] ERROR: invalid gossipsub behavior");
        }

        // Configure Kademlia
        let store = MemoryStore::new(peer_id);
        let kademlia = kad::Behaviour::new(peer_id, store);

        Self {
            gossipsub: gossipsub_behaviour,
            kademlia,
        }
    }

    // Method to publish inventory
    fn publish_new_inventory(
        &mut self,
        inv: &NewInventory,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Serialize inventory
        let serialized_inv = serde_json::to_vec(inv)?;

        // Publish to topic
        self.gossipsub
            .publish(GossipTopic::NewInv.to_ident_topic(), serialized_inv)?;

        Ok(())
    }

    fn handle_new_inventory(&mut self, message: Message) {
        let requesting_peer = if let Some(peer) = message.source {
            peer
        } else {
            println!("[network::handle_new_inventory] ERROR: Received message without a source.");
            return;
        };

        match serde_json::from_slice::<NewInventory>(&message.data) {
            Ok(inv) => {
                match inv {
                    NewInventory::Transaction(tx_id) => {
                        if !mempool_contains_tx(tx_id)
                            && !utxo_set_contains_tx(tx_id).unwrap_or(false)
                        {
                            if let Err(e) = self.gossipsub.publish(
                                GossipTopic::InvReq(requesting_peer).to_ident_topic(),
                                message.data,
                            ) {
                                println!(
                                    "[network::handle_new_inventory] ERROR: Failed to publish new inventory: {:?}",
                                    e
                                );
                            }
                        }
                    }
                    NewInventory::Block(block_hash) => {
                        // Check if we have the block
                        // If not, request block from sender
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to deserialize inventory data: {}", e);
            }
        }
    }

    // Handle received inventory message
    fn handle_inventory_req(&mut self, message: Message) {
        let requesting_peer = if let Some(peer) = message.source {
            peer
        } else {
            println!("[network::handle_inventory_req] ERROR: Received message without a source.");
            return;
        };

        match serde_json::from_slice::<NewInventory>(&message.data) {
            Ok(inv) => {
                match inv {
                    NewInventory::Transaction(tx_id) => {
                        let tx = if let Some(tx) = get_tx_from_mempool(tx_id) {
                            tx
                        } else {
                            println!(
                                "[network::handle_inventory_req] ERROR: tx not found in mempool."
                            );
                            return;
                        };
                        let serialized_tx = if let Ok(tx) = serde_json::to_vec(&tx) {
                            tx
                        } else {
                            println!(
                                "[network::handle_inventory_req] ERROR: failed to serialize tx"
                            );
                            return;
                        };
                        if let Err(e) = self.gossipsub.publish(
                            GossipTopic::InvRes(requesting_peer).to_ident_topic(),
                            serialized_tx,
                        ) {
                            println!(
                                "[network::handle_inventory_req] ERROR: Failed to publish inventory req: {:?}",
                                e
                            );
                        }
                    }
                    NewInventory::Block(block_id) => {
                        // Recieving request for block.
                        // Send back to requester as inventory res
                        // If not there, do nothing
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to deserialize inventory data: {}", e);
            }
        }
    }

    fn handle_inventory_res(&mut self, message: Message) {
        match serde_json::from_slice::<Inventory>(&message.data) {
            Ok(inv) => {
                match inv {
                    Inventory::Transaction(tx) => {
                        if let Err(e) = add_tx_to_mempool(&tx) {
                            println!("[network::handle_inventory_res] ERROR: failed to add transaction to mempool: {:?}", e);
                            return;
                        }
                    }
                    Inventory::Block(block) => {
                        // Action on the received block - ex. remove txs from mempool
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to deserialize inventory data: {}", e);
            }
        }
    }
}

// Bootstrap Kademlia with configured seed nodes
async fn bootstrap_kademlia(swarm: &mut Swarm<BlockchainBehaviour>) {
    // Get bootstrap nodes
    let bootstrap_nodes = get_seed_nodes();

    // Connect to each bootstrap node. Successful dial actions create a "connection established" event, at which point they're added to kademlia
    for node_addr in bootstrap_nodes {
        match swarm.dial(node_addr.clone()) {
            Ok(_) => println!("Dialed bootstrap node: {}", node_addr),
            Err(e) => eprintln!("Failed to dial bootstrap node {}: {}", node_addr, e),
        }
    }

    // Bootstrap Kademlia DHT
    match swarm.behaviour_mut().kademlia.bootstrap() {
        Ok(_) => println!("Bootstrapping Kademlia DHT"),
        Err(e) => eprintln!("Failed to bootstrap Kademlia DHT: {}", e),
    }
}

// Once deployed, introduce seed nodes (same as before)
const SEED_NODES: [&str; 2] = ["/ip4/127.0.0.1/tcp/4001", "/ip4/127.0.0.1/tcp/4002"];
fn get_seed_nodes() -> Vec<Multiaddr> {
    SEED_NODES
        .iter()
        .map(|addr| addr.parse().expect("Invalid Multiaddr"))
        .collect()
}

// Create topics

const NEW_INV_TOPIC: &str = "new_inv";
const INV_REQ_TOPIC: &str = "inv_req";
const INV_RES_TOPIC: &str = "inv_res";

#[derive(Debug, Clone)]
pub enum GossipTopic {
    NewInv,
    InvReq(PeerId),
    InvRes(PeerId),
}

impl GossipTopic {
    /// Returns the corresponding `IdentTopic`
    pub fn to_ident_topic(&self) -> IdentTopic {
        match self {
            GossipTopic::NewInv => IdentTopic::new(format!("{}", NEW_INV_TOPIC)),
            GossipTopic::InvReq(peer_id) => {
                IdentTopic::new(format!("direct:{}:{}", peer_id, INV_REQ_TOPIC))
            }
            GossipTopic::InvRes(peer_id) => {
                IdentTopic::new(format!("direct:{}:{}", peer_id, INV_RES_TOPIC))
            }
        }
    }
}

/// Returns all topics relevant to the given peer
fn get_all_topics(peer_id: &PeerId) -> Vec<IdentTopic> {
    vec![
        GossipTopic::NewInv.to_ident_topic(),
        GossipTopic::InvReq(peer_id.clone()).to_ident_topic(),
        GossipTopic::InvRes(peer_id.clone()).to_ident_topic(),
    ]
}
