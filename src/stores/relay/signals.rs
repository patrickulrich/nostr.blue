//! Relay signals and types
//!
//! Centralized relay state management signals and core types.
//! This module provides the reactive state for relay connections.

use dioxus::prelude::*;
// Re-export the Store macro for the RelayPoolStoreStoreExt trait
pub use dioxus_stores::Store;

/// Relay connection status
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum RelayStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Source of relay configuration
#[derive(Clone, Debug, PartialEq)]
pub enum RelaySource {
    /// Default relays configured in the application
    Default,
    /// User's NIP-65 relay list (kind 10002)
    UserNip65,
    /// User's NIP-17 DM relay list (kind 10050)
    #[allow(dead_code)]  // Reserved for NIP-17 DM relay support
    UserNip17,
    /// Manually added by user
    Manual,
}

/// Relay information with NIP-65 metadata
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct RelayInfo {
    pub url: String,
    pub status: RelayStatus,
    /// Whether this relay is used for reading (NIP-65)
    pub has_read: bool,
    /// Whether this relay is used for writing (NIP-65)
    pub has_write: bool,
    /// How this relay was added
    pub source: RelaySource,
}

impl RelayInfo {
    /// Create a new RelayInfo with default flags (read+write, default source)
    pub fn new(url: String, status: RelayStatus) -> Self {
        Self {
            url,
            status,
            has_read: true,
            has_write: true,
            source: RelaySource::Default,
        }
    }

    /// Create a new RelayInfo with specific flags
    pub fn with_flags(url: String, status: RelayStatus, has_read: bool, has_write: bool, source: RelaySource) -> Self {
        Self {
            url,
            status,
            has_read,
            has_write,
            source,
        }
    }
}

/// Global relay pool state
/// Store for relay pool with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct RelayPoolStore {
    pub data: Vec<RelayInfo>,
}

/// Global signal for relay pool state
pub static RELAY_POOL: GlobalSignal<Store<RelayPoolStore>> = Signal::global(|| Store::new(RelayPoolStore::default()));

/// Whether at least one relay has connected (triggers NIP-78 retries)
/// Once true, stays true for the session (relay may reconnect automatically)
pub static RELAY_CONNECTED: GlobalSignal<bool> = Signal::global(|| false);

/// Whether user relay lists (kind 10002/10050) have been applied to the client
/// Use this to wait for user relays before making queries
pub static USER_RELAYS_APPLIED: GlobalSignal<bool> = Signal::global(|| false);
