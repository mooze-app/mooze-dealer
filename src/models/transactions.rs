use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Transaction {
    pub id: String,
    pub user_id: String,
    pub address: String,
    pub fee_address: String,
    pub amount_in_cents: i32,
    pub asset: String,
    pub network: String,
    pub status: String,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Deserialize, Serialize)]
pub struct NewTransaction {
    pub user_id: String,
    pub address: String,
    pub amount_in_cents: i32,
    pub asset: String,
    pub network: String,
}

#[derive(Debug)]
pub enum Assets {
    DEPIX,
    USDT,
    LBTC,
}

impl Assets {
    pub fn hex(&self) -> String {
        match self {
            Assets::DEPIX => {
                "02f22f8d9c76ab41661a2729e4752e2c5d1a263012141b86ea98af5472df5189".to_string()
            }
            Assets::USDT => {
                "ce091c998b83c78bb71a632313ba3760f1763d9cfcffae02258ffa9865a37bd2".to_string()
            }
            Assets::LBTC => {
                "6f0279e9ed041c3d710a9f57d0c02928416460c4b722ae3457a11eec381c526d".to_string()
            }
        }
    }

    pub fn from_hex(hex: &str) -> Result<Self, String> {
        match hex {
            "02f22f8d9c76ab41661a2729e4752e2c5d1a263012141b86ea98af5472df5189" => Ok(Assets::DEPIX),
            "ce091c998b83c78bb71a632313ba3760f1763d9cfcffae02258ffa9865a37bd2" => Ok(Assets::USDT),
            "6f0279e9ed041c3d710a9f57d0c02928416460c4b722ae3457a11eec381c526d" => Ok(Assets::LBTC),
            _ => Err("Invalid asset hex".to_string()),
        }
    }
}
