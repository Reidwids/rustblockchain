use crate::{blockchain::transaction::tx::Tx, networking::p2p::network::P2PMessage};

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::json;
use tokio::{net::TcpListener, sync::mpsc::Sender};

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

fn create_router(tx: Sender<P2PMessage>) -> Router {
    Router::new()
        .route("/", get(handle_root))
        .route("/health", get(handle_health_check))
        .route("/send-transaction", post(handle_send_transaction))
        .with_state(tx)
}

async fn handle_root() -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(json!({
        "name": "Dcoin API",
        "version": "0.0.1"
    })))
}

async fn handle_health_check(
    tx: State<Sender<P2PMessage>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    println!("Received health check request...");
    println!("HTTP Channel sending msg to p2p server...");
    tx.send(P2PMessage::HealthCheck())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "msg": "P2PMessage broadcasted successfully",
    })))
}

async fn handle_send_transaction(
    tx: State<Sender<P2PMessage>>,
    Json(payload): Json<Tx>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    tx.send(P2PMessage::BroadcastTransaction(payload))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({
        "msg": "Tx broadcasted successfully",
    })))
}
