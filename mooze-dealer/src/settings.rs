use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Postgres {
    pub url: String,
    pub port: u32,
    pub user: String,
    pub password: String,
    pub database: String,
}

#[derive(Debug, Deserialize)]
pub struct Electrum {
    pub url: String,
    pub port: u32,
    pub tls: bool,
    pub testnet: bool,
}

#[derive(Debug, Deserialize)]
pub struct Depix {
    pub url: String,
    pub auth_token: String,
    pub tls: bool,
}

#[derive(Debug, Deserialize)]
pub struct Sideswap {
    pub url: String,
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
pub struct Wallet {
    pub mnemonic: String,
    pub mainnet: bool,
}

#[derive(Debug, Deserialize)]
pub struct PriceProviders {
    pub binance_url: String,
    pub coingecko_url: String,
}

#[derive(Debug, Deserialize)]
pub struct Liquidity {
    pub max_depix_amount: u64,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub postgres: Postgres,
    pub electrum: Electrum,
    pub depix: Depix,
    pub liquidity: Liquidity,
    pub price_providers: PriceProviders,
    pub sideswap: Sideswap,
    pub wallet: Wallet,
}

impl Settings {
    pub fn new(path: &str) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name(path))
            .build()?;

        config.try_deserialize()
    }
}
