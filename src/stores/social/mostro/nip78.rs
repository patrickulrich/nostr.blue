//! Mostro P2P exchange Terms-of-Service acceptance (NIP-78)
//!
//! Before a user can take a Mostro order or browse Mostro-sourced orders, they
//! must accept a disclaimer. We publish a NIP-78 (kind 30078) replaceable event
//! with a known `d` tag, and gate the `/p2p` route on its presence + version.
//!
//! This is an improvement over the cashu terms pattern (see
//! `src/stores/cashu/init.rs:196-250`):
//!
//! 1. **Signature verification**: we call `event.verify()` to ensure the
//!    acceptance event was actually signed by the user's pubkey.
//! 2. **Version enforcement**: the event content includes a `version` field;
//!    we re-prompt if the local `P2P_TERMS_VERSION` is higher than the
//!    accepted version in the event.
//! 3. **Local cache**: `platform::storage` caches the last accepted version so
//!    offline users are not stuck on the spinner (cashu has no cache).
//!
//! On first load the global app-init code should call [`check_p2p_terms_accepted`]
//! alongside the other NIP-78 first-load checks (sidebar, reactions, settings).
//! A lazy fallback in `P2PHome` re-runs the check for deep-link direct nav.

use dioxus::prelude::*;
use nostr::prelude::*;
use nostr_sdk::Event as NostrEvent;
use serde::{Deserialize, Serialize};
use std::result::Result;
use std::time::Duration;

use crate::platform::storage;
use crate::stores::auth_store;
use crate::stores::nostr_client;
use crate::stores::publish_queue::{self, types::QueueEventType};

/// NIP-78 d-tag for the Mostro terms agreement event.
pub const P2P_TERMS_D_TAG: &str = "nostr.blue/p2p/terms";

/// Bump this to force all users to re-accept the terms.
pub const P2P_TERMS_VERSION: u32 = 1;

/// Cache key for offline-friendly terms check.
const CACHE_KEY_VERSION: &str = "p2p_terms_accepted_version";

/// `None` = not yet checked, `Some(true)` = accepted (current version), `Some(false)` = not accepted.
#[allow(dead_code)]
pub static P2P_TERMS_ACCEPTED: GlobalSignal<Option<bool>> = Signal::global(|| None);
/// Last accepted terms version found on relays or in cache.
#[allow(dead_code)]
pub static P2P_TERMS_VERSION_ACCEPTED: GlobalSignal<Option<u32>> = Signal::global(|| None);

/// Persisted snapshot of the terms content. Used to read what version was
/// previously accepted (for offline cache) and to write a new acceptance.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct P2PTermsAcceptance {
    pub accepted_at: u64,
    pub version: u32,
}

/// Read the local cache. Returns the cached version if present.
fn read_cache() -> Option<u32> {
    storage::get::<u32>(CACHE_KEY_VERSION).ok()
}

fn write_cache(version: u32) -> Result<(), String> {
    storage::set(CACHE_KEY_VERSION, &version)
        .map_err(|e| format!("failed to persist terms cache: {e}"))
}

/// Verify an event: pubkey matches user, event.verify() passes, content
/// deserializes to a valid `P2PTermsAcceptance` with version >= current.
///
/// Returns `Some(version)` on success, `None` on any failure.
#[allow(dead_code)]
fn evaluate_event(event: &NostrEvent, user_pubkey: &PublicKey) -> Option<u32> {
    if event.pubkey != *user_pubkey {
        return None;
    }
    if event.verify().is_err() {
        return None;
    }
    let parsed: P2PTermsAcceptance = serde_json::from_str(&event.content).ok()?;
    if parsed.version >= P2P_TERMS_VERSION {
        Some(parsed.version)
    } else {
        None
    }
}

