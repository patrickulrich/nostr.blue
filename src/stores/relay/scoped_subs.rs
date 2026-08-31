//! Scoped subscription runtime: deduplicates subscriptions when multiple UI
//! surfaces want the same stream.
//!
//! Scoped-subscription deduplication. When two components declare
//! ownership of the same `(scope, key)` pair with equivalent configs, only
//! one REQ is fired. The second declaration returns `Unchanged`.
//!
//! ## Ownership model
//!
//! ```
//! Component A ─┐
//!              ├─► (scope, key) ──► ScopedSubRuntime ──► ONE relay REQ
//! Component B ─┘         (canonical filter dedup)
//! ```
//!
//! When the last owner drops, the runtime unsubscribes.

// Phase 3 infrastructure — no consumers yet within the crate.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use nostr_sdk::{Client, Filter, RelayUrl, SubscriptionId};

/// Identifies the owner of a scoped subscription (typically one per UI
/// component lifecycle).
pub type OwnerKey = u64;

/// The shareable stream identity (e.g. "home:following:pubkey").
pub type StreamKey = String;

/// The scope of a subscription (per-account or global).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SubScope {
    Account(String),
    Global,
}

/// Identifies a scoped subscription request.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScopedSubIdentity {
    pub owner: OwnerKey,
    pub key: StreamKey,
    pub scope: SubScope,
}

/// Configuration for a subscription.
#[derive(Clone, Debug)]
pub struct SubConfig {
    pub filters: Vec<Filter>,
    pub relay_urls: Option<Vec<RelayUrl>>,
}

impl SubConfig {
    /// Canonical comparison: order-insensitive multiset equality.
    /// Two configs are equivalent if they have the same set of filters
    /// and the same set of relay URLs, regardless of order.
    pub fn canonical_eq(&self, other: &Self) -> bool {
        // Compare filters as order-insensitive multisets
        if self.filters.len() != other.filters.len() {
            return false;
        }
        // For simplicity, compare sorted filter hashes.
        // A full canonical comparison would compare field-by-field, but
        // sorted-by-JSON-serialization is a practical approximation.
        let self_sorted: Vec<String> = self
            .filters
            .iter()
            .map(|f| {
                serde_json::to_string(f).unwrap_or_default()
            })
            .collect();
        let other_sorted: Vec<String> = other
            .filters
            .iter()
            .map(|f| serde_json::to_string(f).unwrap_or_default())
            .collect();
        let mut self_sorted = self_sorted;
        let mut other_sorted = other_sorted;
        self_sorted.sort();
        other_sorted.sort();
        if self_sorted != other_sorted {
            return false;
        }
        // Compare relay URLs as sets
        match (&self.relay_urls, &other.relay_urls) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                let set_a: HashSet<&RelayUrl> = a.iter().collect();
                let set_b: HashSet<&RelayUrl> = b.iter().collect();
                set_a == set_b
            }
            _ => false,
        }
    }
}

/// Result of declaring a scoped subscription.
#[derive(Debug)]
pub enum SetSubResult {
    /// A new subscription was created.
    Created { handle: ScopedSubHandle },
    /// An existing subscription was reused (same canonical config).
    Unchanged { handle: ScopedSubHandle },
    /// The subscription was updated (config changed).
    Updated { handle: ScopedSubHandle },
}

/// Handle returned by `set_sub`. Drop it to release ownership.
/// When the last owner for a `(scope, key)` drops, the runtime unsubscribes.
pub struct ScopedSubHandle {
    identity: ScopedSubIdentity,
    runtime: Arc<ScopedSubRuntimeInner>,
}

impl std::fmt::Debug for ScopedSubHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedSubHandle")
            .field("identity", &self.identity)
            .finish()
    }
}

impl Drop for ScopedSubHandle {
    fn drop(&mut self) {
        self.runtime.release_owner(&self.identity);
    }
}

