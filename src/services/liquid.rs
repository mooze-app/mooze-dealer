use super::{liquidity::LiquidityRequest, RequestHandler, Service, ServiceError};
use crate::repositories::liquid::LiquidRepository;

use async_trait::async_trait;
use log::{error, info};
use lwk_wollet::{elements::pset::PartiallySignedTransaction, UnvalidatedRecipient, WalletTxOut};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub enum LiquidRequest {
    GetNewAddress {
        response: oneshot::Sender<Result<String, ServiceError>>,
    },
    GetChangeAddress {
        response: oneshot::Sender<Result<String, ServiceError>>,
    },
    GetUtxos {
        asset: Option<String>,
        response: oneshot::Sender<Result<Vec<WalletTxOut>, ServiceError>>,
    },
    GetAssetBalance {
        asset_id: String,
        response: oneshot::Sender<Result<u64, ServiceError>>,
    },
    BuildTransaction {
        recipients: Vec<UnvalidatedRecipient>,
        response: oneshot::Sender<Result<PartiallySignedTransaction, ServiceError>>,
    },
    SignTransaction {
        pset: PartiallySignedTransaction,
        response: oneshot::Sender<Result<PartiallySignedTransaction, ServiceError>>,
    },
    FinalizeTransaction {
        pset: PartiallySignedTransaction,
        response: oneshot::Sender<Result<String, ServiceError>>,
    },
}

#[derive(Clone)]
pub struct LiquidRequestHandler {
    liquid_repository: Arc<LiquidRepository>,
    liquidity_channel: mpsc::Sender<LiquidityRequest>,
}

impl LiquidRequestHandler {
    pub fn new(
        liquidity_channel: mpsc::Sender<LiquidityRequest>,
        mnemonic: String,
        electrum_url: String,
        is_mainnet: bool,
    ) -> Self {
        let liquid_repository = LiquidRepository::new(&mnemonic, electrum_url, is_mainnet)
            .expect("Could not instantiate Liquid Repository");

        Self {
            liquid_repository,
            liquidity_channel,
        }
    }

    pub async fn start(&self) -> tokio::task::JoinHandle<()> {
        let repository = self.liquid_repository.clone();
        let liquidity_channel = self.liquidity_channel.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;

                match repository.update_wallet().await {
                    Ok(_) => info!("Wallet updated successfully"),
                    Err(e) => error!("Error updating wallet: {}", e),
                };

                let depix_amount = repository
                    .get_asset_balance(
                        "02f22f8d9c76ab41661a2729e4752e2c5d1a263012141b86ea98af5472df5189",
                    )
                    .await;

                match depix_amount {
                    Ok(amount) => {
                        let _ = liquidity_channel
                            .send(LiquidityRequest::UpdateAssetAmount{
                                asset_id: "02f22f8d9c76ab41661a2729e4752e2c5d1a263012141b86ea98af5472df5189".to_string(),
                                amount,
                            }
                            )
                            .await;
                    }
                    Err(e) => error!("Error getting DEPIX balance: {}", e),
                };
            }
        })
    }

    async fn get_asset_balance(&self, asset_id: &String) -> Result<u64, ServiceError> {
        self.liquid_repository
            .get_asset_balance(asset_id)
            .await
            .map_err(|e| ServiceError::Repository(String::from("Liquid"), e.to_string()))
    }

    async fn get_new_address(&self) -> Result<String, ServiceError> {
        self.liquid_repository
            .generate_address()
            .await
            .map_err(|e| ServiceError::Repository(String::from("Liquid"), e.to_string()))
    }

    async fn get_new_change_address(&self) -> Result<String, ServiceError> {
        self.liquid_repository
            .generate_change_address()
            .await
            .map_err(|e| ServiceError::Repository(String::from("Liquid"), e.to_string()))
    }

    async fn get_utxos(&self, asset: Option<String>) -> Result<Vec<WalletTxOut>, ServiceError> {
        self.liquid_repository
            .get_utxos(asset)
            .await
            .map_err(|e| ServiceError::Repository(String::from("Liquid"), e.to_string()))
    }

    async fn build_liquid_transaction(
        &self,
        recipients: Vec<UnvalidatedRecipient>,
    ) -> Result<PartiallySignedTransaction, ServiceError> {
        let tx = self
            .liquid_repository
            .build_transaction(recipients)
            .await
            .map_err(|e| ServiceError::Repository(String::from("Liquid"), e.to_string()))?;

        Ok(tx)
    }

    async fn sign_transaction(
        &self,
        pset: PartiallySignedTransaction,
    ) -> Result<PartiallySignedTransaction, ServiceError> {
        self.liquid_repository
            .sign_transaction(pset)
            .map_err(|e| ServiceError::Repository(String::from("Liquid"), e.to_string()))
    }

    async fn finalize_transaction(
        &self,
        pset: PartiallySignedTransaction,
    ) -> Result<String, ServiceError> {
        self.liquid_repository
            .finalize_and_broadcast_transaction(pset)
            .await
            .map_err(|e| ServiceError::Repository(String::from("Liquid"), e.to_string()))
    }
}

#[async_trait]
impl RequestHandler<LiquidRequest> for LiquidRequestHandler {
    async fn handle_request(&self, request: LiquidRequest) {
        match request {
            LiquidRequest::GetNewAddress { response } => {
                let address = self.get_new_address().await;
                let _ = response.send(address);
            }
            LiquidRequest::GetChangeAddress { response } => {
                let address = self.get_new_change_address().await;
                let _ = response.send(address);
            }
            LiquidRequest::GetUtxos { asset, response } => {
                let utxos = self.get_utxos(asset).await;
                let _ = response.send(utxos);
            }
            LiquidRequest::GetAssetBalance { asset_id, response } => {
                let balance = self.get_asset_balance(&asset_id).await;
                let _ = response.send(balance);
            }
            LiquidRequest::BuildTransaction {
                recipients,
                response,
            } => {
                let tx = self.build_liquid_transaction(recipients).await;
                let _ = response.send(tx);
            }
            LiquidRequest::SignTransaction { pset, response } => {
                let signed_pset = self.sign_transaction(pset).await;
                let _ = response.send(signed_pset);
            }
            LiquidRequest::FinalizeTransaction { pset, response } => {
                let finalized_pset = self.finalize_transaction(pset).await;
                let _ = response.send(finalized_pset);
            }
        }
    }
}

pub struct LiquidService;

impl LiquidService {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Service<LiquidRequest, LiquidRequestHandler> for LiquidService {}
