use cli::cli::Cli;

mod blockchain {
    pub mod block;
    pub mod merkle;
    pub mod transaction {
        pub mod tx;
        pub mod utxo;
    }
}
mod ownership {
    pub mod address;
    pub mod node;
    pub mod wallet;
}
mod cli {
    pub mod cli;
    pub mod handlers;
}
pub const DB_PATH: &str = "./db";
fn main() {
    Cli::run();
}
