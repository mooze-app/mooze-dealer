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
            .map_err(|e| ServiceError::Database(e.to_string()))
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
