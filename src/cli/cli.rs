use clap::{Parser, Subcommand};

use super::handlers::{
    handle_clear_blockchain, handle_create_blockchain, handle_create_wallet, handle_get_node_id,
    handle_get_wallets,
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
}

impl Cli {
    pub fn run() {
        let cli = Cli::parse();

        match &cli.command {
            Commands::GetNodeId => handle_get_node_id(),
            Commands::CreateWallet => handle_create_wallet(),
            Commands::GetWallets => handle_get_wallets(),
            Commands::CreateBlockchain { address } => handle_create_blockchain(address),
            Commands::ClearBlockchain => handle_clear_blockchain(),
        }
    }
}
