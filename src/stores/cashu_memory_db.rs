/// Minimal in-memory database implementation for CDK wallet in WASM
/// This is a simplified implementation that holds all data in memory without persistence
///
/// Since we're using NIP-60 for actual persistence via Nostr events, this database
/// only needs to work for the duration of send/receive operations.

use async_trait::async_trait;
use cdk::nuts::{CurrencyUnit, Id, KeySetInfo, Keys, KeySet, MintInfo, PublicKey, SpendingConditions, State};
use cdk::mint_url::MintUrl;
use cdk::types::ProofInfo;
use cdk_common::wallet::{MintQuote as WalletMintQuote, MeltQuote, Transaction, TransactionId, TransactionDirection};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// Import the database trait - re-exported by cdk when wallet feature is enabled
use cdk::cdk_database::{WalletDatabase as Database, Error};

#[derive(Debug, Clone)]
pub struct MemoryDatabase {
    proofs: Arc<RwLock<Vec<ProofInfo>>>,
    mints: Arc<RwLock<HashMap<MintUrl, Option<MintInfo>>>>,
    keysets: Arc<RwLock<HashMap<MintUrl, Vec<KeySetInfo>>>>,
    keyset_by_id: Arc<RwLock<HashMap<Id, KeySetInfo>>>,
    keys: Arc<RwLock<HashMap<Id, Keys>>>,
    mint_quotes: Arc<RwLock<HashMap<String, WalletMintQuote>>>,
    melt_quotes: Arc<RwLock<HashMap<String, MeltQuote>>>,
    transactions: Arc<RwLock<HashMap<[u8; 32], Transaction>>>,
    keyset_counters: Arc<RwLock<HashMap<Id, u32>>>,
}

