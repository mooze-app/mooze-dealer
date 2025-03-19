use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::{anyhow, bail};
use lwk_common::Signer;
use lwk_signer::SwSigner;
use lwk_wollet::{
    self,
    blocking::BlockchainBackend,
    elements::{pset::PartiallySignedTransaction, Txid},
    full_scan_with_electrum_client, ElectrumClient, ElectrumUrl, ElementsNetwork, FsPersister,
    Wollet,
};

trait SignerExt {
    fn wpkh_slip77_descriptor(&self) -> Result<lwk_wollet::WolletDescriptor, anyhow::Error>;
}

impl SignerExt for SwSigner {
    fn wpkh_slip77_descriptor(&self) -> Result<lwk_wollet::WolletDescriptor, anyhow::Error> {
        let is_mainnet = lwk_common::Signer::is_mainnet(self).unwrap();

        let descriptor = lwk_common::singlesig_desc(
            self,
            lwk_common::Singlesig::Wpkh,
            lwk_common::DescriptorBlindingKey::Slip77,
            is_mainnet,
        )
        .unwrap();

        match descriptor.parse() {
            Ok(d) => Ok(d),
            Err(_) => bail!("Could not parse descriptor"),
        }
    }
}

#[derive(Debug)]
pub struct LiquidRepository {
    signer: SwSigner,
    wallet: RwLock<Wollet>,
    electrum_client: RwLock<ElectrumClient>,
    network: ElementsNetwork,
}

impl LiquidRepository {
    pub fn new(
        mnemonic: &str,
        electrum_url: String,
        wallet_dir: String,
        network: ElementsNetwork,
    ) -> Result<Arc<LiquidRepository>, anyhow::Error> {
        let is_mainnet = match network {
            ElementsNetwork::Liquid => true,
            _ => false,
        };

        // using expect here to stop at startup if wallet load fails
        let signer = SwSigner::new(mnemonic, is_mainnet)
            .expect("Could not build signer. Maybe mnemonic is invalid?");
        let descriptor = signer.wpkh_slip77_descriptor().unwrap();

        let path = Path::new(&wallet_dir);
        let persister = FsPersister::new(path, network, &descriptor).unwrap();

        let electrum_url =
            ElectrumUrl::new(&electrum_url, true, true).expect("Invalid Electrum URL.");
        let mut wallet =
            Wollet::new(network, persister, descriptor).expect("Could not initialize wallet.");
        let mut electrum_client =
            ElectrumClient::new(&electrum_url).expect("Could not connect to Electrum server.");

        full_scan_with_electrum_client(&mut wallet, &mut electrum_client)?;

        Ok(Arc::new(LiquidRepository {
            signer,
            wallet: RwLock::new(wallet),
            electrum_client: RwLock::new(electrum_client),
            network,
        }))
    }

    pub async fn build_transaction(
        &self,
        recipients: Vec<lwk_wollet::UnvalidatedRecipient>,
    ) -> Result<PartiallySignedTransaction, anyhow::Error> {
        let tx = self
            .wallet
            .read()
            .await
            .tx_builder()
            .set_unvalidated_recipients(&recipients)
            .map_err(|e| anyhow!("Failed to set transaction recipients: {e}"))?
            .enable_ct_discount()
            .finish()
            .map_err(|e| anyhow!("Failed to finish transaction build: {e}"))?;

        Ok(tx)
    }

    pub fn sign_transaction(
        &self,
        mut pset: PartiallySignedTransaction,
    ) -> Result<PartiallySignedTransaction, anyhow::Error> {
        self.signer
            .sign(&mut pset)
            .map_err(|e| anyhow!("Failed to sign transaction: {e}")); // mutates pset in-place, copy it to return to caller if needed

        let signed_pset = pset.clone();
        Ok(signed_pset)
    }

    pub async fn finalize_and_broadcast_transaction(
        &self,
        mut pset: PartiallySignedTransaction,
    ) -> Result<Txid, anyhow::Error> {
        let wallet = self.wallet.read().await;
        let client = self.electrum_client.read().await;

        let tx = wallet
            .finalize(&mut pset)
            .map_err(|e| anyhow!("Could not finalize transaction: {e}"))?;

        let txid = client
            .broadcast(&tx)
            .map_err(|e| anyhow!("Could not broadcast transaction."))?;

        Ok(txid)
    }

    pub async fn generate_address(&self) -> Result<String, anyhow::Error> {
        let wallet = self.wallet.read().await;
        let address = wallet
            .address(None)
            .map_err(|e| anyhow!(e.to_string()))?
            .address()
            .to_string();

        Ok(address)
    }
}
