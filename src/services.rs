use async_trait::async_trait;
use sqlx::PgPool;
use tokio::sync::mpsc;

use crate::settings::Settings;

mod database;
mod http;
mod liquid;
mod liquidity;
mod pix;
mod price;
mod sideswap;
mod transactions;
mod users;

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

pub async fn start_services(pool: PgPool, settings: Settings) -> Result<(), anyhow::Error> {
    let (transaction_tx, mut transaction_rx) = mpsc::channel(512);
    let (liquid_tx, mut liquid_rx) = mpsc::channel(512);
    let (liquidity_tx, mut liquidity_rx) = mpsc::channel(512);
    let (pix_tx, mut pix_rx) = mpsc::channel(512);
    let (price_tx, mut price_rx) = mpsc::channel(512);
    let (sideswap_tx, mut sideswap_rx) = mpsc::channel(512);
    let (user_tx, mut user_rx) = mpsc::channel(512);

    let mut transaction_service = transactions::TransactionService::new();
    let mut liquid_service = liquid::LiquidService::new();
    let mut liquidity_service = liquidity::LiquidityService::new();
    let mut price_service = price::PriceService::new();
    let mut pix_service = pix::PixService::new();
    let mut sideswap_service = sideswap::SideswapService::new();
    let mut user_service = users::UserService::new();

    println!("[*] Starting transaction service.");
    let tx_pool_clone = pool.clone();
    let transaction_liquid_tx = liquid_tx.clone();
    let transaction_pix_tx = pix_tx.clone();
    let transaction_price_tx = price_tx.clone();
    let transaction_user_tx = user_tx.clone();
    tokio::spawn(async move {
        transaction_service
            .run(
                transactions::TransactionRequestHandler::new(
                    tx_pool_clone.clone(),
                    transaction_liquid_tx.clone(),
                    transaction_pix_tx,
                    transaction_price_tx,
                    transaction_user_tx,
                ),
                &mut transaction_rx,
            )
            .await;
    });

    println!("[*] Starting Liquid service.");
    let liquidity_liquid_tx = liquidity_tx.clone();
    tokio::spawn(async move {
        let handler = liquid::LiquidRequestHandler::new(
            liquidity_liquid_tx,
            settings.wallet.mnemonic,
            settings.electrum.url,
            settings.wallet.mainnet,
        );

        handler.start().await;
        liquid_service.run(handler, &mut liquid_rx).await;
    });

    log::info!("Starting liquidity service.");
    let sideswap_liquidity_tx = sideswap_tx.clone();
    tokio::spawn(async move {
        let handler = liquidity::LiquidityHandler::new(
            settings.liquidity.max_depix_amount,
            sideswap_liquidity_tx,
        );

        liquidity_service.run(handler, &mut liquidity_rx).await;
    });

    println!("[*] Starting Pix service.");
    let pix_pool_clone = pool.clone();
    let transaction_tx_clone = transaction_tx.clone();
    tokio::spawn(async move {
        pix_service
            .run(
                pix::PixRequestHandler::new(
                    settings.depix.auth_token,
                    settings.depix.url,
                    pix_pool_clone,
                    transaction_tx_clone,
                ),
                &mut pix_rx,
            )
            .await;
    });

    println!("[*] Starting price service.");
    tokio::spawn(async move {
        let handler = price::PriceRequestHandler::new(
            settings.price_providers.binance_url,
            settings.price_providers.coingecko_url,
        );
        handler.start_price_fetch_task().await;

        price_service.run(handler, &mut price_rx).await;
    });

    log::info!("Starting Sideswap service.");
    let sideswap_liquid_tx = liquid_tx.clone();
    let sideswap_client_tx = sideswap_tx.clone();
    tokio::spawn(async move {
        let handler = sideswap::SideswapRequestHandler::new(
            &settings.sideswap.url,
            &settings.sideswap.api_key,
            sideswap_liquid_tx,
            sideswap_client_tx,
        )
        .await;

        sideswap_service.run(handler, &mut sideswap_rx).await;
    });

    println!("[*] Starting user service.");
    let user_pool_clone = pool.clone();
    tokio::spawn(async move {
        user_service
            .run(
                users::UserRequestHandler::new(user_pool_clone),
                &mut user_rx,
            )
            .await;
    });

    println!("[*] Starting HTTP server.");
    let http_transaction_tx = transaction_tx.clone();
    let http_pix_tx = pix_tx.clone();
    let http_user_tx = user_tx.clone();
    tokio::spawn(async move {
        http::start_http_server(http_transaction_tx, http_pix_tx, http_user_tx)
            .await
            .expect("Could not start HTTP server.");
    });

    println!("[SUCCESS] Started services.");
    Ok(())
}
