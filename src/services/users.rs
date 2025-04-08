use async_trait::async_trait;
use sqlx::PgPool;
use tokio::sync::oneshot;

use super::{RequestHandler, Service, ServiceError};
use crate::{models::users, repositories::users::UserRepository};

pub enum UserRequest {
    CreateUser {
        referral_code: Option<String>,
        response: oneshot::Sender<Result<users::User, ServiceError>>,
    },
    GetUser {
        id: String,
        response: oneshot::Sender<Result<Option<users::User>, ServiceError>>,
    },
    VerifyUser {
        id: String,
        response: oneshot::Sender<Result<(), ServiceError>>,
    },
    GetUserDetails {
        id: String,
        response: oneshot::Sender<Result<users::UserDetails, ServiceError>>,
    },
    GetUserDailySpending {
        id: String,
        response: oneshot::Sender<Result<i64, ServiceError>>,
    },
    GetUserReferrerAddress {
        id: String,
        response: oneshot::Sender<Result<Option<String>, ServiceError>>,
    },
}

#[derive(Clone)]
pub struct UserRequestHandler {
    repository: UserRepository,
}

impl UserRequestHandler {
    pub fn new(sql_conn: PgPool) -> Self {
        let repository = UserRepository::new(sql_conn);

        UserRequestHandler { repository }
    }

    async fn create_user(
        &self,
        referral_code: Option<String>,
    ) -> Result<users::User, ServiceError> {
        self.repository
            .insert_user(referral_code)
            .await
            .map_err(|e| {
                log::error!("Failed to create user: {:?}", e);
                ServiceError::Database(e.to_string())
            })
    }

    async fn get_user(&self, id: &str) -> Result<Option<users::User>, ServiceError> {
        self.repository
            .get_user_by_id(id)
            .await
            .map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn verify_user(&self, id: &str) -> Result<(), ServiceError> {
        self.repository
            .verify_user(id)
            .await
            .map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn get_user_daily_spending(&self, user_id: &str) -> Result<i64, ServiceError> {
        self.repository
            .get_user_daily_spending(user_id)
            .await
            .map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn get_allowed_spending(&self, user_id: &str) -> Result<i64, ServiceError> {
        self.repository
            .get_user_allowed_spending(user_id)
            .await
            .map_err(|e| ServiceError::Database(e.to_string()))
    }

    async fn get_user_details(&self, user_id: &str) -> Result<users::UserDetails, ServiceError> {
        let daily_spending = self.get_user_daily_spending(user_id).await?;
        let allowed_spending = self.get_allowed_spending(user_id).await?;

        Ok(users::UserDetails {
            id: user_id.to_string(),
            daily_spending,
            allowed_spending,
            is_verified: false,
        })
    }

    async fn get_user_referrer_address(
        &self,
        user_id: &str,
    ) -> Result<Option<String>, ServiceError> {
        let referrer = self
            .repository
            .get_user_referrer(user_id)
            .await
            .map_err(|e| ServiceError::Database(e.to_string()))?;
        if let Some(referrer) = referrer {
            let referrer_address = self
                .repository
                .get_user_referral_payment_address(&referrer)
                .await
                .map_err(|e| ServiceError::Database(e.to_string()))?;
            Ok(Some(referrer_address))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl RequestHandler<UserRequest> for UserRequestHandler {
    async fn handle_request(&self, request: UserRequest) {
        match request {
            UserRequest::CreateUser {
                referral_code,
                response,
            } => {
                let user = self.create_user(referral_code).await;
                let _ = response.send(user);
            }
            UserRequest::GetUser { id, response } => {
                let user = self.get_user(&id).await;
                let _ = response.send(user);
            }
            UserRequest::VerifyUser { id, response } => {
                let result = self.verify_user(&id).await;
                let _ = response.send(result);
            }
            UserRequest::GetUserDailySpending { id, response } => {
                let spending = self.get_user_daily_spending(&id).await;
                let _ = response.send(spending);
            }
            UserRequest::GetUserDetails { id, response } => {
                let details = self.get_user_details(&id).await;
                let _ = response.send(details);
            }
            UserRequest::GetUserReferrerAddress { id, response } => {
                let referrer = self.get_user_referrer_address(&id).await;
                let _ = response.send(referrer);
            }
        }
    }
}

pub struct UserService;

impl UserService {
    pub fn new() -> Self {
        UserService {}
    }
}

#[async_trait]
impl Service<UserRequest, UserRequestHandler> for UserService {}
