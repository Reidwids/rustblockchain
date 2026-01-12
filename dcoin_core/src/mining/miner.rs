use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use crate::{
    blockchain::{blocks::block::Block, transaction::utxo::update_utxos},
    cli::db,
    networking::p2p::network::{NewInventory, P2Prx},
    wallets::wallet::WalletStore,
};
use core_lib::wallet::Wallet;
use log::{error, info};
use tokio::{sync::mpsc::Sender, time};

static MINING_LOCK: AtomicBool = AtomicBool::new(false);

pub async fn start_miner(p2p: Sender<P2Prx>, reward_address: Option<String>) {
    let wallet_store = if let Ok(w) = WalletStore::init_wallet_store() {
        w
    } else {
        error!("failed to initialize wallet store");
        return;
    };

    let reward_wallet = match reward_address {
        Some(addr) => match wallet_store.wallets.get(&addr) {
            Some(wallet) => wallet.clone(),
            None => {
                error!("mining failed - no local wallet found for given from address");
                return;
            }
        },
        None => {
            info!("wallet address not provided for mining, using first local wallet instead");
            match wallet_store.wallets.values().next() {
                Some(wallet) => {
                    info!(
                        "first local wallet: {}",
                        wallet.get_wallet_address().get_full_address()
                    );
                    wallet.clone()
                }
                None => {
                    error!("error starting miner: no local wallets found");
                    std::process::exit(1);
                }
            }
        }
    };

    // Trigger mining every 10 seconds for now
    // TODO: implement mining based on mempool size or time
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

    info!("Miner: Txs found in mempool. Starting mining routine...");
    let mut new_block = match Block::new(&reward_wallet.get_wallet_address()) {
        Ok(b) => b,
        Err(e) => {
            error!("failed to create block: {:?}", e);
            return;
        }
    };

    if let Err(e) = new_block.mine() {
        error!("failed to mine block: {:?}", e);
        return;
    }

    if let Err(e) = update_utxos(&new_block) {
        error!("failed to update utxos: {:?}", e);
        return;
    };
    db::delete_mempool();

    if let Err(e) = p2p
        .send(P2Prx::BroadcastNewInv(NewInventory::Block(new_block.hash)))
        .await
    {
        error!("failed to send msg to p2p server: {:?}", e);
        return;
    };
}
