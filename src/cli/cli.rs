use clap::{Parser, Subcommand};

use super::handlers::{
    handle_clear_blockchain, handle_create_blockchain, handle_create_wallet, handle_get_balance,
    handle_get_node_id, handle_get_wallets, handle_mine, handle_print_blockchain, handle_send_tx,
    handle_start_node,
};

#[derive(Parser)]
#[command(name = "dcoin-cli")]
#[command(about = "Official CLI of dcoin - A blockchain-based crypto-currency", long_about = None)]
#[command(version = "1.0")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Gets or generates a node ID
    #[command(about = "Generates a unique node identifier and stores it locally")]
    GetNodeId,

    /// Start the a new dCoin node
    #[command(about = "Start a new dCoin node")]
    StartNode {
        #[arg(short = 'p', long = "p2p_port")]
        p2p_port: Option<u16>,
        #[arg(short = 'r', long = "rest_api_port")]
        rest_api_port: Option<u16>,
    },

    /// Creates a new wallet
    #[command(about = "Creates a new wallet")]
    CreateWallet,

    /// Get existing wallets
    #[command(about = "Gets existing wallets from local storage")]
    GetWallets,

    /// Creates a new blockchain or fails if one exists
    #[command(about = "Creates a new blockchain")]
    CreateBlockchain { address: Option<String> },

    /// Clear the existing blockchain from memory
    #[command(about = "Clears the existing blockchain")]
    ClearBlockchain,

    /// Print the existing blockchain from memory
    #[command(about = "Prints the existing blockchain")]
    PrintBlockchain {
        #[arg(short = 't')]
        show_txs: bool,
    },

    /// Get balance of a given address
    #[command(about = "Get the balance of a given address")]
    GetBalance { address: String },

    /// Send transaction
    #[command(about = "Send a transaction given an destination address and value")]
    SendTx {
        #[arg(short = 't', long = "to")]
        to: String,
        #[arg(short = 'v', long = "value")]
        value: u32,
        #[arg(short = 'f', long = "from")]
        from: Option<String>,
        #[arg(short = 'm', long = "mine")]
        mine: bool,
    },

    /// Mine the existing transactions in the mempool
    #[command(about = "Mine the existing transactions in the mempool ")]
    Mine {
        #[arg(short = 'a', long = "reward_addr")]
        reward_addr: Option<String>,
    },
}

impl Cli {
    pub async fn run() {
        let cli = Cli::parse();

        match &cli.command {
            Commands::GetNodeId => handle_get_node_id(),
            Commands::StartNode {
                p2p_port,
                rest_api_port,
            } => handle_start_node(p2p_port, rest_api_port).await,
            Commands::CreateWallet => handle_create_wallet(),
            Commands::GetWallets => handle_get_wallets(),
            Commands::CreateBlockchain { address } => handle_create_blockchain(address),
            Commands::ClearBlockchain => handle_clear_blockchain(),
            Commands::PrintBlockchain { show_txs } => handle_print_blockchain(*show_txs),
            Commands::GetBalance { address } => handle_get_balance(address),
            Commands::SendTx {
                to,
                value,
                from,
                mine,
            } => handle_send_tx(to, *value, from, *mine),
            Commands::Mine { reward_addr } => handle_mine(reward_addr),
        }
    }
}
