use std::collections::VecDeque;

use super::liquid::LiquidRequest;
use super::pix::PixServiceRequest;
use super::price::PriceRequest;
use super::sideswap::SideswapRequest;
use super::users::UserRequest;
use crate::models::pix::Deposit;
use crate::models::transactions;
use crate::models::transactions::Assets;
use crate::repositories::transactions::TransactionRepository;
use async_trait::async_trait;
use lwk_wollet::elements::pset::PartiallySignedTransaction;
use lwk_wollet::UnvalidatedRecipient;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

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
    UpdateFeeCollected {
        transaction_id: String,
        fee_collected: i32,
    },
}

#[derive(Clone, Debug)]
struct PendingTransaction {
    transaction: transactions::Transaction,
    attempts: u32,
    last_attempt: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct TransactionRequestHandler {
    repository: TransactionRepository,
    liquid_channel: mpsc::Sender<LiquidRequest>,
    pix_channel: mpsc::Sender<PixServiceRequest>,
    price_channel: mpsc::Sender<PriceRequest>,
    user_channel: mpsc::Sender<UserRequest>,
    sideswap_channel: mpsc::Sender<SideswapRequest>,
    pending_transactions: Arc<Mutex<VecDeque<PendingTransaction>>>,
}

impl TransactionRequestHandler {
    pub fn new(
        sql_conn: PgPool,
        liquid_channel: mpsc::Sender<LiquidRequest>,
        pix_channel: mpsc::Sender<PixServiceRequest>,
        price_channel: mpsc::Sender<PriceRequest>,
        user_channel: mpsc::Sender<UserRequest>,
        sideswap_channel: mpsc::Sender<SideswapRequest>,
    ) -> Self {
        let repository = TransactionRepository::new(sql_conn);
        let pending_transactions = Arc::new(Mutex::new(VecDeque::new()));

        let handler = TransactionRequestHandler {
            repository,
            liquid_channel,
            pix_channel,
            price_channel,
            user_channel,
            sideswap_channel,
            pending_transactions,
        };

        handler.start_pending_transaction_processor();

        handler
    }

    fn start_pending_transaction_processor(&self) {
        let handler_clone = self.clone();

        tokio::spawn(async move {
            let mut check_interval = tokio::time::interval(tokio::time::Duration::from_secs(60)); // Check every minute

            loop {
                check_interval.tick().await;
                handler_clone.process_pending_transactions().await;
            }
        });
    }

    async fn process_pending_transactions(&self) {
        let mut pending_txs = self.pending_transactions.lock().await;

        if pending_txs.is_empty() {
            return;
        }

        log::info!("Processing {} pending transactions", pending_txs.len());

        // Take transactions from the queue to process
        let mut transactions_to_process = Vec::new();
        while let Some(pending_tx) = pending_txs.pop_front() {
            transactions_to_process.push(pending_tx);
        }

        // Release the lock before processing
        drop(pending_txs);

        for pending_tx in transactions_to_process {
            log::info!(
                "Attempting to process pending transaction {} (attempt: {})",
                pending_tx.transaction.id,
                pending_tx.attempts + 1
            );

            // Check if we can now process this transaction
            match self.check_asset_balance(&pending_tx.transaction).await {
                Ok(true) => {
                    // We have sufficient balance, try to process the transaction
                    match self
                        .finish_transaction(pending_tx.transaction.clone())
                        .await
                    {
                        Ok(_) => {
                            log::info!(
                                "Successfully processed pending transaction {}",
                                pending_tx.transaction.id
                            );
                        }
                        Err(e) => {
                            log::error!(
                                "Failed to process pending transaction {}: {}",
                                pending_tx.transaction.id,
                                e
                            );
                            // Put it back in the queue with increased attempt count
                            let mut pending_txs = self.pending_transactions.lock().await;
                            pending_txs.push_back(PendingTransaction {
                                transaction: pending_tx.transaction,
                                attempts: pending_tx.attempts + 1,
                                last_attempt: chrono::Utc::now(),
                            });
                        }
                    }
                }
                Ok(false) => {
                    // Still insufficient balance, put it back in the queue
                    let mut pending_txs = self.pending_transactions.lock().await;
                    pending_txs.push_back(PendingTransaction {
                        transaction: pending_tx.transaction,
                        attempts: pending_tx.attempts + 1,
                        last_attempt: chrono::Utc::now(),
                    });
                }
                Err(e) => {
                    log::error!(
                        "Error checking balance for pending transaction {}: {}",
                        pending_tx.transaction.id,
                        e
                    );
                    // Put it back in the queue
                    let mut pending_txs = self.pending_transactions.lock().await;
                    pending_txs.push_back(PendingTransaction {
                        transaction: pending_tx.transaction,
                        attempts: pending_tx.attempts + 1,
                        last_attempt: chrono::Utc::now(),
                    });
                }
            }
        }
    }

