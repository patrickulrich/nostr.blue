//! Hydration fallback for NIP-18 reposts whose `content` does not embed the
//! original event.
//!
//! NIP-18 allows kind-6/16 reposts with empty `content` — the `e` tag is the
//! required, authoritative reference, and several clients emit exactly that
//! shape. [`crate::feeds::classify`] drops those reposts because there is no
//! original to unwrap; without this module they never reach the banner or the
//! rendered feed, and — because the feed cursor only advances past *kept*
//! items — every load re-fetches and re-drops them (observed live: the same
//! repost warned four times across initial/settle/refresh passes while the
//! cursor stayed pinned below it).
//!
//! The fallback lives at the async call sites, not in the classifier:
//! `classify_event` stays a pure, synchronous predicate. Realtime handlers
//! hydrate a single original via [`crate::stores::nostr_client::fetch_event_by_id`]
//! (the sanctioned store-first ladder); load paths batch-hydrate the page's
//! dropped reposts with one `ids` fetch. Originals that cannot be located are
//! tracked in a dead-list with bounded attempts and a cooldown (the
//! `PROFILE_EXHAUSTED` pattern) so unfetchable originals don't turn into a
//! per-pass fetch storm while the cursor remains pinned behind them.
//!
//! Rendering/UX parity: a hydrated [`FeedItem::Repost`] is structurally
//! identical to one unwrapped from embedded JSON, so the banner, NoteCard,
//! cache round-trip and dedup (by *original* event id) all behave the same.
//! Amethyst follows the same model — feeds admit e-tag-only reposts
//! (`RepostRenderability.kt`: "a repost that declares no boosted kind is
//! assumed renderable") and resolve the boosted note lazily by id.

use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use nostr::event::{Event, EventId};
use nostr_sdk::prelude::Filter;

use crate::feeds::types::FeedItem;
use crate::stores::nostr_client;

/// Max reposts hydrated per load page: bounds the single `ids` REQ and the
/// page's added latency. Excess reposts are simply skipped this pass (no
/// failure is recorded, so the next pass retries them).
pub const MAX_HYDRATION_BATCH: usize = 25;

/// Cooldown after a failed attempt before the original is retried.
const HYDRATION_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(300);

/// Max fetch attempts before an original is considered unfetchable until the
/// cooldown elapses. Deletions, NIP-70 protected events (whose reposts are
/// the spec's sanctioned reason for empty content) and unknown events all
/// land here.
const HYDRATION_MAX_ATTEMPTS: u8 = 2;

/// Originals whose hydration keeps failing. Maps
/// `original id -> (attempts, retry_deadline)`.
static HYDRATION_EXHAUSTED: GlobalSignal<HashMap<EventId, (u8, instant::Instant)>> =
    Signal::global(HashMap::new);

/// Pure eligibility predicate over a dead-list snapshot, for tests.
fn eligible_at(
    map: &HashMap<EventId, (u8, instant::Instant)>,
    original_id: &EventId,
    now: instant::Instant,
) -> bool {
    match map.get(original_id) {
        None => true,
        Some(&(attempts, retry_deadline)) => {
            attempts < HYDRATION_MAX_ATTEMPTS || now >= retry_deadline
        }
    }
}

/// Is hydrating this original currently eligible (not past the attempts cap,
/// or past the attempts cap but cooldown elapsed)?
pub fn hydration_eligible(original_id: &EventId) -> bool {
    let now = instant::Instant::now();
    eligible_at(&HYDRATION_EXHAUSTED.read(), original_id, now)
}

/// Record a failed hydration attempt: bumps the attempt count and (re)arms
/// the cooldown deadline.
pub fn record_hydration_failure(original_id: EventId) {
    let now = instant::Instant::now();
    let mut map = HYDRATION_EXHAUSTED.write();
    let entry = map.entry(original_id).or_insert((0, now));
    entry.0 = entry.0.saturating_add(1);
    entry.1 = now + HYDRATION_COOLDOWN;
}

/// Clear dead-list state after a successful hydration (e.g. a late publish
/// or a relay that finally has the original).
fn clear_hydration_exhaustion(original_id: &EventId) {
    HYDRATION_EXHAUSTED.write().remove(original_id);
}

/// Single-original hydration for realtime handlers: the facade's store-first
/// id ladder (SDK database → broadcast → hints). Returns `None` when the
/// original can't be located within the `Fast` (3s) budget — the caller
/// records the failure.
pub async fn hydrate_original(original_id: &EventId) -> Option<Event> {
    let input = original_id.to_hex();
    match nostr_client::fetch_event_by_id(&input, nostr_client::FetchProfile::Fast).await {
        Ok(Some(located)) => {
            // The facade already re-validated the event against the
            // derived id filter (R1), so `located.event.id == *original_id`.
            clear_hydration_exhaustion(original_id);
            Some(located.event)
        }
        Ok(None) => None,
        Err(e) => {
            log::debug!("Repost-original hydration failed for {}: {}", input, e);
            None
        }
    }
}

