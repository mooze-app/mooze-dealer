use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub enum AssetType {
    Base,
    Quote,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum TradeDir {
    Buy,
    Sell,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Asset {
    pub always_show: Option<bool>,
    pub asset_id: String,
    pub contract: Option<Contract>,
    pub domain: Option<String>,
    pub icon: Option<String>,
    pub icon_url: Option<String>,
    pub instant_swaps: Option<bool>,
    pub issuance_prevout: Option<IssuancePrevout>,
    pub issuer_pubkey: Option<String>,
    pub market_type: Option<String>,
    pub name: String,
    pub payjoin: Option<bool>,
    pub precision: u8,
    pub ticker: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Contract {
    pub entity: Option<Entity>,
    pub issuer_pubkey: Option<String>,
    pub name: String,
    pub precision: u8,
    pub ticker: Option<String>,
    pub version: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entity {
    pub domain: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IssuancePrevout {
    pub txid: String,
    pub vout: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Assets {
    pub assets: Vec<Asset>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AssetPair {
    pub base: String,
    pub quote: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Market {
    pub asset_pair: AssetPair,
    pub fee_asset: String,
    #[serde(rename = "type")]
    pub asset_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListMarkets {
    pub markets: Vec<Market>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SideswapUtxo {
    pub txid: String,
    pub vout: u32,
    pub asset: String,
    pub asset_bf: String,
    pub value: u64,
    pub value_bf: String,
    pub redeem_script: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct QuoteRequest {
    pub asset_pair: AssetPair,
    pub asset_type: AssetType,
    pub trade_dir: TradeDir,
    pub amount: i64,
    pub utxos: Vec<SideswapUtxo>,
    pub receive_address: String,
    pub change_address: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StartQuotes {
    pub fee_asset: String,
    pub quote_sub_id: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Quote {
    pub pset: String,
    pub ttl: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TakerSign {
    pub txid: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum QuoteStatus {
    Success {
        quote_id: i64,
        base_amount: u64,
        quote_amount: u64,
        server_fee: u64,
        fixed_fee: u64,
        ttl: u64,
    },
    LowBalance {
        base_amount: u64,
        quote_amount: u64,
        server_fee: u64,
        fixed_fee: u64,
        available: u64,
    },
    Error {
        error_msg: String,
    },
}
