use crate::{
    models::transactions::Assets, repositories::price::PriceRepository, settings::Settings,
};

use super::{transactions::TransactionService, RequestHandler, Service, ServiceError};

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

pub enum PriceRequest {
    GetPrice {
        asset: Assets,
        response: oneshot::Sender<Result<Option<f64>, ServiceError>>,
    },
}

#[derive(Clone)]
pub struct PriceRequestHandler {
    price_repository: PriceRepository,
}

impl PriceRequestHandler {
    pub fn new(binance_url: String, coingecko_url: String) -> Self {
        let price_repository = PriceRepository::new(binance_url, coingecko_url);

        Self { price_repository }
    }

    pub async fn start_price_fetch_task(&self) {
        self.price_repository.start_price_fetch_task().await
    }

    async fn get_price(&self, asset: Assets) -> Result<Option<f64>, ServiceError> {
        self.price_repository
            .get_asset_price_with_spread(asset)
            .await
            .map_err(|e| ServiceError::Repository("Prices".to_string(), e.to_string()))
    }
}

#[async_trait]
impl RequestHandler<PriceRequest> for PriceRequestHandler {
    async fn handle_request(&self, request: PriceRequest) {
        match request {
            PriceRequest::GetPrice { asset, response } => {
                let price = self.get_price(asset).await;
                let _ = response.send(price);
            }
        }
    }
}

pub struct PriceService;

impl PriceService {
    pub fn new() -> Self {
        PriceService {}
    }
}

#[async_trait]
impl Service<PriceRequest, PriceRequestHandler> for PriceService {}