/// Pure candidate selection: reposts in `events` that `classify_event` drops
/// (empty/unparseable `content`) but that carry an `e`-tag reference, deduped
/// by referenced original id (first repost per original wins, matching the
/// feed's dedup-by-original-id semantics). Unit-testable without I/O.
pub fn select_hydration_candidates(events: &[Event]) -> Vec<Event> {
    let mut seen_refs: HashSet<EventId> = HashSet::new();
    let mut out = Vec::new();
    for event in events {
        if crate::utils::repost::is_repost(event)
            && crate::utils::repost::extract_reposted_event(event).is_err()
        {
            if let Some(ref_id) = crate::utils::repost::repost_original_ref(event) {
                if seen_refs.insert(ref_id) {
                    out.push(event.clone());
                }
            }
        }
    }
    out
}

/// Batch hydration for load paths: one `ids` REQ for the whole page's
/// dropped reposts (bounded by [`MAX_HYDRATION_BATCH`]). Returns one
/// [`FeedItem::Repost`] per repost whose original was located. Unfound
/// originals are recorded against the dead-list; cooldown-exhausted ones are
/// skipped entirely.
pub async fn hydrate_repost_batch(reposts: Vec<Event>) -> Vec<FeedItem> {
    let reposts: Vec<Event> = reposts
        .into_iter()
        .take(MAX_HYDRATION_BATCH)
        .collect();
    if reposts.is_empty() {
        return Vec::new();
    }

    // Skip originals on the dead-list before spending REQ budget on them.
    let eligible: Vec<Event> = reposts
        .into_iter()
        .filter(|repost| {
            repost
                .tags
                .event_ids()
                .next()
                .is_some_and(|ref_id| hydration_eligible(&ref_id))
        })
        .collect();
    if eligible.is_empty() {
        return Vec::new();
    }

    let ids: Vec<EventId> = eligible
        .iter()
        .filter_map(|repost| repost.tags.event_ids().next())
        .collect();
    let filter = Filter::new().ids(ids.clone()).limit(ids.len());
    let fetched = match nostr_client::fetch(filter, nostr_client::FetchProfile::Fast).await {
        Ok(events) => events,
        Err(e) => {
            log::debug!("Repost-original batch fetch failed: {}", e);
            // Network-level failure (not "original absent"): don't burn
            // attempts against ids the REQ never had a chance to serve.
            return Vec::new();
        }
    };
    let by_id: HashMap<EventId, Event> = fetched
        .into_iter()
        .map(|event| (event.id, event))
        .collect();

    let mut items = Vec::new();
    for repost in eligible {
        let Some(ref_id) = repost.tags.event_ids().next() else {
            continue;
        };
        match by_id.get(&ref_id) {
            Some(original) => {
                clear_hydration_exhaustion(&ref_id);
                items.push(FeedItem::Repost {
                    original: original.clone(),
                    reposted_by: repost.pubkey,
                    repost_timestamp: repost.created_at,
                });
            }
            None => record_hydration_failure(ref_id),
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::{EventBuilder, FinalizeEvent, Kind};
    use nostr_sdk::prelude::{EventId, Keys};

    fn keys() -> Keys {
        Keys::generate()
    }

    fn id(byte: u8) -> EventId {
        EventId::from_slice(&[byte; 32]).unwrap()
    }

    fn e_tag_repost(content: &str, ref_id: EventId, secs: u64) -> Event {
        use nostr::event::Tag;
        use nostr_sdk::prelude::Timestamp;
        EventBuilder::new(Kind::Repost, content)
            .tags(vec![Tag::event(ref_id)])
            .custom_created_at(Timestamp::from(secs))
            .finalize(&keys())
            .unwrap()
    }

    #[test]
    fn select_dedups_by_referenced_original() {
        let a = e_tag_repost("", id(1), 100);
        let b = e_tag_repost("", id(1), 200); // same original, second reposter
        let c = e_tag_repost("", id(2), 300);
        let selected = select_hydration_candidates(&[a, b, c]);
        assert_eq!(selected.len(), 2, "first repost per original wins");
    }

    #[test]
    fn select_skips_embedded_reposts_and_non_reposts() {
        let original = EventBuilder::new(Kind::TextNote, "hi")
            .finalize(&keys())
            .unwrap();
        let embedded = e_tag_repost(&original.as_json(), id(1), 100);
        let note = EventBuilder::new(Kind::TextNote, "plain")
            .finalize(&keys())
            .unwrap();
        assert!(select_hydration_candidates(&[embedded, note]).is_empty());
    }

    #[test]
    fn select_skips_unparseable_without_e_tag() {
        let orphan = EventBuilder::new(Kind::Repost, "not json")
            .finalize(&keys())
            .unwrap();
        assert!(select_hydration_candidates(&[orphan]).is_empty());
    }

    #[test]
    fn eligible_at_enforces_attempts_and_cooldown() {
        let mut map = HashMap::new();
        let target = id(9);
        // Under the attempts cap: always eligible, even mid-"cooldown".
        map.insert(target, (1, instant::Instant::now() + std::time::Duration::from_secs(600)));
        assert!(eligible_at(&map, &target, instant::Instant::now()));
        // Past the cap with a future deadline: blocked.
        map.insert(target, (HYDRATION_MAX_ATTEMPTS, instant::Instant::now() + std::time::Duration::from_secs(600)));
        assert!(!eligible_at(&map, &target, instant::Instant::now()));
        // Past the cap with a reached deadline: eligible again.
        map.insert(target, (HYDRATION_MAX_ATTEMPTS, instant::Instant::now()));
        assert!(eligible_at(&map, &target, instant::Instant::now()));
        // Unknown id: eligible.
        assert!(eligible_at(&map, &id(8), instant::Instant::now()));
    }
}
