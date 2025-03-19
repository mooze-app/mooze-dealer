use super::{RequestHandler, Service};
use crate::repositories::liquid::LiquidRepository;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::oneshot;

pub enum LiquidRequest {
    GetNewAddress {
        response: oneshot::Sender<Result<String, anyhow::Error>>,
    },
}

#[derive(Clone)]
struct LiquidRequestHandler {
    liquid_repository: Arc<LiquidRepository>,
}

#[async_trait]
impl RequestHandler<LiquidRequest> for LiquidRequestHandler {
    async fn handle_request(&self, request: LiquidRequest) {
        match request {
            LiquidRequest::GetNewAddress { response } => {
                let address = self.liquid_repository.generate_address().await;
                let _ = response.send(address);
            }
        }
    }
}

pub struct LiquidService;

impl LiquidService {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Service<LiquidRequest, LiquidRequestHandler> for LiquidService {}
