use actix_web::{get, post, web, HttpResponse};
use anyhow::{anyhow, bail};

use crate::models::server::pix::PixDeposit;

// #[post("/api/pix/deposit")]
async fn new_pix_deposit(req: web::Json<PixDeposit>) -> Result<HttpResponse, anyhow::Error> {
    todo!();
}