    async fn check_asset_balance(
        &self,
        transaction: &transactions::Transaction,
    ) -> Result<bool, ServiceError> {
        let asset_price_in_cents = self.request_asset_price(&transaction.asset).await?;

        // Calculate asset amount with precision already included
        let asset_amount =
            (transaction.amount_in_cents as u64 * 10_u64.pow(8)) / asset_price_in_cents;

        let referral_addr = self.check_for_referral(&transaction.user_id).await?;
        let fee_in_asset = self.calculate_fee_amount(
            transaction.amount_in_cents as u64,
            asset_price_in_cents,
            referral_addr.is_some(),
        );

        let referral_bonus = if let Some(_) = &referral_addr {
            (transaction.amount_in_cents as u64 * 50 * 10_u64.pow(8)) / 10000 / asset_price_in_cents
        } else {
            0
        };

        // Calculate total amount needed
        let total_needed = asset_amount;

        // Check current balance
        let (liquid_tx, liquid_rx) = oneshot::channel();
        self.liquid_channel
            .send(LiquidRequest::GetAssetBalance {
                asset_id: transaction.asset.clone(),
                response: liquid_tx,
            })
            .await
            .map_err(|e| {
                ServiceError::Communication("Transaction => Liquid".to_string(), e.to_string())
            })?;

        let balance = liquid_rx.await.map_err(|e| {
            ServiceError::Communication("Transaction => Liquid".to_string(), e.to_string())
        })??;

        Ok(balance >= total_needed)
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
        
        let (user_tx, user_rx) = oneshot::channel();
        self.user_channel.send(
            UserRequest::GetUser { id: user_id.clone(), response: user_tx }
        ).await.map_err(|e| {
            ServiceError::Communication("Transaction => User".to_string(), e.to_string())
        })?;

        let user = user_rx.await.map_err(|e| {
            log::error!("Failed to get user: {:?}", e);
            ServiceError::Communication("Transaction => User".to_string(), e.to_string())
        })??;

        if let None = user {
            let (create_user_tx, create_user_rx) = oneshot::channel();
            self.user_channel.send(
                UserRequest::CreateUser {
                    referral_code: None,
                    response: create_user_tx,
                }
            ).await.map_err(|e| {
                log::error!("Failed to create user: {:?}", e);
                ServiceError::Communication("Transaction => User".to_string(), e.to_string())
        })?;
        }

        self.liquid_channel
            .send(LiquidRequest::GetNewAddress {
                response: liquid_tx,
            })
            .await
            .map_err(|e| ServiceError::Communication("Transaction".to_string(), e.to_string()))?;

        let fee_address = liquid_rx.await.map_err(|e| {
            ServiceError::ExternalService(
                "TransactionService".to_string(),
                "LiquidService".to_string(),
                e.to_string(),
            )
        })??;

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
                    dbg!(&e);
                    ServiceError::Internal(format!(
                        "Could not retrieve transaction: {}.",
                        e.to_string()
                    ))
                })?;

            dbg!("Entered eulen_depix_sent");

