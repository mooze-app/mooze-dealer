use sqlx;
use uuid::Uuid;

mod sideswap;

pub struct SwapRepository {
    conn: sqlx::PgPool,
    sideswap_client: sideswap::SideswapClient,
}

impl SwapRepository {
    pub async fn new(conn: sqlx::PgPool, sideswap_client: sideswap::SideswapClient) -> Self {
        Self {
            conn,
            sideswap_client,
        }
    }

    pub async fn start(&self) {
        self.sideswap_client
            .start()
            .await
            .expect("Could not initialize Sideswap client.");
    }

    pub async fn swap_assets(
        &self,
        offer: String,
        receive: String,
        amount: i32,
        transaction_id: String,
    ) -> Result<(), anyhow::Error> {
        let swap_id = Uuid::new_v4().to_string();

        sqlx::query!(
            r#"
                INSERT INTO swaps (id, offer, receive, amount_offered, transaction_id)
                VALUES ($1, $2, $3, $4, $5)
            "#,
            swap_id,
            offer,
            receive,
            amount,
            transaction_id
        )
        .execute(&self.conn)
        .await?;

        Ok(())
    }

    pub async fn peg_in(&self) {
        todo!();
    }

    pub async fn peg_out(&self) {
        todo!();
    }
}
