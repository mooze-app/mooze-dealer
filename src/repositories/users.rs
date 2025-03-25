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
        let user = sqlx::query_as!(
            users::User,
            "SELECT id, verified, referred_by FROM users WHERE id = $1",
            user_id
        )
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
}
