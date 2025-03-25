use crate::{
    blockchain::{
        chain::get_blockchain_json,
        transaction::{
            tx::Tx,
            utxo::{find_utxos, reindex_utxos},
        },
    },
    networking::p2p::network::P2PMessage,
    wallets::address::Address,
};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
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
        "msg": "Service is healthy",
        "categories": {
            "p2p": "healthy",
            "api": "healthy"
        },
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
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let wallet_addr: Address = match Address::new_from_str(&addr) {
        Ok(addr) => addr,
        Err(e) => {
            return Err(ErrorResponse {
                code: StatusCode::BAD_REQUEST.as_u16(),
                error: e.to_string(),
            })
        }
    };

    reindex_utxos().map_err(|e| ErrorResponse {
        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
        error: e.to_string(),
    })?;

    let utxos = find_utxos(wallet_addr.pub_key_hash());

    let mut balance = 0;

    for utxo in utxos {
        balance += utxo.value;
    }

    Ok(Json(json!({
        "address": addr,
        "balance": balance
    })))
}

#[derive(Deserialize)]
pub struct ChainQuery {
    show_txs: Option<bool>,
}
pub async fn handle_get_chain(
    Query(params): Query<ChainQuery>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    match get_blockchain_json(params.show_txs.unwrap_or(false)) {
        Ok(blocks) => Ok(Json(json!(blocks))),
        Err(e) => Err(ErrorResponse {
            code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            error: e.to_string(),
        }),
    }
}

#[derive(Serialize)]
pub struct ErrorResponse {
    error: String,
    code: u16,
}
impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let json_body = Json(json!(self));
        (StatusCode::INTERNAL_SERVER_ERROR, json_body).into_response()
    }
}
