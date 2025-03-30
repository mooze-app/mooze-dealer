use super::liquid::LiquidRequest;
use super::pix::PixServiceRequest;
use super::price::PriceRequest;
use super::users::UserRequest;
use crate::models::pix::Deposit;
use crate::models::transactions;
use crate::models::transactions::Assets;
use crate::repositories::transactions::TransactionRepository;
use async_trait::async_trait;
use futures_util::TryFutureExt;
use lwk_wollet::elements::pset::PartiallySignedTransaction;
use lwk_wollet::UnvalidatedRecipient;
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
}

#[derive(Clone)]
pub struct TransactionRequestHandler {
    repository: TransactionRepository,
    liquid_channel: mpsc::Sender<LiquidRequest>,
    pix_channel: mpsc::Sender<PixServiceRequest>,
    price_channel: mpsc::Sender<PriceRequest>,
    user_channel: mpsc::Sender<UserRequest>,
}

impl TransactionRequestHandler {
    pub fn new(
        sql_conn: PgPool,
        liquid_channel: mpsc::Sender<LiquidRequest>,
        pix_channel: mpsc::Sender<PixServiceRequest>,
        price_channel: mpsc::Sender<PriceRequest>,
        user_channel: mpsc::Sender<UserRequest>,
    ) -> Self {
        let repository = TransactionRepository::new(sql_conn);

        TransactionRequestHandler {
            repository,
            liquid_channel,
            pix_channel,
            price_channel,
            user_channel,
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

            match transaction {
                None => {
                    return Err(ServiceError::Database(format!(
                        "Transaction not found: {}.",
                        transaction_id
                    )));
                }
                Some(transaction) => {
                    let pset = self.continue_with_transaction(transaction).await?;
                    self.finalize_transaction(pset).await?;
                }
            }
        }

        Ok(transaction_id.clone())
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
