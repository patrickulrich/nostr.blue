use cdk_common::database::{self, WalletDatabase};
use cdk_common::common::ProofInfo;
use cdk_common::wallet::{MintQuote, MeltQuote, Transaction, TransactionDirection, TransactionId};
use cdk_common::mint_url::MintUrl;
use cdk_common::nuts::{
    CurrencyUnit, Id, KeySetInfo, Keys, MintInfo, PublicKey as CashuPublicKey,
    SpendingConditions, State, KeySet,
};
use indexed_db_futures::prelude::*;
use indexed_db_futures::IdbQuerySource;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::IntoFuture;
use std::str::FromStr;
use std::sync::Arc;
use wasm_bindgen::JsValue;
use web_sys::IdbTransactionMode;

// Database constants
const DB_NAME: &str = "cashu_wallet_db";
const DB_VERSION: u32 = 1;

// Object store names
const STORE_MINTS: &str = "mints";
const STORE_KEYSETS: &str = "keysets";
const STORE_KEYSET_BY_ID: &str = "keyset_by_id";
const STORE_KEYS: &str = "keys";
const STORE_MINT_QUOTES: &str = "mint_quotes";
const STORE_MELT_QUOTES: &str = "melt_quotes";
const STORE_PROOFS: &str = "proofs";
const STORE_TRANSACTIONS: &str = "transactions";
const STORE_KEYSET_COUNTERS: &str = "keyset_counters";

/// IndexedDB-backed implementation of WalletDatabase
#[derive(Clone, Debug)]
pub struct IndexedDbDatabase {
    db: Arc<IdbDatabase>,
}

// SAFETY: In WASM, there's only one thread, so Send + Sync are safe
// even though IdbDatabase contains JsValue and closures
unsafe impl Send for IndexedDbDatabase {}
unsafe impl Sync for IndexedDbDatabase {}

