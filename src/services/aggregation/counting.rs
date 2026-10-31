use super::*;

use crate::stores::nostr_client::{fetch_events_from_connected_relays_with_client, get_client};
use crate::stores::signer::SIGNER_INFO;
use crate::utils::bolt11::parse_bolt11_amount;
use dioxus::prelude::ReadableExt;
use futures::join;
use instant::{Duration, Instant};
use nostr::nips::nip10::Nip10Tag;
use nostr_sdk::relay::{SyncDirection, SyncOptions};
use nostr_sdk::client::Client;
use nostr::prelude::{Event, EventId, Filter, Kind, Timestamp};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

const MAX_RELAY_LIMIT: usize = 5_000;
/// Relay-side counts backfill burst bounds (v2 boot-stall plan) — shared by
/// the boot/interaction entry points (`fetch_interaction_counts_batch` and
/// the first-load variant) so no caller can regress to unbounded pagination.
/// The 0.45 SDK parses + verifies every fetched event on the main thread
/// with no yield point inside a page; uncapped pagination pulled 233-449
/// events (observed) onto the main thread during boot. 200/page × 2 pages ×
/// 300 total bounds the contiguous burst; truncated counts converge via the
/// live stream. The zap-receipts path uses its own dedicated bounds (see
/// `fetch_zap_receipt_counts`) sized for single popular notes.
const INTERACTION_BACKFILL_PAGE_LIMIT: usize = 200;
const INTERACTION_BACKFILL_MAX_PAGES: usize = 2;
const INTERACTION_BACKFILL_MAX_TOTAL: usize = 300;

/// Zap-receipts backfill bounds: receipts for a single popular note reach
/// the thousands, so the shared boot bounds (300) are too tight — but the
/// previous `usize::MAX` unbounded pagination contradicted the burst-bound
/// invariant above. 1000/page × 3 pages × 2500 total keeps the worst-case
/// parse+verify burst bounded while still counting large zaps.
const ZAP_PAGE_LIMIT: usize = 1_000;
const ZAP_MAX_PAGES: usize = 3;
const ZAP_MAX_TOTAL: usize = 2_500;

/// Pagination count-flush bounds (3c-3): tighter than the shared backfill
/// caps because the scroll-triggered flush runs while the user is
/// reading. Every fetched event costs JSON parse + unconditional
/// schnorr verify + IndexedDB save on the single wasm thread (SDK 0.45.4
/// `verify_and_cache`, no skip knob), and `page_limit` applies PER RELAY
/// in the targeted fast fetch (page × ~8 relays raw before dedup) — the
/// shared 200/page cap observed 300-351 events per page-batch and 2-4s
/// main-thread stalls mid-scroll. 100/page × 2 × 150 halved that; 3c-7
/// halved again to 50/page × 2 × 100 after the single-flight re-measure:
/// span stacking was gone (max 2 open count_fetch spans) but the one big
/// coalesced round (66 anchors, ≤150 events) parsing beside a coincident
/// micro batch still blocked the loop 2-3s at scroll pauses. Truncated
/// counts converge via the live stream (Amethyst does not bulk-fetch
/// counts at all — local DB + live stream).
const INTERACTION_PAGINATION_PAGE_LIMIT: usize = 50;
const INTERACTION_PAGINATION_MAX_PAGES: usize = 2;
const INTERACTION_PAGINATION_MAX_TOTAL: usize = 100;

/// Single source of truth for the relay backfill burst bounds (see the
/// consts above). Shared by all counts entry points so the paths cannot
/// drift apart.
pub(crate) fn interaction_backfill_limits() -> (usize, usize, usize) {
    (
        INTERACTION_BACKFILL_PAGE_LIMIT,
        INTERACTION_BACKFILL_MAX_PAGES,
        INTERACTION_BACKFILL_MAX_TOTAL,
    )
}

/// Single source of truth for the zap-receipts bounds (see consts above).
pub(crate) fn zap_backfill_limits() -> (usize, usize, usize) {
    (ZAP_PAGE_LIMIT, ZAP_MAX_PAGES, ZAP_MAX_TOTAL)
}

/// Burst bounds for the idle-deferred pagination count flush (3c-2/3c-3).
/// See [`INTERACTION_PAGINATION_PAGE_LIMIT`] for why these sit below the
/// shared backfill caps.
pub(crate) fn interaction_pagination_limits() -> (usize, usize, usize) {
    (
        INTERACTION_PAGINATION_PAGE_LIMIT,
        INTERACTION_PAGINATION_MAX_PAGES,
        INTERACTION_PAGINATION_MAX_TOTAL,
    )
}

/// Phase 2 escalation switch — FLIPPED (v2 boot-stail plan, pre-authorized):
/// the first-load bulk backfill is disabled entirely — the Amethyst model,
/// where counts come from the local DB (`fetch_local_db_counts` counts each
/// kind via db.count) plus the live since-now stream and nothing is
/// bulk-fetched at paint time. The `98630372` smoke traced the boot burst to
/// the ALL-kinds paths (global interaction processor / refresh fallback),
/// which are now burst-bounded by `interaction_backfill_limits`; this const
/// closes the remaining first-load path.
pub const COUNTS_FIRST_LOAD_BACKFILL: bool = false;
/// Full kind set (replies included) — pagination, polls and AI-tool callers.
const ALL_COUNT_KINDS: [Kind; 6] = [
    Kind::TextNote,
    Kind::Comment,
    Kind::Reaction,
    Kind::Repost,
    Kind::ZapReceipt,
    Kind::Custom(1010),
];
/// Trimmed kind set for the first-load backfill: TextNote/Comment (reply
/// bodies — the heavy payloads) are dropped; reply counts remain served by
/// the local DB (`fetch_local_db_counts` counts them via db.count) and the
/// live stream (which subscribes to all kinds).
const FIRST_LOAD_COUNT_KINDS: [Kind; 4] = [
    Kind::Reaction,
    Kind::Repost,
    Kind::ZapReceipt,
    Kind::Custom(1010),
];
const INTERACTION_EVENT_BATCH_SIZE: usize = 100;

#[derive(Clone, Eq, Hash, PartialEq)]
struct Nip45CacheKey {
    relay_url: String,
    kind: u16,
}

