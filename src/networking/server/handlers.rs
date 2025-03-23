use crate::{
    blockchain::transaction::{
        tx::Tx,
        utxo::{find_utxos, reindex_utxos},
    },
    networking::p2p::network::P2PMessage,
    wallets::address::Address,
};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc::Sender;

pub async fn handle_root() -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(json!({
        "name": "Dcoin API",
        "version": "0.0.1"
    })))
}

pub async fn handle_health_check(
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

pub async fn handle_send_transaction(
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

pub async fn handle_get_wallet_balance(
    Path(addr): Path<String>,
) -> Result<Json<serde_json::Value>, Json<ErrorResponse>> {
    let wallet_addr: Address = match Address::new_from_str(&addr) {
        Ok(addr) => addr,
        Err(e) => return Err(fmt_json_err(StatusCode::BAD_REQUEST, e.to_string())),
    };

    reindex_utxos().map_err(|e| fmt_json_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let utxos = find_utxos(wallet_addr.pub_key_hash());

    let mut balance = 0;

    for utxo in utxos {
        balance += utxo.value;
    }

    // let balance = get_wallet_balance_from_store(&params.pubkey).ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(json!({
        "address": addr,
        "balance": balance
    })))
}

pub async fn handle_get_chain() {}

#[derive(Serialize)]
pub struct ErrorResponse {
    error: String,
    code: u16,
}
pub fn fmt_json_err(code: StatusCode, msg: String) -> Json<ErrorResponse> {
    Json(ErrorResponse {
        code: code.as_u16(),
        error: msg,
    })
}
