use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PixTransaction {
    pub id: String,
    pub transaction_id: String,
    pub eulen_id: String,
    pub address: String,
    pub amount_in_cents: i32,
    pub status: String,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EulenDeposit {
    pub id: String,
    #[serde(rename = "qrCopyPaste")]
    pub qr_copy_paste: String,
    #[serde(rename = "qrImageUrl")]
    pub qr_image_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EulenDepositStatus {
    pub bank_tx_id: String,
    #[serde(rename = "blockchainTxID")]
    pub blockchain_tx_id: String,
    pub customer_message: String,
    pub payer_name: String,
    pub payer_tax_number: String,
    pub expiration: String,
    pub pix_key: String,
    pub qr_id: String,
    pub status: String,
    pub value_in_cents: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Deposit {
    pub id: String,
    pub transaction_id: String,
    pub eulen_id: String,
    pub amount_in_cents: i32,
    pub qr_copy_paste: String,
    pub qr_image_url: String,
}