/// NIP-45 support status for a relay
#[derive(Clone)]
struct Nip45SupportStatus {
    /// Whether relay supports COUNT
    supported: bool,
    /// When this status was recorded
    checked_at: Instant,
}

impl Nip45SupportStatus {
    fn new(supported: bool) -> Self {
        Self {
            supported,
            checked_at: Instant::now(),
        }
    }

    /// Negative results expire after 10 minutes (relay may have been updated)
    /// Positive results don't expire (once confirmed, unlikely to change)
    fn is_valid(&self) -> bool {
        if self.supported {
            true
        } else {
            self.checked_at.elapsed() < Duration::from_secs(600)
        }
    }
}

/// Cache for tracking which relays support NIP-45 COUNT
///
/// - `Nip45SupportStatus { supported: true }`: Relay supports COUNT (permanent)
/// - `Nip45SupportStatus { supported: false }`: Relay failed COUNT (TTL: 10 minutes)
/// - Not present: Unknown, needs testing
static NIP45_SUPPORT: OnceLock<Mutex<HashMap<Nip45CacheKey, Nip45SupportStatus>>> = OnceLock::new();

/// Get or initialize the NIP-45 support cache
fn get_nip45_cache() -> &'static Mutex<HashMap<Nip45CacheKey, Nip45SupportStatus>> {
    NIP45_SUPPORT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Attempt to get COUNT from relays that support NIP-45
///
/// This is a best-effort optimization - if no relays support COUNT or
/// all COUNT requests fail, returns None and caller should fall back
/// to full event fetch.
///
/// # Arguments
/// * `event_id` - The event to count interactions for
/// * `kind` - The interaction kind to count (Reaction, Repost, etc.)
/// * `timeout` - Short timeout for COUNT request (should be quick)
///
/// # Returns
/// * `Some(count)` - COUNT succeeded on at least one relay
/// * `None` - COUNT not supported or failed on all relays
#[allow(dead_code)]
async fn try_count_from_relays(event_id: &EventId, kind: Kind, timeout: Duration) -> Option<usize> {
    let client = get_client()?;
    let filter = Filter::new().kind(kind).event(*event_id);
    let relays = client.relays().await;
    for (url, relay) in relays.iter() {
        let url_str = url.to_string();
        let cache_key = Nip45CacheKey {
            relay_url: url_str.clone(),
            kind: kind.as_u16(),
        };
        let should_try = {
            let cache = get_nip45_cache().lock().unwrap_or_else(|poisoned| {
                log::warn!("NIP-45 cache mutex was poisoned, recovering");
                poisoned.into_inner()
            });
            match cache.get(&cache_key) {
                Some(status) if status.is_valid() => status.supported,
                Some(_) => true,
                None => true,
            }
        };
        if !should_try {
            continue;
        }
        let count_timeout = Duration::from_millis(timeout.as_millis().min(2000) as u64);
        match relay.count_events(filter.clone(), count_timeout).await {
            Ok(count) => {
                {
                    let mut cache = get_nip45_cache().lock().unwrap_or_else(|poisoned| {
                        log::warn!("NIP-45 cache mutex was poisoned, recovering");
                        poisoned.into_inner()
                    });
                    cache.insert(cache_key.clone(), Nip45SupportStatus::new(true));
                }
                log::debug!("COUNT from {}: {} events", url, count);
                return Some(count);
            }
            Err(e) => {
                {
                    let mut cache = get_nip45_cache().lock().unwrap_or_else(|poisoned| {
                        log::warn!("NIP-45 cache mutex was poisoned, recovering");
                        poisoned.into_inner()
                    });
                    cache.insert(cache_key.clone(), Nip45SupportStatus::new(false));
                }
                log::debug!("COUNT failed on {}: {}", url, e);
            }
        }
    }
    None
}

/// Get interaction counts using COUNT when available, with fallback to full fetch
///
/// This is the COUNT-first strategy with silent fallback:
/// 1. Try COUNT on supporting relays (fast, low bandwidth)
/// 2. If COUNT unavailable, fall back to full event fetch
///
/// Note: COUNT only returns totals, not user's own reaction state.
/// User reaction state is determined separately via full fetch or cache.
#[allow(dead_code)]
pub async fn get_counts_with_count_fallback(
    event_id: &EventId,
    timeout: Duration,
) -> InteractionCounts {
    let mut counts = InteractionCounts::default();
    let mut zap_receipts_materialized = false;
    let (reactions, reposts, replies, zaps) = join!(
        try_count_from_relays(event_id, Kind::Reaction, timeout),
        try_count_from_relays(event_id, Kind::Repost, timeout),
        async {
            let (text_notes, comments) = join!(
                try_count_from_relays(event_id, Kind::TextNote, timeout),
                try_count_from_relays(event_id, Kind::Comment, timeout)
            );
            match (text_notes, comments) {
                (Some(text_notes), Some(comments)) => Some(text_notes + comments),
                _ => None,
            }
        },
        try_count_from_relays(event_id, Kind::ZapReceipt, timeout),
    );
    let mut missing_reactions = false;
    let mut missing_reposts = false;
    let mut missing_replies = false;
    let mut missing_zaps = false;
    if let Some(count) = reactions {
        counts.likes = count;
    } else {
        missing_reactions = true;
    }
    if let Some(count) = reposts {
        counts.reposts = count;
    } else {
        missing_reposts = true;
    }
    if let Some(count) = replies {
        counts.replies = count;
    } else {
        missing_replies = true;
    }
    if let Some(count) = zaps {
        counts.zaps = count;
    } else {
        missing_zaps = true;
    }
    let needs_fallback = missing_reactions || missing_reposts || missing_replies;
    if needs_fallback {
        log::debug!(
            "COUNT incomplete for {}, using full fetch",
            event_id.to_hex()
        );
        if let Ok(batch_counts) = fetch_interaction_counts_batch(vec![*event_id], timeout).await {
            if let Some(fetched) = batch_counts.get(&event_id.to_hex()) {
                counts = fetched.clone();
                zap_receipts_materialized = true;
            }
        }
    }

    let needs_zap_details = !zap_receipts_materialized
        && (missing_zaps
            || (counts.zaps > 0 && (counts.user_zapped.is_none() || counts.zap_amount_sats == 0)));
    if needs_zap_details {
        log::debug!("Fetching zap receipt details for {}", event_id.to_hex());
        if let Ok(fetched) = fetch_zap_receipt_counts(event_id, timeout).await {
            counts.zaps = fetched.zaps;
            counts.zap_amount_sats = fetched.zap_amount_sats;
            counts.user_zapped = fetched.user_zapped;
        }
    }
    counts
}

