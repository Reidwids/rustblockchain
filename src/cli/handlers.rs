use tokio::sync::mpsc;

use crate::{
    blockchain::{
        block::Block,
        chain::{clear_blockchain, create_blockchain, get_last_block},
        transaction::{
            tx::Tx,
            utxo::{find_utxos, reindex_utxos, update_utxos},
        },
    },
    cli::db,
    networking::{node::Node, p2p::start_p2p_network, rest_api::start_rest_api},
    wallets::{
        address::{bytes_to_hex_string, Address},
        wallet::{Wallet, WalletStore},
    },
};

use super::db::get_block;

pub fn handle_get_node_id() {
    let node = Node::get_or_create_peer_id();
    println!("Node ID: {}", node.get_peer_id());
}

pub async fn handle_start_node(rest_api_port: &Option<u16>, p2p_port: &Option<u16>) {
    // Create a channel to pass messages from the server to the p2p network
    let (tx, rx) = mpsc::channel(32);

    // Spawn the P2P network task
    tokio::spawn(start_p2p_network(rx, *p2p_port));

    // Start the HTTP server
    start_rest_api(tx, *rest_api_port).await;
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

pub fn handle_print_blockchain(show_txs: &bool) {
    let mut current_block = get_last_block();

    loop {
        println!("====================================");
        println!("Block Height: {}", current_block.height);
        println!(
            "Block Hash: {:x?}",
            bytes_to_hex_string(&current_block.hash)
        );
        println!(
            "Previous Hash: {:x?}",
            bytes_to_hex_string(&current_block.prev_hash)
        );
        println!("Timestamp: {}", current_block.timestamp);
        println!("Nonce: {}", current_block.nonce);
        println!("------------------------------------");

        if *show_txs {
            println!("Transactions:");
            for tx in &current_block.txs {
                println!("  Tx ID: {:x?}", bytes_to_hex_string(&tx.id));
                for input in &tx.inputs {
                    println!(
                        "    Input: Prev Tx ID: {:x?}, Output Index: {}",
                        bytes_to_hex_string(&input.prev_tx_id),
                        input.out
                    );
                }
                for output in &tx.outputs {
                    println!(
                        "    Output: Value: {}, Recipient Hash: {:x?}",
                        output.value,
                        bytes_to_hex_string(&output.pub_key_hash)
                    );
                }
            }
        }

        println!("====================================");

        // Break if we have reached the genesis block
        if current_block.is_genesis() {
            break;
        }

        // Get the previous block
        current_block = get_block(&current_block.prev_hash)
            .expect("[handlers::handle_print_blockchain] ERROR: Failed to fetch previous block");
    }
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

pub fn handle_send_tx(to: &String, value: u32, from: &Option<String>, mine: bool) {
    let wallet_store = WalletStore::init_wallet_store();
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

    let to_address = Address::new_from_str(to.as_str());
    reindex_utxos();

    let tx = Tx::new(from_wallet, &to_address, value);
    db::put_mempool(&tx);

    println!(
        "Successfully added TX: Sent {} from {} to {}",
        value,
        from_wallet.get_wallet_address().get_full_address(),
        to
    );

    if mine {
        let mut new_block = Block::new(&db::get_mempool(), &from_wallet.get_wallet_address());
        new_block.mine();
        update_utxos(&new_block);
        db::reset_mempool();
    }
}

pub fn handle_mine(reward_addr: &Option<String>) {
    let wallet_store = WalletStore::init_wallet_store();
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

    let mut new_block = Block::new(&mempool, &from_wallet.get_wallet_address());
    new_block.mine();
    update_utxos(&new_block);
    db::reset_mempool();
}
