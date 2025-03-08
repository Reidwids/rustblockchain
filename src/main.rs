use cli::cli::Cli;

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
}
mod cli {
    pub mod cli;
    pub mod db;
    pub mod handlers;
}

fn main() {
    Cli::run();
}