struct FetchEventsPage {
    events: Vec<Event>,
    relay_page_len: usize,
    relay_oldest_created_at: Option<u64>,
}

/// R5a: caps concurrent count-backfill fetches. The SDK parses + saves
/// every incoming event on the single wasm thread as streams arrive, so
/// concurrent count fetches stack their parse bursts into one long
/// main-thread stall (observed: three simultaneous count_fetch spans, a
/// 3.4s event-loop slip, and a 5s render-probe reading contaminated by
/// ingest work). tokio::sync::Semaphore is executor-agnostic (no
/// timer/reactor), so it works on the wasm single-thread runtime too.
/// const_new avoids lazy init.
///
/// Permit count history: 2 (R5a) → 1 (3c-9). Even after 3c-8 capped the
/// per-fetch fan-out at 3 targeted relays, two independent count
/// pipelines — the idle-deferred pagination flush and the global
/// processor's micro batch — could still hold both permits and land
/// their parse bursts together (observed twice: 2450ms and 3762ms
/// stalls, each with exactly two open count_fetch spans). One permit
/// serializes them: the second waits on the gate instead of parsing
/// concurrently. Wall-clock cost during boot only; counts converge via
/// the live stream regardless of fetch order.
static COUNT_FETCH_GATE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

async fn fetch_events_for_filter(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: Duration,
    authors: &[nostr::key::PublicKey],
) -> Result<FetchEventsPage, String> {
    // R5a: serialize count fetches through the gate before the DB join —
    // the permit covers the whole fetch (DB + relay stream) so a queued
    // caller does not also parse events while another is in flight.
    let _fetch_permit = COUNT_FETCH_GATE
        .acquire()
        .await
        .expect("count-fetch semaphore is never closed");
    // F3: count backfills are background data — skip the 2s read-relay
    // quorum wait the user-facing fetch path pays.
    // 3c-8: the relay leg is targeted at the relays covering the anchor
    // authors (`fetch_events_from_author_coverage_relays_with_client`)
    // instead of broadcast to every connected relay — the EOSE-burst
    // multiplier was the last unbounded count-fetch dimension after the
    // page/total caps.
    let (db_result, relay_result) = tokio::join!(
        client.database().query(filter.clone()),
        crate::stores::nostr_client::fetch_events_from_author_coverage_relays_with_client(
            client, filter, timeout, authors
        )
    );

    let db_events: Vec<Event> = match db_result {
        Ok(events) => events.into_iter().collect(),
        Err(e) => {
            log::debug!("Database query for interactions failed: {}", e);
            Vec::new()
        }
    };

    let relay_events: Vec<Event> = match relay_result {
        Ok(events) => events,
        Err(e) => {
            if !db_events.is_empty() {
                log::warn!(
                    "Relay fetch failed but using {} cached events: {}",
                    db_events.len(),
                    e
                );
                Vec::new()
            } else {
                return Err(e);
            }
        }
    };
    let relay_page_len = relay_events.len();
    let relay_oldest_created_at = relay_events
        .iter()
        .map(|event| event.created_at.as_secs())
        .min();

    let mut event_map: HashMap<EventId, Event> = HashMap::new();
    for event in db_events {
        event_map.insert(event.id, event);
    }
    for event in relay_events {
        event_map.insert(event.id, event);
    }
    Ok(FetchEventsPage {
        events: event_map.into_values().collect(),
        relay_page_len,
        relay_oldest_created_at,
    })
}

#[allow(clippy::too_many_arguments)] // bounds triple + targeting input read best inline here
async fn fetch_paginated_interaction_events(
    client: &std::sync::Arc<Client>,
    event_ids: &[EventId],
    kinds: &[Kind],
    timeout: Duration,
    page_limit: usize,
    max_pages: usize,
    max_total: usize,
    authors: &[nostr::key::PublicKey],
) -> Result<Vec<Event>, String> {
    let mut all_events: HashMap<EventId, Event> = HashMap::new();
    let mut until = None;
    let mut previous_oldest_created_at = None;
    let mut pages = 0usize;

    loop {
        if pages >= max_pages || all_events.len() >= max_total {
            break;
        }
        pages += 1;
        let mut filter = Filter::new()
            .kinds(kinds.to_vec())
            .events(event_ids.to_vec())
            .limit(page_limit.min(MAX_RELAY_LIMIT));
        if let Some(until_ts) = until {
            filter = filter.until(until_ts);
        }

        let page = fetch_events_for_filter(client, filter, timeout, authors).await?;
        if page.events.is_empty() {
            break;
        }

        let previous_len = all_events.len();
        for event in page.events {
            #[cfg(feature = "native")]
            {
                if matches!(event.kind, Kind::TextNote | Kind::Comment | Kind::VoiceMessage | Kind::VoiceMessageReply) {
                    crate::stores::ndb::cache_event(&event);
                }
            }
            all_events.insert(event.id, event);
        }
        // Enforce max_total AFTER the page too: the loop-head check runs
        // before each fetch, so page_limit × max_pages could overshoot the
        // documented total (200×2=400 vs 300). Truncate deterministically
        // (keep newest) so the documented burst bound is the enforced one.
        if all_events.len() > max_total {
            enforce_max_total(&mut all_events, max_total);
            break;
        }

        let Some(oldest_created_at) = page.relay_oldest_created_at else {
            break;
        };
        if page.relay_page_len < page_limit
            || oldest_created_at == 0
            || previous_oldest_created_at == Some(oldest_created_at)
            || all_events.len() == previous_len
        {
            break;
        }

        previous_oldest_created_at = Some(oldest_created_at);
        until = Some(Timestamp::from(oldest_created_at.saturating_sub(1)));
    }

    Ok(all_events.into_values().collect())
}

