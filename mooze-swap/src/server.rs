use crate::models::*;
use std::str::FromStr;
use tonic::{Request, Response, Status};
use crate::swap::SideswapClient;

pub struct SwapServiceImpl {
    client: SideswapClient
}

impl SwapServiceImpl {
    pub async fn new(url: &str, api_key: &str) -> Result<Self, Status> {
        let client: SideswapClient = SideswapClient::new(url, api_key).await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Self { client })
    }

    pub async fn start(&self) -> Result<(), Status> {
        self.client.start().await.map_err(|e| Status::internal(e.to_string()))?;
        self.client.start_notification_listener().await;
    }
}