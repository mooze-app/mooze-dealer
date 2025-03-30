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

use crate::models::users;
use crate::services::users::UserRequest;

pub async fn get_user_details(
    State(state): State<super::AppState>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    let (user_tx, user_rx) = oneshot::channel();

    let user_result = state
        .user_channel
        .send(UserRequest::GetUserDetails {
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
        Ok(Ok(user)) => {
            return (
                StatusCode::OK,
                Json(json!({
                        "user_id": user.id,
                        "daily_spending": user.daily_spending,
                        "is_first_transaction": user.is_first_transaction,
                        "verified": user.is_verified
                })),
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
