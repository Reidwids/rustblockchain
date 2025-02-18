use cli::cli::Cli;

mod blockchain {
    pub mod block;
    pub mod merkle;
    pub mod transaction {
        pub mod tx;
        pub mod utxo;
    }
    pub mod chain;
}
mod ownership {
    pub mod address;
    pub mod node;
    pub mod wallet;
}
mod cli {
    pub mod cli;
    pub mod db;
    pub mod handlers;
}

fn main() {
    Cli::run();
}
