use crate::models::{referrals, users};

use anyhow::bail;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct UserRepository {
    conn: PgPool,
}

impl UserRepository {
    pub fn new(conn: PgPool) -> Self {
        Self { conn }
    }

    pub async fn insert_user(
        &self,
        referral_code: Option<String>,
    ) -> Result<users::User, anyhow::Error> {
        let user_id = Uuid::new_v4().hyphenated().to_string();

        let referred_by: Option<String> = match referral_code {
            Some(code) => {
                let referred_by = sqlx::query_as!(
                    referrals::Referral,
                    "SELECT * FROM referrals WHERE referral_code = $1",
                    code
                )
                .fetch_optional(&self.conn)
                .await?;
                if let Some(referral) = referred_by {
                    Some(referral.user_id)
                } else {
                    None
                }
            }
            None => None,
        };

        let user = sqlx::query_as!(
            users::User,
            r#"
                INSERT INTO users (id, referred_by)
                VALUES ($1, $2)
                RETURNING *
            "#,
            user_id,
            referred_by
        )
        .fetch_one(&self.conn)
        .await?;

        Ok(user)
    }

    pub async fn get_user_by_id(
        &self,
        user_id: &str,
    ) -> Result<Option<users::User>, anyhow::Error> {
        let user = sqlx::query_as!(users::User, "SELECT * FROM users WHERE id = $1", user_id)
            .fetch_optional(&self.conn)
            .await?;

        Ok(user)
    }

    pub async fn verify_user(&self, user_id: &str) -> Result<(), anyhow::Error> {
        let user = self.get_user_by_id(user_id).await?;

        if let Some(user) = user {
            sqlx::query!(
                "UPDATE users SET verified = true, updated_at = CURRENT_TIMESTAMP WHERE id = $1",
                user.id
            )
            .execute(&self.conn)
            .await?;

            Ok(())
        } else {
            bail!("User not found")
        }
    }

    pub async fn get_user_daily_spending(&self, user_id: &str) -> Result<i64, anyhow::Error> {
        let amount: i64 = sqlx::query_scalar(
            r#"SELECT COALESCE(SUM(amount_in_cents), 0) FROM transactions WHERE user_id = $1 AND DATE(created_at) = CURRENT_DATE AND status = 'eulen_depix_sent'"#,
        )
        .bind(user_id)
        .fetch_one(&self.conn)
        .await?;

        Ok(amount)
    }

    async fn get_user_spending(&self, user_id: &str) -> Result<i64, anyhow::Error> {
        let amount: i64 = sqlx::query_scalar(
            r#"SELECT COALESCE(SUM(amount_in_cents), 0) FROM transactions WHERE user_id = $1 AND status = 'eulen_depix_sent'"#,
        )
        .bind(user_id)
        .fetch_one(&self.conn)
        .await?;

        Ok(amount)
    }

    pub async fn get_user_allowed_spending(&self, user_id: &str) -> Result<i64, anyhow::Error> {
        let user_spending = self.get_user_spending(user_id).await?;
        let user_daily_spending = self.get_user_daily_spending(user_id).await?;

        let allowed_spending = if user_spending < 250 * 100 {
            250 * 100 - user_daily_spending
        } else if user_spending < 750 * 100 {
            750 * 100 - user_daily_spending
        } else if user_spending < 1500 * 100 {
            1500 * 100 - user_daily_spending
        } else {
            self.get_user_daily_spending(user_id).await?
        };

        Ok(allowed_spending)
    }

    pub async fn get_transaction_count(&self, user_id: &str) -> Result<i64, anyhow::Error> {
        let tx_count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(1) FROM transactions WHERE user_id = $1 AND status = 'eulen_depix_sent'"#)
                .bind(user_id)
                .fetch_one(&self.conn)
                .await?;

        Ok(tx_count)
    }

    pub async fn get_user_referrer(&self, user_id: &str) -> Result<Option<String>, anyhow::Error> {
        let user = self.get_user_by_id(user_id).await?;

        if let Some(user) = user {
            Ok(user.referred_by)
        } else {
            bail!("User not found")
        }
    }

    pub async fn get_user_referral_payment_address(
        &self,
        user_id: &str,
    ) -> Result<String, anyhow::Error> {
        let referral = sqlx::query_as!(
            referrals::Referral,
            r#"SELECT * FROM referrals WHERE user_id = $1"#,
            user_id
        )
        .fetch_one(&self.conn)
        .await?;

        Ok(referral.payment_address)
    }
}
