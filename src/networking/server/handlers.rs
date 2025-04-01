use crate::{
    blockchain::{
        chain::get_blockchain_json,
        transaction::{
            mempool::add_tx_to_mempool,
            utxo::{find_utxos_for_addr, reindex_utxos},
        },
    },
    networking::p2p::network::{NewInventory, P2Prx},
    wallets::address::Address,
};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use hex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc::Sender;

use super::req_types::TxJson;

pub async fn handle_root() -> Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(json!({
        "name": "Dcoin API",
        "version": "0.0.1"
    })))
}

pub async fn handle_health_check(
    tx: State<Sender<P2Prx>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    println!("Received health check request...");
    println!("HTTP Channel sending msg to p2p server...");
    tx.send(P2Prx::HealthCheck())
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

    // TODO: remove reindexing - shouldn't be required for running nodes
    reindex_utxos().map_err(|e| ErrorResponse {
        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
        error: e.to_string(),
    })?;

    let utxos = find_utxos_for_addr(wallet_addr.pub_key_hash());

    let mut balance = 0;

    for utxo in utxos {
        balance += utxo.value;
    }

    Ok(Json(json!({
        "address": addr,
        "balance": balance
    })))
}

pub async fn handle_get_utxos_for_addr(
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
    let utxos = find_utxos_for_addr(wallet_addr.pub_key_hash());

    Ok(Json(json!({
        "address": addr,
        "utxos": utxos.iter().map(|utxo| {
            json!({
                "value": utxo.value,
                "pub_key_hash": hex::encode(&utxo.pub_key_hash),
            })
        }).collect::<Vec<_>>() // Collect into Vec<serde_json::Value>
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

pub async fn handle_send_tx(
    p2p: State<Sender<P2Prx>>,
    Json(payload): Json<TxJson>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let tx = payload.to_tx().map_err(|e| ErrorResponse {
        code: StatusCode::BAD_REQUEST.as_u16(),
        error: e.to_string(),
    })?;

    //TODO: deprecate all reindex utxos
    reindex_utxos().map_err(|e| ErrorResponse {
        code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
        error: e.to_string(),
    })?;

    tx.verify().map_err(|e| ErrorResponse {
        code: StatusCode::BAD_REQUEST.as_u16(),
        error: e.to_string(),
    })?;

    add_tx_to_mempool(&tx).map_err(|e| ErrorResponse {
        code: StatusCode::BAD_REQUEST.as_u16(),
        error: e.to_string(),
    })?;

    let _ = p2p
        .send(P2Prx::BroadcastNewInv(NewInventory::Transaction(tx.id)))
        .await
        .map_err(|e| ErrorResponse {
            code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
            error: e.to_string(),
        })?;

    // Tx must be signed before receiving over http.
    // Therefore, we must think about how a client could sign with
    // the same structure as we expect. The easiest way to go about
    // this is likely to create a little WASM binary that can take in
    // a tx request and pass back tx_bytes to send to this handler.
    // Then we can simply run our usual verification + persistence to p2p

    Ok(Json(json!({
        "msg": "Tx broadcasted successfully",
    })))
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
