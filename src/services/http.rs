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

use super::{transactions::TransactionServiceRequest, ServiceError};
use crate::models::{
    self,
    transactions::{NewTransaction, Transaction},
};

#[derive(Clone)]
struct AppState {
    transaction_channel: mpsc::Sender<TransactionServiceRequest>,
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

pub async fn start_http_server(
    transaction_channel: mpsc::Sender<TransactionServiceRequest>,
) -> Result<(), anyhow::Error> {
    let app_state = AppState {
        transaction_channel,
    };

    let app = Router::new()
        .route("/deposit", post(request_new_deposit))
        .route("/health", get(|| async { "OK" }))
        .with_state(app_state)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    println!("Listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
