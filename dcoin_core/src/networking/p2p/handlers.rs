// pub fn handle_broadcast_inv() {}
// pub fn handle_broadcast_tx() {}
// pub fn handle_broadcast_block() {}
// pub fn handle_receive_inv() {}
// pub fn handle_receive_tx() {}
// pub fn handle_receive_block() {}
// pub fn sync_blocks() {}
// - handle block race
// - handle bad txs

use core_lib::tx::Tx;
use libp2p::{
    gossipsub::{self, IdentTopic, Message},
    kad::{self, store::MemoryStore, NoKnownPeers, QueryId, RoutingUpdate},
    swarm::NetworkBehaviour,
    Multiaddr, PeerId,
};
use log::{error, info, warn};
use serde::{Deserialize, Serialize};

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
    db::rocks::{get_block, utxo_set_contains_tx},
    networking::node::Node,
};

// Create topics
pub const NEW_INV_TOPIC: &str = "new_inv";
pub const INV_REQ_TOPIC: &str = "inv_req";
pub const INV_RES_TOPIC: &str = "inv_res";
pub const CHAIN_SYNC_REQ_TOPIC: &str = "chain_sync_req";
pub const CHAIN_SYNC_RES_TOPIC: &str = "chain_sync_res";

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

// Inventory enum matching your existing type
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum NewInventory {
    TransactionID([u8; 32]),
    BlockID([u8; 32]),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Inventory {
    Transaction(Tx),
    Block(Block),
}

// Custom network behavior with Kademlia added
#[derive(NetworkBehaviour)]
pub struct BlockchainBehaviour {
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<MemoryStore>,
}

impl BlockchainBehaviour {
    pub fn create() -> Self {
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

    pub fn bootstrap_kademlia(&mut self) -> Result<QueryId, NoKnownPeers> {
        self.kademlia.bootstrap()
    }
    pub fn add_peer_to_kademlia(&mut self, peer: &PeerId, address: Multiaddr) -> RoutingUpdate {
        self.kademlia.add_address(peer, address)
    }
    // Method to publish inventory to all peers
    pub fn publish_new_inventory(
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
    pub fn publish_chainsync_req(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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

    pub fn handle_new_inventory(&mut self, message: &Message) {
        info!("received inventory message from network");
        let requesting_peer = if let Some(peer) = message.source {
            peer
        } else {
            error!("received message without a source.");
            return;
        };

        match serde_json::from_slice::<NewInventory>(&message.data) {
            Ok(inv) => match inv {
                NewInventory::TransactionID(tx_id) => {
                    if !mempool_contains_tx(tx_id) && !utxo_set_contains_tx(tx_id).unwrap_or(false)
                    {
                        match self.gossipsub.publish(
                            GossipTopic::InvReq(requesting_peer).to_ident_topic(),
                            message.data.clone(),
                        ) {
                            Err(e) => error!("failed to publish inventory request: {:?}", e),
                            Ok(_) => {
                                info!("tx not found in chain - requesting tx from sender...",)
                            }
                        }
                    }
                }
                NewInventory::BlockID(block_hash) => match get_block(&block_hash) {
                    Ok(None) => {
                        match self.gossipsub.publish(
                            GossipTopic::InvReq(requesting_peer).to_ident_topic(),
                            message.data.clone(),
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
    pub fn handle_inventory_req(&mut self, message: &Message) {
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
                    NewInventory::TransactionID(tx_id) => {
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
                    NewInventory::BlockID(block_hash) => {
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

    pub fn handle_inventory_res(&mut self, message: &Message) {
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

    pub fn handle_chainsync_req(&mut self, message: &Message) {
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

    pub fn handle_chainsync_res(&mut self, message: &Message) {
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
                    let inventory = NewInventory::BlockID(block_hash);
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
