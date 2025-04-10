use std::collections::HashMap;

use reqwest::Client;
use tokio::sync::mpsc;

use crate::{
    blockchain::{
        block::Block,
        chain::{clear_blockchain, create_blockchain, get_blockchain_json},
        transaction::{
            tx::Tx,
            utxo::{find_utxos_for_addr, reindex_utxos, update_utxos, UTXOSet},
        },
    },
    cli::db,
    networking::{
        node::Node,
        p2p::network::start_p2p_network,
        server::{
            handlers::GetUTXORes,
            req_types::{convert_json_to_utxoset, TxJson},
            rest_api::{start_rest_api, SEED_API_NODE},
        },
    },
    wallets::{
        address::Address,
        wallet::{Wallet, WalletStore},
    },
};

pub fn handle_get_node_id() {
    let node = Node::get_or_create_keys();
    println!("Node ID: {}", node.get_peer_id());
}

pub async fn handle_start_node(rest_api_port: &Option<u16>, p2p_port: &Option<u16>) {
    // Create a channel to pass messages from the server to the p2p network
    let (tx, rx) = mpsc::channel(32);

    // Spawn the P2P network task
    let p2p_port = p2p_port.unwrap_or(4001);
    tokio::spawn(start_p2p_network(rx, p2p_port));

    // Start the HTTP server
    start_rest_api(tx, *rest_api_port).await;
}

pub fn handle_create_wallet() {
    let mut wallet_store = WalletStore::init_wallet_store()
        .expect("[WalletStore::init_wallet_store] Failed to initialize wallet store");
    let addr = wallet_store
        .add_wallet()
        .expect("[WalletStore::add_wallet] Failed to add wallet to wallet store");

    println!("New wallet address: {:?}", addr.get_full_address());
}

pub fn handle_get_wallets() {
    let wallet_store = WalletStore::init_wallet_store()
        .expect("[WalletStore::init_wallet_store] Failed to initialize wallet store");
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
        Some(a) => address = Address::new_from_str(a).unwrap(),
        None => {
            let mut wallet_store = WalletStore::init_wallet_store()
                .expect("[WalletStore::init_wallet_store] Failed to initialize wallet store");
            address = wallet_store
                .add_wallet()
                .expect("[WalletStore::add_wallet] Failed to add wallet to wallet store");
            println!("Wallet address not provided");
            println!(
                "Created new local wallet to receive mining rewards: {}",
                address.get_full_address()
            );
        }
    }
    create_blockchain(&address).unwrap();
    println!("Successfully created blockchain!");
    println!("Mining rewards sent to {}", address.get_full_address());
}

pub fn handle_clear_blockchain() {
    clear_blockchain();
    println!("Blockchain data removed successfully")
}

pub fn handle_print_blockchain(show_txs: bool) {
    let printable_chain = get_blockchain_json(show_txs).unwrap();
    println!(
        "{}",
        serde_json::to_string_pretty(&printable_chain).unwrap()
    );
}

pub fn handle_get_balance(req_addr: &String) {
    // TODO: Refactor to be an API call
    let address = Address::new_from_str(req_addr).unwrap();
    reindex_utxos().unwrap();

    let utxos = find_utxos_for_addr(address.pub_key_hash());

    let mut balance = 0;

    for utxo in utxos {
        balance += utxo.value;
    }

    println!("Address: {}", req_addr);
    println!("Balance: {}", balance);
}

