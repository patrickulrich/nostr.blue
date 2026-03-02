//! Wallet database type alias
//!
//! Provides a platform-appropriate wallet database type:
//! - Web: IndexedDB-backed database
//! - Native (Desktop/Mobile): SQLite-backed database

#[cfg(all(feature = "web", feature = "native"))]
compile_error!("Cannot enable both 'web' and 'native' features simultaneously");

#[cfg(not(any(feature = "web", feature = "native")))]
compile_error!("Must enable either 'web' or 'native' feature");

#[allow(dead_code)]
#[cfg(all(feature = "web", not(feature = "native")))]
pub type WalletDb = crate::stores::indexeddb_database::IndexedDbDatabase;

#[allow(dead_code)]
#[cfg(feature = "native")]
pub type WalletDb = cdk_sqlite::WalletSqliteDatabase;
