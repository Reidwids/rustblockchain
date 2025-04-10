use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use crate::{
    blockchain::{block::Block, transaction::utxo::update_utxos},
    cli::db,
    networking::p2p::network::{NewInventory, P2Prx},
    wallets::wallet::{Wallet, WalletStore},
};
use tokio::{sync::mpsc::Sender, time};

static MINING_LOCK: AtomicBool = AtomicBool::new(false);

pub async fn start_miner(p2p: Sender<P2Prx>, reward_address: Option<String>) {
    let wallet_store = if let Ok(w) = WalletStore::init_wallet_store() {
        w
    } else {
        println!("[miner::handle_mine] ERROR: Failed to initialize wallet store");
        return;
    };

    let reward_wallet = match reward_address {
        Some(addr) => match wallet_store.wallets.get(&addr) {
            Some(wallet) => wallet.clone(),
            None => {
                println!(
                        "[miner::handle_mine] ERROR: Mining failed - no local wallet found for given from address"
                    );
                return;
            }
        },
        None => {
            println!("Wallet address not provided for mining, using first local wallet instead");
            match wallet_store.wallets.values().next() {
                Some(wallet) => {
                    println!(
                        "First local wallet: {}",
                        wallet.get_wallet_address().get_full_address()
                    );
                    wallet.clone()
                }
                None => {
                    panic!("[miner::handle_mine] ERROR: No local wallets found");
                }
            }
        }
    };

    // Trigger mining every 10 seconds for now
    let mut interval = time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        if !MINING_LOCK.swap(true, Ordering::SeqCst) {
            let mine_p2p = p2p.clone();

            tokio::spawn(async move {
                handle_mine(mine_p2p, reward_wallet.clone()).await;
                // Release the lock when done
                MINING_LOCK.store(false, Ordering::SeqCst);
            });
        }
    }
}

pub async fn handle_mine(p2p: Sender<P2Prx>, reward_wallet: Wallet) {
    // Fail fast if there are no txs in the mempool
    let mempool = db::get_mempool();
    if mempool.len() == 0 {
        return;
    }

    println!("Miner: Txs found in mempool. Starting mining routine...");
    let mut new_block = match Block::new(&reward_wallet.get_wallet_address()) {
        Ok(b) => b,
        Err(e) => {
            println!(
                "[miner::handle_mine] ERROR: Failed to create block: {:?}",
                e
            );
            return;
        }
    };

    if let Err(e) = new_block.mine() {
        println!("[miner::handle_mine] ERROR: Failed to mine block: {:?}", e);
        return;
    }

    if let Err(e) = update_utxos(&new_block) {
        println!(
            "[miner::handle_mine] ERROR: Failed to update utxos: {:?}",
            e
        );
        return;
    };
    db::reset_mempool();

    if let Err(e) = p2p
        .send(P2Prx::BroadcastNewInv(NewInventory::Block(new_block.hash)))
        .await
    {
        println!(
            "[miner::handle_mine] ERROR: Failed to send msg to p2p server: {:?}",
            e
        );
        return;
    };
}
