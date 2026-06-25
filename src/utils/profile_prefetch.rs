use crate::stores::profiles;
use nostr_sdk::{Event, PublicKey};
use std::collections::HashSet;
/// Trait for types that have an author public key
pub trait HasAuthor {
    fn author_pubkey(&self) -> PublicKey;
}
/// Standard nostr Event implements HasAuthor
impl HasAuthor for Event {
    fn author_pubkey(&self) -> PublicKey {
        self.pubkey
    }
}
/// Helper to extract pubkey from any event-containing type
pub fn extract_pubkeys<T, F>(items: &[T], extractor: F) -> HashSet<PublicKey>
where
    F: Fn(&T) -> PublicKey,
{
    items.iter().map(extractor).collect()
}

/// Extract ALL pubkeys relevant to an event for metadata loading.
///
/// Collects:
/// 1. The event author
/// 2. All p-tagged pubkeys from event tags (`event.tags.public_keys()`)
/// 3. All `nostr:npub1…` and `nostr:nprofile1…` mentions parsed from the
///    event content (via `extract_mentioned_pubkeys`)
///
/// This matches Amethyst's `linkedPubKeys()` scope (`TextNoteEvent.kt:109-114`):
/// p-tags + npub/nprofile content mentions. It does NOT include `nevent1` /
/// `naddr1` authors — those require fetching the referenced event first and
/// are handled by separate quote/reply fetching logic.
///
/// The returned hex strings are deduplicated. Callers should enqueue each
/// via `profiles::queue_profile_request` for batched fetching.
pub fn extract_all_pubkeys_from_event(event: &Event) -> HashSet<String> {
    let mut pubkeys = HashSet::new();
    pubkeys.insert(event.pubkey.to_hex());
    for pk in event.tags.public_keys() {
        pubkeys.insert(pk.to_hex());
    }
    for pk in crate::utils::parsing::mention_extractor::extract_mentioned_pubkeys(&event.content) {
        pubkeys.insert(pk.to_hex());
    }
    pubkeys
}

/// Prefetch author metadata for a slice of events
///
/// This is the optimized, unified function that replaces all the duplicate
/// prefetch_author_metadata functions across different routes.
///
/// Benefits:
/// - Works with PublicKey natively (no string conversions)
/// - Single lock for cache lookups
/// - Direct database queries before hitting relays
/// - Deduplicates authors automatically
pub async fn prefetch_event_authors<T: HasAuthor>(events: &[T]) {
    if events.is_empty() {
        return;
    }
    let pubkeys: HashSet<PublicKey> = events.iter().map(|e| e.author_pubkey()).collect();
    if let Err(e) = profiles::fetch_profiles_batch_native(pubkeys).await {
        log::warn!("Failed to prefetch author metadata: {}", e);
    }
}
/// Prefetch metadata for a collection of public keys
///
/// Use this when you have pubkeys directly rather than events
pub async fn prefetch_pubkeys(pubkeys: impl IntoIterator<Item = PublicKey>) {
    let pubkey_set: HashSet<PublicKey> = pubkeys.into_iter().collect();
    if pubkey_set.is_empty() {
        return;
    }
    if let Err(e) = profiles::fetch_profiles_batch_native(pubkey_set).await {
        log::warn!("Failed to prefetch metadata: {}", e);
    }
}

/// Prefetch author metadata AND relay lists for a slice of events.
/// Relay lists are resolved in background for nprofile URL generation.
pub async fn prefetch_event_authors_with_relays<T: HasAuthor>(events: &[T]) {
    if events.is_empty() {
        return;
    }
    let pubkeys: HashSet<PublicKey> = events.iter().map(|e| e.author_pubkey()).collect();
    let _ = profiles::fetch_profiles_batch_native(pubkeys.clone()).await;

    let pk_hexes: Vec<String> = pubkeys.iter().map(|pk| pk.to_hex()).collect();
    dioxus::prelude::spawn(async move {
        for pk_hex in pk_hexes {
            let _ = crate::stores::relay::coverage::resolve_user_relays(
                &pk_hex,
                crate::stores::relay::coverage::RelayPurpose::Write,
            )
            .await;
        }
    });
}
