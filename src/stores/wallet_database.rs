//! Wallet database type alias
//!
//! Provides a platform-appropriate wallet database type:
//! - Web: IndexedDB-backed database
//! - Desktop/Mobile: cdk-redb backed database (redb is used instead of
//!   cdk-sqlite because cdk-sqlite pins rusqlite ^0.31, which conflicts with
//!   nostr-sqlite's rusqlite 0.38 via cargo's `links = "sqlite3"` uniqueness
//!   rule). NOTE: the desktop cashu localstore currently goes through the
//!   native stub in `indexeddb_database.rs`; wiring `WalletRedbDatabase` into
//!   that path is a follow-up (desktop cashu state recovers via NIP-60).

#[cfg(all(feature = "web", feature = "desktop"))]
compile_error!("Cannot enable both 'web' and 'desktop' features");

#[cfg(all(feature = "web", feature = "mobile_platform"))]
compile_error!("Cannot enable both 'web' and 'mobile' features");

#[cfg(all(feature = "desktop", feature = "mobile_platform"))]
compile_error!("Cannot enable both 'desktop' and 'mobile' features");

#[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile_platform")))]
compile_error!("Must enable exactly one of 'web', 'desktop', or 'mobile' feature");

#[allow(dead_code)]
#[cfg(feature = "web")]
pub type WalletDb = crate::stores::indexeddb_database::IndexedDbDatabase;

#[allow(dead_code)]
#[cfg(any(feature = "desktop", feature = "mobile_platform"))]
pub type WalletDb = cdk_redb::WalletRedbDatabase;