pub async fn handle_send_tx(to: &String, value: u32, from: &Option<String>, _: bool) {
    let client = Client::new();

    let wallet_store = WalletStore::init_wallet_store()
        .expect("[WalletStore::init_wallet_store] Failed to initialize wallet store");
    let from_wallet: &Wallet;
    match from {
        Some(addr) => {
            from_wallet = wallet_store.wallets.get(addr).expect(
                "[handlers::handle_send_tx] ERROR: No local wallet found for given from address",
            );
        }
        None => {
            let first_wallet = wallet_store.wallets.iter().next();
            println!("From wallet not provided, using first local wallet");
            match first_wallet {
                Some((_, wallet)) => {
                    from_wallet = wallet;
                    println!(
                        "First local wallet: {}",
                        from_wallet.get_wallet_address().get_full_address()
                    )
                }
                None => panic!("[handlers::handle_send_tx] ERROR: No local wallets found"),
            }
        }
    }

    let from_address = from_wallet.get_wallet_address();

    let url = format!(
        "{}/utxo?address={}&amount={}",
        SEED_API_NODE,
        from_address.get_full_address(),
        value
    );

    let mut utxos: UTXOSet = HashMap::new();

    match client.get(url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<GetUTXORes>().await {
                    Ok(data) => match convert_json_to_utxoset(&data.utxos) {
                        Ok(set) => {
                            utxos = set;
                        }
                        Err(e) => {
                            println!("Failed to convert UTXO JSON to UTXOSet: {:?}", e);
                            return;
                        }
                    },
                    Err(e) => {
                        println!("Failed to parse UTXO response: {:?}", e);
                        return;
                    }
                }
            } else {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                println!(
                    "Failed to fetch UTXOs from node: {} - {}",
                    status, error_text
                );
                return;
            }
        }
        Err(e) => {
            println!("Failed to connect to node: {:?}", e);
            return;
        }
    }

    let to_address = match Address::new_from_str(to.as_str()) {
        Ok(a) => a,
        Err(e) => {
            println!("Invalid destination address: {:?}", e);
            return;
        }
    };

    let tx = match Tx::new(from_wallet, &to_address, value, utxos) {
        Ok(tx) => tx,
        Err(e) => {
            println!("Failed to create tx: {:?}", e);
            return;
        }
    };

    let url = format!("{}/tx/send", SEED_API_NODE);

    let tx_json = match TxJson::from_tx(&tx) {
        Ok(tx) => tx,
        Err(e) => {
            println!("Failed to serialize tx: {:?}", e);
            return;
        }
    };

    match client.post(&url).json(&tx_json).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                println!("Transaction successfully sent to node");
            } else {
                let status = resp.status();
                let error_text = resp.text().await.unwrap_or_default();
                println!("Failed to send transaction: {} - {}", status, error_text);
            }
        }
        Err(e) => {
            println!("Error sending request: {:?}", e);
        }
    }

    // if mine {
    //     let mut new_block = Block::new(&from_wallet.get_wallet_address()).unwrap();
    //     new_block.mine().unwrap();
    //     update_utxos(&new_block).unwrap();
    //     db::reset_mempool();
    // }
}

pub fn handle_mine(reward_addr: &Option<String>) {
    let wallet_store = WalletStore::init_wallet_store()
        .expect("[WalletStore::init_wallet_store] Failed to initialize wallet store");
    let from_wallet: &Wallet;
    match reward_addr {
        Some(addr) => {
            from_wallet = wallet_store.wallets.get(addr).expect(
                "[handlers::handle_mine] ERROR: No local wallet found for given from address",
            );
        }
        None => {
            let first_wallet = wallet_store.wallets.iter().next();
            println!("Wallet address not provided, using first local wallet");
            match first_wallet {
                Some((_, wallet)) => {
                    from_wallet = wallet;
                    println!(
                        "First local wallet: {}",
                        from_wallet.get_wallet_address().get_full_address()
                    )
                }
                None => panic!("[handlers::handle_mine] ERROR: No local wallets found"),
            }
        }
    }

    let mempool = db::get_mempool();
    if mempool.len() == 0 {
        println!("No transactions to mine in mempool!");
        return;
    }

    let mut new_block = Block::new(&from_wallet.get_wallet_address()).unwrap();
    new_block.mine().unwrap();
    update_utxos(&new_block).unwrap();
    db::reset_mempool();
}
