use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct EulenDeposit {
    pub id: String,
    pub qrCopyPaste: String,
    pub qrImageUrl: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct EulenDepositStatus {
    pub bankTxId: String,
    pub blockchainTxID: String,
    pub customerMessage: String,
    pub payerName: String,
    pub payerTaxNumber: String,
    pub expiration: String,
    pub pixKey: String,
    pub qrId: String,
    pub status: String,
    pub valueInCents: i64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Deposit {
    pub id: String,
    pub transaction_id: String,
    pub eulen_id: String,
    pub amount_in_cents: i64,
    pub qr_copy_paste: String,
    pub qr_image_url: String,
}
