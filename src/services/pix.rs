use super::transactions::TransactionServiceRequest;
use super::RequestHandler;
use super::ServiceError;

use crate::models::pix;
use crate::repositories::pix::PixRepository;

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::PgPool;
use tokio::{
    spawn,
    sync::{mpsc, oneshot},
};

pub enum PixServiceRequest {
    Deposit {
        address: String,
        amount_in_cents: i64,
        transaction_id: String,
        response: oneshot::Sender<Result<pix::Deposit, ServiceError>>,
    },
}

struct PixRequestHandler {
    repository: Arc<PixRepository>,
    transaction_channel: mpsc::Sender<TransactionServiceRequest>,
}

impl PixRequestHandler {
    pub fn new(
        eulen_auth_token: String,
        eulen_url: String,
        pool: PgPool,
        transaction_channel: mpsc::Sender<TransactionServiceRequest>,
    ) -> Self {
        let repository = Arc::new(PixRepository::new(eulen_auth_token, eulen_url, pool));

        PixRequestHandler {
            repository,
            transaction_channel,
        }
    }

    async fn new_pix_deposit(
        &self,
        amount_in_cents: i64,
        address: String,
        transaction_id: String,
    ) -> Result<pix::Deposit, ServiceError> {
        let deposit = self
            .repository
            .new_pix_deposit(&transaction_id, amount_in_cents, &address)
            .await
            .map_err(|e| ServiceError::Repository("Pix".to_string(), e.to_string()))?;

        Ok(deposit)
    }

    async fn update_deposit_status(
        &self,
        eulen_deposit: pix::EulenDepositStatus,
    ) -> Result<(), ServiceError> {
        let transaction_id = self
            .repository
            .update_eulen_deposit_status(&eulen_deposit)
            .await
            .map_err(|e| ServiceError::Repository("Pix".to_string(), e.to_string()))?;

        let transaction_channel = self.transaction_channel.clone();
        let transaction_id_clone = transaction_id.clone();
        let eulen_deposit_clone = eulen_deposit.clone();

        tokio::spawn(async move {
            let _ = transaction_channel
                .send(TransactionServiceRequest::UpdateTransactionStatus {
                    transaction_id: transaction_id_clone,
                    status: format!("eulen_{}", eulen_deposit_clone.status),
                })
                .await;
        });

        Ok(())
    }
}

#[async_trait]
impl RequestHandler<PixServiceRequest> for PixRequestHandler {
    async fn handle_request(&self, request: PixServiceRequest) {
        match request {
            PixServiceRequest::Deposit {
                amount_in_cents,
                address,
                transaction_id,
                response,
            } => {
                let deposit = self
                    .new_pix_deposit(amount_in_cents, address, transaction_id)
                    .await
                    .map_err(|e| {
                        ServiceError::Repository("PixRepository".to_string(), e.to_string())
                    });
                let _ = response.send(deposit);
            }
        }
    }
}
