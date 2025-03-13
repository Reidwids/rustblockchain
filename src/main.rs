use cli::cli::Cli;
use networking::p2p::start_p2p_network;

mod blockchain {
    pub mod block;
    pub mod merkle;
    pub mod transaction {
        pub mod mempool;
        pub mod tx;
        pub mod utxo;
    }
    pub mod chain;
}
mod wallets {
    pub mod address;
    pub mod wallet;
}
mod networking {
    pub mod node;
    pub mod p2p;
}
mod cli {
    pub mod cli;
    pub mod db;
    pub mod handlers;
}

#[tokio::main]
async fn main() {
    Cli::run().await;
}
