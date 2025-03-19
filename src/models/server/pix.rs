use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct PixDeposit {
    user_id: String,
    amount: u64,
    address: String,
    asset: String,
}
