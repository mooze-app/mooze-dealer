use directories::ProjectDirs;
use std::{any::Any, error::Error, sync::Arc};
use tokio::sync::RwLock;

use anyhow::{anyhow, bail};
use lwk_common::Signer;
use lwk_signer::SwSigner;
use lwk_wollet::{
    self,
    blocking::BlockchainBackend,
    elements::{pset::PartiallySignedTransaction, TxOut, Txid},
    full_scan_with_electrum_client, ElectrumClient, ElectrumUrl, ElementsNetwork, FsPersister,
    NoPersist, WalletTxOut, Wollet,
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
        is_mainnet: bool,
    ) -> Result<Arc<LiquidRepository>, anyhow::Error> {
        let network = match is_mainnet {
            true => ElementsNetwork::Liquid,
            false => ElementsNetwork::LiquidTestnet,
        };

        // using expect here to stop at startup if wallet load fails
        let signer = SwSigner::new(mnemonic, is_mainnet)
            .expect("Could not build signer. Maybe mnemonic is invalid?");
        let descriptor = signer.wpkh_slip77_descriptor().unwrap();

        let proj_dirs = ProjectDirs::from("com", "mooze", "dealer").unwrap();
        let persister = FsPersister::new(proj_dirs.config_dir(), network, &descriptor).unwrap();

        let electrum_url =
            ElectrumUrl::new(&electrum_url, true, true).expect("Invalid Electrum URL.");
        let mut wallet = Wollet::new(network, NoPersist::new(), descriptor)
            .expect("Could not initialize wallet.");
        let mut electrum_client =
            ElectrumClient::new(&electrum_url).expect("Could not connect to Electrum server.");

        full_scan_with_electrum_client(&mut wallet, &mut electrum_client)?;

        let balances = wallet.balance().expect("Could not get balances.");

        println!("[INFO] Wallet sync completed. Balance: ");
        for (asset, balance) in balances {
            println!("Asset: {}, Balance: {}", asset, balance);
        }

        Ok(Arc::new(LiquidRepository {
            signer,
            wallet: RwLock::new(wallet),
            electrum_client: RwLock::new(electrum_client),
            network,
        }))
    }

    pub async fn update_wallet(&self) -> Result<(), anyhow::Error> {
        let mut wallet = self.wallet.write().await;
        let mut electrum_client = self.electrum_client.write().await;

        let update = electrum_client.full_scan(&*wallet)?;
        match update {
            Some(update) => {
                wallet.apply_update(update)?;
                Ok(())
            }
            None => return Ok(()),
        }
    }

    pub async fn build_transaction(
        &self,
        recipients: Vec<lwk_wollet::UnvalidatedRecipient>,
    ) -> Result<PartiallySignedTransaction, anyhow::Error> {
        let validated_recipients = recipients
            .into_iter()
            .map(|recipient| recipient.validate(self.network))
            .collect::<Result<Vec<_>, _>>()?;

        let wallet_guard = self.wallet.read().await;
        let mut tx_builder = wallet_guard.tx_builder();

        for recipient in validated_recipients {
            tx_builder = tx_builder.add_validated_recipient(recipient);
        }

        let tx = tx_builder.finish().map_err(|e| {
            dbg!(&e);
            anyhow!("Failed to finish transaction build: {e}")
        })?;

        let sanity_check = tx.sanity_check();

        if !sanity_check.is_ok() {
            dbg!("{:?}", sanity_check.unwrap_err());
            return Err(anyhow!("Transaction sanity check failed"));
        }

        Ok(tx)
    }

    pub fn sign_transaction(
        &self,
        mut pset: PartiallySignedTransaction,
    ) -> Result<PartiallySignedTransaction, anyhow::Error> {
        let count = self
            .signer
            .sign(&mut pset)
            .map_err(|e| anyhow!("Failed to sign transaction: {e}"))?; // mutates pset in-place, copy it to return to caller if needed

        Ok(pset)
    }

    pub async fn finalize_and_broadcast_transaction(
        &self,
        mut pset: PartiallySignedTransaction,
    ) -> Result<String, anyhow::Error> {
        let wallet = self.wallet.read().await;
        let client = self.electrum_client.read().await;

        let tx = wallet.finalize(&mut pset).map_err(|e| {
            log::error!("{}", e.to_string());
            anyhow!("Could not finalize transaction: {e}")
        })?;

        let txid = client.broadcast(&tx).map_err(|e| {
            log::error!("{}", e.to_string());
            anyhow!("Could not broadcast transaction: {e}")
        })?;

        let txid_string = txid.to_string();
        log::info!("TXID: {}", txid_string);

        Ok(txid_string)
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

    pub async fn generate_change_address(&self) -> Result<String, anyhow::Error> {
        let wallet = self.wallet.read().await;
        let address = wallet
            .change(None)
            .map_err(|e| anyhow!(e.to_string()))?
            .address()
            .to_string();

        Ok(address)
    }

    pub async fn get_utxos(
        &self,
        asset: Option<String>,
    ) -> Result<Vec<WalletTxOut>, anyhow::Error> {
        let wallet = self.wallet.read().await;
        let utxos = wallet
            .utxos()
            .map_err(|e| anyhow!("Failed to fetch UTXOs: {e}"))?;

        if let Some(asset) = asset {
            let filtered_utxos = utxos
                .into_iter()
                .filter(|utxo| utxo.unblinded.asset.to_string() == asset)
                .collect();

            return Ok(filtered_utxos);
        }

        Ok(utxos)
    }

    pub async fn get_asset_balance(&self, asset_id: &str) -> Result<u64, anyhow::Error> {
        let wallet = self.wallet.read().await;
        let balances = wallet
            .balance()
            .map_err(|e| anyhow!("Failed to fetch balances: {e}"))?;

        for (asset, balance) in balances {
            if asset.to_string() == asset_id {
                return Ok(balance);
            }
        }

        // If the asset is not found, return 0 balance
        Ok(0)
    }
}
