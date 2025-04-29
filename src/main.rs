use cli::cli::Cli;

mod blockchain {
    pub mod blocks {
        pub mod block;
        pub mod orphan;
    }
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
    pub mod p2p {
        pub mod handlers;
        pub mod network;
    }
    pub mod server {
        pub mod handlers;
        pub mod req_types;
        pub mod rest_api;
    }
}
mod cli {
    pub mod cli;
    pub mod db;
    pub mod handlers;
}
mod mining {
    pub mod miner;
}

#[tokio::main]
async fn main() {
    Cli::run().await;
}
