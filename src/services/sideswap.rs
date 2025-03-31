use super::{liquid::LiquidRequest, RequestHandler, Service, ServiceError};

use crate::models::sideswap::{AssetPair, QuoteRequest, SideswapUtxo, StartQuotes, TradeDir};
use crate::models::sideswap::{AssetType, QuoteStatus};
use anyhow::{anyhow, bail};
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

mod client;

enum SideswapMessage {
    Request(SideswapRequest),
    Notification(SideswapNotification),
}

enum SideswapNotification {
    Quote {
        quote_sub_id: i64,
        status: QuoteStatus,
    },
}

pub enum SideswapRequest {
    Swap {
        sell_asset: String,
        receive_asset: String,
        amount: i64,
        response: oneshot::Sender<Result<i64, ServiceError>>,
    },
}

struct SideswapRequestHandler {
    client: client::SideswapClient,
    liquid_channel: mpsc::Sender<LiquidRequest>,
}

impl SideswapRequestHandler {
    pub async fn new(
        sideswap_url: &str,
        sideswap_api_key: &str,
        liquid_channel: mpsc::Sender<LiquidRequest>,
        client_channel: mpsc::Sender<SideswapMessage>,
    ) -> Self {
        let client =
            client::SideswapClient::new(sideswap_url, sideswap_api_key.to_string(), client_channel)
                .await;

        Self {
            client,
            liquid_channel,
        }
    }

    async fn request_address(&self) -> Result<String, ServiceError> {
        let (addr_tx, addr_rx) = oneshot::channel();
        self.liquid_channel
            .send(LiquidRequest::GetNewAddress { response: addr_tx })
            .await
            .map_err(|e| {
                ServiceError::Communication("Sideswap => Liquid".to_string(), e.to_string())
            })?;

        addr_rx
            .await
            .map_err(|e| {
                ServiceError::Communication("Sideswap => Liquid".to_string(), e.to_string())
            })?
            .map_err(|e| {
                ServiceError::ExternalService(
                    String::from("SideswapService"),
                    String::from("LiquidService"),
                    e.to_string(),
                )
            })
    }

    async fn request_change_address(&self) -> Result<String, ServiceError> {
        let (addr_tx, addr_rx) = oneshot::channel();
        self.liquid_channel
            .send(LiquidRequest::GetChangeAddress { response: addr_tx })
            .await
            .map_err(|e| {
                ServiceError::Communication("Sideswap => Liquid".to_string(), e.to_string())
            })?;

        addr_rx
            .await
            .map_err(|e| {
                ServiceError::Communication("Sideswap => Liquid".to_string(), e.to_string())
            })?
            .map_err(|e| {
                ServiceError::ExternalService(
                    String::from("SideswapService"),
                    String::from("LiquidService"),
                    e.to_string(),
                )
            })
    }

    async fn start_quotes(
        &self,
        sell_asset: String,
        receive_asset: String,
        amount: i64,
    ) -> Result<i64, ServiceError> {
        let receive_address = self.request_address().await?;
        let change_address = self.request_change_address().await?;

        let (utxo_tx, utxo_rx) = oneshot::channel();
        self.liquid_channel
            .send(LiquidRequest::GetUtxos {
                asset: Some(sell_asset.clone()),
                response: utxo_tx,
            })
            .await
            .map_err(|e| {
                ServiceError::Communication("Sideswap => Liquid".to_string(), e.to_string())
            })?;

        let utxos = utxo_rx.await.map_err(|e| {
            ServiceError::Communication("Liquid => Sideswap".to_string(), e.to_string())
        })?;
        if let Err(e) = utxos {
            log::error!("Error retrieving utxos: {}", e);
            return Err(e);
        }

        let total_sum: i64 = utxos
            .as_ref()
            .unwrap()
            .iter()
            .map(|utxo| utxo.unblinded.value as i64)
            .sum();

        if total_sum < amount {
            return Err(ServiceError::Internal("InsufficientFunds".to_string()));
        }

        let mut current_sum = 0;
        let mut sideswap_utxos = Vec::new();

        for utxo in utxos.unwrap().iter() {
            let sideswap_utxo = SideswapUtxo {
                txid: utxo.outpoint.txid.to_string(),
                vout: utxo.outpoint.vout,
                asset: utxo.unblinded.asset.to_string(),
                asset_bf: utxo.unblinded.asset_bf.to_string(),
                value: utxo.unblinded.value,
                value_bf: utxo.unblinded.value_bf.to_string(),
                redeem_script: None,
            };

            current_sum += utxo.unblinded.value;
            sideswap_utxos.push(sideswap_utxo);

            if current_sum as i64 > amount {
                break;
            }
        }

        let markets = self.client.get_markets().await.map_err(|e| {
            ServiceError::Communication(
                "Sideswap".to_string(),
                format!("Could not fetch markets: {}", e),
            )
        })?;
        let asset_pair = markets
            .markets
            .into_iter()
            .filter(|market| {
                (market.asset_pair.base == sell_asset && market.asset_pair.quote == receive_asset)
                    || (market.asset_pair.base == receive_asset
                        && market.asset_pair.quote == sell_asset)
            })
            .next();

        match asset_pair {
            Some(pair) => {
                let quote_request = QuoteRequest {
                    asset_pair: pair.asset_pair,
                    asset_type: if pair.asset_type == "Quote" {
                        AssetType::Quote
                    } else {
                        AssetType::Base
                    },
                    trade_dir: TradeDir::Sell,
                    amount,
                    utxos: sideswap_utxos,
                    receive_address,
                    change_address,
                };

                dbg!(&quote_request);

                let quote =
                    self.client.start_quotes(quote_request).await.map_err(|e| {
                        ServiceError::Repository("Sideswap".to_string(), e.to_string())
                    })?;

                dbg!(&quote.quote_sub_id);
                Ok(quote.quote_sub_id)
            }
            None => Err(ServiceError::Repository(
                "Sideswap".to_string(),
                "Market not found".to_string(),
            )),
        }
    }

    async fn proceed_with_quote(&self, quote_sub_id: u64, quote: QuoteStatus) {
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
            }
            QuoteStatus::Error { error_msg } => {
                log::warn!("Sideswap error: {error_msg}");
            }
            QuoteStatus::Success {
                quote_id,
                base_amount,
                quote_amount,
                server_fee,
                fixed_fee,
                ttl,
            } => {}
        }
    }

    async fn finish_swap(
        &self,
        quote_id: u64,
        base_amount: u64,
        quote_amount: u64,
        fixed_fee: u64,
        ttl: u64,
    ) -> Result<(), ServiceError> {
        let pset = self.client.get_quote_pset(quote_id).await.map_err(|e| {
            log::error!("Failed to get quote pset: {}", e);
            ServiceError::Repository("Sideswap".to_string(), e.to_string())
        })?;

        Ok(())
    }
}

#[async_trait]
impl RequestHandler<SideswapMessage> for SideswapRequestHandler {
    async fn handle_request(&self, message: SideswapMessage) {
        match message {
            SideswapMessage::Request(request) => match request {
                SideswapRequest::Swap {
                    sell_asset,
                    receive_asset,
                    amount,
                    response,
                } => {
                    let result = self.start_quotes(sell_asset, receive_asset, amount).await;
                    let _ = response.send(result);
                }
            },
            SideswapMessage::Notification(notification) => match notification {
                SideswapNotification::Quote {
                    quote_sub_id,
                    status,
                } => {}
            },
        }
    }
}