/// Inner state of the scoped subscription runtime.
struct ScopedSubRuntimeInner {
    /// Desired subscriptions: (scope, key) → config.
    desired: Mutex<HashMap<(SubScope, StreamKey), SubConfig>>,
    /// Live subscription IDs: (scope, key) → SubscriptionId.
    live: Mutex<HashMap<(SubScope, StreamKey), SubscriptionId>>,
    /// Ownership tracking: (scope, key) → set of owners.
    owners_by_key: Mutex<HashMap<(SubScope, StreamKey), HashSet<OwnerKey>>>,
    /// Reverse: owner → set of (scope, key) they own.
    keys_by_owner: Mutex<HashMap<OwnerKey, HashSet<(SubScope, StreamKey)>>>,
    /// Next owner ID.
    next_owner: Mutex<OwnerKey>,
    client: Client,
}

impl ScopedSubRuntimeInner {
    fn next_owner_id(&self) -> OwnerKey {
        let mut next = self.next_owner.lock().unwrap();
        let id = *next;
        *next += 1;
        id
    }

    fn release_owner(&self, identity: &ScopedSubIdentity) {
        let key = (identity.scope.clone(), identity.key.clone());

        // Remove from owners_by_key
        let should_unsubscribe = {
            let mut owners = self.owners_by_key.lock().unwrap();
            if let Some(owner_set) = owners.get_mut(&key) {
                owner_set.remove(&identity.owner);
                if owner_set.is_empty() {
                    owners.remove(&key);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };

        // Remove from keys_by_owner
        {
            let mut keys = self.keys_by_owner.lock().unwrap();
            if let Some(key_set) = keys.get_mut(&identity.owner) {
                key_set.remove(&key);
                if key_set.is_empty() {
                    keys.remove(&identity.owner);
                }
            }
        }

        // Unsubscribe if last owner
        if should_unsubscribe {
            let mut live = self.live.lock().unwrap();
            let mut desired = self.desired.lock().unwrap();
            if let Some(sub_id) = live.remove(&key) {
                let client = self.client.clone();
                // Spawn unsubscribe (fire-and-forget)
                crate::platform::spawn::spawn_catch_unwind("scoped_unsub", async move {
                    let _ = client.unsubscribe(&sub_id).await;
                });
            }
            desired.remove(&key);
        }
    }
}

/// Manages scoped subscriptions with canonical filter deduplication.
///
/// One instance per application (shared via `Arc`).
pub struct ScopedSubRuntime {
    inner: Arc<ScopedSubRuntimeInner>,
}

impl ScopedSubRuntime {
    /// Construct with the SDK client.
    pub fn new(client: Client) -> Self {
        Self {
            inner: Arc::new(ScopedSubRuntimeInner {
                desired: Mutex::new(HashMap::new()),
                live: Mutex::new(HashMap::new()),
                owners_by_key: Mutex::new(HashMap::new()),
                keys_by_owner: Mutex::new(HashMap::new()),
                next_owner: Mutex::new(1),
                client,
            }),
        }
    }

    /// Generate a unique owner ID.
    pub fn new_owner_id(&self) -> OwnerKey {
        self.inner.next_owner_id()
    }

    /// Declare ownership of a scoped subscription.
    ///
    /// If another owner already has the same `(scope, key)` with an
    /// equivalent config (canonical comparison), no new subscription is
    /// created — returns `SetSubResult::Unchanged`.
    pub async fn set_sub(
        &self,
        identity: ScopedSubIdentity,
        config: SubConfig,
    ) -> Result<SetSubResult, String> {
        let key = (identity.scope.clone(), identity.key.clone());

        // Check if config changed
        let config_changed = {
            let desired = self.inner.desired.lock().unwrap();
            match desired.get(&key) {
                Some(existing) => !existing.canonical_eq(&config),
                None => true,
            }
        };

        // Register ownership
        {
            let mut owners = self.inner.owners_by_key.lock().unwrap();
            owners
                .entry(key.clone())
                .or_default()
                .insert(identity.owner);
        }
        {
            let mut keys = self.inner.keys_by_owner.lock().unwrap();
            keys.entry(identity.owner)
                .or_default()
                .insert(key.clone());
        }

        let handle = ScopedSubHandle {
            identity: identity.clone(),
            runtime: self.inner.clone(),
        };

        if !config_changed {
            // Config is the same — reuse existing subscription
            return Ok(SetSubResult::Unchanged { handle });
        }

        // Update desired config
        {
            let mut desired = self.inner.desired.lock().unwrap();
            desired.insert(key.clone(), config.clone());
        }

        // Unsubscribe old, subscribe new
        {
            let mut live = self.inner.live.lock().unwrap();
            if let Some(old_id) = live.remove(&key) {
                let client = self.inner.client.clone();
                crate::platform::spawn::spawn_catch_unwind("scoped_resub", async move {
                    let _ = client.unsubscribe(&old_id).await;
                });
            }
        }

        // Subscribe with new config
        let sub_id = if let Some(urls) = &config.relay_urls {
            self.inner
                .client
                .subscribe_to(urls.clone(), config.filters.first().cloned().unwrap_or_default(), None)
                .await
                .map_err(|e| e.to_string())?
                .val
        } else {
            self.inner
                .client
                .subscribe(
                    config.filters.first().cloned().unwrap_or_default(),
                    None,
                )
                .await
                .map_err(|e| e.to_string())?
                .val
        };

        {
            let mut live = self.inner.live.lock().unwrap();
            live.insert(key, sub_id);
        }

        if config_changed {
            Ok(SetSubResult::Updated { handle })
        } else {
            Ok(SetSubResult::Created { handle })
        }
    }

    /// Get the subscription ID for a scoped key, if active.
    pub fn get_sub_id(&self, scope: &SubScope, key: &str) -> Option<SubscriptionId> {
        let live = self.inner.live.lock().unwrap();
        live.get(&(scope.clone(), key.to_string())).cloned()
    }

    /// Number of active subscriptions.
    pub fn active_count(&self) -> usize {
        self.inner.live.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::Filter;

    #[test]
    fn canonical_eq_same_filters_different_order() {
        let f1 = Filter::new().kind(nostr_sdk::Kind::TextNote).limit(10);
        let f2 = Filter::new().kind(nostr_sdk::Kind::Repost).limit(5);
        let config_a = SubConfig {
            filters: vec![f1.clone(), f2.clone()],
            relay_urls: None,
        };
        let config_b = SubConfig {
            filters: vec![f2, f1], // different order
            relay_urls: None,
        };
        assert!(config_a.canonical_eq(&config_b));
    }

    #[test]
    fn canonical_eq_different_filters() {
        let config_a = SubConfig {
            filters: vec![Filter::new().kind(nostr_sdk::Kind::TextNote)],
            relay_urls: None,
        };
        let config_b = SubConfig {
            filters: vec![Filter::new().kind(nostr_sdk::Kind::Repost)],
            relay_urls: None,
        };
        assert!(!config_a.canonical_eq(&config_b));
    }

    #[test]
    fn canonical_eq_different_relay_urls() {
        let config_a = SubConfig {
            filters: vec![Filter::new()],
            relay_urls: Some(vec![RelayUrl::parse("wss://a.example.com").unwrap()]),
        };
        let config_b = SubConfig {
            filters: vec![Filter::new()],
            relay_urls: Some(vec![RelayUrl::parse("wss://b.example.com").unwrap()]),
        };
        assert!(!config_a.canonical_eq(&config_b));
    }

    #[test]
    fn canonical_eq_same_relay_urls_different_order() {
        let r1 = RelayUrl::parse("wss://a.example.com").unwrap();
        let r2 = RelayUrl::parse("wss://b.example.com").unwrap();
        let config_a = SubConfig {
            filters: vec![Filter::new()],
            relay_urls: Some(vec![r1.clone(), r2.clone()]),
        };
        let config_b = SubConfig {
            filters: vec![Filter::new()],
            relay_urls: Some(vec![r2, r1]),
        };
        assert!(config_a.canonical_eq(&config_b));
    }

    #[test]
    fn canonical_eq_none_vs_some_relay_urls() {
        let config_a = SubConfig {
            filters: vec![Filter::new()],
            relay_urls: None,
        };
        let config_b = SubConfig {
            filters: vec![Filter::new()],
            relay_urls: Some(vec![RelayUrl::parse("wss://a.example.com").unwrap()]),
        };
        assert!(!config_a.canonical_eq(&config_b));
    }
}
