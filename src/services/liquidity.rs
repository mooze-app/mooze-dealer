use crate::settings::Settings;

use super::{
    sideswap::{SideswapMessage, SideswapRequest},
    RequestHandler, Service, ServiceError,
};

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

pub enum LiquidityRequest {
    UpdateAssetAmount { asset_id: String, amount: u64 },
}

#[derive(Clone)]
pub struct LiquidityHandler {
    sideswap_channel: mpsc::Sender<SideswapRequest>,
    depix_max_amount: u64,
}

impl LiquidityHandler {
    pub fn new(depix_max_amount: u64, sideswap_channel: mpsc::Sender<SideswapRequest>) -> Self {
        Self {
            sideswap_channel,
            depix_max_amount,
        }
    }

    async fn manage_asset_liquidity(&self, asset_id: String, balance: u64) {
        match asset_id.as_str() {
            "02f22f8d9c76ab41661a2729e4752e2c5d1a263012141b86ea98af5472df5189" => {
                self.manage_depix_liquidity(balance).await;
            }
            _ => {
                log::warn!("Unsupported asset ID: {}", asset_id);
            }
        }
    }

    async fn manage_depix_liquidity(&self, current_balance: u64) {
        if current_balance > self.depix_max_amount {
            let (swap_tx, swap_rx) = oneshot::channel();

            // Sends and forgets. If swap fails, the error is logged and liquidity will be handled in the next minute.
            let _ = self
                .sideswap_channel
                .send(SideswapRequest::Swap {
                    sell_asset: "02f22f8d9c76ab41661a2729e4752e2c5d1a263012141b86ea98af5472df5189"
                        .to_string(),
                    receive_asset:
                        "6f0279e9ed041c3d710a9f57d0c02928416460c4b722ae3457a11eec381c526d"
                            .to_string(),
                    amount: (current_balance - self.depix_max_amount) as i64,
                    response: swap_tx,
                })
                .await
                .map_err(|e| {
                    log::warn!("Failed to send swap request: {}", e);
                });
        }
    }
}

#[async_trait]
impl RequestHandler<LiquidityRequest> for LiquidityHandler {
    async fn handle_request(&self, request: LiquidityRequest) {
        match request {
            LiquidityRequest::UpdateAssetAmount { asset_id, amount } => {
                self.manage_asset_liquidity(asset_id, amount).await;
            }
        }
    }
}

pub struct LiquidityService;

impl LiquidityService {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Service<LiquidityRequest, LiquidityHandler> for LiquidityService {}
