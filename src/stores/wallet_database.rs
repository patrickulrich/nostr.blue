//! Wallet database plumbing
//!
//! Provides the platform-appropriate Cashu localstore:
//! - Web: `IndexedDbDatabase` (IndexedDB-backed)
//! - Desktop: `cdk_redb::WalletRedbDatabase` (redb is used instead of
//!   cdk-sqlite because cdk-sqlite pins rusqlite ^0.31, which conflicts with
//!   nostr-sqlite's rusqlite 0.38 via cargo's `links = "sqlite3"` uniqueness
//!   rule)
//!
//! [`SharedLocalstore`] wraps both behind one handle: a type-erased
//! `Arc<dyn WalletDatabase>` for `WalletRepositoryBuilder` plus the concrete
//! backend for `Deref` (the full funds-critical trait surface — proofs,
//! mints, keysets — resolves identically on both platforms). The
//! nostr.blue-specific extras (pending-event queue, sync state, in-flight
//! melts, pending mint secrets) are wasm IndexedDB inherent APIs; the
//! forwarding methods below keep those call sites compiling, returning the
//! same "wasm-only" errors on native the old IndexedDB stub did (desktop
//! rebuilds that state from NIP-60 events).

#[cfg(all(feature = "web", feature = "desktop"))]
compile_error!("Cannot enable both 'web' and 'desktop' features");

#[cfg(all(feature = "web", feature = "mobile_platform"))]
compile_error!("Cannot enable both 'web' and 'mobile' features");

#[cfg(all(feature = "desktop", feature = "mobile_platform"))]
compile_error!("Cannot enable both 'desktop' and 'mobile' features");

#[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile_platform")))]
compile_error!("Must enable exactly one of 'web', 'desktop', or 'mobile' feature");

use cdk_common::database::{self as cdk_database, WalletDatabase};

/// Shared Cashu localstore handle (see the module docs).
pub struct SharedLocalstore {
    /// Type-erased view: the exact type
    /// `WalletRepositoryBuilder::localstore` consumes.
    erased: std::sync::Arc<dyn WalletDatabase<cdk_database::Error> + Send + Sync>,
    /// Concrete platform backend; the `Deref` target. Non-`cashu_native`
    /// (web) builds use `IndexedDbDatabase` — real storage on wasm32, the
    /// error-stub when the web feature is compiled for host tests.
    #[cfg(not(feature = "cashu_native"))]
    concrete: std::sync::Arc<crate::stores::indexeddb_database::IndexedDbDatabase>,
    #[cfg(feature = "cashu_native")]
    concrete: std::sync::Arc<cdk_redb::WalletRedbDatabase>,
}

impl Clone for SharedLocalstore {
    fn clone(&self) -> Self {
        Self {
            erased: self.erased.clone(),
            concrete: self.concrete.clone(),
        }
    }
}

impl std::fmt::Debug for SharedLocalstore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The CDK `Database` trait requires `Debug`, so the erased handle is.
        f.debug_struct("SharedLocalstore")
            .field("backend", &self.erased)
            .finish()
    }
}

impl std::ops::Deref for SharedLocalstore {
    #[cfg(not(feature = "cashu_native"))]
    type Target = crate::stores::indexeddb_database::IndexedDbDatabase;
    #[cfg(feature = "cashu_native")]
    type Target = cdk_redb::WalletRedbDatabase;

    fn deref(&self) -> &Self::Target {
        &self.concrete
    }
}

impl SharedLocalstore {
    /// Type-erased clone for `WalletRepositoryBuilder::localstore`.
    pub fn erased(&self) -> std::sync::Arc<dyn WalletDatabase<cdk_database::Error> + Send + Sync> {
        self.erased.clone()
    }

    #[cfg(not(feature = "cashu_native"))]
    pub(crate) fn from_indexeddb(
        store: std::sync::Arc<crate::stores::indexeddb_database::IndexedDbDatabase>,
    ) -> Self {
        Self {
            erased: store.clone(),
            concrete: store,
        }
    }

    #[cfg(feature = "cashu_native")]
    pub(crate) fn from_redb(store: std::sync::Arc<cdk_redb::WalletRedbDatabase>) -> Self {
        Self {
            erased: store.clone(),
            concrete: store,
        }
    }
}

#[cfg(feature = "cashu_native")]
fn native_extras_unsupported(what: &str) -> cdk_database::Error {
    cdk_database::Error::Database(Box::new(std::io::Error::other(format!(
        "{what} lives in the wasm IndexedDB extras store; native wallets rebuild this state from NIP-60 events"
    ))))
}

