use crate::services::ServiceManager;

use actix_web::{get, post, web, HttpResponse};
use sqlx::PgPool;

mod eulen;
mod pix;
mod transaction;

struct AppState {
    name: String,
    sql_pool: PgPool,
    service_manager: ServiceManager,
}
