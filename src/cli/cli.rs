use clap::{Parser, Subcommand};

use super::handlers::handle_get_node_id;

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
    /// gets or generates a node ID
    #[command(about = "Generates a unique node identifier and stores it locally")]
    GetNodeId,
}

impl Cli {
    pub fn run() {
        let cli = Cli::parse();

        match &cli.command {
            Commands::GetNodeId => handle_get_node_id(),
        }
    }
}
