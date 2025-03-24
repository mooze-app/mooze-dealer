use super::liquid::LiquidRequest;
use super::pix::PixServiceRequest;
use crate::models::pix::Deposit;
use crate::models::transactions::*;
use crate::repositories::transactions::TransactionRepository;
use async_trait::async_trait;
use axum::{http::StatusCode, Json};
use sqlx::PgPool;
use tokio::sync::{mpsc, oneshot};

use super::RequestHandler;
use super::Service;
use super::ServiceError;

pub enum TransactionServiceRequest {
    NewTransaction {
        user_id: String,
        address: String,
        amount_in_cents: i32,
        asset: String,
        network: String,
        response: oneshot::Sender<Result<Deposit, ServiceError>>,
    },
    UpdateTransactionStatus {
        transaction_id: String,
        status: String,
    },
    UpdateTransactionChangeAddress {
        transaction_id: String,
        status: String,
        response: oneshot::Sender<Result<(), ServiceError>>,
    },
}

#[derive(Clone)]
pub struct TransactionRequestHandler {
    repository: TransactionRepository,
    liquid_channel: mpsc::Sender<LiquidRequest>,
    pix_channel: mpsc::Sender<PixServiceRequest>,
}

impl TransactionRequestHandler {
    pub fn new(
        sql_conn: PgPool,
        liquid_channel: mpsc::Sender<LiquidRequest>,
        pix_channel: mpsc::Sender<PixServiceRequest>,
    ) -> Self {
        let repository = TransactionRepository::new(sql_conn);

        TransactionRequestHandler {
            repository,
            liquid_channel,
            pix_channel,
        }
    }

    async fn new_transaction(
        &self,
        user_id: String,
        address: String,
        amount_in_cents: i32,
        asset: String,
        network: String,
    ) -> Result<Deposit, ServiceError> {
        let (liquid_tx, liquid_rx) = oneshot::channel();
        let (pix_tx, pix_rx) = oneshot::channel();

        self.liquid_channel
            .send(LiquidRequest::GetNewAddress {
                response: liquid_tx,
            })
            .await
            .map_err(|e| ServiceError::Communication("Transaction".to_string(), e.to_string()))
            .map_err(|e| ServiceError::Communication("Transaction".to_string(), e.to_string()))?;

        let fee_address = liquid_rx
            .await
            .map_err(|e| {
                ServiceError::ExternalService(
                    "TransactionService".to_string(),
                    "LiquidService".to_string(),
                    e.to_string(),
                )
            })?
            .map_err(|e| {
                ServiceError::ExternalService(
                    "TransactionService".to_string(),
                    "LiquidService".to_string(),
                    e.to_string(),
                )
            })?;

        let transaction = self
            .repository
            .new_transaction(
                &user_id,
                &address,
                &fee_address,
                amount_in_cents,
                &asset,
                &network,
            )
            .await
            .map_err(|e| {
                ServiceError::Repository("TransactionService".to_string(), e.to_string())
            })?;

        self.pix_channel
            .send(PixServiceRequest::Deposit {
                address: fee_address,
                amount_in_cents,
                transaction_id: transaction.id.clone(),
                response: pix_tx,
            })
            .await
            .map_err(|e| {
                ServiceError::ExternalService(
                    "TransactionService".to_string(),
                    "PixService".to_string(),
                    e.to_string(),
                )
            })?;

        let pix_deposit = pix_rx
            .await
            .map_err(|e| {
                ServiceError::ExternalService(
                    "TransactionService".to_string(),
                    "PixService".to_string(),
                    e.to_string(),
                )
            })?
            .map_err(|e| {
                ServiceError::ExternalService(
                    "TransactionService".to_string(),
                    "PixService".to_string(),
                    e.to_string(),
                )
            })?;

        Ok(pix_deposit)
    }

    async fn update_transaction_status(
        &self,
        transaction_id: &String,
        status: &String,
    ) -> Result<String, ServiceError> {
        let transaction_id = self
            .repository
            .update_transaction_status(transaction_id, status)
            .await
            .map_err(|e| {
                ServiceError::Repository("TransactionService".to_string(), e.to_string())
            })?;

        let transaction_id_clone = transaction_id.clone();
        if status == "eulen_depix_sent" {}

        Ok(transaction_id)
    }
}

#[async_trait]
impl RequestHandler<TransactionServiceRequest> for TransactionRequestHandler {
    async fn handle_request(&self, request: TransactionServiceRequest) {
        match request {
            TransactionServiceRequest::NewTransaction {
                user_id,
                address,
                amount_in_cents,
                asset,
                network,
                response,
            } => {
                let result = self
                    .new_transaction(user_id, address, amount_in_cents, asset, network)
                    .await;
                let _ = response.send(result);
            }
            TransactionServiceRequest::UpdateTransactionStatus {
                transaction_id,
                status,
            } => {
                let _ = self
                    .update_transaction_status(&transaction_id, &status)
                    .await;
            }
            _ => (),
        }
    }
}

pub struct TransactionService;

impl TransactionService {
    pub fn new() -> Self {
        TransactionService {}
    }
}

#[async_trait]
impl Service<TransactionServiceRequest, TransactionRequestHandler> for TransactionService {}
