use core_lib::tx::Tx;
use libp2p::{
    futures::StreamExt,
    gossipsub::{self, IdentTopic, Message},
    kad::{self, store::MemoryStore},
    noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::{error::Error, str::FromStr};
use tokio::sync::mpsc;

use crate::{
    blockchain::{
        blocks::block::{get_blocks_since_height, Block},
        chain::{clear_blockchain, commit_block, get_last_block},
        transaction::{
            mempool::{
                add_tx_to_mempool, get_tx_from_mempool, mempool_contains_tx, mempool_contains_txo,
            },
            tx::TxVerify,
        },
    },
    cli::db::{get_block, utxo_set_contains_tx},
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
    info!("local peer id: {}", node.get_peer_id());

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

    // Get bootstrap nodes
    let bootstrap_nodes = get_seed_nodes();

    // Connect to each bootstrap node. Successful dial actions create a "connection established" event, at which point they're added to kademlia
    for node_addr in bootstrap_nodes {
        match swarm.dial(node_addr.clone()) {
            Ok(_) => info!("dialed bootstrap node: {}", node_addr),
            Err(e) => error!("failed to dial bootstrap node {}: {:?}", node_addr, e),
        }
    }

    // Main event loop
    loop {
        tokio::select! {
            // Handle network events
            event = swarm.select_next_some() => {
                match event {
                    SwarmEvent::Behaviour(BlockchainBehaviourEvent::Gossipsub(
                        gossipsub::Event::Subscribed { peer_id: _, topic } ))=> {
                        if topic.as_str() == CHAIN_SYNC_REQ_TOPIC {
                            if let Err(e) = swarm.behaviour_mut().publish_chainsync_req() {
                                error!("failed to publish chain sync request: {:?}", e);
                            }
                        }
                    }
                    // Handle gossipsub messages (original functionality)
                    SwarmEvent::Behaviour(BlockchainBehaviourEvent::Gossipsub(
                        gossipsub::Event::Message { message, .. }
                    )) => {
                        let topic_str = message.topic.to_string();

                        // --- HANDLERS FOR ALL DIRECT MSGS --- //
                        if topic_str.starts_with("direct:") {
                            let parts: Vec<&str> = topic_str.split(':').collect();

                            if parts.len() < 3 {
                                warn!("received invalid direct message: {}", topic_str);
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
                                        CHAIN_SYNC_RES_TOPIC => {
                                            swarm.behaviour_mut().handle_chainsync_res(message)
                                        }
                                        _ => {}
                                    }
                                }
                        } else {
                            // ----- HANDLERS FOR GOSSIP MSGS ----- //
                            match topic_str.as_str() {
                                NEW_INV_TOPIC => {
                                    swarm.behaviour_mut().handle_new_inventory(message);
                                }
                                CHAIN_SYNC_REQ_TOPIC => {
                                    swarm.behaviour_mut().handle_chainsync_req(message);
                                }
                                _ => {}
                        }
                    }
                }

                    // Handle Kademlia events
                    SwarmEvent::Behaviour(BlockchainBehaviourEvent::Kademlia(event)) => {
                        match event {
                            kad::Event::RoutingUpdated { peer, .. } => {
                                info!("kademlia routing updated for peer: {}", peer);
                                // Bootstrap Kademlia on new connections
                                match swarm.behaviour_mut().kademlia.bootstrap() {
                                    Ok(_) => {
                                        info!("bootstrapped Kademlia DHT");
                                    },
                                    Err(e) => error!("failed to bootstrap Kademlia DHT: {:?}", e),
                                }
                            }
                            _ => {}
                        }
                    }

                    // Listen address events (original functionality)
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("listening for p2p events on address {}", address);
                    }

                    // Connection established events - add peer to Kademlia
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                        info!("connected to peer: {}", peer_id);

                        // Add connected peer to Kademlia routing table
                        swarm.behaviour_mut().kademlia.add_address(&peer_id, endpoint.get_remote_address().clone());
                    }
                    _ => {}
                }
            }

            // ----- HANDLERS FOR LOCAL BROADCAST REQUESTS ----- //
            Some(message) = rx.recv() => {
                match message {
                    P2Prx::BroadcastNewInv(inv) => {
                        // Publish inventory to gossipsub topic (original functionality)
                        if let Err(e) = swarm.behaviour_mut().publish_new_inventory(&inv) {
                            error!("failed to broadcast inventory: {}", e);
                        }
                    }
                    P2Prx::HealthCheck() => {
                        info!("P2P Channel received health check")
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
            .unwrap_or_else(|e| {
                error!("invalid gossipsub config: {:?}", e);
                std::process::exit(1);
            });

        let mut gossipsub_behaviour = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(node.get_priv_key().clone()),
            gossipsub_config,
        )
        .unwrap_or_else(|e| {
            error!("invalid gossipsub behavior: {:?}", e);
            std::process::exit(1);
        });

        let topics = get_all_topics(&peer_id);

        for t in topics {
            gossipsub_behaviour.subscribe(&t).unwrap_or_else(|e| {
                error!("invalid gossipsub behavior: {:?}", e);
                std::process::exit(1);
            });
        }

        // Configure Kademlia
        let store = MemoryStore::new(peer_id);
        let kademlia = kad::Behaviour::new(peer_id, store);

        Self {
            gossipsub: gossipsub_behaviour,
            kademlia,
        }
    }

    // Method to publish inventory to all peers
    fn publish_new_inventory(
        &mut self,
        inv: &NewInventory,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Serialize inventory
        let serialized_inv = serde_json::to_vec(inv)?;

        // Publish to topic
        self.gossipsub
            .publish(GossipTopic::NewInv.to_ident_topic(), serialized_inv)?;

        info!("broadcasted inventory message to network!");
        Ok(())
    }

    // Method to publish chainsync request to all peers
    fn publish_chainsync_req(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Send chain height
        let height = match get_last_block() {
            Ok(b) => b.height,
            Err(_) => {
                error!("failed to find latest block - refreshing blockchain");
                clear_blockchain();
                0
            }
        };

        let serialized = serde_json::to_vec(&height)?;

        // Publish to topic
        self.gossipsub
            .publish(GossipTopic::ChainSyncReq.to_ident_topic(), serialized)?;

        info!("broadcasted chainsync message to network!");
        Ok(())
    }

    fn handle_new_inventory(&mut self, message: Message) {
        info!("received inventory message from network");
        let requesting_peer = if let Some(peer) = message.source {
            peer
        } else {
            error!("received message without a source.");
            return;
        };

        match serde_json::from_slice::<NewInventory>(&message.data) {
            Ok(inv) => match inv {
                NewInventory::Transaction(tx_id) => {
                    if !mempool_contains_tx(tx_id) && !utxo_set_contains_tx(tx_id).unwrap_or(false)
                    {
                        match self.gossipsub.publish(
                            GossipTopic::InvReq(requesting_peer).to_ident_topic(),
                            message.data,
                        ) {
                            Err(e) => error!("failed to publish inventory request: {:?}", e),
                            Ok(_) => {
                                info!("tx not found in chain - requesting tx from sender...",)
                            }
                        }
                    }
                }
                NewInventory::Block(block_hash) => match get_block(&block_hash) {
                    Ok(None) => {
                        match self.gossipsub.publish(
                            GossipTopic::InvReq(requesting_peer).to_ident_topic(),
                            message.data,
                        ) {
                            Err(e) => error!("failed to publish inventory request: {:?}", e),
                            Ok(_) => {
                                info!("block not found in chain - requesting block from sender...",)
                            }
                        }
                    }
                    Ok(Some(_)) => {}
                    Err(e) => error!("{}", e),
                },
            },
            Err(e) => {
                error!("failed to deserialize inventory data: {}", e);
            }
        }
    }

    // Handle received inventory message
    fn handle_inventory_req(&mut self, message: Message) {
        let requesting_peer = if let Some(peer) = message.source {
            info!("received inventory request from peer: {:?}", peer);
            peer
        } else {
            error!("received message from an unknown peer");
            return;
        };

        match serde_json::from_slice::<NewInventory>(&message.data) {
            Ok(inv) => {
                match inv {
                    NewInventory::Transaction(tx_id) => {
                        let tx = if let Some(tx) = get_tx_from_mempool(tx_id) {
                            tx
                        } else {
                            error!("tx not found in mempool");
                            return;
                        };
                        let inventory = Inventory::Transaction(tx);
                        let serialized_tx = if let Ok(bytes) = serde_json::to_vec(&inventory) {
                            bytes
                        } else {
                            error!("failed to serialize inventory");
                            return;
                        };
                        match self.gossipsub.publish(
                            GossipTopic::InvRes(requesting_peer).to_ident_topic(),
                            serialized_tx,
                        ) {
                            Err(e) => error!("failed to publish inventory req: {:?}", e),
                            Ok(_) => info!("sending tx record to peer: {:?}", requesting_peer),
                        }
                    }
                    NewInventory::Block(block_hash) => {
                        // Recieving request for block.
                        // Send back to requester as inventory res
                        // If not there, do nothing
                        let block = if let Ok(Some(b)) = get_block(&block_hash) {
                            b
                        } else {
                            error!("block not found in local chain");
                            return;
                        };
                        let inventory = Inventory::Block(block);
                        let serialized_block = if let Ok(bytes) = serde_json::to_vec(&inventory) {
                            bytes
                        } else {
                            error!("failed to serialize inventory");
                            return;
                        };
                        match self.gossipsub.publish(
                            GossipTopic::InvRes(requesting_peer).to_ident_topic(),
                            serialized_block,
                        ) {
                            Err(e) => error!("failed to publish inventory req: {:?}", e),
                            Ok(_) => info!("sending block record to peer: {:?}", requesting_peer),
                        }
                    }
                }
            }
            Err(e) => {
                error!("failed to deserialize inventory data: {}", e);
            }
        }
    }

    fn handle_inventory_res(&mut self, message: Message) {
        info!("inventory record successfully retrieved");
        match serde_json::from_slice::<Inventory>(&message.data) {
            Ok(inv) => {
                match inv {
                    Inventory::Transaction(tx) => {
                        match tx.verify() {
                            Ok(v) => {
                                if !v {
                                    warn!("transaction verification failed for tx {:?}", tx.id);
                                    return;
                                }
                            }
                            Err(e) => {
                                error!("encountered error while verifying tx {:?}: {:?}", tx.id, e);
                                return;
                            }
                        };

                        // Ensure no txs are double spent
                        for tx_input in &tx.inputs {
                            if mempool_contains_txo(tx_input.prev_tx_id, tx_input.out) {
                                warn!("tx contains outputs spent in mempool");
                                return;
                            }
                        }

                        match add_tx_to_mempool(&tx) {
                            Err(e) => error!("failed to add transaction to mempool: {:?}", e),
                            Ok(_) => info!("tx was successfully committed to the mempool"),
                        }
                    }
                    Inventory::Block(block) => match commit_block(&block) {
                        Ok(_) => {}
                        Err(e) => error!("failed to commit block: {:?}", e),
                    },
                }
            }
            Err(e) => {
                error!("failed to deserialize inventory data: {:?}", e);
            }
        }
    }

    fn handle_chainsync_req(&mut self, message: Message) {
        let requesting_peer = if let Some(peer) = message.source {
            info!("received chainsync request from peer: {:?}", peer);
            peer
        } else {
            error!("received message from an unknown peer");
            return;
        };

        let height = match serde_json::from_slice::<u32>(&message.data) {
            Ok(h) => h,
            Err(e) => {
                error!("failed to deserialize height data: {:?}", e);
                return;
            }
        };

        let blocks = match get_blocks_since_height(height) {
            Ok(h) => h,
            Err(e) => {
                error!("failed to handle chainsync request: {:?}", e);
                return;
            }
        };

        let block_hashes: Vec<[u8; 32]> = blocks.iter().map(|b| b.hash).collect();
        let payload = if let Ok(bytes) = serde_json::to_vec(&block_hashes) {
            bytes
        } else {
            error!("failed to serialize block hashes");
            return;
        };
        match self.gossipsub.publish(
            GossipTopic::ChainSyncRes(requesting_peer).to_ident_topic(),
            payload,
        ) {
            Err(e) => error!("failed to publish chainsync res: {:?}", e),
            Ok(_) => info!(
                "sending chainsync block hashes to peer: {:?}",
                requesting_peer
            ),
        }
    }

    fn handle_chainsync_res(&mut self, message: Message) {
        let requesting_peer = if let Some(peer) = message.source {
            info!("received chainsync response from peer: {:?}", peer);
            peer
        } else {
            error!("received message from an unknown peer");
            return;
        };

        match serde_json::from_slice::<Vec<[u8; 32]>>(&message.data) {
            Ok(block_hashes) => {
                for block_hash in block_hashes {
                    let inventory = NewInventory::Block(block_hash);
                    let serialized_bh = if let Ok(bytes) = serde_json::to_vec(&inventory) {
                        bytes
                    } else {
                        error!("failed to serialize inventory");
                        return;
                    };
                    match self.gossipsub.publish(
                        GossipTopic::InvReq(requesting_peer).to_ident_topic(),
                        serialized_bh,
                    ) {
                        Err(e) => error!("failed to publish new inventory: {:?}", e),
                        Ok(_) => info!("requesting blocks from sender...",),
                    }
                }
            }
            Err(e) => {
                error!("failed to deserialize blockhash data: {}", e);
            }
        }
    }
}

// Once deployed, introduce seed nodes (same as before)
const SEED_P2P_NODES: [&str; 2] = ["/ip4/127.0.0.1/tcp/4000", "/ip4/127.0.0.1/tcp/4001"];
fn get_seed_nodes() -> Vec<Multiaddr> {
    SEED_P2P_NODES
        .iter()
        .map(|addr| addr.parse().expect("Invalid Multiaddr"))
        .collect()
}

// Create topics
const NEW_INV_TOPIC: &str = "new_inv";
const INV_REQ_TOPIC: &str = "inv_req";
const INV_RES_TOPIC: &str = "inv_res";
const CHAIN_SYNC_REQ_TOPIC: &str = "chain_sync_req";
const CHAIN_SYNC_RES_TOPIC: &str = "chain_sync_res";

#[derive(Debug, Clone)]
pub enum GossipTopic {
    NewInv,
    InvReq(PeerId),
    InvRes(PeerId),
    ChainSyncReq,
    ChainSyncRes(PeerId),
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
            GossipTopic::ChainSyncReq => IdentTopic::new(format!("{}", CHAIN_SYNC_REQ_TOPIC)),
            GossipTopic::ChainSyncRes(peer_id) => {
                IdentTopic::new(format!("direct:{}:{}", peer_id, CHAIN_SYNC_RES_TOPIC))
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
        GossipTopic::ChainSyncReq.to_ident_topic(),
        GossipTopic::ChainSyncRes(peer_id.clone()).to_ident_topic(),
    ]
}
