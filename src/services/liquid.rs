use super::{RequestHandler, Service, ServiceError};
use crate::repositories::liquid::LiquidRepository;

use async_trait::async_trait;
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
        address: String,
        amount: i32,
        asset: String,
        response: oneshot::Sender<Result<PartiallySignedTransaction, ServiceError>>,
    },
    SignTransaction {
        pset: PartiallySignedTransaction,
        response: oneshot::Sender<Result<PartiallySignedTransaction, ServiceError>>,
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
        amount: i32,
        address: String,
        asset: String,
    ) -> Result<PartiallySignedTransaction, ServiceError> {
        let recipient = UnvalidatedRecipient {
            satoshi: amount as u64,
            address,
            asset,
        };
        let tx = self
            .liquid_repository
            .build_transaction(vec![recipient])
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
                address,
                amount,
                asset,
                response,
            } => {
                let tx = self.build_liquid_transaction(amount, address, asset).await;
                let _ = response.send(tx);
            }
            LiquidRequest::SignTransaction { mut pset, response } => {
                let signed_pset = self.sign_transaction(pset).await;
                let _ = response.send(signed_pset);
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
