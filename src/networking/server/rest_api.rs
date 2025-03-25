use crate::networking::p2p::network::P2PMessage;

use axum::{
    routing::{get, post},
    Router,
};
use tokio::{net::TcpListener, sync::mpsc::Sender};

use super::handlers::{
    handle_get_chain, handle_get_wallet_balance, handle_health_check, handle_root, handle_send_tx,
};

pub async fn start_rest_api(tx: Sender<P2PMessage>, port: Option<u16>) {
    // Start the HTTP server
    let port = port.unwrap_or(3000);
    let addr = format!("0.0.0.0:{}", port);
    let router = create_router(tx.clone());
    let listener = TcpListener::bind(&addr).await.unwrap();
    println!("REST API listening on {port}");
    axum::serve(listener, router.into_make_service())
        .await
        .unwrap();
}

fn create_router(p2p: Sender<P2PMessage>) -> Router {
    Router::new()
        .route("/", get(handle_root))
        .route("/health", get(handle_health_check))
        .route("/wallet/balance/{addr}", get(handle_get_wallet_balance))
        .route("/chain", get(handle_get_chain))
        .route("/tx/send", post(handle_send_tx))
        .with_state(p2p)
}
