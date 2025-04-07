use std::str::FromStr;

use super::{liquid::LiquidRequest, RequestHandler, Service, ServiceError};

use crate::models::sideswap::{AssetType, QuoteStatus};
use crate::models::sideswap::{QuoteRequest, SideswapUtxo, TradeDir};
use async_trait::async_trait;
use lwk_wollet::elements::pset::PartiallySignedTransaction;
use tokio::sync::{mpsc, oneshot};

mod client;

pub enum SideswapMessage {
    Request(SideswapRequest),
    Notification(SideswapNotification),
}

pub enum SideswapNotification {}

pub enum SideswapRequest {
    Swap {
        sell_asset: String,
        receive_asset: String,
        amount: i64,
        response: oneshot::Sender<Result<i64, ServiceError>>,
    },
    Quote {
        quote_sub_id: i64,
        status: QuoteStatus,
    },
}

#[derive(Clone)]
pub struct SideswapRequestHandler {
    client: client::SideswapClient,
    liquid_channel: mpsc::Sender<LiquidRequest>,
}

impl SideswapRequestHandler {
    pub async fn new(
        sideswap_url: &str,
        sideswap_api_key: &str,
        liquid_channel: mpsc::Sender<LiquidRequest>,
        client_channel: mpsc::Sender<SideswapRequest>,
    ) -> Self {
        let mut client =
            client::SideswapClient::new(sideswap_url, sideswap_api_key.to_string(), client_channel)
                .await;

        let _ = client.start().await;
        client.start_notification_listener().await;

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

    async fn proceed_with_quote(&self, quote: QuoteStatus) {
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

    async fn finish_swap(
        &self,
        quote_id: u64,
        base_amount: u64,
        quote_amount: u64,
        fixed_fee: u64,
        ttl: u64,
    ) -> Result<String, ServiceError> {
        let (liquid_tx, liquid_rx) = oneshot::channel();
        let quote_pset = self.client.get_quote_pset(quote_id).await.map_err(|e| {
            log::error!("Failed to get quote pset: {}", e);
            ServiceError::ExternalService(
                "Sideswap".to_string(),
                "wss://api.sideswap.io/".to_string(),
                e.to_string(),
            )
        })?;

        let pset: PartiallySignedTransaction =
            PartiallySignedTransaction::from_str(&quote_pset.pset).map_err(|e| {
                log::error!("Failed to parse pset: {}", e);
                ServiceError::Repository("Sideswap".to_string(), e.to_string())
            })?;

        self.liquid_channel
            .send(LiquidRequest::SignTransaction {
                pset,
                response: liquid_tx,
            })
            .await
            .map_err(|e| {
                log::error!("Failed to send sign transaction request: {}", e);
                ServiceError::Communication("Sideswap => Liquid".to_string(), e.to_string())
            })?;

        let signed_pset = liquid_rx.await.map_err(|e| {
            log::error!("Failed to receive signed transaction: {}", e);
            ServiceError::Communication("Sideswap => Liquid".to_string(), e.to_string())
        })??;
        let serialized_signed_pset = signed_pset.to_string();

        let txid = self
            .client
            .sign_quote(quote_id, serialized_signed_pset)
            .await
            .map_err(|e| {
                log::error!("Failed to sign quote: {}", e);
                ServiceError::ExternalService(
                    "Sideswap".to_string(),
                    "wss://api.sideswap.io/".to_string(),
                    e.to_string(),
                )
            })?;

        Ok(txid.txid)
    }
}

#[async_trait]
impl RequestHandler<SideswapRequest> for SideswapRequestHandler {
    async fn handle_request(&self, message: SideswapRequest) {
        match message {
            SideswapRequest::Swap {
                sell_asset,
                receive_asset,
                amount,
                response,
            } => {
                let result = self.start_quotes(sell_asset, receive_asset, amount).await;
                let _ = response.send(result);
            }
            SideswapRequest::Quote {
                quote_sub_id,
                status,
            } => {
                let _ = self.proceed_with_quote(status);
            }
        }
    }
}

pub struct SideswapService {}

impl SideswapService {
    pub fn new() -> Self {
        SideswapService {}
    }
}

#[async_trait]
impl Service<SideswapRequest, SideswapRequestHandler> for SideswapService {}
