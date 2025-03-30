use crate::models::sideswap;
use crate::utils::json_rpc::JsonRpcClient;

use anyhow::{anyhow, bail};
use serde_json::json;
use sqlx::PgPool;
use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, Notify};

macro_rules! call_sideswap_api {
    ($self:expr, $method:expr, $params:expr, $result_key:expr, $return_type:ty) => {{
        let response = $self
            .client
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

pub struct SideswapRepository {
    client: Arc<JsonRpcClient>,
    api_key: String,
    cache_markets: Option<sideswap::ListMarkets>,
    pending_quotes: Mutex<HashSet<i64>>,
    received_psets: Arc<Mutex<VecDeque<(i64, String)>>>,
    pset_notifier: Arc<Notify>,
}

impl SideswapRepository {
    pub async fn new(url: &str, api_key: String) -> Self {
        let client: Arc<JsonRpcClient> = Arc::new(JsonRpcClient::new(url).await);
        let pending_quotes = Mutex::new(HashSet::new());
        let received_psets = Arc::new(Mutex::new(VecDeque::new()));
        let pset_notifier = Arc::new(Notify::new());

        SideswapRepository {
            client,
            api_key,
            cache_markets: None,
            pending_quotes,
            received_psets,
            pset_notifier,
        }
    }

    pub async fn start(&mut self) -> Result<(), anyhow::Error> {
        self.login().await?;
        self.start_notification_listener().await;

        let markets = self.get_markets().await?;
        self.cache_markets = Some(markets);

        Ok(())
    }

    async fn login(&self) -> Result<(), anyhow::Error> {
        let params =
            json!({"api_key": self.api_key, "user_agent": "mooze-dealer", "version": "0.1.0"});
        self.client.call_method("login", Some(params)).await?;

        Ok(())
    }

    pub async fn start_notification_listener(&self) {
        let notification_listener = self.client.wait_for_notification().await;
    }

    async fn parse_notifications(
        &self,
        notification: serde_json::Value,
    ) -> Result<(), anyhow::Error> {
        let parameters: serde_json::Value = match notification.get("params") {
            Some(p) => p.clone(),
            None => bail!("Missing parameters in notification."),
        };

        if let Some(method) = notification.get("method").and_then(|m| m.as_str()) {
            match method {
                "market" => {
                    self.handle_market_notification(parameters).await?;
                }
                _ => bail!("Unknown method: {}", method),
            }
        }

        Ok(())
    }

    async fn handle_market_notification(
        &self,
        parameters: serde_json::Value,
    ) -> Result<(), anyhow::Error> {
        if let Some(quote) = parameters.get("quote") {
            self.update_quote(quote).await;
        }

        if let Some(chart_update) = parameters.get("chart_update") {
            self.update_chart(chart_update).await;
        }

        Ok(())
    }

    async fn update_quote(&self, quote: &serde_json::Value) -> Result<(), anyhow::Error> {
        if let Some(quote_id) = quote.get("quote_sub_id") {
            let quote_id = quote_id.as_i64().unwrap();
            let status = quote.get("status");

            if status.is_none() {
                return Err(anyhow!("Quote status is not available."));
            }

            if quote.get("LowBalance").is_some() {
                self.pending_quotes.lock().unwrap().remove(&quote_id);
                self.notify_quote_low_balance(quote_id).await;
                return Ok(());
            }

            if quote.get("Error").is_some() {
                self.pending_quotes.lock().unwrap().remove(&quote_id);
                self.notify_quote_error(quote_id).await;
                return Ok(());
            }

            let quote_pset = self.get_quote_pset(quote_id).await?;

            Ok(())
        } else {
            Err(anyhow!("Invalid quote id!"))
        }
    }

    async fn update_chart(&self, chart_update: &serde_json::Value) {
        todo!();
    }

    async fn notify_quote_error(&self, quote_id: i64) {
        todo!();
    }

    async fn notify_quote_low_balance(&self, quote_id: i64) {
        todo!();
    }

    pub async fn notify_swap_error(&self, swap_id: String, error: &str) {
        todo!();
    }

    pub async fn get_markets(&self) -> Result<sideswap::ListMarkets, anyhow::Error> {
        let result = call_sideswap_api!(
            self,
            "market",
            json!({"list_markets": {}}),
            "markets",
            sideswap::ListMarkets
        );

        match result {
            Ok(markets) => Ok(markets),
            Err(e) => Err(anyhow!("Failed to get markets: {}", e)),
        }
    }

    pub async fn start_quotes(
        &self,
        quote_request: sideswap::QuoteRequest,
    ) -> Result<sideswap::StartQuotes, anyhow::Error> {
        let result: Result<sideswap::StartQuotes, anyhow::Error> = call_sideswap_api!(
            self,
            "market",
            json!({"quote": quote_request}),
            "start_quotes",
            sideswap::StartQuotes
        );

        match result {
            Ok(result) => {
                self.pending_quotes
                    .lock()
                    .unwrap()
                    .insert(result.quote_sub_id);
                Ok(result)
            }
            Err(e) => Err(anyhow!("Failed to start quotes: {}", e)),
        }
    }

    pub async fn get_quote_pset(&self, quote_id: i64) -> Result<sideswap::Quote, anyhow::Error> {
        let result: Result<sideswap::Quote, anyhow::Error> = call_sideswap_api!(
            self,
            "market",
            json!({"get_quote": {"quote_id": quote_id}}),
            "get_quote",
            sideswap::Quote
        );

        match result {
            Ok(quote) => Ok(quote),
            Err(e) => Err(anyhow!("Failed to get quote: {}", e)),
        }
    }

    pub async fn sign_quote(
        &self,
        quote_id: i64,
        pset: String,
    ) -> Result<sideswap::TakerSign, anyhow::Error> {
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
            sideswap::TakerSign
        );

        match result {
            Ok(taker_sign) => Ok(taker_sign),
            Err(e) => Err(anyhow!("Failed to sign quote: {}", e)),
        }
    }

    pub async fn get_next_pset(&self) -> Option<(i64, String)> {
        let mut psets = self.received_psets.lock().unwrap();
        psets.pop_front()
    }

    pub async fn wait_for_pset(&self) -> &Notify {
        &self.pset_notifier
    }
}
