use crate::models::pix;
use sqlx::PgPool;
use uuid::Uuid;
mod eulen;

pub struct PixRepository {
    eulen_api: eulen::EulenApi,
    conn: PgPool, // how to define PgPool here?,
}

impl PixRepository {
    pub fn new(eulen_auth_token: String, eulen_url: String, conn: PgPool) -> Self {
        let eulen_api = eulen::EulenApi::new(eulen_auth_token, eulen_url);

        PixRepository { eulen_api, conn }
    }

    pub async fn new_pix_deposit(
        &self,
        transaction_id: &String,
        amount_in_cents: i64,
        address: &String,
    ) -> Result<pix::Deposit, anyhow::Error> {
        let deposit_id = Uuid::new_v4().hyphenated().to_string();
        let eulen_deposit = self.eulen_api.deposit(amount_in_cents, address).await?;

        let tx = self.conn.begin().await?;
        sqlx::query!(
            r#"
            INSERT INTO pix_transactions
            (id, transaction_id, eulen_id, address, amount_in_cents, status)
            VALUES ($1, $2, $3, $4, $5, 'pending')
            "#
        )
        .bind(deposit_id)
        .bind(transaction_id)
        .bind(eulen_deposit.id)
        .bind(address)
        .bind(amount_in_cents);

        tx.commit();

        let deposit = pix::Deposit {
            id: deposit_id,
            transaction_id: transaction_id.clone(),
            eulen_id: eulen_deposit.id.clone(),
            amount_in_cents,
            qr_copy_paste: eulen_deposit.qrCopyPaste.clone(),
            qr_image_url: eulen_deposit.qrImageUrl.clone(),
        };

        Ok(deposit)
    }

    pub async fn update_eulen_deposit_status(
        &self,
        eulen_deposit_status: &pix::EulenDepositStatus,
    ) -> Result<String, anyhow::Error> {
        let payer_id_hash = eulen_deposit_status.payerTaxNumber.
        let tx = self.conn.begin().await?;
        let transaction_id: String =
            sqlx::query_as!(String,
            "UPDATE pix_transactions SET status = ? WHERE eulen_id = ? RETURNING transaction_id")
            .bind(deposit_status)
            .bind(eulen_id);

        tx.commit();

        Ok(transaction_id)
    }
}