impl IndexedDbDatabase {
    /// Helper to create a database error from a string
    fn make_error(msg: String) -> database::Error {
        database::Error::Database(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            msg,
        )))
    }

    /// Create a new IndexedDB database instance
    pub async fn new() -> Result<Self, database::Error> {
        // Open database with version (window is obtained internally by indexed_db_futures)
        let mut db_req: OpenDbRequest = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
            .map_err(|e| Self::make_error(format!("Failed to open database: {:?}", e)))?;

        // Handle upgradeneeded event to create object stores
        db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
            log::info!("IndexedDB upgrade needed, creating object stores");

            let db = evt.db();

            // Create all object stores if they don't exist
            if !db.object_store_names().any(|n| n == STORE_MINTS) {
                db.create_object_store(STORE_MINTS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_KEYSETS) {
                db.create_object_store(STORE_KEYSETS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_KEYSET_BY_ID) {
                db.create_object_store(STORE_KEYSET_BY_ID)?;
            }
            if !db.object_store_names().any(|n| n == STORE_KEYS) {
                db.create_object_store(STORE_KEYS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_MINT_QUOTES) {
                db.create_object_store(STORE_MINT_QUOTES)?;
            }
            if !db.object_store_names().any(|n| n == STORE_MELT_QUOTES) {
                db.create_object_store(STORE_MELT_QUOTES)?;
            }
            if !db.object_store_names().any(|n| n == STORE_PROOFS) {
                db.create_object_store(STORE_PROOFS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_TRANSACTIONS) {
                db.create_object_store(STORE_TRANSACTIONS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_KEYSET_COUNTERS) {
                db.create_object_store(STORE_KEYSET_COUNTERS)?;
            }

            Ok(())
        }));

        // Wait for database to open
        let db: IdbDatabase = db_req.into_future().await
            .map_err(|e| Self::make_error(format!("Failed to open database: {:?}", e)))?;

        log::info!("IndexedDB initialized successfully");

        Ok(Self { db: Arc::new(db) })
    }

    /// Helper: Get a value from a store with JSON deserialization
    async fn get_value<T>(&self, store_name: &str, key: &str) -> Result<Option<T>, database::Error>
    where
        T: for<'de> Deserialize<'de>,
    {
        let tx = self
            .db
            .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readonly)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;

        let store = tx
            .object_store(store_name)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

        let js_key = JsValue::from_str(key);
        let value_opt = store
            .get(&js_key)
            .map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))?;

        if value_opt.is_none() {
            return Ok(None);
        }

        let value = value_opt.unwrap();

        // Deserialize from JSON string
        let json_str = value
            .as_string()
            .ok_or_else(|| Self::make_error("Value is not a string".to_string()))?;

        let deserialized: T = serde_json::from_str(&json_str)
            .map_err(|e| Self::make_error(format!("JSON deserialization error: {}", e)))?;

        Ok(Some(deserialized))
    }

    /// Helper: Put a value into a store with JSON serialization
    async fn put_value<T>(&self, store_name: &str, key: &str, value: &T) -> Result<(), database::Error>
    where
        T: Serialize,
    {
        let tx = self
            .db
            .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;

        let store = tx
            .object_store(store_name)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

        // Serialize to JSON string
        let json_str = serde_json::to_string(value)
            .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;

        let js_key = JsValue::from_str(key);
        let js_value = JsValue::from_str(&json_str);

        store
            .put_key_val(&js_key, &js_value)
            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;

        // Wait for transaction to complete
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;

        Ok(())
    }

    /// Helper: Delete a value from a store
    async fn delete_value(&self, store_name: &str, key: &str) -> Result<(), database::Error> {
        let tx = self
            .db
            .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;

        let store = tx
            .object_store(store_name)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

        let js_key = JsValue::from_str(key);
        store
            .delete(&js_key)
            .map_err(|e| Self::make_error(format!("Delete error: {:?}", e)))?;

        // Wait for transaction to complete
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;

        Ok(())
    }

    /// Helper: Get all values from a store
    async fn get_all_values<T>(&self, store_name: &str) -> Result<Vec<T>, database::Error>
    where
        T: for<'de> Deserialize<'de>,
    {
        let tx = self
            .db
            .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readonly)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;

        let store = tx
            .object_store(store_name)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

        let js_values_array = store
            .get_all()
            .map_err(|e| Self::make_error(format!("Get all error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get all await error: {:?}", e)))?;

        let mut results = Vec::new();

        for js_val in js_values_array.into_iter() {
            if !js_val.is_undefined() && !js_val.is_null() {
                if let Some(json_str) = js_val.as_string() {
                    let deserialized: T = serde_json::from_str(&json_str)
                        .map_err(|e| {
                            Self::make_error(format!("JSON deserialization error: {}", e))
                        })?;
                    results.push(deserialized);
                }
            }
        }

        Ok(results)
    }

    /// Helper: Get all key-value pairs from a store
    async fn get_all_key_values<T>(&self, store_name: &str) -> Result<Vec<(String, T)>, database::Error>
    where
        T: for<'de> Deserialize<'de>,
    {
        let tx = self
            .db
            .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readonly)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;

        let store = tx
            .object_store(store_name)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

        // Get all keys
        let js_keys_array = store
            .get_all_keys()
            .map_err(|e| Self::make_error(format!("Get all keys error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get all keys await error: {:?}", e)))?;

        // Get all values
        let js_values_array = store
            .get_all()
            .map_err(|e| Self::make_error(format!("Get all error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get all await error: {:?}", e)))?;

        let mut results = Vec::new();

        // Pair up keys and values
        for (key_js, val_js) in js_keys_array.into_iter().zip(js_values_array.into_iter()) {
            if !val_js.is_undefined() && !val_js.is_null() {
                if let (Some(key_str), Some(json_str)) = (key_js.as_string(), val_js.as_string()) {
                    let deserialized: T = serde_json::from_str(&json_str)
                        .map_err(|e| {
                            Self::make_error(format!("JSON deserialization error: {}", e))
                        })?;
                    results.push((key_str, deserialized));
                }
            }
        }

        Ok(results)
    }
}

