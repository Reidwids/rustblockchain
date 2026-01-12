use crate::{
    blockchain::{
        chain::get_blockchain_json,
        transaction::{
            mempool::add_tx_to_mempool,
            tx::TxVerify,
            utxo::{find_spendable_utxos, find_utxos_for_addr},
        },
    },
    networking::p2p::network::{NewInventory, P2Prx},
};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use core_lib::{
    address::Address,
    json_types::{convert_utxoset_to_json, TxJson, UTXOSetJson},
    req_types::{GetUTXORes, GetWalletBalanceRes},
};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc::Sender;

pub async fn handle_root() -> Result<Json<serde_json::Value>, StatusCode> {
    info!("http::handle_root");
    Ok(Json(json!({
        "name": "dCoin API",
        "version": "0.0.1"
    })))
}

pub async fn handle_health_check(
    tx: State<Sender<P2Prx>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    info!("http::handle_health_check");
    debug!("sending HealthCheck message to P2P server");
    tx.send(P2Prx::HealthCheck()).await.map_err(|e| {
        error!("p2p service health check failed: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    info!("p2p service health check successful");

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
) -> Result<Json<GetWalletBalanceRes>, ErrorResponse> {
    info!("http::handle_get_wallet_balance");
    let wallet_addr: Address = Address::new_from_str(&addr).map_err(|e| {
        error!("failed to resolve address from request: {:?}", e);
        ErrorResponse {
            code: StatusCode::BAD_REQUEST.as_u16(),
            error: e.to_string(),
        }
    })?;

    let utxos = find_utxos_for_addr(wallet_addr.pub_key_hash());

    let mut balance = 0;

    for utxo in utxos {
        balance += utxo.value;
    }

    Ok(Json(GetWalletBalanceRes {
        address: addr,
        balance: balance,
    }))
}

#[derive(Deserialize)]
pub struct UTXOQuery {
    address: String,
    amount: u32,
}
pub async fn handle_get_spendable_utxos(
    Query(params): Query<UTXOQuery>,
) -> Result<Json<GetUTXORes>, ErrorResponse> {
    info!("http::handle_get_spendable_utxos");
    let wallet_addr: Address = Address::new_from_str(&params.address).map_err(|e| {
        error!("failed to resolve address from request: {:?}", e);
        ErrorResponse {
            code: StatusCode::BAD_REQUEST.as_u16(),
            error: e.to_string(),
        }
    })?;

    let spendable_utxos =
        find_spendable_utxos(wallet_addr.pub_key_hash(), params.amount).map_err(|e| {
            error!("failed to resolve spendable utxos: {:?}", e);
            ErrorResponse {
                // Add check for not enough funds, should be bad request
                code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: e.to_string(),
            }
        })?;

    let utxos: UTXOSetJson = convert_utxoset_to_json(&spendable_utxos);
    Ok(Json(GetUTXORes {
        address: wallet_addr.get_full_address(),
        utxos,
    }))
}

#[derive(Deserialize)]
pub struct ChainQuery {
    show_txs: Option<bool>,
}
pub async fn handle_get_chain(
    Query(params): Query<ChainQuery>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    info!("http::handle_get_chain");
    match get_blockchain_json(params.show_txs.unwrap_or(false)) {
        Ok(blocks) => Ok(Json(json!(blocks))),
        Err(e) => {
            error!("failed to resolve blockchain: {:?}", e);
            Err(ErrorResponse {
                code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: e.to_string(),
            })
        }
    }
}

pub async fn handle_send_tx(
    p2p: State<Sender<P2Prx>>,
    Json(payload): Json<TxJson>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    info!("http::handle_send_tx");
    let tx = payload.to_tx().map_err(|e| {
        error!("failed to marshal tx request payload: {:?}", e);
        ErrorResponse {
            code: StatusCode::BAD_REQUEST.as_u16(),
            error: e.to_string(),
        }
    })?;

    tx.verify().map_err(|e| {
        error!("failed to verify transaction: {:?}", e);
        ErrorResponse {
            code: StatusCode::BAD_REQUEST.as_u16(),
            error: e.to_string(),
        }
    })?;

    add_tx_to_mempool(&tx).map_err(|e| {
        error!("failed to add tx to mempool: {:?}", e);
        ErrorResponse {
            code: StatusCode::BAD_REQUEST.as_u16(),
            error: e.to_string(),
        }
    })?;

    let _ = p2p
        .send(P2Prx::BroadcastNewInv(NewInventory::Transaction(tx.id)))
        .await
        .map_err(|e| {
            error!("p2p node failed to broadcast new inventory object: {:?}", e);
            ErrorResponse {
                code: StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                error: e.to_string(),
            }
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

#[derive(Serialize, Debug)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
}
impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let json_body = Json(json!(self));
        (StatusCode::INTERNAL_SERVER_ERROR, json_body).into_response()
    }
}
