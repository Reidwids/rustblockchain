use crate::{
    blockchain::{
        chain::{clear_blockchain, create_blockchain},
        transaction::utxo::{find_utxos, reindex_utxos},
    },
    ownership::{address::Address, node::get_node_id, wallet::WalletStore},
};

pub fn handle_get_node_id() {
    let node_id = get_node_id();
    println!("Node ID: {}", node_id);
}

pub fn handle_create_wallet() {
    let mut wallet_store = WalletStore::init_wallet_store();
    let addr = wallet_store.add_wallet();

    println!("New wallet address: {:?}", addr.get_full_address());
}

pub fn handle_get_wallets() {
    let wallet_store = WalletStore::init_wallet_store();
    if wallet_store.wallets.is_empty() {
        println!("No wallets found! Try creating a new wallet")
    }
    for (addr, _) in wallet_store.wallets {
        println!("Wallet address: {:?}", addr);
    }
}

pub fn handle_create_blockchain(req_addr: &Option<String>) {
    let address: Address;
    match req_addr {
        Some(a) => address = Address::new_from_str(a),
        None => {
            let mut wallet_store = WalletStore::init_wallet_store();
            address = wallet_store.add_wallet();
            println!("Wallet address not provided");
            println!(
                "Created new local wallet to receive mining rewards: {}",
                address.get_full_address()
            );
        }
    }
    create_blockchain(&address);
    println!("Successfully created blockchain!");
    println!("Mining rewards sent to {}", address.get_full_address());
}

pub fn handle_clear_blockchain() {
    clear_blockchain();
    println!("Blockchain data removed successfully")
}

pub fn handle_get_balance(req_addr: &String) {
    let address = Address::new_from_str(req_addr);
    reindex_utxos();

    let utxos = find_utxos(address.pub_key_hash());

    let mut balance = 0;

    for utxo in utxos {
        balance += utxo.value;
    }

    println!("Address: {}", req_addr);
    println!("Balance: {}", balance);
}
