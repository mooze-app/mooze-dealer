use crate::models::{pix, transactions};
use anyhow::bail;
use sqlx::PgPool;
use uuid::Uuid;

pub struct TransactionRepository {
    conn: PgPool, // how to define PgPool here?
}

impl TransactionRepository {
    pub fn new(conn: PgPool) -> Self {
        TransactionRepository { conn }
    }

    pub async fn new_transaction(
        &self,
        user_id: &String,
        address: &String,
        fee_address: &String,
        amount_in_cents: i64,
        asset: &String,
        network: &String,
    ) -> Result<transactions::Transaction, anyhow::Error> {
        let is_first_transaction = self.check_first_transaction(user_id).await?;
        let daily_spending = self.get_daily_spending(user_id).await?;

        if is_first_transaction && amount_in_cents > 250 * 100 {
            bail!("ExceededFirstTransactionAmount")
        }

        if (amount_in_cents + daily_spending) > 5000 * 100 {
            bail!("ExceededDailyAmount")
        }

        let transaction_id = Uuid::new_v4().hyphenated().to_string();
        let tx = self.conn.begin().await?;

        let transaction = sqlx::query_as!(
            transactions::Transaction,
            r#"INSERT INTO transactions
            (id, user_id, address, fee_address, amount_in_cents, asset, network, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'pending')
            RETURNING *
            "#
        )
        .bind(transaction_id)
        .bind(user_id)
        .bind(address)
        .bind(fee_address)
        .bind(amount_in_cents)
        .bind(asset)
        .bind(network)?;

        tx.commit().await?;

        Ok(transaction)
    }

    async fn get_transaction(
        &self,
        id: &String,
    ) -> Result<Option<transactions::Transaction>, anyhow::Error> {
        let transaction = sqlx::query_as!(
            transactions::Transaction,
            r#"SELECT * FROM transactions WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.conn)
        .await?;

        Ok(transaction)
    }

    async fn check_first_transaction(&self, user_id: &String) -> Result<bool, anyhow::Error> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM transactions WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&self.conn)
            .await?;

        Ok(count == 0)
    }

    async fn get_daily_spending(&self, user_id: &String) -> Result<i64, anyhow::Error> {
        let amount: Option<i64> =
            sqlx::query_scalar(r#"SUM(amount_in_cents) FROM transactions WHERE user_id = $1"#)
                .bind(user_id)
                .fetch_one(&self.conn)
                .await?;

        Ok(amount.unwrap_or(0))
    }

    pub async fn update_transaction_status(
        &self,
        id: &String,
        status: &String,
    ) -> Result<(), anyhow::Error> {
        let tx = self.conn.begin().await?;
        sqlx::query!("UPDATE transactions SET status = $1 WHERE id = $2")
            .bind(status)
            .bind(id)
            .execute(&mut tx)
            .await?;

        tx.commit().await?;

        Ok(())
    }
}
