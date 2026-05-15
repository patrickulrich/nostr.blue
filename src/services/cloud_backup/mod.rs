pub mod crypto;
pub mod manager;
pub mod types;

#[cfg(target_family = "wasm")]
pub mod web;

#[cfg(all(target_os = "android", feature = "mobile_platform"))]
pub mod android;

pub use manager::{backup_to_cloud, delete_cloud_backup, google_sign_in, list_cloud_backups, restore_from_cloud};
pub use types::{BackupEntry, GoogleAuthResult, GoogleBackupState};
