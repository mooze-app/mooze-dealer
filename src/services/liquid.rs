use super::{RequestHandler, Service, ServiceError};
use crate::repositories::liquid::LiquidRepository;

use async_trait::async_trait;
use log::{error, info, warn};
use lwk_wollet::{elements::pset::PartiallySignedTransaction, UnvalidatedRecipient, WalletTxOut};
use std::sync::Arc;
use tokio::sync::oneshot;

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
}

impl LiquidRequestHandler {
    pub fn new(mnemonic: String, electrum_url: String, is_mainnet: bool) -> Self {
        let liquid_repository = LiquidRepository::new(&mnemonic, electrum_url, is_mainnet)
            .expect("Could not instantiate Liquid Repository");

        Self { liquid_repository }
    }

    pub async fn start(&self) -> tokio::task::JoinHandle<()> {
        let repository = self.liquid_repository.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(90));
            loop {
                interval.tick().await;

                match repository.update_wallet().await {
                    Ok(_) => info!("Wallet updated successfully"),
                    Err(e) => error!("Error updating wallet: {}", e),
                }
            }
        })
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
        mut pset: PartiallySignedTransaction,
    ) -> Result<PartiallySignedTransaction, ServiceError> {
        self.liquid_repository
            .sign_transaction(pset)
            .map_err(|e| ServiceError::Repository(String::from("Liquid"), e.to_string()))
    }

    async fn finalize_transaction(
        &self,
        mut pset: PartiallySignedTransaction,
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
            LiquidRequest::BuildTransaction {
                recipients,
                response,
            } => {
                let tx = self.build_liquid_transaction(recipients).await;
                let _ = response.send(tx);
            }
            LiquidRequest::SignTransaction { mut pset, response } => {
                let signed_pset = self.sign_transaction(pset).await;
                let _ = response.send(signed_pset);
            }
            LiquidRequest::FinalizeTransaction { mut pset, response } => {
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
