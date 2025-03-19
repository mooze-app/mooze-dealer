use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Transaction {
    pub id: String,
    pub address: String,
    pub fee_address: String,
    pub amount_in_cents: String,
    pub asset: String,
    pub network: String,
    pub status: String,
}
