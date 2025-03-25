use crate::{
    blockchain::{block::Block, transaction::tx::Tx},
    cli::db,
    networking::node::Node,
    wallets::address::bytes_to_hex_string,
};
use libp2p::{
    futures::StreamExt,
    kad::{store::MemoryStore, Behaviour, Event},
    noise,
    swarm::SwarmEvent,
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use std::{collections::HashMap, error::Error, time::Duration};
use tokio::sync::mpsc;

pub enum P2PMessage {
    HealthCheck(),
    BroadcastTx(Tx),
    BroadcastBlock(Block),
}

pub type PeerCollection = HashMap<PeerId, Vec<Multiaddr>>;

pub async fn start_p2p_network(
    mut rx: mpsc::Receiver<P2PMessage>,
    port: Option<u16>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut swarm = setup_swarm(port);
    loop {
        tokio::select! {
                event = swarm.select_next_some() => {
                    match event {

                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("P2P network listening on {address:?}");
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
            Some(message) = rx.recv() => {
                match message {
                    P2PMessage::HealthCheck() => {
                        println!("P2P Channel received msg");
                    }
                    P2PMessage::BroadcastTx(tx) => {
                        println!("Broadcasting transaction: {}", bytes_to_hex_string(&tx.id));
                    }
                    P2PMessage::BroadcastBlock(block) => {
                        println!("Broadcasting block: {}", bytes_to_hex_string(&block.hash));
                    }
                }
            }
        }
    }
}

fn setup_swarm(port: Option<u16>) -> Swarm<Behaviour<MemoryStore>> {
    // Create a unique identifier for the node
    let node = Node::get_or_create_peer_id();
    println!("Local peer ID: {:?}", node.get_peer_id());
    let port = port.unwrap_or(4001);
    let p2p_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", port).parse().unwrap();

    let store = MemoryStore::new(node.get_peer_id().clone());
    // Define network behavior - init kademlia for peer discovery over the web
    let mut kad_behaviour = Behaviour::new(node.get_peer_id().clone(), store);

    // Persist discovered peers in RocksDB for reconnection on restart
    let peer_addresses = db::get_peers();
    for (peer_id, addresses) in peer_addresses {
        for addr in addresses {
            if addr != p2p_addr {
                kad_behaviour.add_address(&peer_id, addr);
            }
        }
    }

    let mut swarm = SwarmBuilder::with_existing_identity(node.get_priv_key().clone())
        .with_tokio()
        .with_tcp(
            tcp::Config::default(), // Configures tcp as the chosen transport layer
            noise::Config::new, // Adds the noise protocol, which adds encryption to tcp connections
            yamux::Config::default, // Multiplexing, allowing simultaneous data streams over a single connection
        )
        .unwrap()
        .with_behaviour(|_| kad_behaviour)
        .unwrap()
        // Extend idle connection time, since nodes may need long connections to propogate txs, sync blocks, etc.
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX)))
        .build();

    // Start listening for connections
    swarm.listen_on(p2p_addr.clone()).unwrap();

    // Dial known seed nodes for initial connectivity
    for seed in get_seed_nodes() {
        match swarm.dial(seed.clone()) {
            Ok(()) => println!("Attempting to connect to seed node at {seed}"),
            Err(e) => eprintln!("Failed to dial seed node at {seed}: {e}"),
        }
    }

    swarm
}

// Once deployed, introduce seed nodes
const SEED_NODES: [&str; 2] = ["/ip4/127.0.0.1/tcp/4001", "/ip4/127.0.0.1/tcp/4002"];
fn get_seed_nodes() -> Vec<Multiaddr> {
    SEED_NODES
        .iter()
        .map(|addr| addr.parse().expect("Invalid Multiaddr"))
        .collect()
}
