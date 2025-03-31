use log::{debug, info};
use log4rs;
use sqlx::postgres::PgPoolOptions;
use std::fs;
use std::path::Path;

mod models;
mod repositories;
pub mod services;
pub mod settings;
pub mod utils;

#[tokio::main]
async fn main() {
    init_logging().unwrap(); // should not fail

    info!("Starting Mooze dealer service.");
    debug!("Loading configuration");

    let config = settings::Settings::new().expect("Could not load config file.");

    info!(
        "Connecting to PostgreSQL database at {}",
        &config.postgres.url
    );
    let conn = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.postgres.url)
        .await
        .expect("Could not connect to database.");

    info!("Starting services.");
    services::start_services(conn, config)
        .await
        .expect("Could not start services.");

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl-C");
    info!("\n[*] Shutdown signal received, terminating.");

    info!("Service shutting down");
}

fn init_logging() -> Result<(), anyhow::Error> {
    if !Path::new("logs").exists() {
        fs::create_dir("logs")?;
    }

    match log4rs::init_file("log4rs.yaml", Default::default()) {
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
