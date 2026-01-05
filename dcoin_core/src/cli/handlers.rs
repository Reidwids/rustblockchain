use core_lib::{
    address::Address,
    constants::SEED_API_NODE,
    req_types::{convert_json_to_utxoset, GetUTXORes, TxJson},
    tx::Tx,
    wallet::Wallet,
};
use reqwest::Client;
use tokio::sync::mpsc;

use crate::{
    blockchain::{
        chain::{clear_blockchain, create_blockchain, get_blockchain_json},
        transaction::utxo::{find_utxos_for_addr, reindex_utxos, UTXOSet},
    },
    cli::cli::CliUI,
    mining::miner::start_miner,
    networking::{node::Node, p2p::network::start_p2p_network, server::rest_api::start_rest_api},
    wallets::wallet::WalletStore,
};

pub fn handle_get_node_id() {
    CliUI::print_header("Get Node ID");
    let node = Node::get_or_create_keys();
    CliUI::print_kv("Node ID", &node.get_peer_id().to_string());
}

pub async fn handle_start_node(
    rest_api_port: &Option<u16>,
    p2p_port: &Option<u16>,
    reward_address: &Option<String>,
    mine: bool,
) {
    // Create a channel to pass messages from the server to the p2p network
    let (tx, rx) = mpsc::channel(32);

    // Spawn the P2P network task
    let p2p_port = p2p_port.unwrap_or(4001);
    tokio::spawn(start_p2p_network(rx, p2p_port));

    // Start the miner if requested on startup
    if mine {
        tokio::spawn(start_miner(tx.clone(), reward_address.clone()));
    }

    // Start the HTTP server
    start_rest_api(tx, *rest_api_port).await;
}

pub fn handle_create_wallet() {
    CliUI::print_header("Create Wallet");

    let mut wallet_store = unwrap_or_exit(
        WalletStore::init_wallet_store(),
        "failed to initialize wallet store",
    );
    let addr = unwrap_or_exit(
        wallet_store.add_wallet(),
        "failed to add wallet to wallet store",
    );
    CliUI::print_kv("New wallet address", addr.get_full_address().as_str());
}

pub fn handle_get_wallets() {
    CliUI::print_header("Get Wallets");
    let wallet_store = unwrap_or_exit(
        WalletStore::init_wallet_store(),
        "failed to initialize wallet store",
    );

    if wallet_store.wallets.is_empty() {
        CliUI::print_text("No wallets found! Try creating a new wallet");
    }
    for (addr, _) in wallet_store.wallets {
        CliUI::print_kv("Wallet address", addr.as_str());
    }
}

pub fn handle_create_blockchain(req_addr: &Option<String>) {
    CliUI::print_header("Create Blockchain");
    let address: Address;
    match req_addr {
        Some(a) => {
            address = unwrap_or_exit(Address::new_from_str(a), "failed to parse request address")
        }
        None => {
            let mut wallet_store = unwrap_or_exit(
                WalletStore::init_wallet_store(),
                "failed to initialize wallet store",
            );
            address = unwrap_or_exit(
                wallet_store.add_wallet(),
                "failed to add wallet to wallet store",
            );
            CliUI::print_text("Wallet address not provided");
            CliUI::print_kv(
                "Created new local wallet to receive mining rewards",
                address.get_full_address().as_str(),
            );
        }
    }

    unwrap_or_exit(create_blockchain(&address), "failed to create blockchain");

    CliUI::print_text("Successfully created blockchain!");
    CliUI::print_kv(
        "Mining rewards sent to",
        address.get_full_address().as_str(),
    );
}

pub fn handle_clear_blockchain() {
    CliUI::print_header("Clear Blockchain");
    clear_blockchain();
    CliUI::print_text("Blockchain data removed successfully");
}

pub fn handle_print_blockchain(show_txs: bool) {
    CliUI::print_header("Print Blockchain");
    let printable_chain = unwrap_or_exit(get_blockchain_json(show_txs), "failed to get blockchain");
    CliUI::print_text(&format!(
        "{}",
        unwrap_or_exit(
            serde_json::to_string_pretty(&printable_chain),
            "failed to print blockchain"
        )
    ));
}

pub fn handle_get_balance(req_addr: &String) {
    CliUI::print_header("Get Balance");
    // TODO: Refactor to be an API call
    let address = unwrap_or_exit(
        Address::new_from_str(req_addr),
        "failed to parse address from request",
    );
    unwrap_or_exit(reindex_utxos(), "failed to reindex utxos");

    let utxos = find_utxos_for_addr(address.pub_key_hash());

    let mut balance = 0;

    for utxo in utxos {
        balance += utxo.value;
    }

    CliUI::print_kv("Address", req_addr);
    CliUI::print_kv("Balance", &format!("{}", balance));
}

pub async fn handle_send_tx(to: &String, value: u32, from: &Option<String>) {
    CliUI::print_header("Send Transaction");
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
            CliUI::print_text("From wallet not provided, using first local wallet");
            match first_wallet {
                Some((_, wallet)) => {
                    from_wallet = wallet;
                    CliUI::print_kv(
                        "First local wallet",
                        &format!("{}", from_wallet.get_wallet_address().get_full_address()),
                    );
                }
                None => exit_with_error("No local wallets found", None),
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

    let utxos: UTXOSet;

    match client.get(url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<GetUTXORes>().await {
                    Ok(data) => match convert_json_to_utxoset(&data.utxos) {
                        Ok(set) => {
                            utxos = set;
                        }
                        Err(e) => {
                            exit_with_error("failed to convert UTXO JSON to UTXOSet", Some(&e));
                        }
                    },
                    Err(e) => {
                        exit_with_error("failed to parse UTXO response", Some(&e));
                    }
                }
            } else {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                let err = format!("status code: {}, response body: {}", status, error_text);
                exit_with_error("failed to fetch UTXOs from node", Some(&err));
            }
        }
        Err(e) => {
            exit_with_error("failed to connect to node", Some(&e));
        }
    }

    let to_address = match Address::new_from_str(to.as_str()) {
        Ok(a) => a,
        Err(e) => {
            exit_with_error("invalid destination address", Some(&e));
        }
    };

    let tx = match Tx::new(from_wallet, &to_address, value, utxos) {
        Ok(tx) => tx,
        Err(e) => {
            exit_with_error("failed to create tx", Some(&e));
        }
    };

    let url = format!("{}/tx/send", SEED_API_NODE);

    let tx_json = match TxJson::from_tx(&tx) {
        Ok(tx) => tx,
        Err(e) => {
            exit_with_error("failed to serialize tx", Some(&e));
        }
    };

    match client.post(&url).json(&tx_json).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                CliUI::print_text("Transaction successfully sent to node");
            } else {
                let status = resp.status();
                let error_text = resp.text().await.unwrap_or_default();
                let err = format!("status code: {}, response body: {}", status, error_text);
                exit_with_error("failed to send transaction", Some(&err));
            }
        }
        Err(e) => {
            exit_with_error("error sending request", Some(&e));
        }
    }
}

fn unwrap_or_exit<T, E: std::fmt::Debug>(res: Result<T, E>, msg: &str) -> T {
    res.unwrap_or_else(|e| {
        CliUI::print_error(&format!("{}: {:?}", msg, e).as_str());
        std::process::exit(1);
    })
}

fn exit_with_error(msg: &str, err: Option<&dyn std::fmt::Debug>) -> ! {
    match err {
        Some(e) => CliUI::print_error(&format!("{}: {:?}", msg, e)),
        None => CliUI::print_error(msg),
    }

    std::process::exit(1);
}
