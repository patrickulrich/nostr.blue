//! Negentropy (NIP-77) sync wrapper for bounded enumerable data sets.
//!
//! ## IMPORTANT: Scope restriction
//!
//! Negentropy is suitable **ONLY for bounded enumerable sets** where you need
//! gap-free history:
//! - Thread reply chains
//! - DM/giftwrap history (NIP-59)
//! - Relay-list prefetch (kind 10050)
//!
//! Do NOT use this for general timelines (home, global, following, hashtag,
//! profile feeds). Negentropy negotiates a full local-vs-remote set diff per
//! filter per relay — for unbounded feeds with thousands of events across
//! hundreds of authors, this is catastrophic. Those feeds use
//! `subscribe` + `database().query()` + `since_optimize` instead.
//!
//! ## Usage
//!
//! ```rust,ignore
//! let sync = NegentropySync::from_client(client.clone());
//! let received_ids = sync.sync_down(filter, Duration::from_secs(10)).await?;
//! // Events are now in the local DB; query_local() will return them.
//! ```

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use nostr::EventId;
use nostr_sdk::{Client, Filter};
use nostr_relay_pool::{Reconciliation, SyncDirection, SyncOptions};
use nostr_sdk::RelayUrl;

use super::repository::FeedError;

/// Wraps `Client::sync()` to capture the set of newly-received event IDs.
///
/// After `sync_down` completes, the received events are auto-saved to the
/// local database by the SDK's middleware (verified: the sync download path
/// uses normal REQ messages, which go through `handle_event_msg` →
/// `database().save_event` at `relay/inner.rs:1234`).
pub struct NegentropySync {
    client: Arc<Client>,
}

/// Result of a negentropy sync operation.
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Event IDs that were newly received (fetched from remote).
    pub received: HashSet<EventId>,
    /// Event IDs that were sent to the remote (for Up/Both direction).
    pub sent: HashSet<EventId>,
    /// Relays that successfully completed the sync.
    pub successful_relays: HashSet<String>,
    /// Relays that failed, with error messages.
    pub failed_relays: HashMapSimple<String, String>,
}

/// Simple HashMap alias (avoids pulling in the full HashMap type in the
/// public signature).
type HashMapSimple<K, V> = std::collections::HashMap<K, V>;

impl NegentropySync {
    /// Construct with an SDK client.
    pub fn new(client: Arc<Client>) -> Self {
        Self { client }
    }

    /// Backfill via negentropy (Down direction only).
    ///
    /// Uses `client.sync(filter, &opts)` with a configurable initial timeout.
    /// Returns the set of newly-received event IDs.
    ///
    /// After this call, the events are in the local DB — `query_local()`
    /// will return them.
    ///
    /// ## Use for bounded sets only
    ///
    /// Do NOT call this for general timeline feeds. See module docs.
    pub async fn sync_down(
        &self,
        filter: Filter,
        initial_timeout: Duration,
    ) -> Result<SyncResult, FeedError> {
        let opts = SyncOptions::default()
            .direction(SyncDirection::Down)
            .initial_timeout(initial_timeout);

        let output = self.client.sync(filter, &opts).await?;

        let reconciliation: &Reconciliation = &output.val;

        Ok(SyncResult {
            received: reconciliation.received.clone(),
            sent: reconciliation.sent.clone(),
            successful_relays: output.success.iter().map(|u| u.to_string()).collect(),
            failed_relays: output
                .failed
                .iter()
                .map(|(url, err)| (url.to_string(), err.clone()))
                .collect(),
        })
    }

    /// Backfill via negentropy, targeting specific relays only.
    ///
    /// Use this when you know exactly which relays should have the data
    /// (e.g. thread root relay, DM participant's relays).
    pub async fn sync_down_with(
        &self,
        urls: Vec<String>,
        filter: Filter,
        initial_timeout: Duration,
    ) -> Result<SyncResult, FeedError> {
        let opts = SyncOptions::default()
            .direction(SyncDirection::Down)
            .initial_timeout(initial_timeout);

        let relay_urls: Vec<RelayUrl> = urls
            .into_iter()
            .filter_map(|u| RelayUrl::parse(&u).ok())
            .collect();

        let output = self.client.sync_with(relay_urls, filter, &opts).await?;
        let reconciliation: &Reconciliation = &output.val;

        Ok(SyncResult {
            received: reconciliation.received.clone(),
            sent: reconciliation.sent.clone(),
            successful_relays: output.success.iter().map(|u| u.to_string()).collect(),
            failed_relays: output
                .failed
                .iter()
                .map(|(url, err)| (url.to_string(), err.clone()))
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_result_defaults() {
        let result = SyncResult {
            received: HashSet::new(),
            sent: HashSet::new(),
            successful_relays: HashSet::new(),
            failed_relays: std::collections::HashMap::new(),
        };
        assert!(result.received.is_empty());
        assert!(result.sent.is_empty());
    }
}
