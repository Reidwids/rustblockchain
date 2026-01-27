use libp2p::Multiaddr;

const SEED_P2P_NODES: [&str; 2] = ["/ip4/127.0.0.1/tcp/4000", "/ip4/127.0.0.1/tcp/4001"];
pub fn get_seed_nodes() -> Vec<Multiaddr> {
    SEED_P2P_NODES
        .iter()
        .map(|addr| addr.parse().expect("Invalid Multiaddr"))
        .collect()
}
