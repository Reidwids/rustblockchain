use core::panic;
use libp2p::{
    futures::StreamExt, gossipsub, kad, noise, swarm::SwarmEvent, tcp, yamux, Multiaddr, PeerId,
    Swarm, SwarmBuilder,
};
use log::{error, info, warn};
use std::{
    error::Error,
    str::FromStr,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

use crate::networking::{
    node::Node,
    p2p::{
        handlers::{
            BlockchainBehaviour, BlockchainBehaviourEvent, NewInventory, CHAIN_SYNC_REQ_TOPIC,
        },
        seed_nodes::get_seed_nodes,
    },
};

const MIN_BOOTSTRAP_PEERS: u64 = 1;
const MAX_BOOTSTRAP_TIME: u64 = 5;

pub enum P2Prx {
    BroadcastNewInv(NewInventory),
    HealthCheck(),
}

pub async fn start_p2p_network(
    mut rx: mpsc::Receiver<P2Prx>,
    ready_tx: oneshot::Sender<()>,
    port: u16,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let node = Node::get_or_create_keys();
    let p2p_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port).parse().unwrap();
    let mut state: NodeState = NodeState::Discovering(DiscoveringState {
        deadline: Instant::now() + Duration::from_secs(MAX_BOOTSTRAP_TIME),
        found_chain: false,
        peer_count: 0,
        is_boostrapped: false,
    });

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

    info!("P2P network successfully initialized");
    // let _ = ready_tx.send(());

    // Main event loop
    loop {
        tokio::select! {
            // Handle network events
            event = swarm.select_next_some() => {
                if let Some(next) = state.on_event(&event, &mut swarm) {
                    state = next;
                }
            }

            // Handle local broadcast events
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

#[derive(Debug)]
struct DiscoveringState {
    deadline: Instant,
    peer_count: u64,
    found_chain: bool,
    is_boostrapped: bool,
}
impl DiscoveringState {
    fn on_event(
        &mut self,
        event: &SwarmEvent<BlockchainBehaviourEvent>,
        swarm: &mut Swarm<BlockchainBehaviour>,
    ) -> Option<NodeState> {
        match event {
            SwarmEvent::Behaviour(BlockchainBehaviourEvent::Kademlia(event)) => {
                match event {
                    kad::Event::RoutingUpdated { peer, .. } => {
                        info!("kademlia routing updated for peer: {}", peer);
                        // Bootstrap Kademlia on new connections
                        if self.peer_count > MIN_BOOTSTRAP_PEERS {
                            match swarm.behaviour_mut().bootstrap_kademlia() {
                                Ok(_) => {
                                    self.is_boostrapped = true;
                                    info!("bootstrapped Kademlia DHT");
                                    return Some(NodeState::Syncing(SyncingState {
                                        best_height: 0,
                                        init: false,
                                    }));
                                }
                                Err(e) => {
                                    panic!("failed to bootstrap Kademlia DHT: {:?}", e);
                                }
                            }
                        }
                        return None;
                    }
                    _ => return None,
                }
            }

            // Listen address events (original functionality)
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("listening for p2p events on address {}", address);
                return None;
            }

            // Connection established events - add peer to Kademlia
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                info!("connected to peer: {}", peer_id);
                self.peer_count += 1;
                // Add connected peer to Kademlia routing table
                swarm
                    .behaviour_mut()
                    .add_peer_to_kademlia(&peer_id, endpoint.get_remote_address().clone());
                return None;
            }
            _ => {}
        }

        if Instant::now() > self.deadline {
            info!("No chain discovered — creating genesis");
            create_genesis_block();
            send_api_tx();
            return Some(NodeState::Running(RunningState));
        }
        None
    }
}

#[derive(Debug)]
struct SyncingState {
    best_height: u64,
    init: bool,
}
impl SyncingState {
    fn on_event(
        &mut self,
        event: &SwarmEvent<BlockchainBehaviourEvent>,
        swarm: &mut Swarm<BlockchainBehaviour>,
    ) -> Option<NodeState> {
        match event {
            // Listen address events (original functionality)
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("listening for p2p events on address {}", address);
                return None;
            }

            // Connection established events - add peer to Kademlia
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                info!("connected to peer: {}", peer_id);
                // Add connected peer to Kademlia routing table
                swarm
                    .behaviour_mut()
                    .add_peer_to_kademlia(&peer_id, endpoint.get_remote_address().clone());
                return None;
            }

            // When a subscribed event fires, send a chainsync req
            SwarmEvent::Behaviour(BlockchainBehaviourEvent::Gossipsub(
                gossipsub::Event::Subscribed { peer_id: _, topic },
            )) => {
                if topic.as_str() == CHAIN_SYNC_REQ_TOPIC {
                    if let Err(e) = swarm.behaviour_mut().publish_chainsync_req() {
                        error!("failed to publish chain sync request: {:?}", e);
                    }
                }
            }

            // Handle gossipsub messages (original functionality)
            SwarmEvent::Behaviour(BlockchainBehaviourEvent::Gossipsub(
                gossipsub::Event::Message { message, .. },
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
                            // Once we get the final tip, we can send to the next stage
                            INV_REQ_TOPIC => swarm.behaviour_mut().handle_inventory_req(message),
                            INV_RES_TOPIC => swarm.behaviour_mut().handle_inventory_res(message),
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
            _ => {}
        }
        if !self.init {
            info!("Requesting Chainsync");
            send_chainsync_msg();
        }
        None
    }
}
#[derive(Debug)]
struct RunningState;
impl RunningState {
    fn on_event(
        &mut self,
        event: &SwarmEvent<BlockchainBehaviourEvent>,
        swarm: &mut Swarm<BlockchainBehaviour>,
    ) -> Option<NodeState> {
        match event {
            // Listen address events (original functionality)
            SwarmEvent::NewListenAddr { address, .. } => {
                info!("listening for p2p events on address {}", address);
                return None;
            }

            // Connection established events - add peer to Kademlia
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                info!("connected to peer: {}", peer_id);
                // Add connected peer to Kademlia routing table
                swarm
                    .behaviour_mut()
                    .add_peer_to_kademlia(&peer_id, endpoint.get_remote_address().clone());
                return None;
            }
            _ => {}
        }
        // Add all other events excluded chainsync msgs

        // Start api on init
        None
    }
}

#[derive(Debug)]
enum NodeState {
    Discovering(DiscoveringState),
    Syncing(SyncingState),
    Running(RunningState),
}
impl NodeState {
    fn on_event(
        &mut self,
        event: &SwarmEvent<BlockchainBehaviourEvent>,
        swarm: &mut Swarm<BlockchainBehaviour>,
    ) -> Option<NodeState> {
        match self {
            NodeState::Discovering(s) => s.on_event(event, swarm),
            NodeState::Syncing(s) => s.on_event(event, swarm),
            NodeState::Running(s) => s.on_event(event, swarm),
        }
    }
}
