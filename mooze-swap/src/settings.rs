use anyhow::Result;
use config::{Config, ConfigError, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub swap: SwapConfig
}

pub struct SwapConfig {
    pub api_key: String
}

impl Settings {
    pub fn load(path: &str) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name(path))
            .build()?;
        config.try_deserialize()
    }
}
