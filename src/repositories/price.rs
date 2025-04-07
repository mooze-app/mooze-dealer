use serde_json;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::transactions::Assets;

#[derive(Clone)]
struct PriceCache {
    bitcoin: Option<f64>,
    usdt: Option<f64>,
}

#[derive(Clone)]
pub struct PriceRepository {
    binance_url: String,
    coingecko_url: String,
    price_cache: Arc<RwLock<PriceCache>>,
}

impl PriceRepository {
    pub fn new(binance_url: String, coingecko_url: String) -> Self {
        let price_cache = Arc::new(RwLock::new(PriceCache {
            bitcoin: None,
            usdt: None,
        }));

        Self {
            binance_url,
            coingecko_url,
            price_cache,
        }
    }

    pub async fn get_asset_price_with_spread(
        &self,
        asset: Assets,
    ) -> Result<Option<f64>, anyhow::Error> {
        if asset.hex() == Assets::DEPIX.hex() {
            return Ok(Some(1.0));
        }

        let prices = self.get_price_cache().await?;
        let price = match asset {
            Assets::LBTC => Ok(prices.bitcoin),
            Assets::USDT => Ok(prices.usdt),
            _ => Err(anyhow::anyhow!("Unsupported asset")),
        };

        match price {
            Ok(Some(price)) => Ok(Some(price * 1.02)),
            Ok(None) => Err(anyhow::anyhow!("Price not found")),
            Err(e) => Err(e),
        }
    }

    async fn get_price_cache(&self) -> Result<PriceCache, anyhow::Error> {
        let cache = self.price_cache.read().await;
        Ok(cache.clone())
    }

    pub async fn start_price_fetch_task(&self) {
        let repository = self.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

            loop {
                interval.tick().await;

                match repository.fetch_best_prices().await {
                    Ok(()) => {
                        log::info!("Fetched prices.")
                    }
                    Err(e) => {
                        log::error!("Error updating price cache: {}", e);
                    }
                }
            }
        });

        log::info!("Price fetch task started");
    }

    async fn fetch_best_prices(&self) -> Result<(), anyhow::Error> {
        let coingecko_prices = self.fetch_prices_from_coingecko().await?;
        let binance_prices = self.fetch_prices_from_binance().await?;

        let bitcoin = match (coingecko_prices.bitcoin, binance_prices.bitcoin) {
            (Some(cg), Some(bn)) => Some(cg.max(bn)),
            (Some(cg), None) => Some(cg),
            (None, Some(bn)) => Some(bn),
            (None, None) => None,
        };

        let usdt = match (coingecko_prices.usdt, binance_prices.usdt) {
            (Some(cg), Some(bn)) => Some(cg.max(bn)),
            (Some(cg), None) => Some(cg),
            (None, Some(bn)) => Some(bn),
            (None, None) => None,
        };

        let mut cache = self.price_cache.write().await;
        *cache = PriceCache { bitcoin, usdt };

        Ok(())
    }

    async fn fetch_prices_from_coingecko(&self) -> Result<PriceCache, anyhow::Error> {
        let prices: serde_json::Value = reqwest::get(format!(
            "{}/api/v3/simple/price?ids=bitcoin,tether&vs_currencies=brl",
            self.coingecko_url
        ))
        .await?
        .json()
        .await?;

        log::info!("Fetched prices from Coingecko: {:?}", prices);

        let bitcoin = prices["bitcoin"]["brl"].as_f64().map(|v| v);
        let usdt = prices["tether"]["brl"].as_f64().map(|v| v);

        Ok(PriceCache { bitcoin, usdt })
    }

    async fn fetch_prices_from_binance(&self) -> Result<PriceCache, anyhow::Error> {
        let prices: Vec<serde_json::Value> = reqwest::get(format!(
            "{}/api/v3/ticker/price?symbols=[\"BTCBRL\",\"USDTBRL\"]",
            self.binance_url
        ))
        .await?
        .json()
        .await?;

        log::info!("Fetched prices from Binance: {:?}", prices);

        let bitcoin = prices
            .iter()
            .find(|p| p["symbol"] == "BTCBRL")
            .map(|p| p["price"].as_str().unwrap().parse::<f64>().unwrap());
        let usdt = prices
            .iter()
            .find(|p| p["symbol"] == "USDTBRL")
            .map(|p| p["price"].as_str().unwrap().parse::<f64>().unwrap());

        Ok(PriceCache { bitcoin, usdt })
    }
}
