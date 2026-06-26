//! Mostro reputation events (kind 38384).
//!
//! Phase 8: parse, cache, and surface counterparty reputation from
//! the daemon's kind 38384 rating events. The daemon publishes these
//! as NIP-33 parameterized replaceable events with `d`-tag = hex user
//! master pubkey and `y=mostro z=rating` platform tags.
//!
//! See `mostro/src/nip33.rs:83-96` and `mostro-core/src/rating.rs`.

use dioxus::prelude::*;
use mostro_core::prelude::Rating;
use nostr::prelude::*;
use nostr_sdk::Event as NostrEvent;

/// Kind for Mostro rating events.
pub const RATING_EVENT_KIND: u16 = 38384;

/// Parse a kind 38384 event into a `(pubkey_hex, Rating)` pair.
///
/// Validates:
/// - Kind is 38384.
/// - Has `y=mostro` and `z=rating` tags (rejects non-Mostro apps).
/// - Has a `d` tag whose value is the rated user's hex master pubkey.
/// - Tags parse successfully into `mostro_core::prelude::Rating`.
///
/// Returns `None` on any validation failure.
pub fn parse_rating_event(event: &NostrEvent) -> Option<(String, Rating)> {
    if event.kind.as_u16() != RATING_EVENT_KIND {
        return None;
    }

    // Validate platform tags.
    let has_y = event.tags.iter().any(|t| {
        t.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("y"))
            && t.as_slice().get(1).map(|s| s.as_str()) == Some("mostro")
    });
    let has_z = event.tags.iter().any(|t| {
        t.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("z"))
            && t.as_slice().get(1).map(|s| s.as_str()) == Some("rating")
    });
    if !has_y || !has_z {
        return None;
    }

    // Extract the d-tag (hex master pubkey of the rated user).
    let pubkey = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::d())
        .and_then(|t| t.as_slice().get(1).map(String::as_str).map(str::to_owned))?;

    // Parse rating from the event tags.
    let rating = Rating::from_tags(event.tags.clone()).ok()?;

    Some((pubkey, rating))
}

/// Build a subscription filter for a single user's rating event.
///
/// `author` is the daemon's pubkey; `user_pubkey_hex` is the rated user's
/// hex master pubkey (the d-tag value).
pub fn rating_filter(author: PublicKey, user_pubkey_hex: &str) -> nostr_sdk::Filter {
    nostr_sdk::Filter::new()
        .kind(nostr::Kind::Custom(RATING_EVENT_KIND))
        .author(author)
        .identifier(user_pubkey_hex.to_string())
        .limit(1)
}

/// Build a batch subscription filter for multiple users' ratings.
#[allow(dead_code)]
pub fn rating_filter_batch(
    author: PublicKey,
    user_pubkeys_hex: &[String],
) -> nostr_sdk::Filter {
    nostr_sdk::Filter::new()
        .kind(nostr::Kind::Custom(RATING_EVENT_KIND))
        .author(author)
        .identifiers(user_pubkeys_hex.to_vec())
        .limit(user_pubkeys_hex.len())
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

/// Maximum cached entries. Sized for typical usage (a few hundred
/// counterparties across all trades). Bug #4 fix: replaced the previous
/// HashMap-backed cache (which used `keys().next()` for eviction —
/// non-deterministic iteration order could evict a recently-used entry)
/// with a real `lru::LruCache` that tracks access order properly.
const MAX_CACHE: usize = 500;

/// Global reactive cache: hex master pubkey → Rating.
///
/// Read with `RATINGS.read().peek(&hex)` (immutable, doesn't touch LRU
/// order) or `get_rating(&hex)` (helper that does the same). For
/// iteration, use `RATINGS.read().iter()`.
pub static RATINGS: GlobalSignal<lru::LruCache<String, Rating>> =
    Signal::global(|| lru::LruCache::new(std::num::NonZeroUsize::new(MAX_CACHE).unwrap()));

/// Insert a rating into the cache. `LruCache::put` handles automatic
/// LRU eviction when the capacity is exceeded — no manual eviction
/// logic needed (unlike the previous HashMap implementation).
pub fn upsert_rating(pubkey_hex: String, rating: Rating) {
    RATINGS.write().put(pubkey_hex, rating);
}

/// Get a cached rating by hex pubkey. Uses `peek` (immutable borrow)
/// so reading a rating doesn't affect its LRU position — important
/// because `RATINGS()` clones the entire cache for reactive reads,
/// and mutating LRU order on every clone would be incorrect.
#[allow(dead_code)]
pub fn get_rating(pubkey_hex: &str) -> Option<Rating> {
    RATINGS.read().peek(pubkey_hex).cloned()
}

/// Clear the ratings cache (e.g., on logout).
#[allow(dead_code)]
pub fn clear_ratings() {
    RATINGS.write().clear();
}

/// Format a rating as a star string (e.g., "★★★☆☆" for 3.0).
pub fn format_stars(rating: f64) -> String {
    let rounded = rating.round() as u8;
    let stars: String = (1..=5)
        .map(|i| if i <= rounded { '★' } else { '☆' })
        .collect();
    stars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_stars() {
        assert_eq!(format_stars(5.0), "★★★★★");
        assert_eq!(format_stars(4.5), "★★★★★");
        assert_eq!(format_stars(4.0), "★★★★☆");
        assert_eq!(format_stars(3.2), "★★★☆☆");
        assert_eq!(format_stars(0.0), "☆☆☆☆☆");
        assert_eq!(format_stars(1.0), "★☆☆☆☆");
    }

    #[test]
    fn test_rating_filter_shape() {
        let pk = PublicKey::from_hex(
            "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390",
        )
        .unwrap();
        let f = rating_filter(pk, "abcdef1234");
        assert!(f.kinds.is_some());
        assert!(f.authors.is_some());
        // generic_tags contains the d-tag identifier; verify the filter
        // has at least one tag entry.
        assert!(!f.generic_tags.is_empty());
    }

    #[test]
    fn test_rating_filter_batch_shape() {
        let pk = PublicKey::from_hex(
            "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390",
        )
        .unwrap();
        let pks = vec!["abc".to_string(), "def".to_string()];
        let f = rating_filter_batch(pk, &pks);
        assert!(f.kinds.is_some());
        assert!(!f.generic_tags.is_empty());
    }
}
