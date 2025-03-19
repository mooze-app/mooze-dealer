use actix_web::{post, web, HttpResponse};

use crate::models::pix;

// #[post("/api/eulen/deposit_status")]
pub async fn update_deposit_status(
    req: web::Json<pix::EulenDepositStatus>,
) -> Result<HttpResponse, anyhow::Error> {
    todo!();
}
