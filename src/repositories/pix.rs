use crate::models::pix;
use sqlx;
use sqlx::PgPool;
use uuid::Uuid;
mod eulen;

pub struct PixRepository {
    eulen_api: eulen::EulenApi,
    conn: PgPool,
}

impl PixRepository {
    pub fn new(eulen_auth_token: String, eulen_url: String, conn: PgPool) -> Self {
        let eulen_api = eulen::EulenApi::new(eulen_auth_token, eulen_url);

        PixRepository { eulen_api, conn }
    }

    pub async fn new_pix_deposit(
        &self,
        transaction_id: &String,
        amount_in_cents: i32,
        address: &String,
    ) -> Result<pix::Deposit, anyhow::Error> {
        let deposit_id = Uuid::new_v4().hyphenated().to_string();
        let eulen_deposit = self.eulen_api.deposit(amount_in_cents, address).await?;

        sqlx::query!(
            r#"
            INSERT INTO pix_transactions
            (id, transaction_id, eulen_id, address, amount_in_cents, status)
            VALUES ($1, $2, $3, $4, $5, 'pending')
            "#,
            deposit_id,
            transaction_id,
            eulen_deposit.id,
            address,
            amount_in_cents as i32
        )
        .execute(&self.conn)
        .await?;

        let deposit = pix::Deposit {
            id: deposit_id,
            transaction_id: transaction_id.clone(),
            eulen_id: eulen_deposit.id.clone(),
            amount_in_cents,
            qr_copy_paste: eulen_deposit.qr_copy_paste.clone(),
            qr_image_url: eulen_deposit.qr_image_url.clone(),
        };

        Ok(deposit)
    }

    pub async fn update_eulen_deposit_status(
        &self,
        eulen_deposit_status: &pix::EulenDepositStatus,
    ) -> Result<String, anyhow::Error> {
        let transaction = sqlx::query_as!(
            pix::PixTransaction,
            "UPDATE pix_transactions SET status = $1 WHERE eulen_id = $2 returning *",
            eulen_deposit_status.status,
            eulen_deposit_status.bank_tx_id
        )
        .fetch_one(&self.conn)
        .await?;

        Ok(transaction.transaction_id)
    }
}
