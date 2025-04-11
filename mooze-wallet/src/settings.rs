use anyhow::{anyhow, Result};
use config::{Config, ConfigError, File};
use serde::Deserialize;
use std::fs;
use std::path::Path;

use crate::wallet::WalletConfig;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub server: ServerConfig,
    pub wallet: WalletConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
}

impl Settings {
    pub fn load(path: &str) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name(path))
            .build()?;

        config.try_deserialize()
    }
}    