/// Trim an accumulated interaction map to `max_total` events, keeping the
/// NEWEST by `created_at` (deterministic; dropped counts converge via the
/// live stream).
fn enforce_max_total(all_events: &mut HashMap<EventId, Event>, max_total: usize) {
    let mut by_recency: Vec<Event> = std::mem::take(all_events).into_values().collect();
    by_recency.sort_by_key(|e| std::cmp::Reverse(e.created_at));
    by_recency.truncate(max_total);
    *all_events = by_recency.into_iter().map(|e| (e.id, e)).collect();
}

async fn fetch_zap_receipt_counts(
    event_id: &EventId,
    timeout: Duration,
) -> Result<InteractionCounts, String> {
    let client = get_client().ok_or("Client not initialized")?;
    let (zap_page_limit, zap_max_pages, zap_max_total) = zap_backfill_limits();
    // 3c-8: zap receipts for a popular note reach the thousands — the
    // targeted fetch matters most here (2500-event cap × the fan-out
    // multiplier otherwise).
    let anchor_authors =
        resolve_anchor_authors(&client, std::slice::from_ref(event_id)).await;
    let events = fetch_paginated_interaction_events(
        &client,
        &[*event_id],
        &[Kind::ZapReceipt],
        timeout,
        zap_page_limit,
        zap_max_pages,
        zap_max_total,
        &anchor_authors,
    )
    .await?;
    let current_user_pk: Option<nostr::key::PublicKey> = SIGNER_INFO
        .read()
        .as_ref()
        .and_then(|info| nostr::key::PublicKey::from_hex(&info.public_key).ok());
    let mut counts = InteractionCounts::default();

    for event in events {
        counts.zaps += 1;
        if let Some(amount) = extract_zap_amount(&event) {
            counts.zap_amount_sats += amount;
        }
        if let Some(sender) = extract_zap_sender(&event) {
            if current_user_pk
                .map(|pk| sender == pk.to_hex())
                .unwrap_or(false)
            {
                counts.user_zapped = Some(true);
            }
        }
    }

    Ok(counts)
}

/// Resolve the authors of the anchor events with one database query so
/// the count backfill can target author-covering relays (3c-8). Feed
/// anchors are saved by the SDK on receipt, so hits are the norm;
/// misses simply drop out of the coverage ranking, and an empty result
/// falls back to the broadcast fetch inside
/// [`fetch_events_from_author_coverage_relays_with_client`].
async fn resolve_anchor_authors(
    client: &std::sync::Arc<Client>,
    ids: &[EventId],
) -> Vec<nostr::key::PublicKey> {
    if ids.is_empty() {
        return Vec::new();
    }
    let filter = Filter::new().ids(ids.to_vec());
    match client.database().query(filter).await {
        Ok(events) => {
            let mut seen = HashSet::new();
            let authors: Vec<nostr::key::PublicKey> = events
                .into_iter()
                .map(|event| event.pubkey)
                .filter(|pk| seen.insert(*pk))
                .collect();
            log::debug!(
                "Count backfill: resolved {} anchor authors from DB for {} anchors",
                authors.len(),
                ids.len()
            );
            authors
        }
        Err(e) => {
            log::warn!("Count backfill: anchor author lookup failed: {}", e);
            Vec::new()
        }
    }
}

/// Batch fetch interaction counts for multiple events
///
/// # Arguments
/// * `event_ids` - Vector of event IDs to fetch interactions for
/// * `timeout` - Query timeout duration
///
/// # Returns
/// HashMap mapping event_id (hex) to its interaction counts
///
/// # Example
/// ```
/// let event_ids = feed_events.iter().map(|e| e.id).collect();
/// let counts = fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await?;
///
/// // Pass to NoteCard
/// NoteCard { event, counts: counts.get(&event.id.to_hex()) }
/// ```
pub async fn fetch_interaction_counts_batch(
    event_ids: Vec<EventId>,
    timeout: Duration,
) -> Result<HashMap<String, InteractionCounts>, String> {
    // v2 boot-stall plan: bound EVERY caller of the shared entry point —
    // the sync_interaction_counts fallback and explicit UI callers (note
    // viewer, pagination, polls, AI tools) — with the shared burst caps.
    // (The global interaction processor uses the tighter
    // `fetch_interaction_counts_batch_micro`; the deferred pagination
    // flush uses `flush_paginated_interaction_counts`.)
    // Observed boot fetches (75-133 events) sit under the caps; this is the
    // safety net that keeps any path from regressing to unbounded.
    let (page_limit, max_pages, max_total) = interaction_backfill_limits();
    fetch_interaction_counts_batch_with_limits(
        event_ids,
        timeout,
        &ALL_COUNT_KINDS,
        page_limit,
        max_pages,
        max_total,
    )
    .await
}

/// Flush the idle-deferred pagination count queue (3c-2/3c-3).
///
/// Same pipeline as [`fetch_interaction_counts_batch`] but with the
/// tighter pagination burst bounds ([`interaction_pagination_limits`]):
/// this runs while the user is reading the feed, where a 300-event
/// parse+verify+save burst was measured as a 2-4s main-thread stall
/// mid-scroll. The idle-deferred queue (see `engagement.rs`) coalesces
/// consecutive pages into one flush, so `event_ids` may span several
/// pagination pages; counts the cap truncates converge via the live
/// interaction stream.
pub async fn flush_paginated_interaction_counts(
    event_ids: Vec<EventId>,
    timeout: Duration,
) -> Result<HashMap<String, InteractionCounts>, String> {
    let (page_limit, max_pages, max_total) = interaction_pagination_limits();
    log::info!(
        "Flushing coalesced pagination counts for {} events (page {}, pages {}, total {})",
        event_ids.len(),
        page_limit,
        max_pages,
        max_total
    );
    fetch_interaction_counts_batch_with_limits(
        event_ids,
        timeout,
        &ALL_COUNT_KINDS,
        page_limit,
        max_pages,
        max_total,
    )
    .await
}