impl MemoryDatabase {
    pub fn new() -> Self {
        Self {
            proofs: Arc::new(RwLock::new(Vec::new())),
            mints: Arc::new(RwLock::new(HashMap::new())),
            keysets: Arc::new(RwLock::new(HashMap::new())),
            keyset_by_id: Arc::new(RwLock::new(HashMap::new())),
            keys: Arc::new(RwLock::new(HashMap::new())),
            mint_quotes: Arc::new(RwLock::new(HashMap::new())),
            melt_quotes: Arc::new(RwLock::new(HashMap::new())),
            transactions: Arc::new(RwLock::new(HashMap::new())),
            keyset_counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl Database for MemoryDatabase {
    type Err = Error;

    // Mint methods
    async fn add_mint(
        &self,
        mint_url: MintUrl,
        mint_info: Option<MintInfo>,
    ) -> Result<(), Error> {
        let mut mints = self.mints.write().await;
        mints.insert(mint_url, mint_info);
        Ok(())
    }

    async fn remove_mint(&self, mint_url: MintUrl) -> Result<(), Error> {
        let mut mints = self.mints.write().await;
        mints.remove(&mint_url);
        Ok(())
    }

    async fn get_mint(&self, mint_url: MintUrl) -> Result<Option<MintInfo>, Error> {
        let mints = self.mints.read().await;
        Ok(mints.get(&mint_url).cloned().flatten())
    }

    async fn get_mints(&self) -> Result<HashMap<MintUrl, Option<MintInfo>>, Error> {
        let mints = self.mints.read().await;
        Ok(mints.clone())
    }

    async fn update_mint_url(
        &self,
        old_mint_url: MintUrl,
        new_mint_url: MintUrl,
    ) -> Result<(), Error> {
        let mut mints = self.mints.write().await;
        if let Some(info) = mints.remove(&old_mint_url) {
            mints.insert(new_mint_url, info);
        }
        Ok(())
    }

    // Keyset methods
    async fn add_mint_keysets(
        &self,
        mint_url: MintUrl,
        keysets: Vec<KeySetInfo>,
    ) -> Result<(), Error> {
        let mut all_keysets = self.keysets.write().await;
        let mut by_id = self.keyset_by_id.write().await;

        // Update keyset_by_id with incoming keysets
        for keyset in &keysets {
            by_id.insert(keyset.id.clone(), keyset.clone());
        }

        // Merge with existing keysets, deduplicating by id
        let existing = all_keysets.get(&mint_url).cloned().unwrap_or_default();
        let mut keyset_map: HashMap<Id, KeySetInfo> = HashMap::new();

        // First, add all existing keysets
        for keyset in existing {
            keyset_map.insert(keyset.id.clone(), keyset);
        }

        // Then, add/update with incoming keysets (replaces if same id)
        for keyset in keysets {
            keyset_map.insert(keyset.id.clone(), keyset);
        }

        // Collect back into a Vec and store
        let merged_keysets: Vec<KeySetInfo> = keyset_map.into_values().collect();
        all_keysets.insert(mint_url, merged_keysets);
        Ok(())
    }

    async fn get_mint_keysets(
        &self,
        mint_url: MintUrl,
    ) -> Result<Option<Vec<KeySetInfo>>, Error> {
        let keysets = self.keysets.read().await;
        Ok(keysets.get(&mint_url).cloned())
    }

    async fn get_keyset_by_id(&self, keyset_id: &Id) -> Result<Option<KeySetInfo>, Error> {
        let by_id = self.keyset_by_id.read().await;
        Ok(by_id.get(keyset_id).cloned())
    }

    // Mint quote methods
    async fn add_mint_quote(&self, quote: WalletMintQuote) -> Result<(), Error> {
        let mut quotes = self.mint_quotes.write().await;
        quotes.insert(quote.id.clone(), quote);
        Ok(())
    }

    async fn get_mint_quote(&self, quote_id: &str) -> Result<Option<WalletMintQuote>, Error> {
        let quotes = self.mint_quotes.read().await;
        Ok(quotes.get(quote_id).cloned())
    }

    async fn get_mint_quotes(&self) -> Result<Vec<WalletMintQuote>, Error> {
        let quotes = self.mint_quotes.read().await;
        Ok(quotes.values().cloned().collect())
    }

    async fn remove_mint_quote(&self, quote_id: &str) -> Result<(), Error> {
        let mut quotes = self.mint_quotes.write().await;
        quotes.remove(quote_id);
        Ok(())
    }

    // Melt quote methods
    async fn add_melt_quote(&self, quote: MeltQuote) -> Result<(), Error> {
        let mut quotes = self.melt_quotes.write().await;
        quotes.insert(quote.id.clone(), quote);
        Ok(())
    }

    async fn get_melt_quote(&self, quote_id: &str) -> Result<Option<MeltQuote>, Error> {
        let quotes = self.melt_quotes.read().await;
        Ok(quotes.get(quote_id).cloned())
    }

    async fn get_melt_quotes(&self) -> Result<Vec<MeltQuote>, Error> {
        let quotes = self.melt_quotes.read().await;
        Ok(quotes.values().cloned().collect())
    }

    async fn remove_melt_quote(&self, quote_id: &str) -> Result<(), Error> {
        let mut quotes = self.melt_quotes.write().await;
        quotes.remove(quote_id);
        Ok(())
    }

    // Keys methods
    async fn add_keys(&self, keyset: KeySet) -> Result<(), Error> {
        let mut keys = self.keys.write().await;
        keys.insert(keyset.id.clone(), keyset.keys);
        Ok(())
    }

    async fn get_keys(&self, id: &Id) -> Result<Option<Keys>, Error> {
        let keys = self.keys.read().await;
        Ok(keys.get(id).cloned())
    }

    async fn remove_keys(&self, id: &Id) -> Result<(), Error> {
        let mut keys = self.keys.write().await;
        keys.remove(id);
        Ok(())
    }

    // Proof methods
    async fn update_proofs(
        &self,
        add_proofs: Vec<ProofInfo>,
        remove_ys: Vec<PublicKey>,
    ) -> Result<(), Error> {
        let mut stored_proofs = self.proofs.write().await;

        // Remove proofs by Y value (PublicKey)
        stored_proofs.retain(|p| !remove_ys.contains(&p.proof.c));

        // Add new proofs
        stored_proofs.extend(add_proofs);

        Ok(())
    }

    async fn get_proofs(
        &self,
        _mint_url: Option<MintUrl>,
        _unit: Option<CurrencyUnit>,
        _state: Option<Vec<State>>,
        _spending_conditions: Option<Vec<SpendingConditions>>,
    ) -> Result<Vec<ProofInfo>, Error> {
        let proofs = self.proofs.read().await;
        // TODO: Apply filters if needed
        Ok(proofs.clone())
    }

    async fn update_proofs_state(&self, _ys: Vec<PublicKey>, _state: State) -> Result<(), Error> {
        // No-op: we don't track proof states separately
        Ok(())
    }

    async fn increment_keyset_counter(
        &self,
        keyset_id: &Id,
        count: u32,
    ) -> Result<u32, Error> {
        let mut counters = self.keyset_counters.write().await;
        let current = counters.entry(keyset_id.clone()).or_insert(0);
        *current += count;
        Ok(*current)
    }

    // Transaction methods
    async fn add_transaction(&self, transaction: Transaction) -> Result<(), Error> {
        let mut transactions = self.transactions.write().await;
        let id_bytes = *transaction.id().as_bytes();
        transactions.insert(id_bytes, transaction);
        Ok(())
    }

    async fn get_transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Result<Option<Transaction>, Error> {
        let transactions = self.transactions.read().await;
        let id_bytes = transaction_id.as_bytes();
        Ok(transactions.get(id_bytes).cloned())
    }

    async fn list_transactions(
        &self,
        _mint_url: Option<MintUrl>,
        _direction: Option<TransactionDirection>,
        _unit: Option<CurrencyUnit>,
    ) -> Result<Vec<Transaction>, Error> {
        let transactions = self.transactions.read().await;
        // TODO: Apply filters if needed
        Ok(transactions.values().cloned().collect())
    }

    async fn remove_transaction(&self, transaction_id: TransactionId) -> Result<(), Error> {
        let mut transactions = self.transactions.write().await;
        let id_bytes = transaction_id.as_bytes();
        transactions.remove(id_bytes);
        Ok(())
    }
}
