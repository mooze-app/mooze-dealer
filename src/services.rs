use async_trait::async_trait;
use liquid::LiquidRequest;
use sqlx::PgPool;
use tokio::sync::mpsc;

use crate::settings::Settings;

mod database;
mod http;
mod liquid;
mod pix;
mod transactions;

#[derive(Debug, thiserror::Error)]
enum ServiceError {
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Repository error: {0} - {0}")]
    Repository(String, String),
    #[error("Communication error: {0} - {1}")]
    Communication(String, String),
    #[error("External service error: {0} -> {1} => {2}")]
    ExternalService(String, String, String),
}

#[async_trait]
pub trait RequestHandler<T>: Send + Sync + 'static
where
    T: Send + 'static,
{
    async fn handle_request(&self, request: T);
}

#[async_trait]
pub trait Service<T, H>: Send + Sync + 'static
where
    T: Send + 'static,
    H: RequestHandler<T> + Clone + Send,
{
    async fn run(&mut self, handler: H, receiver: &mut mpsc::Receiver<T>) {
        while let Some(request) = receiver.recv().await {
            let handler = handler.clone();

            tokio::spawn(async move {
                handler.handle_request(request).await;
            });
        }
    }
}

async fn start_services(pool: PgPool, settings: Settings) -> Result<(), anyhow::Error> {
    let (transaction_tx, mut transaction_rx) = mpsc::channel(512);
    let (liquid_tx, mut liquid_rx) = mpsc::channel(512);
    let (pix_tx, mut pix_rx) = mpsc::channel(512);

    let mut transaction_service = transactions::TransactionService::new();
    let mut liquid_service = liquid::LiquidService::new();
    let mut pix_service = pix::PixService::new();

    transaction_service
        .run(
            transactions::TransactionRequestHandler::new(
                pool.clone(),
                liquid_tx.clone(),
                pix_tx.clone(),
            ),
            &mut transaction_rx,
        )
        .await;

    liquid_service
        .run(
            liquid::LiquidRequestHandler::new(
                settings.wallet.mnemonic,
                settings.electrum.url,
                settings.wallet.wallet_dir,
                false,
            ),
            &mut liquid_rx,
        )
        .await;

    pix_service
        .run(
            pix::PixRequestHandler::new(
                settings.depix.auth_token,
                settings.depix.url,
                pool,
                transaction_tx.clone(),
            ),
            &mut pix_rx,
        )
        .await;

    http::start_http_server(transaction_tx.clone()).await?;

    Ok(())
}