/// Check whether the user has accepted the current version of the terms.
///
/// Order of operations:
/// 1. **Local cache first** (offline-resilient): if the cache says we
///    accepted a version >= current, return true without network.
/// 2. **Network fetch**: a kind 30078 with our d-tag, signed by us.
/// 3. For each event, validate signature + parse content + check version.
/// 4. Update cache and signals.
///
/// The cashu pattern doesn't have the cache; we add it for offline use.
#[allow(dead_code)]
pub async fn check_p2p_terms_accepted() -> Result<bool, String> {
    // Fast path: cache hit
    if let Some(cached) = read_cache() {
        if cached >= P2P_TERMS_VERSION {
            *P2P_TERMS_ACCEPTED.write() = Some(true);
            *P2P_TERMS_VERSION_ACCEPTED.write() = Some(cached);
            return Ok(true);
        }
    }

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {e}"))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(30078))
        .identifier(P2P_TERMS_D_TAG)
        .limit(1);
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            let accepted_version = events
                .iter()
                .filter_map(|e| evaluate_event(e, &pubkey))
                .max();
            let accepted = accepted_version.is_some();
            if let Some(v) = accepted_version {
                let _ = write_cache(v);
                *P2P_TERMS_VERSION_ACCEPTED.write() = Some(v);
            }
            *P2P_TERMS_ACCEPTED.write() = Some(accepted);
            Ok(accepted)
        }
        Err(e) => {
            // On network failure, if cache says we were good, return true
            if read_cache().is_some_and(|v| v >= P2P_TERMS_VERSION) {
                *P2P_TERMS_ACCEPTED.write() = Some(true);
                return Ok(true);
            }
            log::warn!("Failed to check Mostro terms: {e}");
            Err(format!("Failed to check terms: {e}"))
        }
    }
}

/// Publish a NIP-78 acceptance event for the current `P2P_TERMS_VERSION`.
#[allow(dead_code)]
pub async fn accept_p2p_terms() -> Result<(), String> {
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    let now = crate::platform::timestamp::now_secs();
    let payload = P2PTermsAcceptance {
        accepted_at: now,
        version: P2P_TERMS_VERSION,
    };
    let content = serde_json::to_string(&payload)
        .map_err(|e| format!("Failed to serialize terms: {e}"))?;

    let builder = EventBuilder::new(Kind::from(30078), content).tag(Tag::identifier(P2P_TERMS_D_TAG));
    let event = publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign terms: {e}"))?;

    publish_queue::enqueue_and_await(
        event,
        QueueEventType::Other("p2p_terms".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await
    .map_err(|e| format!("Failed to publish terms: {e}"))?;

    write_cache(P2P_TERMS_VERSION)?;
    *P2P_TERMS_ACCEPTED.write() = Some(true);
    *P2P_TERMS_VERSION_ACCEPTED.write() = Some(P2P_TERMS_VERSION);
    Ok(())
}

/// Reset the local cache and in-memory signals. Used when logging out.
#[allow(dead_code)]
pub fn reset() {
    let _ = storage::delete(CACHE_KEY_VERSION);
    *P2P_TERMS_ACCEPTED.write() = None;
    *P2P_TERMS_VERSION_ACCEPTED.write() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_p2p_terms_d_tag_matches_convention() {
        // The d-tag follows the project's "nostr.blue/<feature>" convention
        // (see sidebar_store.rs, reactions_store.rs, etc.)
        assert!(P2P_TERMS_D_TAG.starts_with("nostr.blue/"));
        assert!(P2P_TERMS_D_TAG.ends_with("/terms"));
    }

    #[test]
    fn test_p2p_terms_version_is_positive() {
        assert!(P2P_TERMS_VERSION >= 1);
    }

    #[test]
    fn test_p2p_terms_acceptance_serde_roundtrip() {
        let payload = P2PTermsAcceptance {
            accepted_at: 1_700_000_000,
            version: 1,
        };
        let json = serde_json::to_string(&payload).unwrap();
        let parsed: P2PTermsAcceptance = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.accepted_at, payload.accepted_at);
        assert_eq!(parsed.version, payload.version);
    }
}
