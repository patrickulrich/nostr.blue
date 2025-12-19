//! Application-wide error types for nostr.blue

use thiserror::Error;

/// Convenience type alias for Results using NostrBlueError
pub type Result<T> = core::result::Result<T, NostrBlueError>;

/// Application-wide error type
#[derive(Debug, Error)]
pub enum NostrBlueError {
    // Authentication
    #[error("Not authenticated")]
    NotAuthenticated,
    #[error("Signer not available")]
    SignerNotAvailable,

    // Nostr (wrap nostr-sdk errors)
    #[error("Nostr client error: {0}")]
    NostrClient(#[from] nostr_sdk::client::Error),
    #[error("Event builder error: {0}")]
    EventBuilder(#[from] nostr::event::builder::Error),
    #[error("Key error: {0}")]
    Key(#[from] nostr::key::Error),

    // Relay
    #[error("No relays configured")]
    NoRelaysConfigured,
    #[error("Relay connection failed: {0}")]
    RelayConnectionFailed(String),

    // Profile
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    // MLS/MDK (wrap mdk_core::Error)
    #[error("MDK not initialized")]
    MdkNotInitialized,
    #[error("MLS error: {0}")]
    MlsError(String),
    #[error("No key packages found for: {0}")]
    NoKeyPackages(String),
    #[error("Key package error: {0}")]
    KeyPackage(String),
    #[error("Welcome error: {0}")]
    Welcome(String),
    #[error("MLS group error: {0}")]
    MlsGroup(String),

    // Storage
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // Generic (for backwards compat with existing String errors)
    #[error("{0}")]
    Other(String),
}

impl From<String> for NostrBlueError {
    fn from(s: String) -> Self {
        NostrBlueError::Other(s)
    }
}

impl From<&str> for NostrBlueError {
    fn from(s: &str) -> Self {
        NostrBlueError::Other(s.to_string())
    }
}