// Implement WalletDatabase trait for IndexedDbDatabase
#[async_trait::async_trait(?Send)]
impl WalletDatabase for IndexedDbDatabase {
    type Err = database::Error;

    async fn add_mint(
        &self,
        mint_url: MintUrl,
        mint_info: Option<MintInfo>,
    ) -> Result<(), Self::Err> {
        let key = mint_url.to_string();
        self.put_value(STORE_MINTS, &key, &mint_info).await
    }

    async fn remove_mint(&self, mint_url: MintUrl) -> Result<(), Self::Err> {
        let key = mint_url.to_string();
        self.delete_value(STORE_MINTS, &key).await
    }

    async fn get_mint(&self, mint_url: MintUrl) -> Result<Option<MintInfo>, Self::Err> {
        let key = mint_url.to_string();
        self.get_value::<Option<MintInfo>>(STORE_MINTS, &key)
            .await
            .map(|opt| opt.flatten())
    }

    async fn get_mints(&self) -> Result<HashMap<MintUrl, Option<MintInfo>>, Self::Err> {
        // Load all stored mint entries and rebuild the map
        let key_values = self.get_all_key_values::<Option<MintInfo>>(STORE_MINTS).await?;

        let mut result = HashMap::new();
        for (key_str, mint_info) in key_values {
            // Parse the key string back into a MintUrl
            match MintUrl::from_str(&key_str) {
                Ok(mint_url) => {
                    result.insert(mint_url, mint_info);
                }
                Err(e) => {
                    log::warn!("Failed to parse stored mint URL '{}': {:?}", key_str, e);
                    // Continue processing other entries even if one fails
                }
            }
        }

        log::debug!("Loaded {} mints from IndexedDB", result.len());
        Ok(result)
    }

