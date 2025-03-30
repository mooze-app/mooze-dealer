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

use super::{
    pix::PixServiceRequest,
    transactions::TransactionServiceRequest,
    users::{UserRequest, UserService},
    ServiceError,
};
use crate::{
    models::{
        self, pix,
        transactions::{Assets, NewTransaction, Transaction},
        users::NewUser,
    },
    settings::Settings,
};

mod users;

#[derive(Clone)]
struct AppState {
    transaction_channel: mpsc::Sender<TransactionServiceRequest>,
    pix_channel: mpsc::Sender<PixServiceRequest>,
    user_channel: mpsc::Sender<UserRequest>,
}

#[derive(Serialize)]
struct DepositResponse {
    id: String,
    qr_copy_paste: String,
    qr_image_url: String,
}

async fn create_new_user(
    State(state): State<AppState>,
    Json(req): Json<NewUser>,
) -> impl IntoResponse {
    log::debug!("[DEBUG] Received new user registration request");
    let (user_tx, user_rx) = oneshot::channel();

    let user_result = state
        .user_channel
        .send(UserRequest::CreateUser {
            referral_code: req.referral_code,
            response: user_tx,
        })
        .await;

    if let Err(e) = user_result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "Internal server error",
                "details": e.to_string()
            })),
        );
    }

    match user_rx.await {
        Ok(Ok(user)) => {
            dbg!(&user.id);
            return (StatusCode::CREATED, Json(json!({"user_id": user.id})));
        }
        Ok(Err(service_error)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "Database error",
                    "details": "Código de indicação inválido."
                })),
            )
        }
        Err(e) => {
            return {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": "Internal server error",
                        "details": e.to_string()
                    })),
                )
            }
        }
    }
}

async fn get_user_daily_spending(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    let (user_tx, user_rx) = oneshot::channel();

    let user_result = state
        .user_channel
        .send(UserRequest::GetUserDailySpending {
            id: user_id,
            response: user_tx,
        })
        .await;
    if let Err(e) = user_result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "Internal server error",
                "details": e.to_string()
            })),
        );
    }

    match user_rx.await {
        Ok(Ok(daily_spending)) => {
            return (
                StatusCode::OK,
                Json(json!({"daily_spending": daily_spending})),
            )
        }
        Ok(Err(service_error)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "Database error",
                    "details": service_error.to_string()
                })),
            )
        }
        Err(e) => {
            return {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": "Internal server error",
                        "details": e.to_string()
                    })),
                )
            }
        }
    }
}

async fn get_user_details(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    let (user_tx, user_rx) = oneshot::channel();

    let user_result = state
        .user_channel
        .send(UserRequest::IsFirstTransaction {
            id: user_id,
            response: user_tx,
        })
        .await;
    if let Err(e) = user_result {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "Internal server error",
                "details": e.to_string()
            })),
        );
    }

    match user_rx.await {
        Ok(Ok(daily_spending)) => {
            return (
                StatusCode::OK,
                Json(json!({"is_first_transaction": daily_spending})),
            )
        }
        Ok(Err(service_error)) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "Database error",
                    "details": service_error.to_string()
                })),
            )
        }
        Err(e) => {
            return {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({
                        "error": "Internal server error",
                        "details": e.to_string()
                    })),
                )
            }
        }
    }
}

async fn request_new_deposit(
    State(state): State<AppState>,
    Json(req): Json<NewTransaction>,
) -> impl IntoResponse {
    let (transaction_tx, transaction_rx) = oneshot::channel();

    if req.asset != Assets::DEPIX.hex() {
        return (
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "error": "Invalid asset",
                "details": "Em breve!"
            })),
        );
    }

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
            Json(
                json!({"error": format!("Internal server error."), "details": service_error.to_string()}),
            ),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                json!({"error": format!("Failed to receive response: {}", e), "details": e.to_string()}),
            ),
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
    user_channel: mpsc::Sender<UserRequest>,
) -> Result<(), anyhow::Error> {
    let app_state = AppState {
        transaction_channel,
        pix_channel,
        user_channel,
    };

    let app = Router::new()
        .route("/register", post(create_new_user))
        .route("/deposit", post(request_new_deposit))
        .route("/webhook/eulen_status", post(eulen_update_status))
        .route("/user/{user_id}", get(users::get_user_details))
        .route("/hello", get(|| async { "Hello, World!" }))
        .route("/health", get(|| async { "OK" }))
        .with_state(app_state)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    println!("[INFO] Listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
