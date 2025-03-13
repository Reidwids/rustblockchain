use crate::{cli::db, networking::node::Node};
use libp2p::{
    futures::StreamExt,
    kad::{store::MemoryStore, Behaviour, Event},
    noise,
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, SwarmBuilder,
};
use std::{collections::HashMap, error::Error, time::Duration};

pub type PeerCollection = HashMap<PeerId, Vec<Multiaddr>>;

pub async fn start_p2p_network() -> Result<(), Box<dyn Error>> {
    // Create a unique identifier for the node
    let node = Node::get_or_create_peer_id();
    println!("Local peer ID: {:?}", node.get_peer_id());

    let store = MemoryStore::new(node.get_peer_id().clone());
    // Define network behavior - init kademlia for peer discovery over the web
    let mut kad_behaviour = Behaviour::new(node.get_peer_id().clone(), store);

    // Persist discovered peers in RocksDB for reconnection on restart
    let peer_addresses = db::get_peers();
    for (peer_id, addresses) in peer_addresses {
        for addr in addresses {
            kad_behaviour.add_address(&peer_id, addr);
        }
    }

    let mut swarm = SwarmBuilder::with_existing_identity(node.get_priv_key().clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(), // Configures tcp as the chosen transport layer
            noise::Config::new, // Adds the noise protocol, which adds encryption to tcp connections
            yamux::Config::default, // Multiplexing, allowing simultaneous data streams over a single connection
        )?
        .with_behaviour(|_| kad_behaviour)?
        // Extend idle connection time, since nodes may need long connections to propogate txs, sync blocks, etc.
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX)))
        .build();

    // Start listening for connections
    let listen_addr: Multiaddr = "/ip4/0.0.0.0/tcp/4001".parse()?;
    swarm.listen_on(listen_addr.clone())?;

    // Dial known seed nodes for initial connectivity
    for seed in get_seed_nodes() {
        match swarm.dial(seed.clone()) {
            Ok(()) => println!("Attempting to connect to seed node at {seed}"),
            Err(e) => eprintln!("Failed to dial seed node at {seed}: {e}"),
        }
    }

    loop {
        match swarm.select_next_some().await {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {address:?}");
            }
            SwarmEvent::Behaviour(Event::RoutingUpdated {
                peer, addresses, ..
            }) => {
                for addr in addresses.iter() {
                    db::put_peer(peer.clone(), addr.clone());
                }
                println!("Connected to peer: {:?}", peer);
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
}

// Once deployed, introduce seed nodes
// const SEED_NODES: [&str; 2] = [
//     "/ip4/203.0.113.0/tcp/4001",
//     "/ip4/198.51.100.5/tcp/4001",
// ];
fn get_seed_nodes() -> Vec<Multiaddr> {
    // SEED_NODES
    //     .iter()
    //     .map(|addr| addr.parse().expect("Invalid Multiaddr"))
    //     .collect();
    Vec::new()
}

// pub fn broadcast_tx() {}
// pub fn broadcast_block() {}
// pub fn handle_p2p_msg() {}
// pub fn sync_blocks() {}
