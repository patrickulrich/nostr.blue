// Profile prefetch utility
// Consolidated, optimized profile prefetching for various event types

use nostr_sdk::{Event, PublicKey};
use std::collections::HashSet;
use crate::stores::profiles;

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

    // Extract unique pubkeys - no string conversion!
    let pubkeys: HashSet<PublicKey> = events
        .iter()
        .map(|e| e.author_pubkey())
        .collect();

    // Use optimized batch fetch
    match profiles::fetch_profiles_batch_native(pubkeys).await {
        Ok(_) => {
            // Profiles are now cached
        }
        Err(e) => {
            log::warn!("Failed to prefetch author metadata: {}", e);
        }
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

    match profiles::fetch_profiles_batch_native(pubkey_set).await {
        Ok(_) => {
            // Profiles are now cached
        }
        Err(e) => {
            log::warn!("Failed to prefetch metadata: {}", e);
        }
    }
}
