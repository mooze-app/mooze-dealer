use serde::{Deserialize, Serialize};
use sqlx::postgres::types;

#[derive(Deserialize, Serialize)]
pub struct Transaction {
    pub id: String,
    pub user_id: String,
    pub address: String,
    pub fee_address: String,
    pub amount_in_cents: i32,
    pub asset: String,
    pub network: String,
    pub status: String,
}

#[derive(Deserialize, Serialize)]
pub struct NewTransaction {
    pub user_id: String,
    pub address: String,
    pub amount_in_cents: i32,
    pub asset: String,
    pub network: String,
}
