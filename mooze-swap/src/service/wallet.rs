use anyhow::Result;
use proto::wallet::wallet_service_client::WalletServiceClient;
use proto::wallet::{
    GenerateAddressRequest, GenerateAddressResponse, GenerateChangeAddressRequest, GetUtxosRequest, SignWithExtraDetailsRequest, Utxo
};
use tokio::sync::RwLock;
use tonic::transport::Channel;
use tonic::Request;

pub struct WalletClient {
    client: RwLock<WalletServiceClient<Channel>>,
}

impl WalletClient {
    pub async fn new(url: String) -> Result<Self> {
        let client = WalletServiceClient::connect(url).await?;
        Ok(Self {
            client: RwLock::new(client),
        })
    }

    pub async fn request_address(&self) -> Result<String> {
        let mut client = self.client.write().await;
        let request = Request::new(GenerateAddressRequest {});

        let response = client
            .generate_address(request)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to generate address: {}", e))?;

        Ok(response.into_inner().address)
    }

    pub async fn request_change_address(&self) -> Result<String> {
        let mut client = self.client.write().await;
        let request = Request::new(GenerateChangeAddressRequest {});

        let response = client
            .generate_change_address(request)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to generate address: {}", e))?;

        Ok(response.into_inner().address)
    }

    pub async fn get_utxos(&self, asset_id: Option<String>) -> Result<Vec<Utxo>> {
        let mut client = self.client.write().await;
        let request = Request::new(GetUtxosRequest { asset_id });

        let response = client
            .get_utxos(request)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get UTXOs: {}", e))?;

        Ok(response.into_inner().utxos)
    }

    pub async fn sign_pset(&self, pset: &str) -> Result<String> {
        let mut client = self.client.write().await;
        let request = Request::new(SignWithExtraDetailsRequest {
            pset: pset.to_string(),
        });

        let response = client
            .sign_with_extra_details(request)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to sign transaction with extra details: {}", e))?;

        Ok(response.into_inner().signed_pset)
    }
}