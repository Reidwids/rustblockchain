use crate::ownership::{node::get_node_id, wallet::WalletStore};

pub fn handle_get_node_id() {
    let node_id = get_node_id();
    println!("Node ID: {}", node_id);
}

pub fn handle_create_wallet() {
    let node_id = get_node_id();
    let mut wallet_store = WalletStore::init_wallet_store(&node_id);
    let addr = wallet_store.add_wallet(&node_id);

    println!("New wallet address: {:?}", addr.get_full_address());
}

pub fn handle_get_wallets() {
    let wallet_store = WalletStore::init_wallet_store(&get_node_id());
    if wallet_store.wallets.is_empty() {
        println!("No wallets found! Try creating a new wallet")
    }

    for (addr, _) in wallet_store.wallets {
        println!("Wallet address: {:?}", addr);
    }
}

pub fn handle_create_blockchain() {
    // create_blockchain(addr);
    // println!("Node ID: {}", node_id);
}
