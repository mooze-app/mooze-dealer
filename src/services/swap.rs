use super::{sideswap::SideswapRequest, RequestHandler, Service, ServiceError};
use crate::models::transactions;

use anyhow::anyhow;
use async_trait::async_trait;
use sqlx::PgPool;
use tokio::sync::{mpsc, oneshot};

static DEPIX_ASSET_ID: &str = "02f22f8d9c76ab41661a2729e4752e2c5d1a263012141b86ea98af5472df5189";
static LBTC_ASSET_ID: &str = "6f0279e9ed041c3d710a9f57d0c02928416460c4b722ae3457a11eec381c526d";
static USDT_ASSET_ID: &str = "ce091c998b83c78bb71a632313ba3760f1763d9cfcffae02258ffa9865a37bd2";

pub enum SwapRequest {
    SwapDepix {
        asset: transactions::Assets,
        amount_offered: i64,
        response: oneshot::Sender<Result<(), ServiceError>>,
    },
}

/*
struct SwapRequestHandler {
    sideswap_channel: mpsc::Sender<SideswapRequest>,
    pool: PgPool,
}

impl SwapRequestHandler {
    pub fn new(sideswap_channel: mpsc::Sender<SideswapRequest>, pool: PgPool) -> Self {
        Self {
            sideswap_channel,
            pool,
        }
    }

    async fn swap_depix_for_lbtc(&self, amount_offered: i64) -> Result<(), ServiceError> {
        self.send_to_sideswap(
            transactions::Assets::LBTC.hex(),
            transactions::Assets::DEPIX.hex(),
            sideswap::TradeDir::Sell,
            sideswap::AssetType::Quote,
            amount_offered,
        )
        .await
    }

    async fn swap_depix_for_usdt(&self, amount_offered: i64) -> Result<(), ServiceError> {
        self.send_to_sideswap(
            transactions::Assets::USDT.hex(),
            transactions::Assets::DEPIX.hex(),
            sideswap::TradeDir::Sell,
            sideswap::AssetType::Quote,
            amount_offered,
        )
        .await
    }

    async fn send_to_sideswap(
        &self,
        base_asset: String,
        quote_asset: String,
        trade_dir: sideswap::TradeDir,
        asset_type: sideswap::AssetType,
        amount: i64,
    ) -> Result<(), ServiceError> {
        let (swap_tx, swap_rx) = oneshot::channel();

        self.sideswap_channel
            .send(SideswapRequest::Swap {
                base_asset,
                quote_asset,
                trade_dir,
                asset_type,
                amount,
                response: swap_tx,
            })
            .await
            .map_err(|e| {
                ServiceError::Communication("Swap => Sideswap".to_string(), e.to_string())
            })?;

        swap_rx
            .await
            .map_err(|e| {
                ServiceError::Communication("Swap => Sideswap".to_string(), e.to_string())
            })?
            .map_err(|e| {
                ServiceError::ExternalService(
                    "SwapService".to_string(),
                    "SideswapService".to_string(),
                    e.to_string(),
                )
            })
    }
}

#[async_trait]
impl RequestHandler<SwapRequest> for SwapRequestHandler {
    async fn handle_request(&self, request: SwapRequest) {
        match request {
            SwapRequest::SwapDepix {
                asset,
                amount_offered,
                response,
            } => {
                let result = match asset {
                    transactions::Assets::USDT => self.swap_depix_for_usdt(amount_offered).await,
                    transactions::Assets::LBTC => self.swap_depix_for_lbtc(amount_offered).await,
                    _ => Err(ServiceError::Repository(
                        "Swap".to_string(),
                        "Unsupported asset".to_string(),
                    )),
                };
                let _ = response.send(result);
            }
        }
    }
}
*/