/// Micro-path count backfill for the global interaction processor
/// (`use_global_interaction`): newly-mounted ids under the
/// `LARGE_INTERACTION_BATCH_SKIP` threshold, fetched with the tighter
/// pagination burst bounds instead of the shared 200x2x300 caps.
///
/// During boot, cards mount continuously and the processor drains small
/// batches on every 300ms tick — each shared-limits fetch could pull a
/// 300-event parse+verify burst while competing with flush rounds for the
/// `COUNT_FETCH_GATE` permits (observed: a 10-event batch firing beside the
/// first flush rounds). ≤30 anchors rarely need more than the 100-event
/// cap; whatever truncates converges via the live interaction stream.
pub async fn fetch_interaction_counts_batch_micro(
    event_ids: Vec<EventId>,
    timeout: Duration,
) -> Result<HashMap<String, InteractionCounts>, String> {
    let (page_limit, max_pages, max_total) = interaction_pagination_limits();
    fetch_interaction_counts_batch_with_limits(
        event_ids,
        timeout,
        &ALL_COUNT_KINDS,
        page_limit,
        max_pages,
        max_total,
    )
    .await
}

/// First-load interaction backfill with a burst bound (phase 2 stall
/// mitigation): pagination is capped so a hot feed can't pull hundreds of
/// parse+verify-heavy events onto the main thread during boot. Counts for
/// whatever the cap truncates converge via the live stream and later loads.
pub async fn fetch_interaction_counts_batch_first_load(
    event_ids: Vec<EventId>,
    timeout: Duration,
) -> Result<HashMap<String, InteractionCounts>, String> {
    // Option B escalation (see const docs): local DB counts + live stream
    // only. Returns empty so the caller keeps the already-painted local
    // counts instead of overwriting them.
    if !COUNTS_FIRST_LOAD_BACKFILL {
        return Ok(HashMap::new());
    }
    let (page_limit, max_pages, max_total) = interaction_backfill_limits();
    fetch_interaction_counts_batch_with_limits(
        event_ids,
        timeout,
        &FIRST_LOAD_COUNT_KINDS,
        page_limit,
        max_pages,
        max_total,
    )
    .await
}

async fn fetch_interaction_counts_batch_with_limits(
    event_ids: Vec<EventId>,
    timeout: Duration,
    kinds: &[Kind],
    page_limit: usize,
    max_pages: usize,
    max_total: usize,
) -> Result<HashMap<String, InteractionCounts>, String> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let (cached_counts, cache_hits, uncached_ids) = {
        let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Counts cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        let cached_counts = cache.get_batch(&event_ids);
        let cache_hits = cached_counts.len();
        let uncached_ids: Vec<EventId> = event_ids
            .iter()
            .filter(|id| !cached_counts.contains_key(&id.to_hex()))
            .cloned()
            .collect();
        (cached_counts, cache_hits, uncached_ids)
    };
    log::info!(
        "Batch fetching interaction counts for {} events ({} cache hits, {} cache misses)",
        event_ids.len(),
        cache_hits,
        uncached_ids.len()
    );
    if uncached_ids.is_empty() {
        log::info!("All counts served from cache!");
        return Ok(cached_counts);
    }
    let client = get_client().ok_or("Client not initialized")?;
    // Phase 1 attribution: this bulk backfill is a main-thread stall
    // suspect (every fetched event parses + verifies on the UI thread).
    let _span = crate::platform::perf::begin("count_fetch");
    // 3c-8: resolve the anchor authors once (single DB query by ids) so
    // every chunk's relay leg targets the relays covering these authors
    // instead of broadcasting to the whole connected pool.
    let anchor_authors = resolve_anchor_authors(&client, &uncached_ids).await;
    let mut event_map: HashMap<EventId, Event> = HashMap::new();
    let batch_futures: Vec<_> = uncached_ids
        .chunks(INTERACTION_EVENT_BATCH_SIZE)
        .map(|batch| {
            fetch_paginated_interaction_events(
                &client,
                batch,
                kinds,
                timeout,
                page_limit,
                max_pages,
                max_total,
                &anchor_authors,
            )
        })
        .collect();
    let batch_results = futures::future::join_all(batch_futures).await;
    for batch_events in batch_results {
        let batch_events = batch_events?;
        for event in batch_events {
            event_map.insert(event.id, event);
        }
    }
    let mut events: Vec<Event> = event_map.into_values().collect();
    events.sort_by_key(|right| std::cmp::Reverse(right.created_at));
    // Phase 1 evidence: per-kind mix of the backfill batch — quantifies what
    // the kinds-trim lever actually removes (TextNote/Comment carry the
    // heavy bodies; reactions/zaps are tiny).
    if !events.is_empty() {
        let mut kind_mix = std::collections::BTreeMap::new();
        for event in &events {
            *kind_mix.entry(event.kind.as_u16()).or_insert(0usize) += 1;
        }
        log::info!("Interaction backfill kind mix: {:?}", kind_mix);
    }
    log::info!(
        "Processing {} total interaction events (DB + relay, deduplicated)",
        events.len()
    );
    let edit_events: Vec<Event> = events
        .iter()
        .filter(|e| e.kind.as_u16() == 1010)
        .cloned()
        .collect();
    let mut freshly_fetched: HashMap<String, InteractionCounts> = HashMap::new();
    let requested_ids: HashSet<String> = uncached_ids.iter().map(|id| id.to_hex()).collect();
    for event_id in &uncached_ids {
        freshly_fetched.insert(event_id.to_hex(), InteractionCounts::default());
    }
    let current_user_pk: Option<nostr::key::PublicKey> = SIGNER_INFO
        .read()
        .as_ref()
        .and_then(|info| nostr::key::PublicKey::from_hex(&info.public_key).ok());
    let mut user_reactions_seen: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    // R5b: yield to the event loop every INGEST_YIELD_EVERY events so a
    // 300-event parse+count burst cannot hold the single wasm thread for
    // its full duration (the browser gets paint/task opportunities
    // between chunks). Secondary to R5a: the SDK-side parse+save of
    // arriving events happens inside the fetch stream and only the
    // semaphore cap can bound that share.
    const INGEST_YIELD_EVERY: usize = 50;
    for (processed, event) in events.into_iter().enumerate() {
        if processed > 0 && processed % INGEST_YIELD_EVERY == 0 {
            crate::platform::timer::sleep_ms(0).await;
        }
        let referenced_event_id = match extract_referenced_event(&event, &requested_ids) {
            Some(id) => id,
            None => continue,
        };
        let event_key = referenced_event_id.to_hex();
        let counts = freshly_fetched.entry(event_key.clone()).or_default();
        let is_current_user = current_user_pk
            .map(|pk| event.pubkey == pk)
            .unwrap_or(false);
        match event.kind {
            kind if is_reply_kind(kind) => counts.replies += 1,
            Kind::Reaction => {
                let content = event.content.trim();
                if content != "-" {
                    counts.likes += 1;
                }
                if is_current_user && !user_reactions_seen.contains(&event_key) {
                    user_reactions_seen.insert(event_key.clone());
                    if content == "-" {
                        counts.user_liked = Some(false);
                        counts.user_reaction = None;
                        counts.user_reaction_url = None;
                    } else {
                        counts.user_liked = Some(true);
                        counts.user_reaction = Some(content.to_string());
                        if content.starts_with(':') && content.ends_with(':') && content.len() > 2 {
                            let shortcode = &content[1..content.len() - 1];
                            let emoji_url = event.tags.iter().find_map(|tag| {
                                let tag_slice = tag.as_slice();
                                if tag_slice.len() >= 3
                                    && tag_slice.first().map(|s| s.as_str()) == Some("emoji")
                                    && tag_slice.get(1).map(|s| s.as_str()) == Some(shortcode)
                                {
                                    tag_slice.get(2).map(|s| s.to_string())
                                } else {
                                    None
                                }
                            });
                            counts.user_reaction_url = emoji_url;
                        } else {
                            counts.user_reaction_url = None;
                        }
                    }
                }
            }
            Kind::Repost => {
                counts.reposts += 1;
                if is_current_user {
                    counts.user_reposted = Some(true);
                    counts.user_repost_id = Some(event.id.to_hex());
                }
            }
            Kind::ZapReceipt => {
                counts.zaps += 1;
                if let Some(amount) = extract_zap_amount(&event) {
                    counts.zap_amount_sats += amount;
                }
                if let Some(sender) = extract_zap_sender(&event) {
                    if current_user_pk
                        .map(|pk| sender == pk.to_hex())
                        .unwrap_or(false)
                    {
                        counts.user_zapped = Some(true);
                    }
                }
            }
            _ => {}
        }
    }
    {
        if !edit_events.is_empty() {
            let mut event_map_for_edits: HashMap<String, Event> = HashMap::new();
            for id in &uncached_ids {
                if let Ok(Some(event)) = client.database().event_by_id(id).await {
                    event_map_for_edits.insert(id.to_hex(), event);
                }
            }
            crate::stores::edit_cache::apply_edits_to_event_map(&edit_events, &event_map_for_edits);
        }
    }
    {
        let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
            log::warn!("Counts cache mutex was poisoned, recovering");
            poisoned.into_inner()
        });
        cache.insert_batch(freshly_fetched.clone());
    }
    let mut final_counts = cached_counts;
    final_counts.extend(freshly_fetched);
    log::info!(
        "Returning {} interaction counts ({} from cache, {} freshly fetched)",
        final_counts.len(),
        cache_hits,
        uncached_ids.len()
    );
    Ok(final_counts)
}

