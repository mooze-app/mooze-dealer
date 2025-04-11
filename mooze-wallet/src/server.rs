use lwk_wollet::elements::pset::PartiallySignedTransaction;
use lwk_wollet::UnvalidatedRecipient;
use tonic::{Request, Response, Status};
use std::sync::Arc;
use std::str::FromStr;
use crate::wallet::Wallet;
use proto::wallet::wallet_service_server::WalletService;
use proto::wallet::*;

#[derive(Debug)]
pub struct WalletServiceImpl {
    wallet: Arc<Wallet>,
}

impl WalletServiceImpl {
    pub fn new(
        mnemonic: &str,
        electrum_url: &str,
        mainnet: bool
    ) -> Result<Self, Status> {
        let wallet = Wallet::new(mnemonic, electrum_url, mainnet)
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Self { wallet })
    }
}

#[tonic::async_trait]
impl WalletService for WalletServiceImpl {
    async fn generate_address(
        &self,
        _request: Request<GenerateAddressRequest>,
    ) -> Result<Response<GenerateAddressResponse>, Status> {
        let address = self.wallet.generate_address().await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GenerateAddressResponse { address }))
    }

    async fn generate_change_address(
        &self,
        _request: Request<GenerateChangeAddressRequest>,
    ) -> Result<Response<GenerateAddressResponse>, Status> {
        let address = self.wallet.generate_change_address().await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GenerateAddressResponse { address }))
    }

    async fn get_asset_balance(
        &self,
        request: Request<AssetBalanceRequest>,
    ) -> Result<Response<AssetBalanceResponse>, Status> {
        let asset_id = request.into_inner().asset_id;
        let balance = self.wallet.get_asset_balance(&asset_id).await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(AssetBalanceResponse { balance }))
    }

    async fn get_utxos(
        &self,
        _request: Request<GetUtxosRequest>,
    ) -> Result<Response<GetUtxosResponse>, Status> {
        let wallet_utxos = self.wallet.get_utxos(None).await.map_err(|e| Status::internal(e.to_string()))?;
        let utxos = wallet_utxos.into_iter().map(|utxo| Utxo {
            txid: utxo.outpoint.txid.to_string(),
            vout: utxo.outpoint.vout as u32,
            asset: utxo.unblinded.asset.to_string(),
            value: utxo.unblinded.value,
            asset_bf: utxo.unblinded.asset_bf.to_string(),
            value_bf: utxo.unblinded.value_bf.to_string(),
        }).collect();

        Ok(Response::new(GetUtxosResponse { utxos }))
    }

    async fn build_transaction(
        &self,
        request: Request<BuildTransactionRequest>,
    ) -> Result<Response<BuildTransactionResponse>, Status> {
        let recipients: Vec<UnvalidatedRecipient> = request.into_inner().recipients.into_iter().map(|r| UnvalidatedRecipient {
            address: r.address,
            satoshi: r.amount,
            asset: r.asset
        }).collect();

        let pset = self.wallet.build_transaction(recipients).await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(BuildTransactionResponse { pset: pset.to_string()  }))
    }

    async fn sign_transaction(
        &self,
        request: Request<SignTransactionRequest>,
    ) -> Result<Response<SignTransactionResponse>, Status> {
        let pset = PartiallySignedTransaction::from_str(&request.into_inner().pset).map_err(|e| Status::internal(e.to_string()))?;
        let signed_pset = self.wallet.sign_transaction(pset).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(SignTransactionResponse { signed_pset: signed_pset.to_string() }))
    }

    async fn sign_with_extra_details(
        &self,
        request: Request<SignWithExtraDetailsRequest>,
    ) -> Result<Response<SignWithExtraDetailsResponse>, Status> {
        let pset = PartiallySignedTransaction::from_str(&request.into_inner().pset).map_err(|e| Status::internal(e.to_string()))?;
        let signed_pset = self.wallet.sign_with_extra_details(pset).await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(SignWithExtraDetailsResponse { signed_pset: signed_pset.to_string() }))
    }

    async fn finalize_transaction(
        &self,
        request: Request<FinalizeTransactionRequest>,
    ) -> Result<Response<FinalizeTransactionResponse>, Status> {
        let pset = PartiallySignedTransaction::from_str(&request.into_inner().pset).map_err(|e| Status::internal(e.to_string()))?;
        let txid = self.wallet.finalize_and_broadcast_transaction(pset).await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(FinalizeTransactionResponse { txid }))
    }
}