use async_trait::async_trait;
use tokio::sync::mpsc;

mod blockchain;
mod database;
mod liquid;
mod pix;
mod transactions;

#[derive(Debug, thiserror::Error)]
enum ServiceError {
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

pub struct ServiceManager {}
