use anyhow::Result;
use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub sideswap: Sideswap,
    pub wallet: Wallet,
}

#[derive(Debug, Deserialize)]
pub struct Sideswap {
    pub url: String,
    pub api_key: String
}

#[derive(Debug, Deserialize)]
pub struct Wallet {
    pub url: String,
}

impl Settings {
    pub fn load(path: &str) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name(path))
            .build()?;
        config.try_deserialize()
    }
}
