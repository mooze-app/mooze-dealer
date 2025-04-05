use crate::models::transactions;
use anyhow::bail;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct TransactionRepository {
    conn: PgPool,
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
        amount_in_cents: i32,
        asset: &String,
        network: &String,
    ) -> Result<transactions::Transaction, anyhow::Error> {
        let transaction_count = self.get_transaction_count(user_id).await?;
        let daily_spending = self.get_daily_spending(user_id).await?;

        if ((transaction_count == 0 && amount_in_cents > 250 * 100)
            || (transaction_count == 1 && amount_in_cents > 750 * 100)
            || (transaction_count == 2 && amount_in_cents > 1500 * 100))
        {
            bail!("ExceededAllowedTransactionAmount")
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
            "#,
            transaction_id,
            user_id,
            address,
            fee_address,
            amount_in_cents as i32,
            asset,
            network
        )
        .fetch_one(&self.conn)
        .await?;

        tx.commit().await?;

        Ok(transaction)
    }

    pub async fn get_transaction(
        &self,
        id: &String,
    ) -> Result<Option<transactions::Transaction>, anyhow::Error> {
        let transaction = sqlx::query_as!(
            transactions::Transaction,
            r#"SELECT
            *
            FROM transactions WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.conn)
        .await?;

        Ok(transaction)
    }

    pub async fn get_allowed_spending(&self, user_id: &String) -> Result<i32, anyhow::Error> {
        let transaction_count = self.get_transaction_count(user_id).await?;

        let allowed_spending = match transaction_count {
            0 => 250 * 100,
            1 => 750 * 100,
            2 => 1500 * 100,
            _ => self.get_daily_spending(user_id).await?,
        };

        Ok(allowed_spending)
    }

    async fn get_transaction_count(&self, user_id: &String) -> Result<i64, anyhow::Error> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM transactions WHERE user_id = $1 AND status = 'eulen_depix_sent'",
        )
        .bind(user_id)
        .fetch_one(&self.conn)
        .await?;

        Ok(count)
    }

    async fn get_daily_spending(&self, user_id: &String) -> Result<i32, anyhow::Error> {
        let amount: i64 = sqlx::query_scalar(
            r#"SELECT COALESCE(SUM(amount_in_cents), 0) FROM transactions WHERE user_id = $1 AND DATE(created_at) = CURRENT_DATE AND status = 'eulen_depix_sent'"#,
        )
        .bind(user_id)
        .fetch_one(&self.conn)
        .await?;

        Ok(amount as i32)
    }

    pub async fn update_transaction_status(
        &self,
        id: &String,
        status: &String,
    ) -> Result<String, anyhow::Error> {
        let transaction = sqlx::query_as!(
            transactions::Transaction,
            "UPDATE transactions SET status = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2 RETURNING *",
            status,
            id
        )
        .fetch_one(&self.conn)
        .await?;

        Ok(transaction.id)
    }
}