            match transaction {
                None => {
                    return Err(ServiceError::Database(format!(
                        "Transaction not found: {}.",
                        transaction_id
                    )));
                }
                Some(transaction) => {
                    match self.finish_transaction(transaction).await {
                        Ok(_) => {}
                        Err(e) => {
                            // If the error is due to insufficient balance, we'll just log it
                            // The transaction was already added to the pending queue in finish_transaction
                            if let ServiceError::Internal(msg) = &e {
                                if msg == "InsufficientBalance" {
                                    log::warn!(
                                        "Transaction {} queued due to insufficient balance",
                                        transaction_id
                                    );
                                    return Ok(transaction_id.clone());
                                }
                            }
                            return Err(e);
                        }
                    }
                }
            }
        }

        Ok(transaction_id.clone())
    }

    async fn update_fee_collected(
        &self,
        transaction_id: &String,
        fee_collected: i32,
    ) -> Result<String, ServiceError> {
        let _ = self
            .repository
            .update_fee_collected(transaction_id, fee_collected)
            .await
            .map_err(|e| {
                ServiceError::Repository("TransactionService".to_string(), e.to_string())
            })?;

        Ok(transaction_id.clone())
    }

    async fn finish_transaction(
        &self,
        transaction: transactions::Transaction,
    ) -> Result<(), ServiceError> {
        let pset = self.continue_with_transaction(transaction.clone()).await?;
        let signed_pset = self
            .sign_transaction(pset)
            .await
            .map_err(|e| ServiceError::Internal(format!("Could not sign transaction: {}", e)))?;
        let txid = self.finalize_transaction(signed_pset).await?;

        self.repository
            .update_transaction_status(&transaction.id, &"finished".to_string())
            .await
            .map_err(|e| {
                ServiceError::Database(format!("Could not update transaction status: {}", e))
            })?;

        Ok(())
    }

    async fn request_asset_price(&self, asset: &String) -> Result<u64, ServiceError> {
        let (price_tx, price_rx) = oneshot::channel();
        let asset_object = Assets::from_hex(asset)
            .map_err(|e| ServiceError::Internal("Invalid asset".to_string()))?;
        self.price_channel
            .send(PriceRequest::GetPrice {
                asset: asset_object,
                response: price_tx,
            })
            .await
            .map_err(|err| {
                ServiceError::Communication("Transactions => Price".to_string(), err.to_string())
            })?;

        let asset_price = price_rx.await.map_err(|e| {
            ServiceError::ExternalService(
                "TransactionService".to_string(),
                "PriceService".to_string(),
                e.to_string(),
            )
        })??;

        match asset_price {
            Some(price) => {
                let asset_price_in_cents = (price * 100.0) as u64;
                Ok(asset_price_in_cents)
            }
            None => Err(ServiceError::Internal("Asset price not found".to_string())),
        }
    }

    async fn check_for_referral(&self, user_id: &String) -> Result<Option<String>, ServiceError> {
        let (referral_addr_tx, referral_addr_rx) = oneshot::channel();
        self.user_channel
            .send(UserRequest::GetUserReferrerAddress {
                id: user_id.clone(),
                response: referral_addr_tx,
            })
            .await
            .map_err(|e| {
                log::error!("Failed to send request to user service: {:?}", e);
                ServiceError::Communication("Transaction => User".to_string(), e.to_string())
            })?;

        match referral_addr_rx.await {
            Ok(Ok(addr)) => Ok(addr),
            Ok(Err(addr)) => {
                log::error!("Failed to get user referrer address: {}", addr);
                Ok(None)
            }
            Err(e) => {
                log::error!("Failed to get user referrer address: {}", e);
                Ok(None)
            }
        }
    }

    fn calculate_fee_amount(
        &self,
        fiat_amount_in_cents: u64,
        asset_price_in_cents: u64,
        has_referral: bool,
    ) -> u64 {
        // Calculate fee in asset terms with precision already adjusted
        let fee_in_asset = if fiat_amount_in_cents < 55 * 100 {
            (2 * 100 * 10_u64.pow(8)) / asset_price_in_cents
        } else if fiat_amount_in_cents < 500 * 100 {
            (fiat_amount_in_cents * 350 * 10_u64.pow(8)) / 10000 / asset_price_in_cents
        } else if fiat_amount_in_cents < 5000 * 100 {
            (fiat_amount_in_cents * 325 * 10_u64.pow(8)) / 10000 / asset_price_in_cents
        } else {
            (fiat_amount_in_cents * 275 * 10_u64.pow(8)) / 10000 / asset_price_in_cents
        };

        // If there's a referral, reduce the fee by 0.5% of the total transaction amount
        if has_referral {
            let referral_discount =
                (fiat_amount_in_cents * 50 * 10_u64.pow(8)) / 10000 / asset_price_in_cents;
            fee_in_asset - referral_discount
        } else {
            fee_in_asset
        }
    }

    async fn continue_with_transaction(
        &self,
        transaction: transactions::Transaction,
    ) -> Result<PartiallySignedTransaction, ServiceError> {
        // First check if we have sufficient balance
        if let Ok(has_sufficient_balance) = self.check_asset_balance(&transaction).await {
            if !has_sufficient_balance {
                log::warn!(
                    "Insufficient balance for transaction {}, adding to pending queue",
                    transaction.id
                );

                // Add to pending transactions queue
                let mut pending_txs = self.pending_transactions.lock().await;
                pending_txs.push_back(PendingTransaction {
                    transaction: transaction.clone(),
                    attempts: 0,
                    last_attempt: chrono::Utc::now(),
                });

                let (sideswap_tx, sideswap_rx) = oneshot::channel();
                let _ = self.sideswap_channel.send(
                    SideswapRequest::Swap {
                        sell_asset: "02f22f8d9c76ab41661a2729e4752e2c5d1a263012141b86ea98af5472df5189".to_string(),
                        receive_asset: transaction.asset.clone(),
                        amount: ((transaction.amount_in_cents - 100) as u64 * 10_u64.pow(6)) as i64,
                        response: sideswap_tx,
                    }
                ).await.map_err(|e| {
                    log::error!("Failed to send sideswap request: {:?}", e);
                    ServiceError::Communication("Transaction => Sideswap".to_string(), e.to_string())
                })?;

                return Err(ServiceError::Internal("InsufficientBalance".to_string()));
            }
        }

        let asset_price_in_cents = self.request_asset_price(&transaction.asset).await?;

        let asset_amount =
            (transaction.amount_in_cents as u64 * 10_u64.pow(8)) / asset_price_in_cents;

        let referral_addr = self.check_for_referral(&transaction.user_id).await?;
        let fee_in_asset = self.calculate_fee_amount(
            transaction.amount_in_cents as u64,
            asset_price_in_cents,
            referral_addr.is_some(),
        );

        // Update the fee_collected field in the database
        self.repository
            .update_fee_collected(&transaction.id, fee_in_asset as i32)
            .await
            .map_err(|e| {
                ServiceError::Repository("TransactionService".to_string(), e.to_string())
            })?;

        let referral_bonus = if let Some(addr) = &referral_addr {
            (transaction.amount_in_cents as u64 * 50 * 10_u64.pow(8)) / 10000 / asset_price_in_cents
        } else {
            0
        };

        let amount_to_send_user = asset_amount - fee_in_asset - referral_bonus;
        let user_recipient = UnvalidatedRecipient {
            address: transaction.address,
            satoshi: amount_to_send_user,
            asset: transaction.asset.clone(),
        };

        let recipients = match referral_addr {
            Some(referral_addr) => {
                let referral_recipient = UnvalidatedRecipient {
                    address: referral_addr,
                    satoshi: referral_bonus,
                    asset: transaction.asset.clone(),
                };
                vec![user_recipient, referral_recipient]
            }
            None => vec![user_recipient],
        };

        let (liquid_tx, liquid_rx) = oneshot::channel();
        self.liquid_channel
            .send(LiquidRequest::BuildTransaction {
                recipients,
                response: liquid_tx,
            })
            .await
            .map_err(|e| {
                log::error!("{:?}", e);
                ServiceError::Communication("Transaction => Liquid".to_string(), e.to_string())
            })?;

        let pset = liquid_rx.await.map_err(|e| {
            log::error!("{:?}", e);
            ServiceError::Communication("Transaction => Liquid".to_string(), e.to_string())
        })??;

        Ok(pset)
    }

    async fn sign_transaction(
        &self,
        pset: PartiallySignedTransaction,
    ) -> Result<PartiallySignedTransaction, anyhow::Error> {
        let (liquid_tx, liquid_rx) = oneshot::channel();
        self.liquid_channel
            .send(LiquidRequest::SignTransaction {
                pset,
                response: liquid_tx,
            })
            .await
            .map_err(|e| {
                log::error!("{:?}", e);
                ServiceError::Communication("Transaction => Liquid".to_string(), e.to_string())
            })?;

        let signed_pset = liquid_rx.await.map_err(|e| {
            log::error!("{:?}", e);
            ServiceError::Communication("Transaction => Liquid".to_string(), e.to_string())
        })??;

        Ok(signed_pset)
    }

    async fn finalize_transaction(
        &self,
        pset: PartiallySignedTransaction,
    ) -> Result<(), ServiceError> {
        let (liquid_tx, liquid_rx) = oneshot::channel();
        self.liquid_channel
            .send(LiquidRequest::FinalizeTransaction {
                pset,
                response: liquid_tx,
            })
            .await
            .map_err(|e| {
                log::error!("{:?}", e);
                ServiceError::Communication("Transaction => Liquid".to_string(), e.to_string())
            })?;

        let txid = liquid_rx.await.map_err(|e| {
            log::error!("{:?}", e);
            ServiceError::Communication("Transaction => Liquid".to_string(), e.to_string())
        })??;

        log::info!("Finished transaction: {}", txid);

        Ok(())
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
            TransactionServiceRequest::UpdateFeeCollected {
                transaction_id,
                fee_collected,
            } => {
                let _ = self
                    .update_fee_collected(&transaction_id, fee_collected)
                    .await;
            }
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
