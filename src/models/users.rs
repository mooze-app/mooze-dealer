use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    pub verified: bool,
    pub referred_by: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NewUser {
    pub referral_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UserDetails {
    pub id: String,
    pub daily_spending: i64,
    pub allowed_spending: i64,
    pub is_verified: bool, // reserved field
}
