use std::mem::transmute;
use std::sync::mpsc;

use super::liquid::LiquidRequest;
use crate::models::transactions::*;
use crate::repositories::transactions::TransactionRepository;
use async_trait::async_trait;
use tokio::sync::oneshot;

use super::RequestHandler;
use super::ServiceError;

pub enum TransactionServiceRequest {
    NewTransaction {
        user_id: String,
        address: String,
        amount_in_cents: i64,
        asset: String,
        network: String,
        response: oneshot::Sender<Result<Transaction, ServiceError>>,
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

struct TransactionRequestHandler {
    repository: TransactionRepository,
    liquid_channel: mpsc::Sender<LiquidRequest>,
}

impl TransactionRequestHandler {
    pub fn new(
        repository: TransactionRepository,
        liquid_channel: mpsc::Sender<LiquidRequest>,
    ) -> Self {
        TransactionRequestHandler {
            repository,
            liquid_channel,
        }
    }

    async fn new_transaction(
        &self,
        user_id: String,
        address: String,
        amount_in_cents: i64,
        asset: String,
        network: String,
    ) -> Result<Transaction, ServiceError> {
        let (liquid_tx, liquid_rx) = oneshot::channel();
        self.liquid_channel
            .send(LiquidRequest::GetNewAddress {
                response: liquid_tx,
            })
            .map_err(|e| ServiceError::Communication("Transaction".to_string(), e.to_string()));

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

        Ok(transaction)
    }

    async fn update_transaction_status(
        &self,
        transaction_id: &String,
        status: &String,
    ) -> Result<(), ServiceError> {
        let _ = self
            .repository
            .update_transaction_status(transaction_id, status)
            .await
            .map_err(|e| {
                ServiceError::Repository("TransactionService".to_string(), e.to_string())
            })?;

        let transaction_id_clone = transaction_id.clone();
        if status == "eulen_depix_sent" {}

        Ok(())
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
                let result = self
                    .update_transaction_status(&transaction_id, &status)
                    .await;
            }
            _ => (),
        }
    }
}