/// nostr.blue-specific persistence extras (IndexedDB inherent APIs — NOT
/// part of CDK's `WalletDatabase` trait). Forwards to real storage on web
/// builds, stubs on `cashu_native` builds with the same semantics the old
/// native `IndexedDbDatabase` stub had.
impl SharedLocalstore {
    pub async fn remove_pending_event(&self, event_id: &str) -> Result<(), cdk_database::Error> {
        #[cfg(not(feature = "cashu_native"))]
        {
            crate::stores::indexeddb_database::IndexedDbDatabase::remove_pending_event(
                &self.concrete,
                event_id,
            )
            .await
        }
        #[cfg(feature = "cashu_native")]
        {
            let _ = event_id;
            Err(native_extras_unsupported("remove_pending_event"))
        }
    }

    pub async fn get_all_pending_events(
        &self,
    ) -> Result<Vec<crate::stores::cashu::types::PendingNostrEvent>, cdk_database::Error> {
        #[cfg(not(feature = "cashu_native"))]
        {
            crate::stores::indexeddb_database::IndexedDbDatabase::get_all_pending_events(
                &self.concrete,
            )
            .await
        }
        #[cfg(feature = "cashu_native")]
        {
            Err(native_extras_unsupported("get_all_pending_events"))
        }
    }

    pub async fn load_sync_state(
        &self,
    ) -> Result<Option<crate::stores::cashu::types::SyncState>, cdk_database::Error> {
        #[cfg(not(feature = "cashu_native"))]
        {
            crate::stores::indexeddb_database::IndexedDbDatabase::load_sync_state(&self.concrete)
                .await
        }
        #[cfg(feature = "cashu_native")]
        {
            Err(native_extras_unsupported("load_sync_state"))
        }
    }

    pub async fn save_sync_state(
        &self,
        state: &crate::stores::cashu::types::SyncState,
    ) -> Result<(), cdk_database::Error> {
        #[cfg(not(feature = "cashu_native"))]
        {
            crate::stores::indexeddb_database::IndexedDbDatabase::save_sync_state(
                &self.concrete,
                state,
            )
            .await
        }
        #[cfg(feature = "cashu_native")]
        {
            let _ = state;
            Err(native_extras_unsupported("save_sync_state"))
        }
    }

    pub async fn save_pending_mint_secrets(
        &self,
        secrets: &std::collections::HashMap<String, u64>,
    ) -> Result<(), cdk_database::Error> {
        #[cfg(not(feature = "cashu_native"))]
        {
            crate::stores::indexeddb_database::IndexedDbDatabase::save_pending_mint_secrets(
                &self.concrete,
                secrets,
            )
            .await
        }
        #[cfg(feature = "cashu_native")]
        {
            let _ = secrets;
            Err(native_extras_unsupported("save_pending_mint_secrets"))
        }
    }

    pub async fn load_pending_mint_secrets(
        &self,
    ) -> Result<Option<std::collections::HashMap<String, u64>>, cdk_database::Error> {
        #[cfg(not(feature = "cashu_native"))]
        {
            crate::stores::indexeddb_database::IndexedDbDatabase::load_pending_mint_secrets(
                &self.concrete,
            )
            .await
        }
        #[cfg(feature = "cashu_native")]
        {
            Err(native_extras_unsupported("load_pending_mint_secrets"))
        }
    }

    pub async fn save_in_flight_melt_requests(
        &self,
        requests: &[crate::stores::cashu::types::InFlightMeltRequest],
    ) -> Result<(), cdk_database::Error> {
        #[cfg(not(feature = "cashu_native"))]
        {
            crate::stores::indexeddb_database::IndexedDbDatabase::save_in_flight_melt_requests(
                &self.concrete,
                requests,
            )
            .await
        }
        #[cfg(feature = "cashu_native")]
        {
            let _ = requests;
            Err(native_extras_unsupported("save_in_flight_melt_requests"))
        }
    }

    pub async fn load_in_flight_melt_requests(
        &self,
    ) -> Result<Option<Vec<crate::stores::cashu::types::InFlightMeltRequest>>, cdk_database::Error>
    {
        #[cfg(not(feature = "cashu_native"))]
        {
            crate::stores::indexeddb_database::IndexedDbDatabase::load_in_flight_melt_requests(
                &self.concrete,
            )
            .await
        }
        #[cfg(feature = "cashu_native")]
        {
            Err(native_extras_unsupported("load_in_flight_melt_requests"))
        }
    }
}
