//! Relay signals and types
//!
//! Centralized relay state management signals and core types.
//! This module provides the reactive state for relay connections.
use dioxus::prelude::*;
pub use dioxus_stores::Store;
pub use nostr_relay_pool::RelayStatus;

#[derive(Clone, Debug, PartialEq)]
pub enum RelaySource {
    Default,
    UserNip65,
    #[allow(dead_code)]
    UserNip17,
    Manual,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct RelayInfo {
    pub url: String,
    pub status: RelayStatus,
    pub has_read: bool,
    pub has_write: bool,
    pub source: RelaySource,
}

impl RelayInfo {
    pub fn new(url: String, status: RelayStatus) -> Self {
        Self {
            url,
            status,
            has_read: true,
            has_write: true,
            source: RelaySource::Default,
        }
    }

    pub fn with_flags(
        url: String,
        status: RelayStatus,
        has_read: bool,
        has_write: bool,
        source: RelaySource,
    ) -> Self {
        Self {
            url,
            status,
            has_read,
            has_write,
            source,
        }
    }
}

#[derive(Clone, Debug, Default, Store)]
pub struct RelayPoolStore {
    pub data: Vec<RelayInfo>,
}

pub static RELAY_POOL: GlobalSignal<Store<RelayPoolStore>> =
    Signal::global(|| Store::new(RelayPoolStore::default()));

pub static RELAY_CONNECTED: GlobalSignal<bool> = Signal::global(|| false);

pub static USER_RELAYS_APPLIED: GlobalSignal<bool> = Signal::global(|| false);
