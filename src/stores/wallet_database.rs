//! Wallet database type alias
//!
//! Provides a platform-appropriate wallet database type:
//! - Web: IndexedDB-backed database
//! - Native: SQLite-backed database

#[allow(dead_code)]
#[cfg(all(feature = "web", not(feature = "native")))]
pub type WalletDb = crate::stores::indexeddb_database::IndexedDbDatabase;

#[allow(dead_code)]
#[cfg(feature = "native")]
pub type WalletDb = cdk_sqlite::WalletSqliteDatabase;
