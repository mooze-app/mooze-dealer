use crate::models::sideswap;
use crate::utils::json_rpc::JsonRpcClient;

use anyhow::{anyhow, bail};
use serde_json::json;
use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex},
};
use tokio::sync::{mpsc, Notify};

pub struct SideswapClient {
    client: JsonRpcClient,
    api_key: String,
    pending_quotes: Mutex<HashSet<i64>>,
    received_psets: Arc<Mutex<VecDeque<(i64, String)>>>,
    pset_notifier: Arc<Notify>,
}

impl SideswapClient {
    pub async fn new(url: &str, api_key: String) -> Self {
        let client: JsonRpcClient = JsonRpcClient::new(url).await;
        let pending_quotes = Mutex::new(HashSet::new());
        let received_psets = Arc::new(Mutex::new(VecDeque::new()));
        let pset_notifier = Arc::new(Notify::new());

        SideswapClient {
            client,
            api_key,
            pending_quotes,
            received_psets,
            pset_notifier,
        }
    }

    pub async fn start(&self) -> Result<(), anyhow::Error> {
        self.login().await?;
        self.start_notification_listener().await;

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
                _ => bail!("Unknown method: {}", method),
                "market" => {
                    self.handle_market_notification(parameters);
                }
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
        let response = self
            .client
            .call_method("markets", Some(json!({"list_markets": {}})))
            .await
            .map_err(|e| anyhow!("Failed to get markets: {}", e))?;

        let result = response["result"].clone();

        match result.get("markets") {
            Some(r) => {
                let markets: sideswap::ListMarkets = serde_json::from_value(r.clone())
                    .map_err(|e| anyhow!("Failed to parse markets: {}", e))?;

                Ok(markets)
            }
            None => Err(anyhow!("No markets found")),
        }
    }

    pub async fn start_quote(
        &self,
        quote_request: sideswap::QuoteRequest,
    ) -> Result<sideswap::StartQuotes, anyhow::Error> {
        let params = json!({"quote": quote_request});
        let response = self
            .client
            .call_method("market", Some(params))
            .await
            .map_err(|e| anyhow!("Failed to start quote: {}", e))?;

        let result = response["result"].clone();

        match result.get("start_quotes") {
            Some(r) => {
                let start_quotes: sideswap::StartQuotes = serde_json::from_value(r.clone())
                    .map_err(|e| anyhow!("Failed to parse start quotes: {}", e))?;

                self.pending_quotes
                    .lock()
                    .unwrap()
                    .insert(start_quotes.quote_sub_id);
                Ok(start_quotes)
            }
            None => Err(anyhow!("No start quotes found")),
        }
    }

    pub async fn get_quote_pset(&self, quote_id: i64) -> Result<sideswap::Quote, anyhow::Error> {
        let params = json!({"get_quote": {"quote_id": quote_id}});
        let response = self
            .client
            .call_method("market", Some(params))
            .await
            .map_err(|e| anyhow!("Failed to get quote: {}", e))?;

        let result = response["result"].clone();

        match result.get("get_quote") {
            Some(r) => {
                let quote: sideswap::Quote = serde_json::from_value(r.clone())
                    .map_err(|e| anyhow!("Failed to parse quote: {}", e))?;
                Ok(quote)
            }
            None => Err(anyhow!("No quote found")),
        }
    }

    pub async fn sign_quote(
        &self,
        quote_id: i64,
        pset: String,
    ) -> Result<sideswap::TakerSign, anyhow::Error> {
        let params = json!({
            "taker_sign": {
                "quote_id": quote_id,
                "pset": pset
            }
        });

        let response = self
            .client
            .call_method("market", Some(params))
            .await
            .map_err(|e| anyhow!("Failed to sign quote: {}", e))?;

        let result = response["result"].clone();

        match result.get("taker_sign") {
            Some(r) => {
                let quote: sideswap::TakerSign = serde_json::from_value(r.clone())
                    .map_err(|e| anyhow!("Failed to parse taker sign: {}", e))?;
                Ok(quote)
            }
            None => Err(anyhow!("No taker sign found")),
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