/// Sync interaction counts using negentropy set reconciliation
///
/// This is more efficient than full fetch for subsequent refreshes:
/// - Uses negentropy to determine which events are missing locally
/// - Only fetches new events that appeared since last sync
/// - Incrementally updates cached counts without refetching everything
///
/// # When to use
/// - First load: Use `fetch_interaction_counts_batch` (no local data to reconcile)
/// - Subsequent refreshes: Use `sync_interaction_counts` (incremental updates)
///
/// # Fallback
/// If sync fails, silently falls back to full fetch behavior.
///
/// Retained for the `COUNTS_FIRST_LOAD_BACKFILL` re-enable path but
/// currently unwired: its only caller (the home feed's settle pass) now
/// enqueues into the idle-deferred count queue instead — a full
/// negentropy counts sync here measured a ~12s main-thread ingest stall
/// on wasm (every reconciled event parses + verifies inline).
#[allow(dead_code)]
pub async fn sync_interaction_counts(
    event_ids: Vec<EventId>,
    timeout: Duration,
) -> Result<HashMap<String, InteractionCounts>, String> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let client = get_client().ok_or("Client not initialized")?;
    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Comment,
            Kind::Reaction,
            Kind::Repost,
            Kind::ZapReceipt,
            Kind::Custom(1010),
        ])
        .events(event_ids.clone());
    let sync_opts = SyncOptions::default()
        .direction(SyncDirection::Down)
        .initial_timeout(timeout);
    // Phase 1 attribution: refresh-path negentropy reconcile over the pool.
    let _span = crate::platform::perf::begin("counts_sync");
    let sync_result = client.sync(filter.clone()).opts(sync_opts).await;
    match sync_result {
        Ok(output) => {
            let reconciliation = output.value;
            let new_event_count = reconciliation.received.len();
            if new_event_count == 0 {
                log::info!("Negentropy sync: no new interaction events found");
                let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
                    log::warn!("Counts cache mutex was poisoned, recovering");
                    poisoned.into_inner()
                });
                return Ok(cache.get_batch(&event_ids));
            }
            log::info!(
                "Negentropy sync: {} new interaction events to process",
                new_event_count
            );
            let mut new_events = Vec::new();
            for event_id in reconciliation.received.keys() {
                if let Ok(Some(event)) = client.database().event_by_id(event_id).await {
                    new_events.push(event);
                }
            }
            new_events.sort_by_key(|right| std::cmp::Reverse(right.created_at));
            let sync_edit_events: Vec<Event> = new_events
                .iter()
                .filter(|e| e.kind.as_u16() == 1010)
                .cloned()
                .collect();
            let mut result = {
                let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
                    log::warn!("Counts cache mutex was poisoned, recovering");
                    poisoned.into_inner()
                });
                cache.get_batch(&event_ids)
            };
            for event_id in &event_ids {
                let hex = event_id.to_hex();
                result.entry(hex).or_insert_with(InteractionCounts::default);
            }
            let current_user_pk: Option<nostr::key::PublicKey> = SIGNER_INFO
                .read()
                .as_ref()
                .and_then(|info| nostr::key::PublicKey::from_hex(&info.public_key).ok());
            let requested_ids: std::collections::HashSet<String> =
                event_ids.iter().map(|id| id.to_hex()).collect();
            let mut user_reactions_seen: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for event in new_events {
                let referenced_event_id = match extract_referenced_event(&event, &requested_ids) {
                    Some(id) => id,
                    None => continue,
                };
                let event_key = referenced_event_id.to_hex();
                let counts = result.entry(event_key.clone()).or_default();
                let is_current_user = current_user_pk
                    .map(|pk| event.pubkey == pk)
                    .unwrap_or(false);
                match event.kind {
                    kind if is_reply_kind(kind) => counts.replies += 1,
                    Kind::Reaction => {
                        let content = event.content.trim();
                        if content != "-" {
                            counts.likes += 1;
                        }
                        if is_current_user && !user_reactions_seen.contains(&event_key) {
                            user_reactions_seen.insert(event_key.clone());
                            if content == "-" {
                                counts.user_liked = Some(false);
                                counts.user_reaction = None;
                                counts.user_reaction_url = None;
                            } else {
                                counts.user_liked = Some(true);
                                counts.user_reaction = Some(content.to_string());
                                if content.starts_with(':')
                                    && content.ends_with(':')
                                    && content.len() > 2
                                {
                                    let shortcode = &content[1..content.len() - 1];
                                    let emoji_url = event.tags.iter().find_map(|tag| {
                                        let tag_slice = tag.as_slice();
                                        if tag_slice.len() >= 3
                                            && tag_slice.first().map(|s| s.as_str())
                                                == Some("emoji")
                                            && tag_slice.get(1).map(|s| s.as_str())
                                                == Some(shortcode)
                                        {
                                            tag_slice.get(2).map(|s| s.to_string())
                                        } else {
                                            None
                                        }
                                    });
                                    counts.user_reaction_url = emoji_url;
                                } else {
                                    counts.user_reaction_url = None;
                                }
                            }
                        }
                    }
                    Kind::Repost => {
                        counts.reposts += 1;
                        if is_current_user {
                            counts.user_reposted = Some(true);
                            counts.user_repost_id = Some(event.id.to_hex());
                        }
                    }
                    Kind::ZapReceipt => {
                        counts.zaps += 1;
                        if let Some(amount) = extract_zap_amount(&event) {
                            counts.zap_amount_sats += amount;
                        }
                        if let Some(sender) = extract_zap_sender(&event) {
                            if current_user_pk
                                .map(|pk| sender == pk.to_hex())
                                .unwrap_or(false)
                            {
                                counts.user_zapped = Some(true);
                            }
                        }
                    }
                    _ => {}
                }
            }
            {
                if !sync_edit_events.is_empty() {
                    let mut original_author_map: HashMap<nostr::event::EventId, nostr::key::PublicKey> = HashMap::new();
                    for event_id in &event_ids {
                        if let Ok(Some(orig_event)) = client.database().event_by_id(event_id).await {
                            original_author_map.insert(*event_id, orig_event.pubkey);
                        }
                    }
                    crate::stores::edit_cache::process_edit_events_batch(&sync_edit_events, &original_author_map);
                }
            }
            {
                let mut cache = get_counts_cache().lock().unwrap_or_else(|poisoned| {
                    log::warn!("Counts cache mutex was poisoned, recovering");
                    poisoned.into_inner()
                });
                cache.insert_batch(result.clone());
            }
            log::info!(
                "Negentropy sync complete: updated {} interaction counts",
                result.len()
            );
            Ok(result)
        }
        Err(e) => {
            log::debug!("Negentropy sync failed, falling back to full fetch: {}", e);
            fetch_interaction_counts_batch(event_ids, timeout).await
        }
    }
}

