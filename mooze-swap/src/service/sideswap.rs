use crate::json_rpc::JsonRpcClient;
use crate::models::*;

use anyhow::anyhow;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;
use proto::wallet::wallet_service_client::WalletServiceClient;

macro_rules! call_sideswap_api {
    ($self:expr, $method:expr, $params:expr, $result_key:expr, $return_type:ty) => {{
        let response = $self
            .wss_client
            .call_method($method, Some($params))
            .await
            .map_err(|e| anyhow!(concat!("Failed to call", stringify!($method), ": {}"), e))?;

        let result = response.get("result").unwrap();

        match result.get($result_key) {
            Some(r) => {
                let data: $return_type = serde_json::from_value(r.clone()).map_err(|e| {
                    anyhow!(
                        concat!("Failed to deserialize", stringify!($return_type), ": {}"),
                        e
                    )
                })?;
                Ok(data)
            }
            None => Err(anyhow!("Missing result key: {}", $result_key)),
        }
    }};
}

#[derive(Clone)]
pub struct SideswapClient {
    sideswap_channel: mpsc::Sender<super::SideswapNotification>,
    wss_client: Arc<JsonRpcClient>,
    api_key: String,
}

impl SideswapClient {
    pub async fn new(
        url: &str,
        api_key: &str,
        sideswap_channel: mpsc::Sender<super::SideswapNotification>,
    ) -> Self {
        let wss_client = Arc::new(JsonRpcClient::new(url).await);

        Self {
            sideswap_channel,
            wss_client,
            api_key: api_key.to_string(),
        }
    }

    pub async fn start(&self) -> Result<(), anyhow::Error> {
        self.login().await?;
        self.get_markets().await?;

        Ok(())
    }

    async fn login(&self) -> Result<(), anyhow::Error> {
        let params = json!({
            "api_key": self.api_key,
            "user-agent": "mooze-dealer",
            "version": "0.1.0"
        });

        self.wss_client.call_method("login", Some(params)).await?;
        Ok(())
    }

    pub async fn start_notification_listener(&self) {
        let wss_client = self.wss_client.clone();
        let tx = self.sideswap_channel.clone();

        tokio::spawn(async move {
            loop {
                let notification = wss_client.wait_for_notification().await;
                log::debug!("Received notification: {:?}", notification);

                if let Err(e) = process_notification(notification, &tx).await {
                    log::error!("Error handling notification: {}", e);
                }
            }
        });
    }

    pub async fn get_markets(&self) -> Result<ListMarkets, anyhow::Error> {
        log::debug!("Requesting markets from Sideswap");
        let result = call_sideswap_api!(
            self,
            "market",
            json!({"list_markets": {}}),
            "list_markets",
            ListMarkets
        );

        match result {
            Ok(markets) => {
                log::debug!("Successfully retrieved {} markets", markets.markets.len());
                Ok(markets)
            }
            Err(e) => {
                log::error!("Failed to get markets: {}", e);
                Err(anyhow!("Failed to get markets: {}", e))
            }
        }
    }

    pub async fn start_quotes(
        &self,
        quote_request: QuoteRequest,
    ) -> Result<StartQuotes, anyhow::Error> {
        log::debug!("Starting quotes with request: {:?}", quote_request);
        
        let request_json = json!({
            "id": 1,
            "method": "market",
            "params": {
                "start_quotes": quote_request
            }
        });
        
        log::info!("Sending request to Sideswap: {}", serde_json::to_string_pretty(&request_json).unwrap());
        
        let result = call_sideswap_api!(
            self,
            "market",
            json!({
                "start_quotes": quote_request
            }),
            "start_quotes",
            StartQuotes
        );

        match result {
            Ok(start_quotes) => {
                log::debug!("Successfully started quotes: {:?}", start_quotes);
                Ok(start_quotes)
            }
            Err(e) => {
                log::error!("Failed to start quotes: {}", e);
                Err(anyhow!("Failed to start quotes: {}", e))
            }
        }
    }

    pub async fn stop_quotes(&self) {
        let _ = self
            .wss_client
            .call_method("market", Some(json!({"stop_quotes": {}})))
            .await;
    }

