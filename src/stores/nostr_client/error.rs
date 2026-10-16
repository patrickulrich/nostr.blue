//! Nostr client error types
//!
//! Error enum following nostr-sdk patterns with proper From implementations.
use std::fmt;
/// Errors that can occur during client operations
#[derive(Clone, Debug)]
pub enum Error {
    /// Client not initialized
    NotInitialized,
    /// No signer attached
    NoSigner,
    /// Relay error
    Relay(String),
    /// Event not found
    EventNotFound,
    /// Invalid coordinate
    InvalidCoordinate(String),
    /// Publish failed
    PublishFailed(String),
    /// Invalid input
    InvalidInput(String),
    /// Not authenticated
    NotAuthenticated,
}
impl std::error::Error for Error {}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitialized => write!(f, "client not initialized"),
            Self::NoSigner => write!(f, "no signer attached"),
            Self::Relay(e) => write!(f, "relay: {e}"),
            Self::EventNotFound => write!(f, "event not found"),
            Self::InvalidCoordinate(e) => write!(f, "invalid coordinate: {e}"),
            Self::PublishFailed(e) => write!(f, "publish failed: {e}"),
            Self::InvalidInput(e) => write!(f, "invalid input: {e}"),
            Self::NotAuthenticated => write!(f, "not authenticated"),
        }
    }
}
impl From<Error> for String {
    fn from(e: Error) -> String {
        e.to_string()
    }
}

impl From<nostr_sdk::error::Error> for Error {
    fn from(e: nostr_sdk::error::Error) -> Self {
        Self::Relay(e.to_string())
    }
}

impl Error {
    /// Wrap any error with the calling module's identity — used by
    /// [`crate::map_err_ctx!`] so log/toast messages carry uniform context
    /// instead of hand-typed `format!("Failed to X: {e}")` strings.
    pub fn context(ctx: &str, e: impl std::fmt::Display) -> Self {
        Self::Relay(format!("{ctx}: {e}"))
    }
}

/// Map any error into [`Error`] tagged with the calling module's path.
///
/// Replaces the ad-hoc `.map_err(|e| format!("Failed to …: {e}"))` pattern
/// scattered across fetch/publish call sites (plan item #8).
#[macro_export]
macro_rules! map_err_ctx {
    ($result:expr) => {
        $result.map_err(|e| $crate::stores::nostr_client::Error::context(module_path!(), e))
    };
}
