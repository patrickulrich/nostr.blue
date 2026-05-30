//! # IndexedDB Wallet Database
//!
//! Browser-native implementation of CDK's `WalletDatabase` trait using IndexedDB.
//!
//! ## Overview
//!
//! This module provides persistent storage for Cashu wallet data in web browsers.
//! It implements the `WalletDatabase` trait from CDK, enabling full ecash wallet
//! functionality in WASM environments.
//!
//! ## Storage Model
//!
//! Data is stored as JSON strings in IndexedDB object stores:
//! - `mints` - Mint URLs and their info
//! - `keysets` - Keysets per mint
//! - `keyset_by_id` - Keyset lookup by ID
//! - `keys` - Cryptographic keys
//! - `mint_quotes` - Pending mint (receive) quotes
//! - `melt_quotes` - Pending melt (send) quotes
//! - `proofs` - Ecash proofs (tokens)
//! - `transactions` - Transaction history
//! - `keyset_counters` - Deterministic derivation counters
//!
//! ## Thread Safety
//!
//! In WASM, JavaScript is single-threaded. `Send` and `Sync` are implemented
//! via unsafe impl since there's no actual concurrency. IndexedDB handles
//! transaction serialization internally.
//!
//! ## Storage Limits
//!
//! Subject to browser storage quota (typically ~50MB, varies by browser).
//! Use `navigator.storage.estimate()` to check available space.
//!
//! ## Platform Support
//!
//! This module only compiles on wasm32 targets. A stub type is provided for
//! native targets to allow type checking, but it cannot be instantiated.
#![cfg_attr(
    not(target_arch = "wasm32"),
    allow(dead_code, unused_imports, unused_variables)
)]
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[cfg(not(target_arch = "wasm32"))]
mod native_stub {
    use crate::stores::cashu::{PendingNostrEvent, SyncState};
    use cdk_common::common::ProofInfo;
    use cdk_common::database::{self, WalletDatabase};
    use cdk_common::mint_url::MintUrl;
    use cdk_common::nuts::{
        CurrencyUnit, Id, KeySet, KeySetInfo, Keys, MintInfo, PublicKey as CashuPublicKey,
        SpendingConditions, State,
    };
    use cdk_common::wallet::{
        MeltQuote, MintQuote, Transaction, TransactionDirection, TransactionId,
    };
    use cdk_common::wallet::{WalletSaga, P2PKSigningKey};
    use bitcoin::bip32::DerivationPath;
    use std::collections::HashMap;
    /// Stub type for native targets - cannot be instantiated
    #[derive(Clone, Debug)]
    pub struct IndexedDbDatabase {
        _private: (),
    }
    unsafe impl Send for IndexedDbDatabase {}
    unsafe impl Sync for IndexedDbDatabase {}
    impl IndexedDbDatabase {
        fn make_error(msg: String) -> database::Error {
            database::Error::Database(Box::new(std::io::Error::other(msg)))
        }
        pub async fn new() -> Result<Self, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn add_pending_event(
            &self,
            _event: &PendingNostrEvent,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn remove_pending_event(&self, _event_id: &str) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn get_all_pending_events(
            &self,
        ) -> Result<Vec<PendingNostrEvent>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn update_pending_event(
            &self,
            _event: &PendingNostrEvent,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn load_sync_state(&self) -> Result<Option<SyncState>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn save_sync_state(&self, _state: &SyncState) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn clear_sync_state(&self) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn save_order(
            &self,
            _order: &crate::utils::nip99::ShopOrder,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn get_order(
            &self,
            _order_id: &str,
        ) -> Result<Option<crate::utils::nip99::ShopOrder>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn get_all_orders(
            &self,
        ) -> Result<Vec<crate::utils::nip99::ShopOrder>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn update_order(
            &self,
            _order: &crate::utils::nip99::ShopOrder,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn delete_order(&self, _order_id: &str) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn save_pending_mint_secrets(
            &self,
            _secrets: &std::collections::HashMap<String, u64>,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn load_pending_mint_secrets(
            &self,
        ) -> Result<Option<std::collections::HashMap<String, u64>>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn clear_pending_mint_secrets(&self) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn save_in_flight_melt_requests(
            &self,
            _requests: &[crate::stores::cashu::types::InFlightMeltRequest],
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn load_in_flight_melt_requests(
            &self,
        ) -> Result<Option<Vec<crate::stores::cashu::types::InFlightMeltRequest>>, database::Error>
        {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn clear_in_flight_melt_requests(&self) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn save_nutzap_settings(
            &self,
            _settings: &crate::stores::cashu::nutzap::NutzapInfo,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn load_nutzap_settings(
            &self,
        ) -> Result<Option<crate::stores::cashu::nutzap::NutzapInfo>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn save_pending_nutzaps(
            &self,
            _nutzaps: &[crate::stores::cashu::nutzap::PendingNutzap],
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn load_pending_nutzaps(
            &self,
        ) -> Result<Option<Vec<crate::stores::cashu::nutzap::PendingNutzap>>, database::Error>
        {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn add_queued_event(
            &self,
            _event: &crate::stores::publish_queue::types::QueuedEvent,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn get_all_queued_events(
            &self,
        ) -> Result<Vec<crate::stores::publish_queue::types::QueuedEvent>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn remove_queued_event(
            &self,
            _event_id: &str,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        pub async fn update_queued_event(
            &self,
            _event: &crate::stores::publish_queue::types::QueuedEvent,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
    }
    #[async_trait::async_trait]
    impl WalletDatabase<database::Error> for IndexedDbDatabase {
        async fn add_mint(
            &self,
            _mint_url: MintUrl,
            _mint_info: Option<MintInfo>,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn remove_mint(&self, _mint_url: MintUrl) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_mint(&self, _mint_url: MintUrl) -> Result<Option<MintInfo>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_mints(&self) -> Result<HashMap<MintUrl, Option<MintInfo>>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn update_mint_url(
            &self,
            _old_mint_url: MintUrl,
            _new_mint_url: MintUrl,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn add_mint_keysets(
            &self,
            _mint_url: MintUrl,
            _keysets: Vec<KeySetInfo>,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_mint_keysets(
            &self,
            _mint_url: MintUrl,
        ) -> Result<Option<Vec<KeySetInfo>>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_keyset_by_id(&self, _keyset_id: &Id) -> Result<Option<KeySetInfo>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn add_mint_quote(&self, _quote: MintQuote) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_mint_quote(&self, _quote_id: &str) -> Result<Option<MintQuote>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_mint_quotes(&self) -> Result<Vec<MintQuote>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn remove_mint_quote(&self, _quote_id: &str) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn add_melt_quote(&self, _quote: MeltQuote) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_melt_quote(&self, _quote_id: &str) -> Result<Option<MeltQuote>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_melt_quotes(&self) -> Result<Vec<MeltQuote>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn remove_melt_quote(&self, _quote_id: &str) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn add_keys(&self, _keys: KeySet) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_keys(&self, _keyset_id: &Id) -> Result<Option<Keys>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn remove_keys(&self, _keyset_id: &Id) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn increment_keyset_counter(
            &self,
            _keyset_id: &Id,
            _count: u32,
        ) -> Result<u32, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn update_proofs(
            &self,
            _added: Vec<ProofInfo>,
            _removed_ys: Vec<CashuPublicKey>,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_proofs(
            &self,
            _mint_url: Option<MintUrl>,
            _unit: Option<CurrencyUnit>,
            _state: Option<Vec<State>>,
            _spending_conditions: Option<Vec<SpendingConditions>>,
        ) -> Result<Vec<ProofInfo>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn update_proofs_state(
            &self,
            _ys: Vec<CashuPublicKey>,
            _state: State,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn add_transaction(&self, _transaction: Transaction) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_transaction(
            &self,
            _transaction_id: TransactionId,
        ) -> Result<Option<Transaction>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn list_transactions(
            &self,
            _mint_url: Option<MintUrl>,
            _direction: Option<TransactionDirection>,
            _unit: Option<CurrencyUnit>,
        ) -> Result<Vec<Transaction>, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn remove_transaction(
            &self,
            _transaction_id: TransactionId,
        ) -> Result<(), database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_balance(
            &self,
            _mint_url: Option<MintUrl>,
            _unit: Option<CurrencyUnit>,
            _state: Option<Vec<State>>,
        ) -> Result<u64, database::Error> {
            Err(Self::make_error(
                "IndexedDB is only available on wasm32 targets".to_string(),
            ))
        }
        async fn get_unissued_mint_quotes(&self) -> Result<Vec<MintQuote>, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn get_proofs_by_ys(&self, _ys: Vec<CashuPublicKey>) -> Result<Vec<ProofInfo>, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn add_saga(&self, _saga: WalletSaga) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn get_saga(&self, _id: &uuid::Uuid) -> Result<Option<WalletSaga>, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn update_saga(&self, _saga: WalletSaga) -> Result<bool, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn delete_saga(&self, _id: &uuid::Uuid) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn get_incomplete_sagas(&self) -> Result<Vec<WalletSaga>, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn reserve_proofs(&self, _ys: Vec<CashuPublicKey>, _operation_id: &uuid::Uuid) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn release_proofs(&self, _operation_id: &uuid::Uuid) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn get_reserved_proofs(&self, _operation_id: &uuid::Uuid) -> Result<Vec<ProofInfo>, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn reserve_melt_quote(&self, _quote_id: &str, _operation_id: &uuid::Uuid) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn release_melt_quote(&self, _operation_id: &uuid::Uuid) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn reserve_mint_quote(&self, _quote_id: &str, _operation_id: &uuid::Uuid) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn release_mint_quote(&self, _operation_id: &uuid::Uuid) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn kv_read(&self, _primary_namespace: &str, _secondary_namespace: &str, _key: &str) -> Result<Option<Vec<u8>>, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn kv_list(&self, _primary_namespace: &str, _secondary_namespace: &str) -> Result<Vec<String>, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn kv_write(&self, _primary_namespace: &str, _secondary_namespace: &str, _key: &str, _value: &[u8]) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn kv_remove(&self, _primary_namespace: &str, _secondary_namespace: &str, _key: &str) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn add_p2pk_key(&self, _pubkey: &CashuPublicKey, _derivation_path: DerivationPath, _derivation_index: u32) -> Result<(), database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn get_p2pk_key(&self, _pubkey: &CashuPublicKey) -> Result<Option<P2PKSigningKey>, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn list_p2pk_keys(&self) -> Result<Vec<P2PKSigningKey>, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
        async fn latest_p2pk(&self) -> Result<Option<P2PKSigningKey>, database::Error> {
            Err(Self::make_error("IndexedDB is only available on wasm32 targets".to_string()))
        }
    }
}
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use cdk_common::common::ProofInfo;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use cdk_common::database::{self, WalletDatabase};
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use cdk_common::mint_url::MintUrl;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use cdk_common::nuts::{
    CurrencyUnit, Id, KeySet, KeySetInfo, Keys, MintInfo, PublicKey as CashuPublicKey,
    SpendingConditions, State,
};
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use cdk_common::wallet::{MeltQuote, MintQuote, Transaction, TransactionDirection, TransactionId};
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use cdk_common::wallet::{WalletSaga, P2PKSigningKey};
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use bitcoin::bip32::DerivationPath;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use indexed_db_futures::prelude::*;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use indexed_db_futures::IdbQuerySource;
#[cfg(not(target_arch = "wasm32"))]
pub use native_stub::IndexedDbDatabase;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use serde::{Deserialize, Serialize};
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use std::collections::HashMap;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use std::future::IntoFuture;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use std::rc::Rc;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use std::str::FromStr;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use wasm_bindgen::JsValue;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
use web_sys::IdbTransactionMode;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const DB_NAME: &str = "cashu_wallet_db";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const DB_VERSION: u32 = 8;
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_MINTS: &str = "mints";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_KEYSETS: &str = "keysets";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_KEYSET_BY_ID: &str = "keyset_by_id";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_KEYS: &str = "keys";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_MINT_QUOTES: &str = "mint_quotes";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_MELT_QUOTES: &str = "melt_quotes";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_PROOFS: &str = "proofs";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_TRANSACTIONS: &str = "transactions";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_KEYSET_COUNTERS: &str = "keyset_counters";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_PENDING_EVENTS: &str = "pending_events";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_SYNC_STATE: &str = "sync_state";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_PENDING_SECRETS: &str = "pending_secrets";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_IN_FLIGHT_MELTS: &str = "in_flight_melts";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_NUTZAP_SETTINGS: &str = "nutzap_settings";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_PENDING_NUTZAPS: &str = "pending_nutzaps";
const STORE_PUBLISH_QUEUE: &str = "publish_queue";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_SAGAS: &str = "sagas";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_PROOF_RESERVATIONS: &str = "proof_reservations";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_QUOTE_RESERVATIONS: &str = "quote_reservations";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_KV: &str = "kv_store";
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
const STORE_P2PK_KEYS: &str = "p2pk_keys";
/// IndexedDB-backed implementation of WalletDatabase
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
#[derive(Clone, Debug)]
pub struct IndexedDbDatabase {
    db: Rc<IdbDatabase>,
}
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
unsafe impl Send for IndexedDbDatabase {}
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
unsafe impl Sync for IndexedDbDatabase {}
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
impl IndexedDbDatabase {
    /// Helper to create a database error from a string
    fn make_error(msg: String) -> database::Error {
        database::Error::Database(Box::new(std::io::Error::other(msg)))
    }
    /// Create a new IndexedDB database instance
    pub async fn new() -> Result<Self, database::Error> {
        let mut db_req: OpenDbRequest = IdbDatabase::open_u32(DB_NAME, DB_VERSION)
            .map_err(|e| Self::make_error(format!("Failed to open database: {:?}", e)))?;
        db_req.set_on_upgrade_needed(Some(|evt: &IdbVersionChangeEvent| {
            log::info!("IndexedDB upgrade needed, creating object stores");
            let db = evt.db();
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
            if !db.object_store_names().any(|n| n == STORE_PENDING_EVENTS) {
                db.create_object_store(STORE_PENDING_EVENTS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_SYNC_STATE) {
                db.create_object_store(STORE_SYNC_STATE)?;
            }
            if !db.object_store_names().any(|n| n == STORE_PENDING_SECRETS) {
                db.create_object_store(STORE_PENDING_SECRETS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_IN_FLIGHT_MELTS) {
                db.create_object_store(STORE_IN_FLIGHT_MELTS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_NUTZAP_SETTINGS) {
                db.create_object_store(STORE_NUTZAP_SETTINGS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_PENDING_NUTZAPS) {
                db.create_object_store(STORE_PENDING_NUTZAPS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_PUBLISH_QUEUE) {
                db.create_object_store(STORE_PUBLISH_QUEUE)?;
            }
            if !db.object_store_names().any(|n| n == STORE_SAGAS) {
                db.create_object_store(STORE_SAGAS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_PROOF_RESERVATIONS) {
                db.create_object_store(STORE_PROOF_RESERVATIONS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_QUOTE_RESERVATIONS) {
                db.create_object_store(STORE_QUOTE_RESERVATIONS)?;
            }
            if !db.object_store_names().any(|n| n == STORE_KV) {
                db.create_object_store(STORE_KV)?;
            }
            if !db.object_store_names().any(|n| n == STORE_P2PK_KEYS) {
                db.create_object_store(STORE_P2PK_KEYS)?;
            }
            Ok(())
        }));
        let db: IdbDatabase = db_req
            .into_future()
            .await
            .map_err(|e| Self::make_error(format!("Failed to open database: {:?}", e)))?;
        log::info!("IndexedDB initialized successfully");
        Ok(Self { db: Rc::new(db) })
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
        let json_str = value
            .as_string()
            .ok_or_else(|| Self::make_error("Value is not a string".to_string()))?;
        let deserialized: T = serde_json::from_str(&json_str)
            .map_err(|e| Self::make_error(format!("JSON deserialization error: {}", e)))?;
        Ok(Some(deserialized))
    }
    /// Helper: Put a value into a store with JSON serialization
    async fn put_value<T>(
        &self,
        store_name: &str,
        key: &str,
        value: &T,
    ) -> Result<(), database::Error>
    where
        T: Serialize + ?Sized,
    {
        let tx = self
            .db
            .transaction_on_one_with_mode(store_name, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;
        let store = tx
            .object_store(store_name)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
        let json_str = serde_json::to_string(value)
            .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
        let js_key = JsValue::from_str(key);
        let js_value = JsValue::from_str(&json_str);
        store
            .put_key_val(&js_key, &js_value)
            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
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
                    let deserialized: T = serde_json::from_str(&json_str).map_err(|e| {
                        Self::make_error(format!("JSON deserialization error: {}", e))
                    })?;
                    results.push(deserialized);
                }
            }
        }
        Ok(results)
    }
    /// Helper: Get all key-value pairs from a store
    async fn get_all_key_values<T>(
        &self,
        store_name: &str,
    ) -> Result<Vec<(String, T)>, database::Error>
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
        let js_keys_array = store
            .get_all_keys()
            .map_err(|e| Self::make_error(format!("Get all keys error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get all keys await error: {:?}", e)))?;
        let js_values_array = store
            .get_all()
            .map_err(|e| Self::make_error(format!("Get all error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get all await error: {:?}", e)))?;
        let mut results = Vec::new();
        for (key_js, val_js) in js_keys_array.into_iter().zip(js_values_array) {
            if !val_js.is_undefined() && !val_js.is_null() {
                if let (Some(key_str), Some(json_str)) = (key_js.as_string(), val_js.as_string()) {
                    let deserialized: T = serde_json::from_str(&json_str).map_err(|e| {
                        Self::make_error(format!("JSON deserialization error: {}", e))
                    })?;
                    results.push((key_str, deserialized));
                }
            }
        }
        Ok(results)
    }
    /// Store a pending Nostr event
    #[allow(dead_code)]
    pub async fn add_pending_event(
        &self,
        event: &crate::stores::cashu::types::PendingNostrEvent,
    ) -> Result<(), database::Error> {
        let key = event.id.clone();
        self.put_value(STORE_PENDING_EVENTS, &key, event).await
    }
    /// Get a pending event by ID
    #[allow(dead_code)]
    pub async fn get_pending_event(
        &self,
        event_id: &str,
    ) -> Result<Option<crate::stores::cashu::types::PendingNostrEvent>, database::Error> {
        self.get_value(STORE_PENDING_EVENTS, event_id).await
    }
    /// Get all pending events
    #[allow(dead_code)]
    pub async fn get_all_pending_events(
        &self,
    ) -> Result<Vec<crate::stores::cashu::types::PendingNostrEvent>, database::Error> {
        self.get_all_values(STORE_PENDING_EVENTS).await
    }
    /// Remove a pending event
    #[allow(dead_code)]
    pub async fn remove_pending_event(&self, event_id: &str) -> Result<(), database::Error> {
        self.delete_value(STORE_PENDING_EVENTS, event_id).await
    }
    /// Update a pending event (for retry count increments)
    #[allow(dead_code)]
    pub async fn update_pending_event(
        &self,
        event: &crate::stores::cashu::types::PendingNostrEvent,
    ) -> Result<(), database::Error> {
        let key = event.id.clone();
        self.put_value(STORE_PENDING_EVENTS, &key, event).await
    }
    #[allow(dead_code)]
    pub async fn add_queued_event(
        &self,
        event: &crate::stores::publish_queue::types::QueuedEvent,
    ) -> Result<(), database::Error> {
        let key = event.id.clone();
        self.put_value(STORE_PUBLISH_QUEUE, &key, event).await
    }
    #[allow(dead_code)]
    pub async fn get_all_queued_events(
        &self,
    ) -> Result<Vec<crate::stores::publish_queue::types::QueuedEvent>, database::Error> {
        self.get_all_values(STORE_PUBLISH_QUEUE).await
    }
    #[allow(dead_code)]
    pub async fn remove_queued_event(&self, event_id: &str) -> Result<(), database::Error> {
        self.delete_value(STORE_PUBLISH_QUEUE, event_id).await
    }
    #[allow(dead_code)]
    pub async fn update_queued_event(
        &self,
        event: &crate::stores::publish_queue::types::QueuedEvent,
    ) -> Result<(), database::Error> {
        let key = event.id.clone();
        self.put_value(STORE_PUBLISH_QUEUE, &key, event).await
    }
    /// Save sync state for incremental Nostr event fetching
    pub async fn save_sync_state(
        &self,
        state: &crate::stores::cashu::types::SyncState,
    ) -> Result<(), database::Error> {
        self.put_value(STORE_SYNC_STATE, "current", state).await
    }
    /// Load sync state for incremental Nostr event fetching
    pub async fn load_sync_state(
        &self,
    ) -> Result<Option<crate::stores::cashu::types::SyncState>, database::Error> {
        self.get_value(STORE_SYNC_STATE, "current").await
    }
    /// Clear sync state (forces full resync on next fetch)
    #[allow(dead_code)]
    pub async fn clear_sync_state(&self) -> Result<(), database::Error> {
        self.delete_value(STORE_SYNC_STATE, "current").await
    }
    /// Save pending mint secrets with timestamps
    ///
    /// These track proofs that are currently pending at the mint level
    /// (e.g., during lightning payments). Persisting ensures we don't lose
    /// this state on app restart.
    pub async fn save_pending_mint_secrets(
        &self,
        secrets: &std::collections::HashMap<String, u64>,
    ) -> Result<(), database::Error> {
        self.put_value(STORE_PENDING_SECRETS, "current", secrets)
            .await
    }
    /// Load pending mint secrets
    ///
    /// Returns the map of proof secrets to timestamps, or None if no data exists.
    pub async fn load_pending_mint_secrets(
        &self,
    ) -> Result<Option<std::collections::HashMap<String, u64>>, database::Error> {
        self.get_value(STORE_PENDING_SECRETS, "current").await
    }
    /// Clear pending mint secrets
    #[allow(dead_code)]
    pub async fn clear_pending_mint_secrets(&self) -> Result<(), database::Error> {
        self.delete_value(STORE_PENDING_SECRETS, "current").await
    }
    /// Save in-flight melt requests for crash recovery
    ///
    /// CRITICAL: This must be called BEFORE the melt network call to ensure
    /// we can recover change proofs if the app crashes during the operation.
    /// The melt operation should be aborted if this fails.
    pub async fn save_in_flight_melt_requests(
        &self,
        requests: &[crate::stores::cashu::types::InFlightMeltRequest],
    ) -> Result<(), database::Error> {
        self.put_value(STORE_IN_FLIGHT_MELTS, "current", requests)
            .await
    }
    /// Load in-flight melt requests for crash recovery
    ///
    /// Returns the list of in-flight melt requests from the previous session,
    /// or None if no data exists.
    pub async fn load_in_flight_melt_requests(
        &self,
    ) -> Result<Option<Vec<crate::stores::cashu::types::InFlightMeltRequest>>, database::Error>
    {
        self.get_value(STORE_IN_FLIGHT_MELTS, "current").await
    }
    /// Clear in-flight melt requests after recovery is complete
    #[allow(dead_code)]
    pub async fn clear_in_flight_melt_requests(&self) -> Result<(), database::Error> {
        self.delete_value(STORE_IN_FLIGHT_MELTS, "current").await
    }
    /// Save nutzap settings (NutzapInfo)
    ///
    /// Persists the user's nutzap configuration including P2PK pubkey,
    /// accepted mints, and delivery relays.
    pub async fn save_nutzap_settings(
        &self,
        settings: &crate::stores::cashu::nutzap::NutzapInfo,
    ) -> Result<(), database::Error> {
        self.put_value(STORE_NUTZAP_SETTINGS, "current", settings)
            .await
    }
    /// Load nutzap settings
    ///
    /// Returns the saved nutzap configuration, or None if not configured.
    pub async fn load_nutzap_settings(
        &self,
    ) -> Result<Option<crate::stores::cashu::nutzap::NutzapInfo>, database::Error> {
        self.get_value(STORE_NUTZAP_SETTINGS, "current").await
    }
    /// Save pending nutzaps awaiting redemption
    ///
    /// Persists nutzaps that have been received but not yet redeemed.
    pub async fn save_pending_nutzaps(
        &self,
        nutzaps: &[crate::stores::cashu::nutzap::PendingNutzap],
    ) -> Result<(), database::Error> {
        self.put_value(STORE_PENDING_NUTZAPS, "current", nutzaps)
            .await
    }
    /// Load pending nutzaps
    ///
    /// Returns the list of pending nutzaps, or None if none exist.
    pub async fn load_pending_nutzaps(
        &self,
    ) -> Result<Option<Vec<crate::stores::cashu::nutzap::PendingNutzap>>, database::Error> {
        self.get_value(STORE_PENDING_NUTZAPS, "current").await
    }
}
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
#[async_trait::async_trait(?Send)]
impl WalletDatabase<database::Error> for IndexedDbDatabase {
    async fn add_mint(
        &self,
        mint_url: MintUrl,
        mint_info: Option<MintInfo>,
    ) -> Result<(), database::Error> {
        let key = mint_url.to_string();
        self.put_value(STORE_MINTS, &key, &mint_info).await
    }
    async fn remove_mint(&self, mint_url: MintUrl) -> Result<(), database::Error> {
        let key = mint_url.to_string();
        self.delete_value(STORE_MINTS, &key).await
    }
    async fn get_mint(&self, mint_url: MintUrl) -> Result<Option<MintInfo>, database::Error> {
        let key = mint_url.to_string();
        self.get_value::<Option<MintInfo>>(STORE_MINTS, &key)
            .await
            .map(|opt| opt.flatten())
    }
    async fn get_mints(&self) -> Result<HashMap<MintUrl, Option<MintInfo>>, database::Error> {
        let key_values = self
            .get_all_key_values::<Option<MintInfo>>(STORE_MINTS)
            .await?;
        let mut result = HashMap::new();
        for (key_str, mint_info) in key_values {
            match MintUrl::from_str(&key_str) {
                Ok(mint_url) => {
                    result.insert(mint_url, mint_info);
                }
                Err(e) => {
                    log::warn!("Failed to parse stored mint URL '{}': {:?}", key_str, e);
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
    ) -> Result<(), database::Error> {
        log::info!(
            "Migrating mint URL from {} to {}",
            old_mint_url,
            new_mint_url
        );
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
        {
            let new_key = JsValue::from_str(&new_url_str);
            let mints_store = tx
                .object_store(STORE_MINTS)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
            if mints_store
                .get(&new_key)
                .map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?
                .await
                .map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))?
                .is_some()
            {
                return Err(Self::make_error(format!(
                    "Cannot migrate: destination mint URL {} already exists in STORE_MINTS. \
                    This would overwrite existing mint data.",
                    new_url_str,
                )));
            }
            let keysets_store = tx
                .object_store(STORE_KEYSETS)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
            if keysets_store
                .get(&new_key)
                .map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?
                .await
                .map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))?
                .is_some()
            {
                return Err(Self::make_error(format!(
                    "Cannot migrate: destination mint URL {} already exists in STORE_KEYSETS. \
                    This would overwrite existing keyset data.",
                    new_url_str,
                )));
            }
            log::debug!(
                "Destination mint URL validation passed - no existing data will be overwritten"
            );
        }
        {
            let store = tx
                .object_store(STORE_MINTS)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
            let old_key = JsValue::from_str(&old_url_str);
            if let Some(value) = store
                .get(&old_key)
                .map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?
                .await
                .map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))?
            {
                let new_key = JsValue::from_str(&new_url_str);
                store
                    .put_key_val(&new_key, &value)
                    .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                store
                    .delete(&old_key)
                    .map_err(|e| Self::make_error(format!("Delete error: {:?}", e)))?;
                log::debug!("Migrated mint info");
            }
        }
        {
            let store = tx
                .object_store(STORE_KEYSETS)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
            let old_key = JsValue::from_str(&old_url_str);
            if let Some(value) = store
                .get(&old_key)
                .map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?
                .await
                .map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))?
            {
                let new_key = JsValue::from_str(&new_url_str);
                store
                    .put_key_val(&new_key, &value)
                    .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                store
                    .delete(&old_key)
                    .map_err(|e| Self::make_error(format!("Delete error: {:?}", e)))?;
                log::debug!("Migrated keysets");
            }
        }
        {
            let store = tx
                .object_store(STORE_PROOFS)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
            let get_all_request = store
                .get_all()
                .map_err(|e| Self::make_error(format!("Get all error: {:?}", e)))?;
            let all_values = get_all_request
                .await
                .map_err(|e| Self::make_error(format!("Get all await error: {:?}", e)))?;
            let get_all_keys_request = store
                .get_all_keys()
                .map_err(|e| Self::make_error(format!("Get all keys error: {:?}", e)))?;
            let all_keys = get_all_keys_request
                .await
                .map_err(|e| Self::make_error(format!("Get all keys await error: {:?}", e)))?;
            let mut migrated_count = 0;
            for (i, value) in all_values.iter().enumerate() {
                let key = all_keys.get(i as u32);
                if !key.is_undefined() && !key.is_null() {
                    let json_str = value
                        .as_string()
                        .ok_or_else(|| Self::make_error("Value is not a string".to_string()))?;
                    let proof_info: ProofInfo = serde_json::from_str(&json_str).map_err(|e| {
                        Self::make_error(format!("JSON deserialization error: {}", e))
                    })?;
                    if proof_info.mint_url == old_mint_url {
                        let mut updated_proof = proof_info;
                        updated_proof.mint_url = new_mint_url.clone();
                        let json = serde_json::to_string(&updated_proof).map_err(|e| {
                            Self::make_error(format!("JSON serialization error: {}", e))
                        })?;
                        store
                            .put_key_val(&key, &JsValue::from_str(&json))
                            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                        migrated_count += 1;
                    }
                }
            }
            log::debug!("Migrated {} proofs", migrated_count);
        }
        {
            let store = tx
                .object_store(STORE_MINT_QUOTES)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
            let get_all_request = store
                .get_all()
                .map_err(|e| Self::make_error(format!("Get all error: {:?}", e)))?;
            let all_values = get_all_request
                .await
                .map_err(|e| Self::make_error(format!("Get all await error: {:?}", e)))?;
            let get_all_keys_request = store
                .get_all_keys()
                .map_err(|e| Self::make_error(format!("Get all keys error: {:?}", e)))?;
            let all_keys = get_all_keys_request
                .await
                .map_err(|e| Self::make_error(format!("Get all keys await error: {:?}", e)))?;
            let mut migrated_count = 0;
            for (i, value) in all_values.iter().enumerate() {
                let key = all_keys.get(i as u32);
                if !key.is_undefined() && !key.is_null() {
                    let json_str = value
                        .as_string()
                        .ok_or_else(|| Self::make_error("Value is not a string".to_string()))?;
                    let mint_quote: MintQuote = serde_json::from_str(&json_str).map_err(|e| {
                        Self::make_error(format!("JSON deserialization error: {}", e))
                    })?;
                    if mint_quote.mint_url == old_mint_url {
                        let mut updated_quote = mint_quote;
                        updated_quote.mint_url = new_mint_url.clone();
                        let json = serde_json::to_string(&updated_quote).map_err(|e| {
                            Self::make_error(format!("JSON serialization error: {}", e))
                        })?;
                        store
                            .put_key_val(&key, &JsValue::from_str(&json))
                            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                        migrated_count += 1;
                    }
                }
            }
            log::debug!("Migrated {} mint quotes", migrated_count);
        }
        {
            let store = tx
                .object_store(STORE_TRANSACTIONS)
                .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
            let get_all_request = store
                .get_all()
                .map_err(|e| Self::make_error(format!("Get all error: {:?}", e)))?;
            let all_values = get_all_request
                .await
                .map_err(|e| Self::make_error(format!("Get all await error: {:?}", e)))?;
            let get_all_keys_request = store
                .get_all_keys()
                .map_err(|e| Self::make_error(format!("Get all keys error: {:?}", e)))?;
            let all_keys = get_all_keys_request
                .await
                .map_err(|e| Self::make_error(format!("Get all keys await error: {:?}", e)))?;
            let mut migrated_count = 0;
            for (i, value) in all_values.iter().enumerate() {
                let key = all_keys.get(i as u32);
                if !key.is_undefined() && !key.is_null() {
                    let json_str = value
                        .as_string()
                        .ok_or_else(|| Self::make_error("Value is not a string".to_string()))?;
                    let transaction: Transaction =
                        serde_json::from_str(&json_str).map_err(|e| {
                            Self::make_error(format!("JSON deserialization error: {}", e))
                        })?;
                    if transaction.mint_url == old_mint_url {
                        let mut updated_tx = transaction;
                        updated_tx.mint_url = new_mint_url.clone();
                        let json = serde_json::to_string(&updated_tx).map_err(|e| {
                            Self::make_error(format!("JSON serialization error: {}", e))
                        })?;
                        store
                            .put_key_val(&key, &JsValue::from_str(&json))
                            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                        migrated_count += 1;
                    }
                }
            }
            log::debug!("Migrated {} transactions", migrated_count);
        }
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;
        log::info!(
            "Successfully migrated all data from {} to {}",
            old_mint_url,
            new_mint_url
        );
        Ok(())
    }
    async fn add_mint_keysets(
        &self,
        mint_url: MintUrl,
        keysets: Vec<KeySetInfo>,
    ) -> Result<(), database::Error> {
        let tx = self
            .db
            .transaction_on_multi_with_mode(
                &[STORE_KEYSETS, STORE_KEYSET_BY_ID],
                IdbTransactionMode::Readwrite,
            )
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;
        let keysets_store = tx
            .object_store(STORE_KEYSETS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
        let keyset_by_id_store = tx
            .object_store(STORE_KEYSET_BY_ID)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
        let key = mint_url.to_string();
        let json_str = serde_json::to_string(&keysets)
            .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
        let js_key = JsValue::from_str(&key);
        let js_value = JsValue::from_str(&json_str);
        keysets_store
            .put_key_val(&js_key, &js_value)
            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
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
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;
        Ok(())
    }
    async fn get_mint_keysets(
        &self,
        mint_url: MintUrl,
    ) -> Result<Option<Vec<KeySetInfo>>, database::Error> {
        let key = mint_url.to_string();
        self.get_value(STORE_KEYSETS, &key).await
    }
    async fn get_keyset_by_id(&self, keyset_id: &Id) -> Result<Option<KeySetInfo>, database::Error> {
        let key = keyset_id.to_string();
        self.get_value(STORE_KEYSET_BY_ID, &key).await
    }
    async fn add_mint_quote(&self, quote: MintQuote) -> Result<(), database::Error> {
        let key = quote.id.clone();
        log::debug!("Storing mint quote: {}", key);
        self.put_value(STORE_MINT_QUOTES, &key, &quote).await
    }
    async fn get_mint_quote(&self, quote_id: &str) -> Result<Option<MintQuote>, database::Error> {
        self.get_value(STORE_MINT_QUOTES, quote_id).await
    }
    async fn get_mint_quotes(&self) -> Result<Vec<MintQuote>, database::Error> {
        self.get_all_values(STORE_MINT_QUOTES).await
    }
    async fn remove_mint_quote(&self, quote_id: &str) -> Result<(), database::Error> {
        log::debug!("Removing mint quote: {}", quote_id);
        self.delete_value(STORE_MINT_QUOTES, quote_id).await
    }
    async fn add_melt_quote(&self, quote: MeltQuote) -> Result<(), database::Error> {
        let key = quote.id.clone();
        log::debug!("Storing melt quote: {}", key);
        self.put_value(STORE_MELT_QUOTES, &key, &quote).await
    }
    async fn get_melt_quote(&self, quote_id: &str) -> Result<Option<MeltQuote>, database::Error> {
        self.get_value(STORE_MELT_QUOTES, quote_id).await
    }
    async fn get_melt_quotes(&self) -> Result<Vec<MeltQuote>, database::Error> {
        self.get_all_values(STORE_MELT_QUOTES).await
    }
    async fn remove_melt_quote(&self, quote_id: &str) -> Result<(), database::Error> {
        log::debug!("Removing melt quote: {}", quote_id);
        self.delete_value(STORE_MELT_QUOTES, quote_id).await
    }
    async fn add_keys(&self, keyset: KeySet) -> Result<(), database::Error> {
        let key = keyset.id.to_string();
        self.put_value(STORE_KEYS, &key, &keyset.keys).await
    }
    async fn get_keys(&self, id: &Id) -> Result<Option<Keys>, database::Error> {
        let key = id.to_string();
        self.get_value(STORE_KEYS, &key).await
    }
    async fn remove_keys(&self, id: &Id) -> Result<(), database::Error> {
        let key = id.to_string();
        self.delete_value(STORE_KEYS, &key).await
    }
    async fn increment_keyset_counter(&self, keyset_id: &Id, count: u32) -> Result<u32, database::Error> {
        log::debug!(
            "Incrementing counter for keyset: {} by {}",
            keyset_id,
            count
        );
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_KEYSET_COUNTERS, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;
        let store = tx
            .object_store(STORE_KEYSET_COUNTERS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
        let key = JsValue::from_str(&keyset_id.to_string());
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
        let new_value = current + count;
        let js_value = JsValue::from_f64(new_value as f64);
        store
            .put_key_val(&key, &js_value)
            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
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
    ) -> Result<(), database::Error> {
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_PROOFS, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;
        let store = tx
            .object_store(STORE_PROOFS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
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
        for y in removed_ys {
            let key = y.to_string();
            let js_key = JsValue::from_str(&key);
            store
                .delete(&js_key)
                .map_err(|e| Self::make_error(format!("Delete error: {:?}", e)))?;
        }
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
    ) -> Result<Vec<ProofInfo>, database::Error> {
        let all_proofs: Vec<ProofInfo> = self.get_all_values(STORE_PROOFS).await?;
        let filtered: Vec<ProofInfo> = all_proofs
            .into_iter()
            .filter(|proof_info| {
                if let Some(ref filter_mint_url) = mint_url {
                    if &proof_info.mint_url != filter_mint_url {
                        return false;
                    }
                }
                if let Some(ref filter_unit) = unit {
                    if &proof_info.unit != filter_unit {
                        return false;
                    }
                }
                if let Some(ref states) = state {
                    if !states.contains(&proof_info.state) {
                        return false;
                    }
                }
                if let Some(ref filter_conditions) = spending_conditions {
                    match &proof_info.spending_condition {
                        Some(proof_condition) => {
                            if !filter_conditions.contains(proof_condition) {
                                return false;
                            }
                        }
                        None => {
                            return false;
                        }
                    }
                }
                true
            })
            .collect();
        Ok(filtered)
    }
    async fn get_balance(
        &self,
        mint_url: Option<MintUrl>,
        unit: Option<CurrencyUnit>,
        state: Option<Vec<State>>,
    ) -> Result<u64, database::Error> {
        let proofs = self.get_proofs(mint_url, unit, state, None).await?;
        let total: u64 = proofs.iter().map(|p| u64::from(p.proof.amount)).sum();
        Ok(total)
    }
    async fn update_proofs_state(
        &self,
        ys: Vec<CashuPublicKey>,
        state: State,
    ) -> Result<(), database::Error> {
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_PROOFS, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;
        let store = tx
            .object_store(STORE_PROOFS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
        for y in ys {
            let key = y.to_string();
            let js_key = JsValue::from_str(&key);
            let value_opt = store
                .get(&js_key)
                .map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?
                .await
                .map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))?;
            if let Some(value) = value_opt {
                let json_str = value
                    .as_string()
                    .ok_or_else(|| Self::make_error("Value is not a string".to_string()))?;
                let mut proof_info: ProofInfo = serde_json::from_str(&json_str)
                    .map_err(|e| Self::make_error(format!("JSON deserialization error: {}", e)))?;
                proof_info.state = state;
                let updated_json = serde_json::to_string(&proof_info)
                    .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
                let js_value = JsValue::from_str(&updated_json);
                store
                    .put_key_val(&js_key, &js_value)
                    .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
            }
        }
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;
        Ok(())
    }
    async fn add_transaction(&self, transaction: Transaction) -> Result<(), database::Error> {
        let key = transaction.id().to_string();
        self.put_value(STORE_TRANSACTIONS, &key, &transaction).await
    }
    async fn get_transaction(
        &self,
        transaction_id: TransactionId,
    ) -> Result<Option<Transaction>, database::Error> {
        let key = transaction_id.to_string();
        self.get_value(STORE_TRANSACTIONS, &key).await
    }
    async fn list_transactions(
        &self,
        mint_url: Option<MintUrl>,
        direction: Option<TransactionDirection>,
        unit: Option<CurrencyUnit>,
    ) -> Result<Vec<Transaction>, database::Error> {
        let all_transactions: Vec<Transaction> = self.get_all_values(STORE_TRANSACTIONS).await?;
        let filtered: Vec<Transaction> = all_transactions
            .into_iter()
            .filter(|transaction| {
                if let Some(ref filter_mint_url) = mint_url {
                    if &transaction.mint_url != filter_mint_url {
                        return false;
                    }
                }
                if let Some(ref filter_direction) = direction {
                    if &transaction.direction != filter_direction {
                        return false;
                    }
                }
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
    async fn remove_transaction(&self, transaction_id: TransactionId) -> Result<(), database::Error> {
        let key = transaction_id.to_string();
        self.delete_value(STORE_TRANSACTIONS, &key).await
    }
    async fn get_unissued_mint_quotes(&self) -> Result<Vec<MintQuote>, database::Error> {
        let all: Vec<MintQuote> = self.get_all_values(STORE_MINT_QUOTES).await?;
        Ok(all.into_iter().filter(|q| q.amount_issued < q.amount.unwrap_or_default()).collect())
    }
    async fn get_proofs_by_ys(&self, ys: Vec<CashuPublicKey>) -> Result<Vec<ProofInfo>, database::Error> {
        let all: Vec<ProofInfo> = self.get_all_values(STORE_PROOFS).await?;
        let y_strs: std::collections::HashSet<String> = ys.iter().map(|y| y.to_string()).collect();
        Ok(all.into_iter().filter(|p| y_strs.contains(&p.y.to_string())).collect())
    }
    async fn add_saga(&self, saga: WalletSaga) -> Result<(), database::Error> {
        let key = saga.id.to_string();
        self.put_value(STORE_SAGAS, &key, &saga).await
    }
    async fn get_saga(&self, id: &uuid::Uuid) -> Result<Option<WalletSaga>, database::Error> {
        let key = id.to_string();
        self.get_value(STORE_SAGAS, &key).await
    }
    async fn update_saga(&self, saga: WalletSaga) -> Result<bool, database::Error> {
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_SAGAS, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;
        let store = tx
            .object_store(STORE_SAGAS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
        let key = JsValue::from_str(&saga.id.to_string());
        let existing_opt = store
            .get(&key)
            .map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))?;
        match existing_opt {
            Some(existing_val) => {
                let json_str = existing_val
                    .as_string()
                    .ok_or_else(|| Self::make_error("Saga value is not a string".to_string()))?;
                let old: WalletSaga = serde_json::from_str(&json_str)
                    .map_err(|e| Self::make_error(format!("JSON deserialization error: {}", e)))?;
                if saga.version != old.version + 1 {
                    return Ok(false);
                }
                let saga_json = serde_json::to_string(&saga)
                    .map_err(|e| Self::make_error(format!("JSON serialization error: {}", e)))?;
                let js_value = JsValue::from_str(&saga_json);
                store
                    .put_key_val(&key, &js_value)
                    .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
                tx.await
                    .into_result()
                    .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
    async fn delete_saga(&self, id: &uuid::Uuid) -> Result<(), database::Error> {
        let key = id.to_string();
        self.delete_value(STORE_SAGAS, &key).await
    }
    async fn get_incomplete_sagas(&self) -> Result<Vec<WalletSaga>, database::Error> {
        let all: Vec<WalletSaga> = self.get_all_values(STORE_SAGAS).await?;
        Ok(all)
    }
    async fn reserve_proofs(&self, ys: Vec<CashuPublicKey>, operation_id: &uuid::Uuid) -> Result<(), database::Error> {
        let key = operation_id.to_string();
        self.put_value(STORE_PROOF_RESERVATIONS, &key, &ys).await
    }
    async fn release_proofs(&self, operation_id: &uuid::Uuid) -> Result<(), database::Error> {
        let key = operation_id.to_string();
        self.delete_value(STORE_PROOF_RESERVATIONS, &key).await
    }
    async fn get_reserved_proofs(&self, operation_id: &uuid::Uuid) -> Result<Vec<ProofInfo>, database::Error> {
        let key = operation_id.to_string();
        let ys: Option<Vec<CashuPublicKey>> = self.get_value(STORE_PROOF_RESERVATIONS, &key).await?;
        match ys {
            Some(reserved_ys) => {
                let all: Vec<ProofInfo> = self.get_all_values(STORE_PROOFS).await?;
                let y_set: std::collections::HashSet<String> = reserved_ys.iter().map(|y| y.to_string()).collect();
                Ok(all.into_iter().filter(|p| y_set.contains(&p.y.to_string())).collect())
            }
            None => Ok(vec![]),
        }
    }
    async fn reserve_melt_quote(&self, quote_id: &str, operation_id: &uuid::Uuid) -> Result<(), database::Error> {
        let key = operation_id.to_string();
        self.put_value(STORE_QUOTE_RESERVATIONS, &key, &quote_id.to_string()).await
    }
    async fn release_melt_quote(&self, operation_id: &uuid::Uuid) -> Result<(), database::Error> {
        let key = operation_id.to_string();
        self.delete_value(STORE_QUOTE_RESERVATIONS, &key).await
    }
    async fn reserve_mint_quote(&self, quote_id: &str, operation_id: &uuid::Uuid) -> Result<(), database::Error> {
        let key = operation_id.to_string();
        self.put_value(STORE_QUOTE_RESERVATIONS, &key, &quote_id.to_string()).await
    }
    async fn release_mint_quote(&self, operation_id: &uuid::Uuid) -> Result<(), database::Error> {
        let key = operation_id.to_string();
        self.delete_value(STORE_QUOTE_RESERVATIONS, &key).await
    }
    async fn kv_read(&self, primary_namespace: &str, secondary_namespace: &str, key: &str) -> Result<Option<Vec<u8>>, database::Error> {
        let composite_key = format!("{}:{}:{}", primary_namespace, secondary_namespace, key);
        self.get_value::<String>(STORE_KV, &composite_key).await.map(|opt| opt.map(|s| s.into_bytes()))
    }
    async fn kv_list(&self, primary_namespace: &str, secondary_namespace: &str) -> Result<Vec<String>, database::Error> {
        let all: Vec<(String, String)> = self.get_all_key_values(STORE_KV).await?;
        let prefix = format!("{}:{}:", primary_namespace, secondary_namespace);
        Ok(all.into_iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, _)| k[prefix.len()..].to_string())
            .collect())
    }
    async fn kv_write(&self, primary_namespace: &str, secondary_namespace: &str, key: &str, value: &[u8]) -> Result<(), database::Error> {
        let composite_key = format!("{}:{}:{}", primary_namespace, secondary_namespace, key);
        let val_str = String::from_utf8(value.to_vec())
            .map_err(|e| Self::make_error(format!("KV value not valid UTF-8: {}", e)))?;
        self.put_value(STORE_KV, &composite_key, &val_str).await
    }
    async fn kv_remove(&self, primary_namespace: &str, secondary_namespace: &str, key: &str) -> Result<(), database::Error> {
        let composite_key = format!("{}:{}:{}", primary_namespace, secondary_namespace, key);
        self.delete_value(STORE_KV, &composite_key).await
    }
    async fn add_p2pk_key(&self, pubkey: &CashuPublicKey, derivation_path: DerivationPath, derivation_index: u32) -> Result<(), database::Error> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let signing_key = P2PKSigningKey {
            pubkey: *pubkey,
            derivation_path,
            derivation_index,
            created_time: now,
        };
        let key = pubkey.to_string();
        self.put_value(STORE_P2PK_KEYS, &key, &signing_key).await
    }
    async fn get_p2pk_key(&self, pubkey: &CashuPublicKey) -> Result<Option<P2PKSigningKey>, database::Error> {
        let key = pubkey.to_string();
        self.get_value(STORE_P2PK_KEYS, &key).await
    }
    async fn list_p2pk_keys(&self) -> Result<Vec<P2PKSigningKey>, database::Error> {
        self.get_all_values(STORE_P2PK_KEYS).await
    }
    async fn latest_p2pk(&self) -> Result<Option<P2PKSigningKey>, database::Error> {
        let all: Vec<P2PKSigningKey> = self.get_all_values(STORE_P2PK_KEYS).await?;
        Ok(all.into_iter().max_by_key(|k| k.created_time))
    }
}
#[cfg(all(target_arch = "wasm32", feature = "web", not(feature = "native")))]
impl IndexedDbDatabase {
    /// Get current counter value for a keyset (NUT-13)
    ///
    /// Returns 0 if no counter exists for this keyset.
    pub async fn get_keyset_counter(&self, keyset_id: &Id) -> Result<u32, database::Error> {
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_KEYSET_COUNTERS, IdbTransactionMode::Readonly)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;
        let store = tx
            .object_store(STORE_KEYSET_COUNTERS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
        let key = JsValue::from_str(&keyset_id.to_string());
        let value_opt = store
            .get(&key)
            .map_err(|e| Self::make_error(format!("Get error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get await error: {:?}", e)))?;
        let counter = if let Some(value) = value_opt {
            value.as_f64().map(|f| f as u32).unwrap_or(0)
        } else {
            0
        };
        Ok(counter)
    }
    /// Set counter value for a keyset (NUT-13 restore operations)
    ///
    /// Used when restoring from backup or after NUT-09 recovery.
    pub async fn set_keyset_counter(
        &self,
        keyset_id: &Id,
        value: u32,
    ) -> Result<(), database::Error> {
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_KEYSET_COUNTERS, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;
        let store = tx
            .object_store(STORE_KEYSET_COUNTERS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
        let key = JsValue::from_str(&keyset_id.to_string());
        let js_value = JsValue::from_f64(value as f64);
        store
            .put_key_val(&key, &js_value)
            .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;
        log::info!("Counter for keyset {} set to {}", keyset_id, value);
        Ok(())
    }
    /// Get all keyset counters (for backup)
    ///
    /// Returns a map of keyset_id -> counter value.
    pub async fn get_all_keyset_counters(&self) -> Result<HashMap<String, u32>, database::Error> {
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_KEYSET_COUNTERS, IdbTransactionMode::Readonly)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;
        let store = tx
            .object_store(STORE_KEYSET_COUNTERS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
        let keys = store
            .get_all_keys()
            .map_err(|e| Self::make_error(format!("Get all keys error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get all keys await error: {:?}", e)))?;
        let values = store
            .get_all()
            .map_err(|e| Self::make_error(format!("Get all error: {:?}", e)))?
            .await
            .map_err(|e| Self::make_error(format!("Get all await error: {:?}", e)))?;
        let mut counters = HashMap::new();
        for (key_js, value_js) in keys.into_iter().zip(values) {
            if let Some(key_str) = key_js.as_string() {
                let counter = value_js.as_f64().map(|f| f as u32).unwrap_or(0);
                counters.insert(key_str, counter);
            }
        }
        log::debug!("Loaded {} keyset counters", counters.len());
        Ok(counters)
    }
    /// Restore keyset counters from backup
    ///
    /// Used when restoring wallet from seed.
    pub async fn restore_keyset_counters(
        &self,
        counters: &HashMap<String, u32>,
    ) -> Result<(), database::Error> {
        let tx = self
            .db
            .transaction_on_one_with_mode(STORE_KEYSET_COUNTERS, IdbTransactionMode::Readwrite)
            .map_err(|e| Self::make_error(format!("Transaction error: {:?}", e)))?;
        let store = tx
            .object_store(STORE_KEYSET_COUNTERS)
            .map_err(|e| Self::make_error(format!("Store error: {:?}", e)))?;
        for (keyset_id, counter) in counters {
            let key = JsValue::from_str(keyset_id);
            let js_value = JsValue::from_f64(*counter as f64);
            store
                .put_key_val(&key, &js_value)
                .map_err(|e| Self::make_error(format!("Put error: {:?}", e)))?;
        }
        tx.await
            .into_result()
            .map_err(|e| Self::make_error(format!("Transaction commit error: {:?}", e)))?;
        log::info!("Restored {} keyset counters", counters.len());
        Ok(())
    }
}
