use anyhow::Result;
use clap::Parser;
use log::{info, debug, warn};
use log4rs;
use tonic::transport::Server;
use std::fs;
use std::path::Path;

mod settings;
mod server;
mod service;

pub mod json_rpc;
pub mod models;
pub mod sideswap;
pub use proto::swap as swap_proto;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "swap.toml")]
    config: String,
    #[arg(short, long, default_value = "0.0.0.0:50052")]
    listen: String,
    #[arg(long, default_value = "log4rs.yaml")]
    log4rs: String
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let settings = settings::Settings::load(&args.config).expect("Failed to load settings.");

    init_logging(&args.log4rs).expect("Failed to initialize logging.");
    log::info!("Starting Mooze swap service.");

    let swap_service: server::SwapServiceImpl = server::SwapServiceImpl::new(
        settings.sideswap.url,
        settings.sideswap.api_key
    ).expect("Failed to create swap service.");
    let addr = args.listen.parse().expect("Invalid listen address.");

    info!("Starting gRPC server at {}", addr);
    Server::builder()
        .add_service(swap_proto::swap_service_server::SwapServiceServer::new(swap_service))
        .serve(addr)
        .await
        .expect("Failed to start server.");

    Ok(())
}

fn init_logging(path: &str) -> Result<(), anyhow::Error> {
    if !Path::new("logs").exists() {
        fs::create_dir("logs")?;
    }

    match log4rs::init_file(path, Default::default()) {
        Ok(_) => {
            println!("[*] Logging initialized successfully.");
            Ok(())
        }
        Err(e) => {
            println!("[ERROR] Failed to initialize logging: {}", e);
            Err(anyhow::anyhow!("Could not initialize logging: {}", e))
        }
    }
}