    async fn update_mint_url(
        &self,
        old_mint_url: MintUrl,
        new_mint_url: MintUrl,
    ) -> Result<(), Self::Err> {
        log::info!("Migrating mint URL from {} to {}", old_mint_url, new_mint_url);

        // Perform all operations in a single multi-store transaction for atomicity
        // This ensures all data is migrated together or none at all
        let tx = self
            .db
            .transaction_on_multi_with_mode(
                &[
                    STORE_MINTS,
                    STORE_KEYSETS,
                    STORE_PROOFS,
                    STORE_MINT_QUOTES,
                    STORE_TRANSACTIONS,
                ],
                IdbTransactionMode::Readwrite,
            )
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;

        let old_url_str = old_mint_url.to_string();
        let new_url_str = new_mint_url.to_string();

        // 1. Migrate STORE_MINTS
        {
            let store = tx.object_store(STORE_MINTS)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

            let old_key = JsValue::from_str(&old_url_str);
            if let Some(value) = store.get(&old_key).map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?.await.map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))? {
                let new_key = JsValue::from_str(&new_url_str);
                store.put_key_val(&new_key, &value).map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                store.delete(&old_key).map_err(|e| Self::make_error(format!("Delete error: {:?}", e)))?;
                log::debug!("Migrated mint info");
            }
        }

        // 2. Migrate STORE_KEYSETS (keyed by mint_url, data is Vec<KeySetInfo>)
        {
            let store = tx.object_store(STORE_KEYSETS)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

            let old_key = JsValue::from_str(&old_url_str);
            if let Some(value) = store.get(&old_key).map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?.await.map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))? {
                // Simply re-key the data under new URL (KeySetInfo doesn't contain mint_url)
                let new_key = JsValue::from_str(&new_url_str);
                store.put_key_val(&new_key, &value)
                    .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                store.delete(&old_key).map_err(|e| Self::make_error(format!("Delete error: {:?}", e)))?;
                log::debug!("Migrated keysets");
            }
        }

        // 3. STORE_KEYS is keyed by keyset_id and doesn't contain mint_url - no migration needed
        // The keys are associated with keysets which are migrated above

        // 4. Migrate STORE_PROOFS (each ProofInfo contains mint_url)
        {
            let store = tx.object_store(STORE_PROOFS)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

            // Read all proofs using the transaction's store (avoid nested transaction)
            let get_all_request = store.get_all()
                .map_err(|e| Self::make_error(format!("Get all error: {:?}", e)))?;
            let all_values = get_all_request.await
                .map_err(|e| Self::make_error(format!("Get all await error: {:?}", e)))?;

            let get_all_keys_request = store.get_all_keys()
                .map_err(|e| Self::make_error(format!("Get all keys error: {:?}", e)))?;
            let all_keys = get_all_keys_request.await
                .map_err(|e| Self::make_error(format!("Get all keys await error: {:?}", e)))?;

            let mut migrated_count = 0;
            for (i, value) in all_values.iter().enumerate() {
                let key = all_keys.get(i as u32);
                if !key.is_undefined() && !key.is_null() {
                    let json_str = value.as_string().ok_or_else(|| Self::make_error("Value is not a string".to_string()))?;

                    let proof_info: ProofInfo = serde_json::from_str(&json_str)
                        .map_err(|e| Self::make_error(format!("JSON deserialization error: {}", e)))?;

                    if proof_info.mint_url == old_mint_url {
                        let mut updated_proof = proof_info;
                        updated_proof.mint_url = new_mint_url.clone();
                        let json = serde_json::to_string(&updated_proof)
                            .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
                        store.put_key_val(&key, &JsValue::from_str(&json))
                            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                        migrated_count += 1;
                    }
                }
            }
            log::debug!("Migrated {} proofs", migrated_count);
        }

        // 5. Migrate STORE_MINT_QUOTES (each MintQuote contains mint_url)
        {
            let store = tx.object_store(STORE_MINT_QUOTES)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

            // Read all mint quotes using the transaction's store (avoid nested transaction)
            let get_all_request = store.get_all()
                .map_err(|e| Self::make_error(format!("Get all error: {:?}", e)))?;
            let all_values = get_all_request.await
                .map_err(|e| Self::make_error(format!("Get all await error: {:?}", e)))?;

            let get_all_keys_request = store.get_all_keys()
                .map_err(|e| Self::make_error(format!("Get all keys error: {:?}", e)))?;
            let all_keys = get_all_keys_request.await
                .map_err(|e| Self::make_error(format!("Get all keys await error: {:?}", e)))?;

            let mut migrated_count = 0;
            for (i, value) in all_values.iter().enumerate() {
                let key = all_keys.get(i as u32);
                if !key.is_undefined() && !key.is_null() {
                    let json_str = value.as_string().ok_or_else(|| Self::make_error("Value is not a string".to_string()))?;

                    let mint_quote: MintQuote = serde_json::from_str(&json_str)
                        .map_err(|e| Self::make_error(format!("JSON deserialization error: {}", e)))?;

                    if mint_quote.mint_url == old_mint_url {
                        let mut updated_quote = mint_quote;
                        updated_quote.mint_url = new_mint_url.clone();
                        let json = serde_json::to_string(&updated_quote)
                            .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
                        store.put_key_val(&key, &JsValue::from_str(&json))
                            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                        migrated_count += 1;
                    }
                }
            }
            log::debug!("Migrated {} mint quotes", migrated_count);
        }

        // 6. STORE_MELT_QUOTES - keyed by quote_id, doesn't have mint_url field in the CDK type
        // These quotes are temporary and will expire, so no migration needed

        // 7. Migrate STORE_TRANSACTIONS (each Transaction contains mint_url)
        {
            let store = tx.object_store(STORE_TRANSACTIONS)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

            // Read all transactions using the transaction's store (avoid nested transaction)
            let get_all_request = store.get_all()
                .map_err(|e| Self::make_error(format!("Get all error: {:?}", e)))?;
            let all_values = get_all_request.await
                .map_err(|e| Self::make_error(format!("Get all await error: {:?}", e)))?;

            let get_all_keys_request = store.get_all_keys()
                .map_err(|e| Self::make_error(format!("Get all keys error: {:?}", e)))?;
            let all_keys = get_all_keys_request.await
                .map_err(|e| Self::make_error(format!("Get all keys await error: {:?}", e)))?;

            let mut migrated_count = 0;
            for (i, value) in all_values.iter().enumerate() {
                let key = all_keys.get(i as u32);
                if !key.is_undefined() && !key.is_null() {
                    let json_str = value.as_string().ok_or_else(|| Self::make_error("Value is not a string".to_string()))?;

                    let transaction: Transaction = serde_json::from_str(&json_str)
                        .map_err(|e| Self::make_error(format!("JSON deserialization error: {}", e)))?;

                    if transaction.mint_url == old_mint_url {
                        let mut updated_tx = transaction;
                        updated_tx.mint_url = new_mint_url.clone();
                        let json = serde_json::to_string(&updated_tx)
                            .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
                        store.put_key_val(&key, &JsValue::from_str(&json))
                            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                        migrated_count += 1;
                    }
                }
            }
            log::debug!("Migrated {} transactions", migrated_count);
        }

        // Commit the entire multi-store transaction
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;

        log::info!("Successfully migrated all data from {} to {}", old_mint_url, new_mint_url);
        Ok(())
    }

    async fn add_mint_keysets(
        &self,
        mint_url: MintUrl,
        keysets: Vec<KeySetInfo>,
    ) -> Result<(), Self::Err> {
        // Perform all writes in a single transaction for atomicity
        let tx = self
            .db
            .transaction_on_multi_with_mode(
                &[STORE_KEYSETS, STORE_KEYSET_BY_ID],
                IdbTransactionMode::Readwrite,
            )
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;

        // Get both object stores
        let keysets_store = tx
            .object_store(STORE_KEYSETS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

        let keyset_by_id_store = tx
            .object_store(STORE_KEYSET_BY_ID)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

        // Store keysets for this mint
        let key = mint_url.to_string();
        let json_str = serde_json::to_string(&keysets)
            .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
        let js_key = JsValue::from_str(&key);
        let js_value = JsValue::from_str(&json_str);

        keysets_store
            .put_key_val(&js_key, &js_value)
            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;

        // Store each keyset individually for lookup by ID
        for keyset in keysets {
            let keyset_key = keyset.id.to_string();
            let keyset_json = serde_json::to_string(&keyset)
                .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
            let js_keyset_key = JsValue::from_str(&keyset_key);
            let js_keyset_value = JsValue::from_str(&keyset_json);

            keyset_by_id_store
                .put_key_val(&js_keyset_key, &js_keyset_value)
                .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
        }

        // Commit the transaction
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;

        Ok(())
    }

    async fn get_mint_keysets(
        &self,
        mint_url: MintUrl,
    ) -> Result<Option<Vec<KeySetInfo>>, Self::Err> {
        let key = mint_url.to_string();
        self.get_value(STORE_KEYSETS, &key).await
    }

    async fn get_keyset_by_id(&self, keyset_id: &Id) -> Result<Option<KeySetInfo>, Self::Err> {
        let key = keyset_id.to_string();
        self.get_value(STORE_KEYSET_BY_ID, &key).await
    }

    async fn add_mint_quote(&self, quote: MintQuote) -> Result<(), Self::Err> {
        let key = quote.id.clone();
        log::debug!("Storing mint quote: {}", key);
        self.put_value(STORE_MINT_QUOTES, &key, &quote).await
    }

    async fn get_mint_quote(&self, quote_id: &str) -> Result<Option<MintQuote>, Self::Err> {
        self.get_value(STORE_MINT_QUOTES, quote_id).await
    }

    async fn get_mint_quotes(&self) -> Result<Vec<MintQuote>, Self::Err> {
        self.get_all_values(STORE_MINT_QUOTES).await
    }

    async fn remove_mint_quote(&self, quote_id: &str) -> Result<(), Self::Err> {
        log::debug!("Removing mint quote: {}", quote_id);
        self.delete_value(STORE_MINT_QUOTES, quote_id).await
    }

    async fn add_melt_quote(&self, quote: MeltQuote) -> Result<(), Self::Err> {
        let key = quote.id.clone();
        log::debug!("Storing melt quote: {}", key);
        self.put_value(STORE_MELT_QUOTES, &key, &quote).await
    }

    async fn get_melt_quote(&self, quote_id: &str) -> Result<Option<MeltQuote>, Self::Err> {
        self.get_value(STORE_MELT_QUOTES, quote_id).await
    }

    async fn get_melt_quotes(&self) -> Result<Vec<MeltQuote>, Self::Err> {
        self.get_all_values(STORE_MELT_QUOTES).await
    }

    async fn remove_melt_quote(&self, quote_id: &str) -> Result<(), Self::Err> {
        log::debug!("Removing melt quote: {}", quote_id);
        self.delete_value(STORE_MELT_QUOTES, quote_id).await
    }

    async fn add_keys(&self, keyset: KeySet) -> Result<(), Self::Err> {
        let key = keyset.id.to_string();
        self.put_value(STORE_KEYS, &key, &keyset.keys).await
    }

    async fn get_keys(&self, id: &Id) -> Result<Option<Keys>, Self::Err> {
        let key = id.to_string();
        self.get_value(STORE_KEYS, &key).await
    }

    async fn remove_keys(&self, id: &Id) -> Result<(), Self::Err> {
        let key = id.to_string();
        self.delete_value(STORE_KEYS, &key).await
    }

    async fn increment_keyset_counter(&self, keyset_id: &Id, count: u32) -> Result<u32, Self::Err> {
        log::debug!("Incrementing counter for keyset: {} by {}", keyset_id, count);

        // CRITICAL: Entire operation in single transaction for atomicity
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_KEYSET_COUNTERS, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;

        let store = tx
            .object_store(STORE_KEYSET_COUNTERS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

        let key = JsValue::from_str(&keyset_id.to_string());

        // Get current value
        let value_opt = store
            .get(&key)
            .map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))?;

        let current: u32 = if let Some(value) = value_opt {
            value.as_f64().map(|f| f as u32).unwrap_or(0)
        } else {
            0
        };

        // Increment
        let new_value = current + count;

        // Store new value (still in transaction)
        let js_value = JsValue::from_f64(new_value as f64);
        store
            .put_key_val(&key, &js_value)
            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;

        // Wait for transaction to complete (commit)
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;

        log::info!(
            "Counter for keyset {} incremented: {} → {}",
            keyset_id,
            current,
            new_value
        );

        Ok(new_value)
    }

    async fn update_proofs(
        &self,
        added: Vec<ProofInfo>,
        removed_ys: Vec<CashuPublicKey>,
    ) -> Result<(), Self::Err> {
        // Perform all operations in a single transaction for atomicity
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_PROOFS, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;

        let store = tx
            .object_store(STORE_PROOFS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

        // Add new proofs
        for proof_info in added {
            let key = proof_info.y.to_string();
            let json_str = serde_json::to_string(&proof_info)
                .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
            let js_key = JsValue::from_str(&key);
            let js_value = JsValue::from_str(&json_str);

            store
                .put_key_val(&js_key, &js_value)
                .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
        }

        // Remove proofs by Y value
        for y in removed_ys {
            let key = y.to_string();
            let js_key = JsValue::from_str(&key);

            store
                .delete(&js_key)
                .map_err(|e| Self::make_error(format!("Delete error: {:?}", e)))?;
        }

        // Commit the transaction
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;

        Ok(())
    }

    async fn get_proofs(
        &self,
        mint_url: Option<MintUrl>,
        unit: Option<CurrencyUnit>,
        state: Option<Vec<State>>,
        spending_conditions: Option<Vec<SpendingConditions>>,
    ) -> Result<Vec<ProofInfo>, Self::Err> {
        let all_proofs: Vec<ProofInfo> = self.get_all_values(STORE_PROOFS).await?;

        // Filter proofs based on all criteria
        let filtered: Vec<ProofInfo> = all_proofs
            .into_iter()
            .filter(|proof_info| {
                // Filter by mint_url
                if let Some(ref filter_mint_url) = mint_url {
                    if &proof_info.mint_url != filter_mint_url {
                        return false;
                    }
                }

                // Filter by currency unit
                if let Some(ref filter_unit) = unit {
                    if &proof_info.unit != filter_unit {
                        return false;
                    }
                }

                // Filter by state
                if let Some(ref states) = state {
                    if !states.contains(&proof_info.state) {
                        return false;
                    }
                }

                // Filter by spending conditions
                if let Some(ref filter_conditions) = spending_conditions {
                    // If filter specifies conditions, proof must have matching condition
                    match &proof_info.spending_condition {
                        Some(proof_condition) => {
                            if !filter_conditions.contains(proof_condition) {
                                return false;
                            }
                        }
                        None => {
                            // Proof has no spending condition, only matches if filter is empty
                            // or we're looking for proofs without conditions
                            return false;
                        }
                    }
                }

                true
            })
            .collect();

        Ok(filtered)
    }

    async fn update_proofs_state(&self, ys: Vec<CashuPublicKey>, state: State) -> Result<(), Self::Err> {
        // Perform all operations in a single write transaction for atomicity
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_PROOFS, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;

        let store = tx
            .object_store(STORE_PROOFS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;

        // Update each proof's state within the transaction
        for y in ys {
            let key = y.to_string();
            let js_key = JsValue::from_str(&key);

            // Get existing proof
            let value_opt = store
                .get(&js_key)
                .map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?
                .await
                .map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))?;

            if let Some(value) = value_opt {
                // Deserialize the proof info
                let json_str = value
                    .as_string()
                    .ok_or_else(|| Self::make_error("Value is not a string".to_string()))?;

                let mut proof_info: ProofInfo = serde_json::from_str(&json_str)
                    .map_err(|e| Self::make_error(format!("JSON deserialization error: {}", e)))?;

                // Update state
                proof_info.state = state;

                // Write back
                let updated_json = serde_json::to_string(&proof_info)
                    .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
                let js_value = JsValue::from_str(&updated_json);

                store
                    .put_key_val(&js_key, &js_value)
                    .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
            }
        }

        // Commit the transaction
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;

        Ok(())
    }

    async fn add_transaction(&self, transaction: Transaction) -> Result<(), Self::Err> {
        let key = transaction.id().to_string();
        self.put_value(STORE_TRANSACTIONS, &key, &transaction).await
    }

    async fn get_transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Result<Option<Transaction>, Self::Err> {
        let key = transaction_id.to_string();
        self.get_value(STORE_TRANSACTIONS, &key).await
    }

    async fn list_transactions(
        &self,
        mint_url: Option<MintUrl>,
        direction: Option<TransactionDirection>,
        unit: Option<CurrencyUnit>,
    ) -> Result<Vec<Transaction>, Self::Err> {
        let all_transactions: Vec<Transaction> = self.get_all_values(STORE_TRANSACTIONS).await?;

        // Filter transactions based on all criteria
        let filtered: Vec<Transaction> = all_transactions
            .into_iter()
            .filter(|transaction| {
                // Filter by mint_url
                if let Some(ref filter_mint_url) = mint_url {
                    if &transaction.mint_url != filter_mint_url {
                        return false;
                    }
                }

                // Filter by transaction direction (Incoming/Outgoing)
                if let Some(ref filter_direction) = direction {
                    if &transaction.direction != filter_direction {
                        return false;
                    }
                }

                // Filter by currency unit
                if let Some(ref filter_unit) = unit {
                    if &transaction.unit != filter_unit {
                        return false;
                    }
                }

                true
            })
            .collect();

        Ok(filtered)
    }

    async fn remove_transaction(&self, transaction_id: TransactionId) -> Result<(), Self::Err> {
        let key = transaction_id.to_string();
        self.delete_value(STORE_TRANSACTIONS, &key).await
    }
}
