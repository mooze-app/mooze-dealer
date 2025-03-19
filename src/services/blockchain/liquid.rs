use crate::repositories::liquid;
use crate::services::RequestHandler;

use lwk_wollet::{
    elements::{pset::PartiallySignedTransaction, Txid},
    UnvalidatedRecipient,
};
use std::sync::Arc;
use tokio::sync::oneshot;

pub enum LiquidRequest {
    GetNewAddress {
        response: oneshot::Sender<Result<String, anyhow::Error>>,
    },
    BuildTransaction {
        recipients: Vec<UnvalidatedRecipient>,
        response: oneshot::Sender<Result<PartiallySignedTransaction, anyhow::Error>>,
    },
    SignTransaction {
        pset: PartiallySignedTransaction,
        response: oneshot::Sender<Result<PartiallySignedTransaction, anyhow::Error>>,
    },
    FinalizeAndBroadcast {
        pset: PartiallySignedTransaction,
        response: oneshot::Sender<Result<Txid, anyhow::Error>>,
    },
}

struct LiquidHandler {
    wallet: Arc<liquid::LiquidWallet>,
}

#[async_trait::async_trait]
impl RequestHandler<LiquidRequest> for LiquidHandler {
    async fn handle_request(&self, request: LiquidRequest) {
        match request {
            LiquidRequest::GetNewAddress { response } => {
                let result = self.wallet.generate_address().await;
                let _ = response.send(result);
            }
            LiquidRequest::BuildTransaction {
                recipients,
                response,
            } => {
                let result = self.wallet.build_transaction(recipients).await;
                let _ = response.send(result);
            }
            LiquidRequest::SignTransaction { pset, response } => {
                let result = self.wallet.sign_transaction(pset);
                let _ = response.send(result);
            }
            LiquidRequest::FinalizeAndBroadcast { pset, response } => {
                let result = self.wallet.finalize_and_broadcast_transaction(pset).await;
                let _ = response.send(result);
            }
        }
    }
}
