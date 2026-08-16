//! Mostro reputation events (kind 38384).
//!
//! Phase 8: parse, cache, and surface counterparty reputation from
//! the daemon's kind 38384 rating events. The daemon publishes these
//! as NIP-33 parameterized replaceable events with `y=mostro z=rating`
//! platform tags.
//!
//! **d-tag keying caveat** (daemon-side bug, tracked upstream): the spec
//! (`mostro/docs/SEPARATE_EVENT_KINDS_SPEC.md` §Rating) says `d` = the
//! rated user's master pubkey, but mostrod 0.18.x keys it by the *rater's*
//! single-use trade key (`mostro/src/app/rate_user.rs:23-35` resolves the
//! sender-side key). A `#d` query by the counterparty's pubkey can
//! therefore never match — the fetch below is kept as best-effort for
//! future fixed daemons. The authoritative in-trade source is the inline
//! `Payload::Peer.reputation` snapshot (`record_peer_reputation`), which
//! current mostrod leaves `None` until it populates it.
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

    // Extract the d-tag value (the pubkey the daemon indexed the rating
    // under — see the module doc for the keying caveat; keyed literally).
    let pubkey = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::d())
        .and_then(|t| t.as_slice().get(1).map(String::as_str).map(str::to_owned))?;

    // Parse rating from the event tags.
    let rating = Rating::from_tags(event.tags.clone()).ok()?;

    Some((pubkey, rating))
}

/// Record a reputation snapshot delivered inline in `Payload::Peer`.
///
/// The daemon attaches `Peer.reputation: Option<UserInfo>` when disclosing
/// the counterparty (`FiatSentOk`) or an assigned solver
/// (`AdminTookDispute`). The cache is keyed by the normalized hex pubkey
/// carried in the payload. Current mostrod sends `None`; this keeps the
/// client correct the day it populates the field.
pub fn record_peer_reputation(peer: &mostro_core::prelude::Peer) {
    let Some((hex, rating)) = peer_reputation_snapshot(peer) else {
        return;
    };
    // GlobalSignal writes require a Dioxus runtime; `apply_mostro_action`
    // unit tests exercise the FSM without one, so skip the cache write
    // there (the mapping itself is covered by its own test below).
    if dioxus::prelude::dioxus_core::Runtime::try_current().is_none() {
        return;
    }
    upsert_rating(hex, rating);
}

/// Pure `Peer` → `(normalized hex pubkey, Rating)` mapping.
///
/// Returns `None` when the payload carries no reputation or the pubkey
/// can't be normalized. `UserInfo` carries only the aggregate
/// (rating/reviews/days); the min/max/last snapshot fields default to
/// the rounded aggregate.
fn peer_reputation_snapshot(peer: &mostro_core::prelude::Peer) -> Option<(String, Rating)> {
    let info = peer.reputation.as_ref()?;
    let hex = PublicKey::from_hex(&peer.pubkey)
        .or_else(|_| PublicKey::from_bech32(&peer.pubkey))
        .ok()
        .map(|pk| pk.to_hex())
        .or_else(|| {
            log::debug!(
                "peer reputation with unparseable pubkey: {}",
                peer.pubkey
            );
            None
        })?;
    let rounded = info.rating.round().clamp(1.0, 5.0) as u8;
    Some((
        hex,
        Rating::new(
            info.reviews.max(0) as u64,
            info.rating,
            rounded,
            rounded,
            rounded,
        ),
    ))
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
    fn test_record_peer_reputation_mapping() {
        // No reputation → no snapshot.
        let bare = mostro_core::prelude::Peer::new("abc123".to_string(), None);
        assert!(peer_reputation_snapshot(&bare).is_none());

        // With reputation → normalized hex key + aggregate mapping.
        let pk = PublicKey::from_hex(
            "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390",
        )
        .unwrap();
        let info = mostro_core::prelude::UserInfo {
            rating: 4.6,
            reviews: 21,
            operating_days: 90,
        };
        let peer = mostro_core::prelude::Peer::new(pk.to_hex(), Some(info));
        let (hex, rating) = peer_reputation_snapshot(&peer).expect("snapshot");
        assert_eq!(hex, pk.to_hex());
        assert_eq!(rating.total_reviews, 21);
        assert!((rating.total_rating - 4.6).abs() < f64::EPSILON);
        assert_eq!(rating.last_rating, 5);

        // Unparseable pubkey → no snapshot.
        let junk = mostro_core::prelude::Peer::new("not-a-pubkey".to_string(), Some(
            mostro_core::prelude::UserInfo {
                rating: 3.0,
                reviews: 1,
                operating_days: 1,
            },
        ));
        assert!(peer_reputation_snapshot(&junk).is_none());
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
