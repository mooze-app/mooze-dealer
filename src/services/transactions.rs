use super::liquid::LiquidRequest;
use super::pix::PixServiceRequest;
use crate::models::pix::Deposit;
use crate::models::transactions;
use crate::repositories::transactions::TransactionRepository;
use async_trait::async_trait;
use futures_util::TryFutureExt;
use lwk_wollet::elements::pset::PartiallySignedTransaction;
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
        let _ = self
            .repository
            .update_transaction_status(transaction_id, status)
            .await
            .map_err(|e| {
                ServiceError::Repository("TransactionService".to_string(), e.to_string())
            })?;

        if status == "eulen_depix_sent" {
            let transaction = self
                .repository
                .get_transaction(&transaction_id)
                .await
                .map_err(|e| {
                    ServiceError::Internal(format!(
                        "Could not retrieve transaction: {}.",
                        e.to_string()
                    ))
                })?;

            match transaction {
                None => {
                    return Err(ServiceError::Database(format!(
                        "Transaction not found: {}.",
                        transaction_id
                    )))
                }
                Some(transaction) => {
                    self.continue_with_transaction(transaction).await;
                }
            }
        }

        Ok(transaction_id.clone())
    }

    async fn continue_with_transaction(
        &self,
        transaction: transactions::Transaction,
    ) -> Result<(), ServiceError> {
        if transaction.asset == transactions::Assets::DEPIX.hex() {
            let asset_amount = transaction.amount_in_cents * 10_i32.pow(8); // adjust precision
            let pset = self
                .collect_fees_and_build_transaction(transaction, asset_amount)
                .await?;
        } else {
            self.send_to_swap(transaction).await;
        }

        Ok(())
    }

    fn calculate_fees(&self, amount_in_cents: i32) -> i32 {
        if amount_in_cents < 60 * 100 {
            return 2 * 100;
        } else if amount_in_cents < 500 {
            return (amount_in_cents * 350) / 10000;
        } else if amount_in_cents < 5000 {
            return (amount_in_cents * 325) / 10000;
        } else {
            return (amount_in_cents * 275) / 10000;
        }
    }

    async fn collect_fees_and_build_transaction(
        &self,
        transaction: transactions::Transaction,
        asset_amount: i32,
    ) -> Result<PartiallySignedTransaction, ServiceError> {
        let fees = self.calculate_fees(transaction.amount_in_cents);
        let net_amount_in_cents = transaction.amount_in_cents - fees;

        let amount_to_send = (asset_amount * net_amount_in_cents) / transaction.amount_in_cents;
        let asset_fee_amount = asset_amount - amount_to_send;

        let (liquid_tx, liquid_rx) = oneshot::channel();
        self.liquid_channel
            .send(LiquidRequest::BuildTransaction {
                address: transaction.address,
                amount: amount_to_send,
                asset: transaction.asset,
                response: liquid_tx,
            })
            .await
            .map_err(|e| {
                ServiceError::Communication("Transaction => Liquid".to_string(), e.to_string())
            })?;

        let pset = liquid_rx
            .await
            .map_err(|e| {
                ServiceError::Communication("Liquid => Transaction".to_string(), e.to_string())
            })?
            .map_err(|e| ServiceError::Repository("Liquid".to_string(), e.to_string()))?;

        Ok(pset)
    }

    async fn send_to_swap(&self, transaction: transactions::Transaction) {
        todo!();
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
