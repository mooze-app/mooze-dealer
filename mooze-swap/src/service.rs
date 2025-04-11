mod sideswap;
mod wallet;

use anyhow::Result;
use proto::swap::SwapResponse;
use tokio_tungstenite::tungstenite::handshake::client::Response;
use tonic::Status;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

use crate::swap_proto::swap_service_server::SwapService;
use crate::settings::Settings;
use crate::models::*;

enum SideswapNotification {
    Quote {
        quote_sub_id: i64,
        status: QuoteStatus
    }
}

pub struct SwapServiceImpl {
    sideswap_client: Arc<sideswap::SideswapClient>,
    notification_rx: mpsc::Receiver<SideswapNotification>,
    wallet_client: Arc<wallet::WalletClient>,
}

impl SwapServiceImpl {
    pub async fn new(
        sideswap_url: &str,
        sideswap_api_key: &str,
        wallet_url: &str,
    ) -> Result<Self, Status> {
        let (notification_tx, notification_rx) = mpsc::channel(100);

        let sideswap_client = sideswap::SideswapClient::new(sideswap_url, sideswap_api_key, notification_tx).await;
        let wallet_client = wallet::WalletClient::new(wallet_url).await;

        let _ = sideswap_client.start().await;
        sideswap_client.start_notification_listener().await; 

        Self {
            sideswap_client,
            notification_rx,
            wallet_client,
        }
    }

    pub async fn start_notification_listener(&self) {
        while let Ok(notification) = self.notification_rx.recv().await {
            match notification {
                SideswapNotification::Quote { quote_sub_id, status } => {
                    log::info!("Quote ID: {}", quote_sub_id);
                    self.proceed_with_quote(status).await;
                }
            }
        }
    }

    async fn swap(&self, sell_asset: &str, receive_asset: &str, amount: u64) -> Result<SwapResponse, Status> {
        let mut utxos = self.wallet_client.get_utxos(sell_asset).await?;
        let total_sum: u64 = utxos.as_ref().unwrap().iter().map(|utxo| utxo.value as u64).sum();

        if total_sum < amount {
            return Err(Status::internal("InsufficientFunds"));
        }

        let mut current_sum = 0;
        let mut sideswap_utxos: Vec<SideswapUtxo> = Vec::new();

        for utxo in utxos.unwrap().iter() {
            current_sum += utxo.value as u64;
            sideswap_utxos.push(SideswapUtxo { 
                txid: utxo.txid.clone(), 
                vout: utxo.vout, 
                asset: utxo.asset.clone(), 
                asset_bf: utxo.asset_bf, 
                value: utxo.value, 
                value_bf: utxo.value_bf, 
                redeem_script: None 
            });
            if current_sum >= amount {
                break;
            }
        }

        log::info!("Found {} utxos for sell_asset={}, receive_asset={}, amount={}", sideswap_utxos.len(), sell_asset, receive_asset, amount);

        let markets = self.sideswap_client.get_markets().await?;
        let asset_pair = markets.markets.iter().find(|market| {
            (market.asset_pair.base == sell_asset && market.asset_pair.quote == receive_asset) ||
            (market.asset_pair.base == receive_asset && market.asset_pair.quote == sell_asset)
        });

        if asset_pair.is_none() {
            return Err(Status::internal("AssetPairNotFound"));
        }

        let quote_req = QuoteRequest {
            asset_pair: asset_pair.unwrap().asset_pair,
            asset_type: if asset_pair.unwrap().asset_type == "Quote" {
                AssetType::Base
            } else {
                AssetType::Quote
            },
            trade_dir: TradeDir::Sell,
            amount,
            utxos: sideswap_utxos,
            receive_address,
            change_address,
        };

        let quote = self.sideswap_client.start_quotes(quote_request).await.map_err(|e| {
            Status::internal(format!("Failed to start quotes: {}", e))
        })?;

        log::debug!("Quote ID: {}", quote.quote_sub_id);
    }

    async fn proceed_with_quote(&self, quote: QuoteStatus) {
        log::debug!("Proceeding with quote: {:?}", quote);

        match quote {
            QuoteStatus::LowBalance {
                base_amount,
                quote_amount,
                server_fee,
                fixed_fee,
                available,
            } => {
                log::warn!(
                    r"
                    Could not finalize quote: low balance.
                    Base amount: {base_amount}, Quote amount: {quote_amount}, Server fee: {server_fee}, Fixed fee: {fixed_fee}, Available: {available}
                    "
                );
                self.client.stop_quotes().await;
            }
            QuoteStatus::Error { error_msg } => {
                log::warn!("Sideswap error: {error_msg}");
                self.client.stop_quotes().await;
            }
            QuoteStatus::Success {
                quote_id,
                base_amount,
                quote_amount,
                server_fee,
                fixed_fee,
                ttl,
            } => {
                log::info!("Received quote: id={quote_id}, base_amount={base_amount}, quote_amount={quote_amount}, server_fee={server_fee}, fixed_fee={fixed_fee}, ttl={ttl}");
                let txid = self
                    .finish_swap(quote_id, base_amount, quote_amount, fixed_fee, ttl)
                    .await;

                match txid {
                    Ok(txid) => {
                        log::info!("Swap completed successfully: txid={txid}");
                    }
                    Err(err) => {
                        log::error!("Failed to complete swap: {}", err);
                    }
                }
            }
        }
    }

    async fn finish_swap(&self, quote_id: u64, base_amount: u64, quote_amount: u64, fixed_fee: u64, ttl: u64) -> Result<String, Status> {
        let quote_pset = self.sideswap_client.get_quote_pset(quote_id).await.map_err(|e| {
            Status::internal(format!("Failed to get quote pset: {}", e))
        })?;

        let signed_pset = self.wallet_client.sign_pset(&quote_pset.pset).await.map_err(|e| {
            Status::internal(format!("Failed to sign pset: {}", e))
        })?;

        let txid = self.sideswap_client.sign_quote(quote_id, signed_pset).await.map_err(|e| {
            Status::internal(format!("Failed to sign quote: {}", e))
        })?;

        self.sideswap_client.stop_quotes().await;
        Ok(txid.txid)
    }
}

#[tonic::async_trait]
impl SwapService for SwapServiceImpl {
    async fn swap(&self, request: Request<SwapRequest>) -> Result<Response<SwapResponse>, Status> {
        let swap_request = request.into_inner();
        let result = self.swap(swap_request.sell_asset, swap_request.receive_asset, swap_request.amount).await.map_err(|e| {
            Status::internal(format!("Failed to swap: {}", e))
        })?;

        Ok(Response::new(SwapResponse {
            quote_sub_id: result.quote_sub_id,
        }))
    }
}