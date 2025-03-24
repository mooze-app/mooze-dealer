use sqlx::postgres::PgPoolOptions;

mod models;
mod repositories;
pub mod services;
pub mod settings;
pub mod utils;

#[tokio::main]
async fn main() {
    let config = settings::Settings::new().expect("Could not load config file.");
    let conn = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.postgres.url)
        .await
        .expect("Could not connect to database.");

    println!("[*] Starting services.");
    services::start_services(conn, config)
        .await
        .expect("Could not start services.");
}