/// Extract the event ID being referenced by an interaction event
/// Only returns the event ID if it matches one of the requested IDs
/// If requested_ids is empty, returns the first 'e' tag found (for trending/all events)
pub(super) fn extract_referenced_event(
    event: &Event,
    requested_ids: &std::collections::HashSet<String>,
) -> Option<EventId> {
    for tag in event.tags.iter() {
        if let Ok(Nip10Tag::Event { id, .. }) = Nip10Tag::try_from(tag) {
            if requested_ids.is_empty() {
                return Some(id);
            }
            if requested_ids.contains(&id.to_hex()) {
                return Some(id);
            }
        }
    }
    None
}

/// Extract the zap sender's pubkey from a zap receipt event (kind 9735)
///
/// Checks uppercase `P` tag first (standard), then falls back to
/// parsing the `description` tag JSON for the `pubkey` field.
pub(super) fn extract_zap_sender(event: &Event) -> Option<String> {
    // Try uppercase P tag first (zap sender)
    if let Some(sender) = event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.len() >= 2 && slice.first()?.as_str() == "P" {
            Some(slice.get(1)?.as_str().to_string())
        } else {
            None
        }
    }) {
        return Some(sender);
    }
    // Fall back to description tag JSON pubkey
    event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first()?.as_str() == "description" {
            let zap_request_json = slice.get(1)?.as_str();
            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(zap_request_json) {
                return zap_request
                    .get("pubkey")
                    .and_then(|p| p.as_str())
                    .map(|s| s.to_string());
            }
        }
        None
    })
}

/// Extract zap amount in satoshis from a zap event (kind 9735)
pub fn extract_zap_amount(event: &Event) -> Option<u64> {
    if let Some(bolt11_tag) = event.tags.iter().find(|tag| {
        tag.as_slice()
            .first()
            .map(|k| k.as_str() == "bolt11")
            .unwrap_or(false)
    }) {
        if let Some(bolt11) = bolt11_tag.as_slice().get(1) {
            if let Some(amount) = parse_bolt11_amount(bolt11.as_str()) {
                return Some(amount);
            }
        }
    }
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        tag.as_slice()
            .first()
            .map(|k| k.as_str() == "description")
            .unwrap_or(false)
    }) {
        if let Some(desc) = description_tag.as_slice().get(1) {
            return parse_amount_from_description(desc.as_str());
        }
    }
    None
}

