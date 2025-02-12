mod blockchain {
    mod block;
    mod transaction {
        mod tx;
        mod utxo;
    }
}
mod ownership {
    pub mod address;
    pub mod wallet;
}

fn main() {
    println!("Hello, world!");
}
