use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde::Serialize;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};
use tower_http::trace::TraceLayer;

use super::{pix::PixServiceRequest, transactions::TransactionServiceRequest, ServiceError};
use crate::models::{
    self, pix,
    transactions::{NewTransaction, Transaction},
};

#[derive(Clone)]
struct AppState {
    transaction_channel: mpsc::Sender<TransactionServiceRequest>,
    pix_channel: mpsc::Sender<PixServiceRequest>,
}

#[derive(Serialize)]
struct DepositResponse {
    id: String,
    qr_copy_paste: String,
    qr_image_url: String,
}

async fn request_new_deposit(
    State(state): State<AppState>,
    Json(req): Json<NewTransaction>,
) -> impl IntoResponse {
    let (transaction_tx, transaction_rx) = oneshot::channel();

    let tx_result = state
        .transaction_channel
        .send(TransactionServiceRequest::NewTransaction {
            user_id: req.user_id,
            address: req.address,
            amount_in_cents: req.amount_in_cents,
            asset: req.asset,
            network: req.network,
            response: transaction_tx,
        })
        .await;

    if let Err(e) = tx_result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"description": format!("Failed to process request: {}", e)})),
        );
    }

    match transaction_rx.await {
        Ok(Ok(deposit)) => {
            let response = DepositResponse {
                id: deposit.id,
                qr_image_url: deposit.qr_image_url,
                qr_copy_paste: deposit.qr_copy_paste,
            };
            (StatusCode::CREATED, Json(json!(response)))
        }
        Ok(Err(service_error)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"description": format!("Internal server error.")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"description": format!("Failed to receive response: {}", e)})),
        ),
    }
}

async fn eulen_update_status(
    State(state): State<AppState>,
    Json(req): Json<pix::EulenDepositStatus>,
) -> impl IntoResponse {
    let (pix_tx, pix_rx) = oneshot::channel();
    let pix_result = state
        .pix_channel
        .send(PixServiceRequest::UpdateEulenStatus {
            eulen_status: req,
            response: pix_tx,
        })
        .await;

    if let Err(e) = pix_result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"description": format!("Failed to process request: {}", e)})),
        );
    };

    match pix_rx.await {
        Ok(Ok(update)) => (
            StatusCode::OK,
            Json(json!({"description": "Status updated successfully"})),
        ),
        Ok(Err(service_error)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"description": format!("Internal server error.")})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"description": format!("Failed to receive response: {}", e)})),
        ),
    }
}

pub async fn start_http_server(
    transaction_channel: mpsc::Sender<TransactionServiceRequest>,
    pix_channel: mpsc::Sender<PixServiceRequest>,
) -> Result<(), anyhow::Error> {
    let app_state = AppState {
        transaction_channel,
        pix_channel,
    };

    let app = Router::new()
        .route("/deposit", post(request_new_deposit))
        .route("/eulen_update_status", post(eulen_update_status))
        .route("/hello", get(|| async { "Hello, World!" }))
        .route("/health", get(|| async { "OK" }))
        .with_state(app_state)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    println!("[INFO] Listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