/// Parse amount from zap request description
pub(super) fn parse_amount_from_description(description: &str) -> Option<u64> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(description) {
        if let Some(tags) = json.get("tags").and_then(|t| t.as_array()) {
            for tag in tags {
                if let Some(tag_vals) = tag.as_array() {
                    if tag_vals.first().and_then(|v| v.as_str()) == Some("amount") {
                        if let Some(amount_str) = tag_vals.get(1).and_then(|v| v.as_str()) {
                            if let Ok(millisats) = amount_str.parse::<u64>() {
                                return Some(millisats / 1000);
                            }
                        }
                    }
                }
            }
        }
        if let Some(amount) = json.get("amount") {
            if let Some(amount_str) = amount.as_str() {
                if let Ok(millisats) = amount_str.parse::<u64>() {
                    return Some(millisats / 1000);
                }
            } else if let Some(amount_num) = amount.as_u64() {
                return Some(amount_num / 1000);
            }
        }
    }
    None
}

/// Fetch interaction counts for a time range (useful for trending/popular feeds)
///
/// Get interaction counts from the local database only (zero relay round-trips).
///
/// This provides instant counts by querying the nostr-sdk's local database.
/// Useful as a first-pass fast path before the full relay fetch resolves user-state.
///
/// The counts may be incomplete if the local DB hasn't seen all interactions.
/// Callers should overwrite with full relay-fetched counts when they arrive.
pub async fn fetch_local_db_counts(
    event_ids: &[EventId],
) -> HashMap<String, InteractionCounts> {
    let client = match get_client() {
        Some(c) => c,
        None => return HashMap::new(),
    };
    let db = client.database();
    let mut result = HashMap::new();
    for event_id in event_ids {
        let hex = event_id.to_hex();
        let mut counts = InteractionCounts::default();
        let filter_base = Filter::new().event(*event_id);
        let reply_filter = filter_base
            .clone()
            .kinds(vec![Kind::TextNote, Kind::Comment]);
        if let Ok(count) = db.count(reply_filter).await {
            counts.replies = count;
        }
        let reaction_filter = filter_base.clone().kind(Kind::Reaction);
        if let Ok(count) = db.count(reaction_filter).await {
            counts.likes = count;
        }
        let repost_filter = filter_base.clone().kind(Kind::Repost);
        if let Ok(count) = db.count(repost_filter).await {
            counts.reposts = count;
        }
        let zap_filter = filter_base.kind(Kind::ZapReceipt);
        if let Ok(count) = db.count(zap_filter).await {
            counts.zaps = count;
        }
        result.insert(hex, counts);
    }
    result
}

/// This fetches all interactions in a given time period and groups by event.
/// Useful for "trending" or "popular" feeds that want to rank by recent engagement.
#[allow(dead_code)]
pub async fn fetch_trending_interactions(
    since: Timestamp,
    limit: usize,
) -> Result<HashMap<String, InteractionCounts>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    log::info!("Fetching trending interactions since {}", since);
    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Comment,
            Kind::Reaction,
            Kind::Repost,
            Kind::ZapReceipt,
            Kind::Custom(1010),
        ])
        .since(since)
        .limit(limit);
    let events = crate::stores::nostr_client::fetch_with_client(
        &client,
        crate::stores::nostr_client::FetchTarget::Auto(vec![filter]),
        crate::stores::nostr_client::FetchProfile::Std,
    )
    .await
    .map_err(|e| format!("Failed to fetch trending interactions: {}", e))?;
    let mut counts_map: HashMap<String, InteractionCounts> = HashMap::new();
    let empty_filter = std::collections::HashSet::new();
    for event in events {
        let referenced_event_id = match extract_referenced_event(&event, &empty_filter) {
            Some(id) => id,
            None => continue,
        };
        let event_key = referenced_event_id.to_hex();
        let counts = counts_map.entry(event_key).or_default();
        match event.kind {
            kind if is_reply_kind(kind) => counts.replies += 1,
            Kind::Reaction if event.content.trim() != "-" => {
                counts.likes += 1;
            }
            Kind::Repost => counts.reposts += 1,
            Kind::ZapReceipt => {
                counts.zaps += 1;
                if let Some(amount) = extract_zap_amount(&event) {
                    counts.zap_amount_sats += amount;
                }
            }
            _ => {}
        }
    }
    Ok(counts_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::{EventBuilder, FinalizeEvent};
    use nostr::key::Keys;

    /// The shared burst bounds must stay bounded: a single page must not be
    /// able to deliver an unbounded parse+verify burst (200 events max),
    /// and the total must not exceed pages × page size (no silent
    /// multiplier). If a use case legitimately needs more, raise the consts
    /// deliberately — never remove the relationship.
    #[test]
    fn interaction_backfill_bounds_are_bounded() {
        let (page_limit, max_pages, max_total) = interaction_backfill_limits();
        assert_eq!(page_limit, 200, "per-page burst bound (200/page)");
        assert_eq!(max_pages, 2, "page count bound");
        assert_eq!(max_total, 300, "total burst bound");
        assert!(
            max_total <= page_limit * max_pages,
            "total must not exceed pages × page size"
        );
        assert!(page_limit > 0 && max_pages > 0 && max_total > 0);
    }

    /// The zap bounds keep the same invariant: bounded pages, bounded
    /// total, total ≤ pages × page size.
    #[test]
    fn zap_backfill_bounds_are_bounded() {
        let (page_limit, max_pages, max_total) = zap_backfill_limits();
        assert!(page_limit > 0 && max_pages > 0 && max_total > 0);
        assert!(
            max_total <= page_limit * max_pages,
            "zap total must not exceed pages × page size"
        );
    }

    /// `enforce_max_total` trims to the cap and keeps the NEWEST events
    /// (covers the loop's post-page truncation, which the consts-only test
    /// above cannot).
    #[test]
    fn enforce_max_total_keeps_newest() {
        let mut map: HashMap<EventId, Event> = HashMap::new();
        for i in 0..5u64 {
            let event = EventBuilder::new(Kind::Reaction, i.to_string().as_str())
                .custom_created_at(Timestamp::from(1_000 + i))
                .finalize(&Keys::generate())
                .unwrap();
            map.insert(event.id, event);
        }
        enforce_max_total(&mut map, 2);
        assert_eq!(map.len(), 2, "must trim to the cap");
        assert!(
            map.values().all(|e| e.created_at.as_secs() >= 1_003),
            "must keep the newest events, got created_at {:?}",
            map.values().map(|e| e.created_at.as_secs()).collect::<Vec<_>>()
        );
    }
}