    pub async fn get_quote_pset(&self, quote_id: u64) -> Result<Quote, anyhow::Error> {
        let result: Result<Quote, anyhow::Error> = call_sideswap_api!(
            self,
            "market",
            json!({"get_quote": {"quote_id": quote_id}}),
            "get_quote",
            Quote
        );

        match result {
            Ok(quote) => Ok(quote),
            Err(e) => Err(anyhow!("Failed to get quote: {}", e)),
        }
    }

    pub async fn sign_quote(
        &self,
        quote_id: u64,
        pset: String,
    ) -> Result<TakerSign, anyhow::Error> {
        let result = call_sideswap_api!(
            self,
            "market",
            json!({
                "taker_sign": {
                    "quote_id": quote_id,
                    "pset": pset
                }
            }),
            "taker_sign",
            TakerSign
        );

        match result {
            Ok(taker_sign) => Ok(taker_sign),
            Err(e) => Err(anyhow!("Failed to sign quote: {}", e)),
        }
    }
}

// Static function to process notifications without requiring &self
async fn process_notification(
    notification: serde_json::Value,
    tx: &mpsc::Sender<super::SideswapNotification>,
) -> Result<(), anyhow::Error> {
    match notification.get("method") {
        Some(method) => match method.as_str() {
            Some("market") => {
                log::debug!("Received market notification: {:?}", notification);
                process_market_notification(&notification["params"], tx).await?;
                Ok(())
            }
            _ => {
                log::warn!("Received unknown notification type: {}", method);
                Ok(())
            }
        },
        None => {
            log::warn!("Received notification without method.");
            Ok(())
        }
    }
}

async fn process_market_notification(
    params: &serde_json::Value,
    tx: &mpsc::Sender<super::SideswapNotification>,
) -> Result<(), anyhow::Error> {
    if let Some(quote) = params.get("quote") {
        process_quote(quote, tx).await?;
    }
    Ok(())
}

async fn process_quote(
    quote: &serde_json::Value,
    tx: &mpsc::Sender<super::SideswapNotification>,
) -> Result<(), anyhow::Error> {
    let quote_sub_id = quote["quote_sub_id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("Missing quote_sub_id"))?;
    log::debug!("Received quote: {:?}", quote);

    match quote.get("status") {
        Some(status) => {
            if let Some(low_balance) = status.get("LowBalance") {
                let quote = QuoteStatus::LowBalance {
                    base_amount: low_balance["base_amount"].as_u64().unwrap_or(0),
                    quote_amount: low_balance["quote_amount"].as_u64().unwrap_or(0),
                    server_fee: low_balance["server_fee"].as_u64().unwrap_or(0),
                    fixed_fee: low_balance["fixed_fee"].as_u64().unwrap_or(0),
                    available: low_balance["available"].as_u64().unwrap_or(0),
                };

                tx.send(super::SideswapNotification::Quote {
                    quote_sub_id,
                    status: quote,
                })
                .await?;
            }

            if let Some(error) = status.get("Error") {
                let quote = QuoteStatus::Error {
                    error_msg: error["error_msg"]
                        .as_str()
                        .unwrap_or("Unknown error")
                        .to_owned(),
                };

                tx.send(super::SideswapNotification::Quote {
                    quote_sub_id,
                    status: quote,
                })
                .await?;
            }

            if let Some(success) = status.get("Success") {
                let quote = QuoteStatus::Success {
                    quote_id: success["quote_id"].as_u64().unwrap_or(0),
                    base_amount: success["base_amount"].as_u64().unwrap_or(0),
                    quote_amount: success["quote_amount"].as_u64().unwrap_or(0),
                    server_fee: success["server_fee"].as_u64().unwrap_or(0),
                    fixed_fee: success["fixed_fee"].as_u64().unwrap_or(0),
                    ttl: success["ttl"].as_u64().unwrap_or(0),
                };

                log::debug!("Got successful quote.");

                let _ = tx.send(super::SideswapNotification::Quote {
                    quote_sub_id,
                    status: quote,
                })
                .await?;
            }

            Ok(())
        }
        None => {
            log::warn!("Received quote without status.");
            Ok(())
        }
    }
}
