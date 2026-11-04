pub mod types;
mod feed_loaders;
pub mod login;
mod engagement;
mod feed_list;

use crate::components::{ClientInitializing, NoteCardSkeleton, NoteComposer};
use crate::components::note_composer::NoteMode;
use crate::error::NostrBlueError;
use crate::feeds::classify::classify_event;
use crate::hooks::{use_infinite_scroll_with_generation, use_stale_guard, use_user_lists};
use crate::routes::home::feed_list::FeedListBody;
use crate::services::aggregation::{InteractionCounts, InteractionStreamHandle};
use crate::stores::feed_cache::{self, FeedCacheKey};
use crate::stores::relay;
use crate::stores::{auth_store, nostr_client, subscription_manager};
use crate::stores::ui::scroll_restore;
use crate::utils::list_kinds::NAMED_RELAYS;
use crate::utils::debounced_collector::DebouncedCollector;
use crate::utils::format::safe_slice;
use crate::utils::pagination::{BackwardPageCursor, RelayCursorSet};
use crate::utils::{get_item_count, DataState, FeedItem};
use dioxus::prelude::*;
#[cfg(feature = "web")]
use gloo_events::EventListener;
use engagement::fetch_and_stream_interactions;
use feed_loaders::{
    feed_kinds, merge_paginated_feed_items, prefetch_author_metadata,
    prefetch_author_metadata_with_relays,
    load_following_feed, load_following_feed_per_relay, load_following_feed_streaming,
    load_following_with_replies, load_following_with_replies_per_relay, load_global_feed,
    load_paginated_global_feed, load_people_list_feed, load_people_list_feed_per_relay,
    load_relay_feed, sync_following_feed_page, FeedPage, PerRelayPage, FEED_LIMIT,
};
use login::LoginSection;
use nostr_sdk::prelude::{EventId, Filter, Kind, PublicKey, Timestamp};

use futures::StreamExt;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use types::{login_method_requires_signer, FeedType};

/// Clamp for live-subscription `since` values (global + follows paths): a
/// stale cached feed or the EOSE tracker's minimum can be hours old, and an
/// unbounded replay of that backlog would flood the pending-posts pill (the
/// follows REQs carry no `limit`). Older gaps are filled by
/// refresh/pagination instead.
const GLOBAL_LIVE_SINCE_CLAMP_SECS: u64 = 600;

/// Realtime `since` fallback for the "feed not Loaded yet" case (plan wave
/// B3): previously `Timestamp::now()`, which silently skipped everything
/// published before the moment the realtime effect beat the initial load.
/// The EOSE tracker's minimum observed `since` (what the SDK already
/// ingested) is the correct floor; `now()` only when nothing was observed.
fn realtime_since_floor() -> Timestamp {
    // Clamp like the global path (constant above): the tracker's minimum
    // spans EVERY relay ever EOSE'd, so one relay that connected hours ago
    // and went quiet would otherwise pin the floor that far back.
    let now_secs = Timestamp::now().as_secs();
    let floor = crate::stores::eose_tracker::EoseTracker::get_min_since()
        .map(|secs| secs.max(now_secs.saturating_sub(GLOBAL_LIVE_SINCE_CLAMP_SECS)))
        .unwrap_or(now_secs);
    Timestamp::from(floor)
}

// --- Page-wake staleness epochs -----------------------------------------
//
// A Home page left open stops receiving realtime events once the browser
// kills its idle WebSockets — and on web (WASM) the SDK cannot ping to
// notice (`PingTracker` is a stub on wasm32), so a socket dropped without
// a close frame (OS sleep, NAT timeout) looks `Connected` forever. The
// feed pipeline is epoch-driven and nothing re-runs on an already-loaded
// page, so returning to a stale Home never re-checked for new posts on
// its own (the manual refresh button was the only recovery).
//
// The wake trigger (installed in `Home`, web-gated) turns "user came back
// and the feed top is stale" into a new unattended epoch: the existing
// cache-top partition buffers everything newer into the pending-posts
// banner (no viewport jump), the pool is revived
// (`relay::connection::revive_stale_pool`), and the realtime session
// re-keys with `since` = the rendered feed top so the missed backlog
// replays into the banner. Native builds skip the listeners (real WS
// pings make zombie sockets unlikely there) but keep the epoch plumbing.
//
// Three signals feed the trigger: visibilitychange (tab switch), window
// focus, and a freeze-detection poll (timer drift: a 30s sleep that took
// ≥5min of wall clock proves the tab was suspended). The poll never gates
// on feed age — a quiet feed is indistinguishable from a dead one, and
// age-gating re-epoch'd every rate-limit window forever on quiet feeds.

/// Rendered-feed staleness required before a page wake may re-epoch:
/// a fresher feed means realtime is alive and an epoch would be pure churn.
#[cfg_attr(not(feature = "web"), allow(dead_code))]
const WAKE_FEED_STALE_SECS: u64 = 180;
/// Minimum spacing between wake epochs. Absorbs quick tab switching so a
/// stale-but-quiet feed (no new posts to find) doesn't re-epoch on every
/// focus flick — the grace-window idea from Amethyst's
/// `LifecycleAwareKeyDataSourceSubscription`.
#[cfg_attr(not(feature = "web"), allow(dead_code))]
const WAKE_MIN_INTERVAL_SECS: u64 = 60;
/// Safety-net poll interval for the freeze-detection wake check: covers
/// OS-sleep-with-visible-tab, where `visibilitychange` never fires. The
/// poll itself never gates on feed age — a quiet feed is indistinguishable
/// from a dead one by age, and age-gating re-epoch'd every rate-limit
/// window forever on quiet feeds (observed: a full epoch every ~60s).
#[cfg_attr(not(feature = "web"), allow(dead_code))]
const WAKE_POLL_INTERVAL_MS: u32 = 30_000;

/// Wall-clock elapsed across one poll sleep that proves the tab was
/// frozen/suspended (OS sleep with the tab visible): JS timers don't fire
/// while frozen, so a sleep asked for [`WAKE_POLL_INTERVAL_MS`] that took
/// this long means the tab was suspended. Normal visible-tab ticks land at
/// ~30s and hidden-tab timer throttling (~1 fire/min) lands at ~60-90s —
/// both far below this. One benign edge: a >5min forward wall-clock jump
/// (NTP/manual clock change) triggers a single rate-limited wake.
#[cfg_attr(not(feature = "web"), allow(dead_code))]
const WAKE_FREEZE_MIN_ELAPSED_MS: u64 = 300_000;

/// Pure decision for the freeze-detection poll: did this tick's
/// wall-clock elapsed prove the tab was frozen?
#[cfg_attr(not(feature = "web"), allow(dead_code))]
fn poll_drift_indicates_freeze(elapsed_ms: u64) -> bool {
    elapsed_ms >= WAKE_FREEZE_MIN_ELAPSED_MS
}

/// Pure decision for the page-wake path: should the just-returned-to page
/// run a staleness epoch? Requires the document to be visible, a rendered
/// feed top staler than [`WAKE_FEED_STALE_SECS`], and at least
/// `min_interval_secs` (see [`wake_min_interval`]; the base value is
/// [`WAKE_MIN_INTERVAL_SECS`]) since the last accepted wake.
#[cfg_attr(not(feature = "web"), allow(dead_code))]
fn should_wake_reload(
    is_hidden_now: bool,
    feed_top_age_secs: Option<u64>,
    now: u64,
    last_wake_bumped: u64,
    min_interval_secs: u64,
) -> bool {
    if is_hidden_now {
        return false;
    }
    // Nothing rendered (initial load / error state): an epoch either
    // already runs or a fresh mount owns the load.
    let Some(age) = feed_top_age_secs else {
        return false;
    };
    if age < WAKE_FEED_STALE_SECS {
        return false;
    }
    if last_wake_bumped > 0 && now.saturating_sub(last_wake_bumped) < min_interval_secs {
        return false;
    }
    true
}

/// Minimum spacing between wake epochs after `streak` consecutive
/// unproductive ones (epochs after which the feed top never moved). A
/// wake epoch can only refresh the feed top by finding new content — on a
/// quiet follow graph the top stays staler than
/// [`WAKE_FEED_STALE_SECS`] forever, so at the base interval every tab
/// return pays a full epoch (unsubscribe_all + pool revive + refetch +
/// realtime rebuild) for nothing, forever. Escalating the interval on
/// unproductive streaks keeps the churn bounded while still checking a
/// possibly-dead pool eventually; any productive epoch (feed top moved —
/// new content landed) resets the ladder. Mirrors the grace-window idea
/// from Amethyst's `LifecycleAwareKeyDataSourceSubscription`, extended
/// over time because a WASM page cannot trust its sockets.
#[cfg_attr(not(feature = "web"), allow(dead_code))]
fn wake_min_interval(unproductive_streak: u32) -> u64 {
    match unproductive_streak {
        0 | 1 => WAKE_MIN_INTERVAL_SECS,
        2 | 3 => 5 * 60,
        _ => 15 * 60,
    }
}

/// Timestamp of the rendered feed's newest item (`None` when nothing
/// rendered) — the productivity marker for wake epochs: a feed whose top
/// did not move since the previous wake means that epoch found nothing.
#[cfg_attr(not(feature = "web"), allow(dead_code))]
fn feed_top_timestamp(feed_state: Signal<DataState<Rc<Vec<FeedItem>>>>) -> Option<u64> {
    match &*feed_state.peek() {
        DataState::Loaded(items) => items.first().map(|i| i.sort_timestamp().as_secs()),
        _ => None,
    }
}

/// Age of the rendered feed's newest item (`None` when nothing rendered).
#[cfg_attr(not(feature = "web"), allow(dead_code))]
fn feed_top_age_secs(
    feed_state: Signal<DataState<Rc<Vec<FeedItem>>>>,
    now: u64,
) -> Option<u64> {
    match &*feed_state.peek() {
        DataState::Loaded(items) => items
            .first()
            .map(|i| now.saturating_sub(i.sort_timestamp().as_secs())),
        _ => None,
    }
}

/// `document.hidden()` — `false` when unknown (act, don't stall the wake).
#[cfg(feature = "web")]
fn document_is_hidden() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .map(|d| d.hidden())
        .unwrap_or(false)
}

/// Shared wake gate for the JS listeners and the safety-net poll: bump
/// `wake_trigger` (a main-load-effect epoch source) when the return-to-page
/// conditions hold. Signals are written from the JS event context — same
/// untracked-write pattern as the settings relay-refresh listeners.
#[cfg(feature = "web")]
#[allow(clippy::too_many_arguments)]
fn attempt_wake_reload(
    source: &str,
    feed_state: Signal<DataState<Rc<Vec<FeedItem>>>>,
    mut wake_trigger: Signal<u32>,
    mut wake_last_bumped: Signal<u64>,
    mut wake_streak: Signal<u32>,
    mut wake_top_at_bump: Signal<Option<u64>>,
) {
    let now = Timestamp::now().as_secs();
    let age = feed_top_age_secs(feed_state, now);
    if !should_wake_reload(
        document_is_hidden(),
        age,
        now,
        *wake_last_bumped.peek(),
        wake_min_interval(*wake_streak.peek()),
    ) {
        return;
    }
    // Productivity accounting vs the previous accepted wake: if the feed
    // top has not moved since it, the previous epoch found nothing (and
    // nothing arrived via realtime either) — escalate the streak so the
    // next wake backs off. Any top movement (new content) resets the
    // ladder. Timestamps (not event ids) keep this cheap; two items
    // landing in the same second merely cost one extra unproductive step.
    let top_now = feed_top_timestamp(feed_state);
    let next_streak = match (top_now, *wake_top_at_bump.peek()) {
        (Some(top), Some(prev)) if top == prev => (*wake_streak.peek()).saturating_add(1),
        _ => 0,
    };
    *wake_streak.write() = next_streak;
    *wake_top_at_bump.write() = top_now;
    *wake_last_bumped.write() = now;
    let next = wake_trigger.peek().wrapping_add(1);
    wake_trigger.set(next);
    log::info!(
        "Page wake ({source}): feed top age {:?}s exceeds {}s — running staleness epoch (unproductive streak {next_streak}, next min interval {}s)",
        age,
        WAKE_FEED_STALE_SECS,
        wake_min_interval(next_streak)
    );
}

/// Set the feed to `Loaded` and measure the render turn it triggers.
///
/// dioxus-web 0.7.9 renders synchronously in the task woken by the signal
/// write (js-sys queues woken tasks via `queueMicrotask`, which runs before
/// any `setTimeout` macrotask) — so a `sleep_ms(0)` probe scheduled right
/// after the write fires only after that render turn completes. The measured
/// lag is the render cost (plus anything else queued ahead of the timer).
/// Phase 1 instrumentation: thresholds kept loud-but-rare (>300ms).
/// Paint-large-batches-progressively: cards to render in the FIRST turn of
/// a feed epoch. Full-feed first turns measured 1010-1212ms (phase 1); a
/// prefix keeps the first paint interactive and the remainder lands on the
/// next task turn.
const PROGRESSIVE_FIRST_PAINT_CARDS: usize = 15;
/// Only split batches at least this large — smaller ones fit one frame.
const PROGRESSIVE_MIN_BATCH: usize = 30;
/// Trailing-debounce window for the idle-deferred pagination count flush
/// (3c-2). Pages arriving within the window coalesce into one backfill;
/// the flush fires once no new page has landed for this long.
const COUNT_FLUSH_DEBOUNCE_MS: u32 = 1_500;
/// Maximum anchors per flush round (3c-4, halved in 3c-7). A long fling
/// queues ~58 anchors per page; flushing everything at pause would
/// re-create one giant burst. Rounds are bounded to 50 anchors ≈ one
/// pagination-capped fetch of ≤100 events (page_limit applies PER RELAY in
/// the targeted fast fetch, so the raw SDK parse burst per page-batch is
/// the quantity being bounded) and chain: leftovers re-arm the debounce
/// for the next round 1.5s later.
const COUNT_FLUSH_MAX_ANCHORS: usize = 50;
/// Failure backoff ceiling for the idle-deferred count flush (R2
/// review): while the fetch keeps failing (offline, all targets
/// refusing), an immediate re-arm loops one 5s-timeout attempt every
/// ~6.5s, each acquiring `COUNT_FETCH_GATE` and serializing every other
/// count path behind it. Consecutive failures double the re-arm delay
/// (1.5s → 3s → 6s → …) up to this ceiling; a successful round or a new
/// enqueue (nonce bump → fresh debounce) resets the streak.
const COUNT_FLUSH_MAX_BACKOFF_MS: u32 = 30_000;
/// Single-flight gate for the flush rounds (3c-6). The nonce supersede only
/// guards the debounce window — once a spawn passes it, nothing prevented it
/// running beside an earlier round: one round holds a `COUNT_FETCH_GATE`
/// permit for up to ~10s (2 sequential pages × 5s timeout), so TWO
/// overlapping rounds saturate that gate and park every other count path
/// (global processor micro-batches, note viewer, polls) behind them
/// (observed: four open count_fetch spans, ages 15.6s/13.2s/8.4s/1.2s).
/// `try_acquire` before reading the queue makes take-and-drain atomic
/// w.r.t. other rounds; a busy spawn returns WITHOUT re-arming — the
/// in-flight round's completion check (below) re-arms for everything
/// enqueued meanwhile, so no anchors are dropped. The permit releases on
/// Drop: round completion, component unmount (dioxus drops scope-owned
/// futures), or panic-unwind — there is no stuck-gate path. Mirrors
/// `COUNT_FETCH_GATE` (R5a); Amethyst gates background parse work the same
/// way (`GiftWrapPrefetch`'s `Semaphore(MAX_IN_FLIGHT)`).
static COUNT_FLUSH_GATE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

/// FIFO enqueue with dedup for the idle-deferred count queue (R2
/// review: a `HashSet`'s iteration order is arbitrary, so the ≤N
/// anchors taken per idle round were a random subset — after a long
/// fling the first round could fetch counts for cards far off-screen
/// while visible cards waited additional chained rounds. A deduped
/// `Vec` keeps insertion order: oldest-enqueued anchors flush first).
fn enqueue_count_anchors(
    queue: &mut Signal<Vec<EventId>>,
    ids: impl IntoIterator<Item = EventId>,
) {
    let mut q = queue.write();
    for id in ids {
        if !q.contains(&id) {
            q.push(id);
        }
    }
}

fn set_loaded_with_probe(
    feed_state: &mut Signal<DataState<Rc<Vec<FeedItem>>>>,
    feed_reset_generation: Signal<u64>,
    items: Vec<FeedItem>,
) {
    // Progressive first paint: only when transitioning INTO Loaded (refresh
    // epochs keep the previous list Loaded and skip this) with a batch big
    // enough to blow the frame budget in a single render turn.
    let already_loaded = matches!(&*feed_state.peek(), DataState::Loaded(_));
    if !already_loaded && items.len() >= PROGRESSIVE_MIN_BATCH {
        let count = items.len();
        let mut head = items;
        let tail = head.split_off(PROGRESSIVE_FIRST_PAINT_CARDS);
        {
            let _span = crate::platform::perf::begin("feed_flush");
            feed_state.set(DataState::Loaded(Rc::new(head)));
        }
        // Epoch captured before the head write: if the feed-load effect
        // resets the feed during the paint split (feed switch / refresh
        // writes `Loading` and bumps the generation), the old epoch's tail
        // must not surface as the new feed's content — neither directly nor
        // merged into the new list.
        let generation_at_write = *feed_reset_generation.read();
        let generation_signal = feed_reset_generation;
        let mut fs = *feed_state;
        spawn(async move {
            // One task turn between the two writes: the browser gets a
            // paint opportunity between the render turns.
            crate::platform::timer::sleep_ms(0).await;
            if *generation_signal.read() != generation_at_write {
                log::debug!("Progressive paint tail skipped: feed epoch changed");
                return;
            }
            // Merge the tail into whatever arrived meanwhile (the collector
            // may have flushed newer items between the turns) instead of
            // clobbering them with a stale list.
            let merged = match &*fs.peek() {
                DataState::Loaded(current) => {
                    feed_cache::merge_feed_items(current.to_vec(), tail)
                }
                _ => tail,
            };
            {
                let _span = crate::platform::perf::begin("feed_flush");
                fs.set(DataState::Loaded(Rc::new(merged)));
            }
        });
        probe_render_turn(count);
        return;
    }
    let count = items.len();
    {
        let _span = crate::platform::perf::begin("feed_flush");
        feed_state.set(DataState::Loaded(Rc::new(items)));
    }
    probe_render_turn(count);
}

/// Measure the render turn triggered by the signal write that just ran:
/// dioxus renders synchronously in the microtask woken by the write, which
/// always executes before this setTimeout(0) macrotask — so the delay we
/// measure here includes that render turn (plus anything queued ahead).
/// `cards` records the loaded-card count so the log exposes ms/card (v3).
///
/// M1: the reading can be CONTAMINATED by concurrent ingest work — the
/// 5062ms@51-cards reading from the 2026-10-29 smoke overlapped three
/// open count_fetch spans (SDK parse+save runs on this same thread). The
/// log now attributes: any perf spans still open when the turn completes
/// were running during (at least part of) the window, and the last
/// completed span shows what finished just before it. Readings with
/// count_fetch attribution are ingest stalls, not render cost.
fn probe_render_turn(cards: usize) {
    let started = instant::Instant::now();
    spawn(async move {
        crate::platform::timer::sleep_ms(0).await;
        let lag = started.elapsed();
        if lag.as_millis() > 300 {
            let (open, prev) = crate::platform::perf::snapshot();
            let concurrent = if open.is_empty() {
                String::new()
            } else {
                let spans: Vec<String> = open
                    .iter()
                    .map(|(op, age)| format!("{op}:{:.1}s", age.as_secs_f64()))
                    .collect();
                format!(" | concurrent: {}", spans.join(", "))
            };
            let last = match prev {
                Some((op, dur)) => format!(" | last: {op}:{:.0}ms", dur.as_secs_f64() * 1000.0),
                None => String::new(),
            };
            log::warn!(
                "FEED RENDER TURN: {}ms ({} cards){}{last}",
                lag.as_millis(),
                cards,
                concurrent
            );
        }
    });
}

/// Delay before the one-shot **settle pass** re-runs the feed fetch.
///
/// The initial load races pool warm-up: the realtime effect's batched
/// `client.subscribe()` gossip breakdowns are still resolving relay lists
/// and adding author outbox relays while the initial stream competes for
/// cold sockets, so slow relays miss the fetch's per-relay lifetime
/// window — and a manual refresh (warm sockets, cached gossip) then finds
/// their events. The settle pass re-runs the identical fetch once after
/// the pool has settled, so the initial load converges to what a refresh
/// would show without the user pressing anything. With the cache-top
/// partition (below), whatever it finds lands in the pending-posts
/// banner — the rendered feed never jumps. Amethyst re-issues its home
/// REQ on outbox-map warm-up the same way (`TopNavFeedSubAssembler`).
const SETTLE_PASS_DELAY_MS: u32 = 15_000;

/// Everything the shared feed network phase needs. `Copy` so the initial
/// load and its settle pass can each hold one; `token` is the epoch's
/// stale token so a superseded epoch's phase (and settle pass) discards
/// before touching any signal.
#[derive(Clone, Copy)]
struct FeedPhaseCtx {
    feed_type: Signal<FeedType>,
    feed_state: Signal<DataState<Rc<Vec<FeedItem>>>>,
    pending_posts: Signal<Vec<FeedItem>>,
    feed_reset_generation: Signal<u64>,
    oldest_timestamp: Signal<Option<u64>>,
    interaction_counts: Signal<HashMap<String, InteractionCounts>>,
    interactions_loaded: Signal<bool>,
    interaction_stream_handle: Signal<Option<InteractionStreamHandle>>,
    stale: use_stale_guard::StaleGuard,
    token: u64,
    is_first_load: bool,
    /// Partition threshold from the painted cache (pending-posts banner):
    /// fetched items strictly newer than this buffer into the banner;
    /// everything else merges directly. `None` → merge everything
    /// directly (manual refresh, or no cache painted).
    cache_top: Option<u64>,
    cached_cursor: Option<u64>,
    cached_count: usize,
    /// Idle-deferred count queue + nonce (the same pair pagination's count
    /// flush uses). The settle pass enqueues its rendered-delta ids here
    /// instead of re-running the counts pipeline; the flush effect in
    /// Home scope coalesces them into ONE bounded backfill after the
    /// debounce window.
    pending_count_flush: Signal<Vec<EventId>>,
    count_flush_nonce: Signal<u64>,
}

/// Buffer items into the pending-posts banner ("Show N new posts"),
/// deduplicating against what is already buffered and what is already
/// rendered — the same rule the realtime subscription callbacks use, so
/// the banner is the single accumulation point and nothing double-counts.
/// Returns how many items were newly buffered.
fn buffer_pending_items(
    mut pending_posts: Signal<Vec<FeedItem>>,
    feed_state: Signal<DataState<Rc<Vec<FeedItem>>>>,
    items: Vec<FeedItem>,
) -> usize {
    if items.is_empty() {
        return 0;
    }
    let mut fresh = Vec::with_capacity(items.len());
    {
        let buffered: HashSet<EventId> =
            pending_posts.read().iter().map(|i| i.event().id).collect();
        let rendered: HashSet<EventId> = match &*feed_state.peek() {
            DataState::Loaded(current) => current.iter().map(|i| i.event().id).collect(),
            _ => HashSet::new(),
        };
        for item in items {
            let id = item.event().id;
            if !buffered.contains(&id) && !rendered.contains(&id) {
                fresh.push(item);
            }
        }
    }
    let added = fresh.len();
    if added > 0 {
        pending_posts.write().extend(fresh);
    }
    added
}

/// The currently rendered feed items (empty when not `Loaded`).
fn rendered_feed_snapshot(feed_state: Signal<DataState<Rc<Vec<FeedItem>>>>) -> Vec<FeedItem> {
    match &*feed_state.peek() {
        DataState::Loaded(items) => items.to_vec(),
        _ => Vec::new(),
    }
}

/// The full known feed state for the disk cache: rendered ∪ accumulated ∪
/// buffered. Banner-buffered items MUST be cached too, so the next app
/// open paints them directly instead of re-buffering the same backlog.
fn cache_store_snapshot(
    feed_state: Signal<DataState<Rc<Vec<FeedItem>>>>,
    pending_posts: Signal<Vec<FeedItem>>,
    accumulated: Vec<FeedItem>,
) -> Vec<FeedItem> {
    let merged =
        feed_cache::merge_feed_items(rendered_feed_snapshot(feed_state), accumulated);
    feed_cache::merge_feed_items(merged, pending_posts.read().clone())
}

/// Disk-cache key for a feed surface + viewer pubkey — the same mapping
/// the main-load effect's paint branches use (PeopleList ids, joined
/// relay URLs), in one place so the accept-time store and the epoch paint
/// can never disagree.
fn cache_key_for_feed(feed_type: &FeedType, pubkey: &str) -> FeedCacheKey {
    match feed_type {
        FeedType::Following => FeedCacheKey::Following {
            pubkey: pubkey.to_string(),
        },
        FeedType::FollowingWithReplies => FeedCacheKey::FollowingWithReplies {
            pubkey: pubkey.to_string(),
        },
        FeedType::Global => FeedCacheKey::Global,
        FeedType::PeopleList(list) => FeedCacheKey::PeopleList {
            pubkey: pubkey.to_string(),
            list_id: list.identifier.clone(),
        },
        FeedType::RelayFeed { .. } | FeedType::RelaySetFeed { .. } => FeedCacheKey::RelayFeed {
            urls: feed_type.relay_urls().join(","),
        },
    }
}

/// Partition threshold for an unattended epoch's fetch: the disk cache's
/// newest item, lifted to at least the rendered feed's newest. The lift
/// matters exactly in the accept-then-epoch race: banner posts accepted
/// after the last cache store are newer than the disk top but already on
/// screen — without the lift their inevitable re-delivery partitions as
/// "newer than cache" and re-enters the banner.
fn effective_cache_top(cached_top: Option<u64>, rendered_top: Option<u64>) -> Option<u64> {
    match (cached_top, rendered_top) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

/// Whether an epoch's cache paint may MERGE with the rendered feed
/// instead of replacing it: only when the rendered list belongs to the
/// same feed surface and the same viewer account, and something is
/// actually rendered. Feed and account switches must fully replace (the
/// rendered list is the old context's content); an empty render has
/// nothing worth keeping.
fn should_merge_paint(
    feed_type_changed: bool,
    pubkey_changed: bool,
    has_rendered_data: bool,
) -> bool {
    !feed_type_changed && !pubkey_changed && has_rendered_data
}

/// Union of the disk-cache paint and the rendered feed — desc-sorted,
/// deduped by visible event id (rendered wins), capped to the live feed
/// cap so the merge can never grow the list unboundedly.
fn merge_and_cap(cached: Vec<FeedItem>, rendered: Vec<FeedItem>) -> Vec<FeedItem> {
    merge_and_cap_with(cached, rendered, crate::feeds::list::DEFAULT_FEED_CAP)
}

/// [`merge_and_cap`] with an explicit cap, so tests can exercise the
/// trim without building a thousand signed events.
fn merge_and_cap_with(
    cached: Vec<FeedItem>,
    rendered: Vec<FeedItem>,
    cap: usize,
) -> Vec<FeedItem> {
    let mut list = crate::feeds::list::FeedList::from_sorted_items(
        feed_cache::merge_feed_items(cached, rendered),
    );
    list.trim_oldest(cap);
    list.into_items()
}

/// Paint an epoch's disk-cache load and return the fetch partition
/// threshold.
///
/// Merge-paint (the vanish-and-rebuffer fix): when the rendered feed
/// belongs to the same feed+user ([`should_merge_paint`]), the paint is
/// the UNION of cache and rendered — banner posts accepted after the
/// last cache store exist only in `feed_state`, so a plain replace drops
/// them from the screen mid-epoch and the epoch's re-fetch feeds them
/// back into the banner. Amethyst's reload is a store query that
/// re-finds everything ever consumed; merging gives unattended epochs
/// the same non-destructive property.
///
/// The returned `cache_top` is the cache top lifted to the rendered top
/// ([`effective_cache_top`]) so already-rendered items merge directly on
/// re-delivery instead of re-buffering. `None` when the epoch was
/// user-initiated via the refresh button (direct insert) or nothing was
/// painted (first-ever load renders whatever arrives).
fn paint_cached_items(
    mut feed_state: Signal<DataState<Rc<Vec<FeedItem>>>>,
    feed_reset_generation: Signal<u64>,
    cached_items: &[FeedItem],
    feed_type_changed: bool,
    pubkey_changed: bool,
    has_data: bool,
    buffer_new_items: bool,
) -> Option<u64> {
    // Snapshot BEFORE the paint replaces/merges the list: this is both
    // the merge source and the rendered-top lift source.
    let rendered_before = rendered_feed_snapshot(feed_state);
    let cache_top = if buffer_new_items {
        effective_cache_top(
            cached_items.first().map(|i| i.sort_timestamp().as_secs()),
            rendered_before.first().map(|i| i.sort_timestamp().as_secs()),
        )
    } else {
        None
    };
    if !cached_items.is_empty() {
        let items = if should_merge_paint(feed_type_changed, pubkey_changed, has_data) {
            merge_and_cap(cached_items.to_vec(), rendered_before)
        } else {
            cached_items.to_vec()
        };
        set_loaded_with_probe(&mut feed_state, feed_reset_generation, items);
    }
    cache_top
}

/// Realtime fallback for NIP-18 reposts whose `content` does not embed the
/// original event (kind 6/16, original referenced only by the required `e`
/// tag): the classifier drops them, so without this they never reach the
/// banner — and, because the feed cursor only advances past kept items,
/// every subsequent load re-fetches and re-drops them.
///
/// Hydrates the original by id (store-first ladder, `Fast` budget) on a
/// detached task, then re-runs the same dedup + profile-prefetch + buffer
/// sequence as an embedded repost would get. The stale guard is re-checked
/// after the await so a superseded feed type never buffers. Shared by the
/// Following and Global realtime handlers.
fn spawn_repost_hydration_buffer(
    repost: nostr::event::Event,
    original_ref: EventId,
    mut pending: Signal<Vec<FeedItem>>,
    feed_state: Signal<DataState<Rc<Vec<FeedItem>>>>,
    rt_stale: crate::hooks::use_stale_guard::StaleGuard,
    rt_token: u64,
) {
    spawn(async move {
        match crate::feeds::repost_hydration::hydrate_original(&original_ref).await {
            Some(original) => {
                if rt_stale.is_stale(rt_token) {
                    return;
                }
                let feed_item = FeedItem::Repost {
                    original,
                    reposted_by: repost.pubkey,
                    repost_timestamp: repost.created_at,
                };
                // Same dedup as the embedded path: an embedded repost of the
                // same original may have been buffered meanwhile (the
                // FeedItem dedup key is the *original's* event id).
                let event_id = feed_item.event().id;
                let already_buffered = pending
                    .read()
                    .iter()
                    .any(|item| item.event().id == event_id);
                let already_in_feed = match &*feed_state.peek() {
                    DataState::Loaded(ref current_items) => current_items
                        .iter()
                        .any(|item| item.event().id == event_id),
                    _ => false,
                };
                if !already_buffered && !already_in_feed {
                    // Prefetch both profiles (original author + reposter),
                    // mirroring the embedded-repost path.
                    let original_author_pk = feed_item.event().pubkey.to_hex();
                    let reposter_pk = repost.pubkey.to_hex();
                    spawn(async move {
                        let _ = crate::stores::profiles::fetch_profile(
                            original_author_pk,
                        )
                        .await;
                    });
                    spawn(async move {
                        let _ = crate::stores::profiles::fetch_profile(
                            reposter_pk,
                        )
                        .await;
                    });
                    pending.write().push(feed_item);
                    log::info!(
                        "Buffered hydrated e-tag repost, total pending: {}",
                        pending.read().len()
                    );
                }
            }
            None => {
                crate::feeds::repost_hydration::record_hydration_failure(
                    original_ref,
                );
            }
        }
    });
}

/// The shared feed network phase: fetch, partition around the cache top
/// (newer items buffer into the pending-posts banner), merge the rest
/// into the rendered feed, store the full set to the disk cache, and fan
/// out the metadata/interaction/sync prefetches.
///
/// Runs once per load epoch from the main effect and — when a stale cache
/// was painted — a second time [`SETTLE_PASS_DELAY_MS`] later via
/// [`phase_with_settle`], with semantics identical to a manual refresh so
/// the initial load converges to what a refresh would show.
async fn run_feed_network_phase(current_feed_type: &FeedType, ctx: &FeedPhaseCtx) {
    let stale = ctx.stale;
    let token = ctx.token;
    let is_stale = || stale.is_stale(token);
    let cache_top = ctx.cache_top;
    // `Signal::set` takes `&mut self`: copy the write-heavy signals into
    // mutable locals once (`Signal` is `Copy`).
    let FeedPhaseCtx {
        feed_type: mut feed_type_sig,
        feed_state: mut feed_state_sig,
        // `has_more` is deliberately not bound here: this phase must not
        // write it (see the note in the Following branch) — it starts
        // true, is reset true on epoch bumps, and only pagination may
        // settle it false.
        oldest_timestamp: mut oldest_timestamp_sig,
        ..
    } = *ctx;

    match current_feed_type {
        FeedType::Following => {
            let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
            let cache_key = FeedCacheKey::Following {
                pubkey: pubkey_str.clone(),
            };
            let mut fs = ctx.feed_state;
            // Seed the accumulator from whatever is live: the effect just
            // painted the cache (or the previous render survives a
            // refresh epoch) — identical to the old cached-items seed.
            let mut accumulated_items: Vec<FeedItem> = rendered_feed_snapshot(ctx.feed_state);
            let stream_stale = stale;
            let stream_token = token;
            let pending_signal = ctx.pending_posts;
            let collector = DebouncedCollector::<FeedItem>::new(50);
            let accumulated_clone: Rc<RefCell<Vec<FeedItem>>> =
                Rc::new(RefCell::new(accumulated_items.clone()));
            let feed_state_clone = ctx.feed_state;
            let result = load_following_feed_streaming(
                None,
                ctx.cached_cursor,
                ctx.cached_count,
                |batch_items| {
                    if stream_stale.is_stale(stream_token) {
                        log::debug!("Discarding stale streaming batch");
                        return;
                    }
                    // Partition each streamed batch around the painted
                    // cache top: newer items buffer in the banner (the
                    // feed stays stable until accepted); the rest merge
                    // into the rendered feed.
                    let (newer, rest) =
                        crate::feeds::list::partition_by_cache_top(batch_items, cache_top);
                    if !newer.is_empty() {
                        buffer_pending_items(pending_signal, feed_state_clone, newer);
                    }
                    collector.extend(rest, {
                        let acc = accumulated_clone.clone();
                        let mut fs_inner = feed_state_clone;
                        move |items| {
                            let mut acc_guard = acc.borrow_mut();
                            *acc_guard =
                                feed_cache::merge_feed_items(acc_guard.clone(), items);
                            // Merge into whatever is live: accepting the
                            // banner between flushes moves buffered items
                            // into the rendered list — replacing wholesale
                            // would clobber them (same lesson as the
                            // progressive-paint tail).
                            let rendered = match &*fs_inner.peek() {
                                DataState::Loaded(current) => feed_cache::merge_feed_items(
                                    current.to_vec(),
                                    acc_guard.clone(),
                                ),
                                _ => acc_guard.clone(),
                            };
                            fs_inner.set(DataState::Loaded(Rc::new(rendered)));
                        }
                    });
                },
            )
            .await;
            accumulated_items = accumulated_clone.borrow().clone();
            let tail = collector.drain();
            if !tail.is_empty() {
                accumulated_items =
                    feed_cache::merge_feed_items(accumulated_items.clone(), tail);
            }
            if is_stale() {
                return;
            }
            match result {
                Ok((page, did_fallback)) => {
                    // The completion page repeats everything already
                    // streamed through `on_batch` (dedup no-op) EXCEPT
                    // fallback pages, which never flowed through it —
                    // partition those the same way before merging.
                    let (newer, rest) =
                        crate::feeds::list::partition_by_cache_top(page.items, cache_top);
                    if !newer.is_empty() {
                        buffer_pending_items(ctx.pending_posts, ctx.feed_state, newer);
                    }
                    let effective_cache_key = if did_fallback {
                        log::info!("No contacts, switched to Global feed");
                        feed_type_sig.set(FeedType::Global);
                        FeedCacheKey::Global
                    } else {
                        cache_key
                    };
                    // NOTE: `has_more` is intentionally NOT re-armed here.
                    // It starts true and is reset true on every epoch bump
                    // (feed switch / refresh — see the reset block). Within
                    // an epoch, `false` means pagination settled the window
                    // (exhausted union page / all relays done) — the settle
                    // pass re-runs this phase under the SAME epoch token
                    // and must not resurrect it (R2 review: re-arming made
                    // `load_more` re-fetch a settled window, and for
                    // per-relay feeds re-armed after every relay retired).
                    accumulated_items = feed_cache::merge_feed_items(accumulated_items, rest);
                    // Settle-pass counts delta: everything this pass
                    // streamed/merged at or below the cache top (the
                    // completion page repeats streamed items dedup-no-op,
                    // so this is the full new-this-pass rendered set). The
                    // initial pass's live interaction stream covers it;
                    // the settle pass enqueues it into the deferred count
                    // queue instead.
                    let delta_ids: Vec<EventId> =
                        accumulated_items.iter().map(|i| i.event().id).collect();
                    let rendered_final = feed_cache::merge_feed_items(
                        rendered_feed_snapshot(ctx.feed_state),
                        accumulated_items.clone(),
                    );
                    {
                        let timestamps: Vec<u64> = rendered_final
                            .iter()
                            .map(|i| i.sort_timestamp().as_secs())
                            .collect();
                        // Monotonic-deepest cursor (R2 review): pagination
                        // deepens `oldest_timestamp` as pages land (union
                        // `next_until` / per-relay `deepest`); the settle
                        // pass re-runs this phase under the SAME epoch
                        // token and its display-gap cursor can sit ABOVE
                        // the paginated floor — a rewound bound re-serves
                        // the already-paged window, folds
                        // `advanced=false`, and kills `has_more`
                        // permanently. The cursor may only move deeper.
                        let display_cursor =
                            crate::utils::pagination::safe_cursor_from_timestamps(&timestamps);
                        let existing = *oldest_timestamp_sig.read();
                        let next = match (existing, display_cursor) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            (Some(a), None) => Some(a),
                            (None, b) => b,
                        };
                        if next != existing {
                            oldest_timestamp_sig.set(next);
                        }
                    }
                    // Metadata for what is on screen AND what sits in the
                    // banner (it renders on accept).
                    let with_pending = feed_cache::merge_feed_items(
                        rendered_final.clone(),
                        ctx.pending_posts.read().clone(),
                    );
                    prefetch_author_metadata(&with_pending).await;
                    set_loaded_with_probe(&mut fs, ctx.feed_reset_generation, rendered_final.clone());
                    if !is_stale() {
                        let cache_key_for_store = effective_cache_key;
                        let store_fs = ctx.feed_state;
                        let store_pending = ctx.pending_posts;
                        let items_for_cache = accumulated_items;
                        let store_stale = stale;
                        let store_token = token;
                        spawn(async move {
                            // v3 render deferral: keep the cache_store
                            // serialization (per-item serde + IndexedDB
                            // puts) off the render-adjacent turn. One idle
                            // beat is harmless: the cache only serves the
                            // NEXT boot.
                            crate::platform::timer::sleep_ms(1500).await;
                            if store_stale.is_stale(store_token) {
                                return;
                            }
                            let snapshot =
                                cache_store_snapshot(store_fs, store_pending, items_for_cache);
                            if let Err(e) =
                                feed_cache::store_feed_items(&cache_key_for_store, &snapshot)
                                    .await
                            {
                                log::warn!("Failed to store feed to cache: {}", e);
                            }
                            if let Err(e) = feed_cache::run_eviction_if_needed().await {
                                log::warn!("Failed to run cache eviction: {}", e);
                            }
                        });
                    }
                    if !is_stale() {
                        if ctx.interaction_stream_handle.peek().is_none() {
                            // Counts cover the rendered set only
                            // (Amethyst's composition-driven model):
                            // banner items load their counts lazily on
                            // accept.
                            let items_for_counts = rendered_final.clone();
                            let ic = ctx.interaction_counts;
                            let il = ctx.interactions_loaded;
                            let ish = ctx.interaction_stream_handle;
                            spawn(async move {
                                fetch_and_stream_interactions(
                                    &items_for_counts,
                                    ic,
                                    stale,
                                    token,
                                    il,
                                    ish,
                                )
                                .await
                            });
                        } else if !ctx.is_first_load {
                            // Settle pass: the initial load's live
                            // interaction stream already covers the
                            // rendered set — only start a fallback stream
                            // above if that subscription never landed.
                            // Enqueue just this pass's rendered delta into
                            // the idle-deferred count queue (pagination's
                            // direct-extend pattern — NOT
                            // enqueue_counts_for_visible, whose >=30 skip
                            // gate would drop large deltas) instead of
                            // re-running the counts pipeline.
                            let mut flush_queue = ctx.pending_count_flush;
                            let mut flush_nonce = ctx.count_flush_nonce;
                            enqueue_count_anchors(&mut flush_queue, delta_ids);
                            *flush_nonce.write() += 1;
                        }
                    }
                    // Coverage resolution is an initial-load concern: the
                    // settle re-fetches mostly-known authors, and its few
                    // new ones resolve lazily (the initial pass's relay
                    // coverage discovery already ran).
                    if !is_stale() && ctx.is_first_load {
                        let items_for_relays = with_pending;
                        spawn(async move {
                            prefetch_author_metadata_with_relays(&items_for_relays).await;
                        });
                    }
                    if !is_stale() && !did_fallback {
                        let pk_for_sync = pubkey_str;
                        spawn(async move {
                            if let Ok(contacts) =
                                nostr_client::fetch_contacts(pk_for_sync).await
                            {
                                let authors: Vec<PublicKey> = contacts
                                    .iter()
                                    .filter_map(|c| PublicKey::parse(c).ok())
                                    .collect();
                                if !authors.is_empty() {
                                    sync_following_feed_page(authors, None).await;
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    if rendered_feed_snapshot(ctx.feed_state).is_empty() {
                        feed_state_sig.set(DataState::Error(e.to_string()));
                    } else {
                        log::warn!("Network error but showing cached data: {}", e);
                    }
                }
            }
        }
        FeedType::FollowingWithReplies => {
            let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
            let pk_for_sync = pubkey_str.clone();
            let cache_key = FeedCacheKey::FollowingWithReplies {
                pubkey: pubkey_str,
            };
            let mut fs = ctx.feed_state;
            let result =
                load_following_with_replies(None, ctx.cached_cursor, ctx.cached_count).await;
            if is_stale() {
                return;
            }
            match result {
                Ok((page, did_fallback)) => {
                    let (newer, rest) =
                        crate::feeds::list::partition_by_cache_top(page.items, cache_top);
                    if !newer.is_empty() {
                        buffer_pending_items(ctx.pending_posts, ctx.feed_state, newer);
                    }
                    let effective_cache_key = if did_fallback {
                        log::info!("No contacts, switched to Global feed");
                        feed_type_sig.set(FeedType::Global);
                        FeedCacheKey::Global
                    } else {
                        cache_key
                    };
                    // `has_more` intentionally NOT re-armed (see the
                    // Following branch note): a settled pagination window
                    // must survive the settle pass's same-epoch re-run.
                    // Settle-pass counts delta: the at-or-below-top items
                    // that merged into the rendered set this pass. The
                    // initial pass's live interaction stream covers them;
                    // the settle pass enqueues them into the deferred
                    // count queue instead.
                    let delta_ids: Vec<EventId> = rest.iter().map(|i| i.event().id).collect();
                    let rendered_final = feed_cache::merge_feed_items(
                        rendered_feed_snapshot(ctx.feed_state),
                        rest,
                    );
                    {
                        let timestamps: Vec<u64> = rendered_final
                            .iter()
                            .map(|i| i.sort_timestamp().as_secs())
                            .collect();
                        // Monotonic-deepest cursor (R2 review): pagination
                        // deepens `oldest_timestamp` as pages land (union
                        // `next_until` / per-relay `deepest`); the settle
                        // pass re-runs this phase under the SAME epoch
                        // token and its display-gap cursor can sit ABOVE
                        // the paginated floor — a rewound bound re-serves
                        // the already-paged window, folds
                        // `advanced=false`, and kills `has_more`
                        // permanently. The cursor may only move deeper.
                        let display_cursor =
                            crate::utils::pagination::safe_cursor_from_timestamps(&timestamps);
                        let existing = *oldest_timestamp_sig.read();
                        let next = match (existing, display_cursor) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            (Some(a), None) => Some(a),
                            (None, b) => b,
                        };
                        if next != existing {
                            oldest_timestamp_sig.set(next);
                        }
                    }
                    let with_pending = feed_cache::merge_feed_items(
                        rendered_final.clone(),
                        ctx.pending_posts.read().clone(),
                    );
                    prefetch_author_metadata(&with_pending).await;
                    set_loaded_with_probe(&mut fs, ctx.feed_reset_generation, rendered_final.clone());
                    if !is_stale() {
                        let cache_key_for_store = effective_cache_key;
                        let store_fs = ctx.feed_state;
                        let store_pending = ctx.pending_posts;
                        let items_for_cache = with_pending.clone();
                        spawn(async move {
                            let snapshot =
                                cache_store_snapshot(store_fs, store_pending, items_for_cache);
                            let _ = feed_cache::store_feed_items(
                                &cache_key_for_store,
                                &snapshot,
                            )
                            .await;
                            let _ = feed_cache::run_eviction_if_needed().await;
                        });
                    }
                    if !is_stale() {
                        if ctx.interaction_stream_handle.peek().is_none() {
                            // Counts cover the rendered set only
                            // (Amethyst's composition-driven model):
                            // banner items load their counts lazily on
                            // accept.
                            let items_for_counts = rendered_final.clone();
                            let ic = ctx.interaction_counts;
                            let il = ctx.interactions_loaded;
                            let ish = ctx.interaction_stream_handle;
                            spawn(async move {
                                fetch_and_stream_interactions(
                                    &items_for_counts,
                                    ic,
                                    stale,
                                    token,
                                    il,
                                    ish,
                                )
                                .await
                            });
                        } else if !ctx.is_first_load {
                            // Settle pass: the initial load's live
                            // interaction stream already covers the
                            // rendered set — only start a fallback stream
                            // above if that subscription never landed.
                            // Enqueue just this pass's rendered delta into
                            // the idle-deferred count queue (pagination's
                            // direct-extend pattern — NOT
                            // enqueue_counts_for_visible, whose >=30 skip
                            // gate would drop large deltas) instead of
                            // re-running the counts pipeline.
                            let mut flush_queue = ctx.pending_count_flush;
                            let mut flush_nonce = ctx.count_flush_nonce;
                            enqueue_count_anchors(&mut flush_queue, delta_ids);
                            *flush_nonce.write() += 1;
                        }
                    }
                    if !is_stale() {
                        spawn(async move {
                            prefetch_author_metadata(&with_pending).await;
                        });
                    }
                    if !is_stale() && !did_fallback {
                        spawn(async move {
                            if let Ok(contacts) = nostr_client::fetch_contacts(pk_for_sync).await {
                                let authors: Vec<PublicKey> = contacts
                                    .iter()
                                    .filter_map(|c| PublicKey::parse(c).ok())
                                    .collect();
                                if !authors.is_empty() {
                                    sync_following_feed_page(authors, None).await;
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    if rendered_feed_snapshot(ctx.feed_state).is_empty() {
                        feed_state_sig.set(DataState::Error(e.to_string()));
                    } else {
                        log::warn!("Network error but showing cached data: {}", e);
                    }
                }
            }
        }
        FeedType::Global => {
            let mut fs = ctx.feed_state;
            let result = load_global_feed(None, ctx.cached_cursor, ctx.cached_count).await;
            if is_stale() {
                return;
            }
            match result {
                Ok(page) => {
                    let (newer, rest) =
                        crate::feeds::list::partition_by_cache_top(page.items, cache_top);
                    if !newer.is_empty() {
                        buffer_pending_items(ctx.pending_posts, ctx.feed_state, newer);
                    }
                    // Settle-pass counts delta: the at-or-below-top items
                    // that merged into the rendered set this pass. The
                    // initial pass's live interaction stream covers them;
                    // the settle pass enqueues them into the deferred
                    // count queue instead.
                    let delta_ids: Vec<EventId> = rest.iter().map(|i| i.event().id).collect();
                    let rendered_final = feed_cache::merge_feed_items(
                        rendered_feed_snapshot(ctx.feed_state),
                        rest,
                    );
                    {
                        let timestamps: Vec<u64> = rendered_final
                            .iter()
                            .map(|i| i.sort_timestamp().as_secs())
                            .collect();
                        // Monotonic-deepest cursor (R2 review): pagination
                        // deepens `oldest_timestamp` as pages land (union
                        // `next_until` / per-relay `deepest`); the settle
                        // pass re-runs this phase under the SAME epoch
                        // token and its display-gap cursor can sit ABOVE
                        // the paginated floor — a rewound bound re-serves
                        // the already-paged window, folds
                        // `advanced=false`, and kills `has_more`
                        // permanently. The cursor may only move deeper.
                        let display_cursor =
                            crate::utils::pagination::safe_cursor_from_timestamps(&timestamps);
                        let existing = *oldest_timestamp_sig.read();
                        let next = match (existing, display_cursor) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            (Some(a), None) => Some(a),
                            (None, b) => b,
                        };
                        if next != existing {
                            oldest_timestamp_sig.set(next);
                        }
                    }
                    // `has_more` intentionally NOT re-armed (see the
                    // Following branch note: settled pagination windows
                    // must survive the settle pass's same-epoch re-run).
                    let with_pending = feed_cache::merge_feed_items(
                        rendered_final.clone(),
                        ctx.pending_posts.read().clone(),
                    );
                    set_loaded_with_probe(&mut fs, ctx.feed_reset_generation, rendered_final.clone());
                    if !is_stale() {
                        let store_fs = ctx.feed_state;
                        let store_pending = ctx.pending_posts;
                        let items_for_cache = with_pending.clone();
                        spawn(async move {
                            let snapshot =
                                cache_store_snapshot(store_fs, store_pending, items_for_cache);
                            let _ = feed_cache::store_feed_items(
                                &FeedCacheKey::Global,
                                &snapshot,
                            )
                            .await;
                            let _ = feed_cache::run_eviction_if_needed().await;
                        });
                    }
                    if !is_stale() {
                        if ctx.interaction_stream_handle.peek().is_none() {
                            // Counts cover the rendered set only
                            // (Amethyst's composition-driven model):
                            // banner items load their counts lazily on
                            // accept.
                            let items_for_counts = rendered_final.clone();
                            let ic = ctx.interaction_counts;
                            let il = ctx.interactions_loaded;
                            let ish = ctx.interaction_stream_handle;
                            spawn(async move {
                                fetch_and_stream_interactions(
                                    &items_for_counts,
                                    ic,
                                    stale,
                                    token,
                                    il,
                                    ish,
                                )
                                .await
                            });
                        } else if !ctx.is_first_load {
                            // Settle pass: the initial load's live
                            // interaction stream already covers the
                            // rendered set — only start a fallback stream
                            // above if that subscription never landed.
                            // Enqueue just this pass's rendered delta into
                            // the idle-deferred count queue (pagination's
                            // direct-extend pattern — NOT
                            // enqueue_counts_for_visible, whose >=30 skip
                            // gate would drop large deltas) instead of
                            // re-running the counts pipeline.
                            let mut flush_queue = ctx.pending_count_flush;
                            let mut flush_nonce = ctx.count_flush_nonce;
                            enqueue_count_anchors(&mut flush_queue, delta_ids);
                            *flush_nonce.write() += 1;
                        }
                    }
                    if !is_stale() {
                        spawn(async move {
                            prefetch_author_metadata(&with_pending).await;
                        });
                    }
                }
                Err(e) => {
                    if rendered_feed_snapshot(ctx.feed_state).is_empty() {
                        feed_state_sig.set(DataState::Error(e.to_string()));
                    } else {
                        log::warn!("Network error but showing cached data: {}", e);
                    }
                }
            }
        }
        FeedType::PeopleList(list) => {
            let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
            let cache_key = FeedCacheKey::PeopleList {
                pubkey: pubkey_str,
                list_id: list.identifier.clone(),
            };
            let mut fs = ctx.feed_state;
            let result =
                load_people_list_feed(list, None, ctx.cached_cursor, ctx.cached_count).await;
            if is_stale() {
                return;
            }
            match result {
                Ok(page) => {
                    let (newer, rest) =
                        crate::feeds::list::partition_by_cache_top(page.items, cache_top);
                    if !newer.is_empty() {
                        buffer_pending_items(ctx.pending_posts, ctx.feed_state, newer);
                    }
                    // Settle-pass counts delta: the at-or-below-top items
                    // that merged into the rendered set this pass. The
                    // initial pass's live interaction stream covers them;
                    // the settle pass enqueues them into the deferred
                    // count queue instead.
                    let delta_ids: Vec<EventId> = rest.iter().map(|i| i.event().id).collect();
                    let rendered_final = feed_cache::merge_feed_items(
                        rendered_feed_snapshot(ctx.feed_state),
                        rest,
                    );
                    {
                        let timestamps: Vec<u64> = rendered_final
                            .iter()
                            .map(|i| i.sort_timestamp().as_secs())
                            .collect();
                        // Monotonic-deepest cursor (R2 review): pagination
                        // deepens `oldest_timestamp` as pages land (union
                        // `next_until` / per-relay `deepest`); the settle
                        // pass re-runs this phase under the SAME epoch
                        // token and its display-gap cursor can sit ABOVE
                        // the paginated floor — a rewound bound re-serves
                        // the already-paged window, folds
                        // `advanced=false`, and kills `has_more`
                        // permanently. The cursor may only move deeper.
                        let display_cursor =
                            crate::utils::pagination::safe_cursor_from_timestamps(&timestamps);
                        let existing = *oldest_timestamp_sig.read();
                        let next = match (existing, display_cursor) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            (Some(a), None) => Some(a),
                            (None, b) => b,
                        };
                        if next != existing {
                            oldest_timestamp_sig.set(next);
                        }
                    }
                    // `has_more` intentionally NOT re-armed (see the
                    // Following branch note: settled pagination windows
                    // must survive the settle pass's same-epoch re-run).
                    let with_pending = feed_cache::merge_feed_items(
                        rendered_final.clone(),
                        ctx.pending_posts.read().clone(),
                    );
                    set_loaded_with_probe(&mut fs, ctx.feed_reset_generation, rendered_final.clone());
                    if !is_stale() {
                        let cache_key_for_store = cache_key;
                        let store_fs = ctx.feed_state;
                        let store_pending = ctx.pending_posts;
                        let items_for_cache = with_pending.clone();
                        spawn(async move {
                            let snapshot =
                                cache_store_snapshot(store_fs, store_pending, items_for_cache);
                            let _ = feed_cache::store_feed_items(
                                &cache_key_for_store,
                                &snapshot,
                            )
                            .await;
                            let _ = feed_cache::run_eviction_if_needed().await;
                        });
                    }
                    if !is_stale() {
                        if ctx.interaction_stream_handle.peek().is_none() {
                            // Counts cover the rendered set only
                            // (Amethyst's composition-driven model):
                            // banner items load their counts lazily on
                            // accept.
                            let items_for_counts = rendered_final.clone();
                            let ic = ctx.interaction_counts;
                            let il = ctx.interactions_loaded;
                            let ish = ctx.interaction_stream_handle;
                            spawn(async move {
                                fetch_and_stream_interactions(
                                    &items_for_counts,
                                    ic,
                                    stale,
                                    token,
                                    il,
                                    ish,
                                )
                                .await
                            });
                        } else if !ctx.is_first_load {
                            // Settle pass: the initial load's live
                            // interaction stream already covers the
                            // rendered set — only start a fallback stream
                            // above if that subscription never landed.
                            // Enqueue just this pass's rendered delta into
                            // the idle-deferred count queue (pagination's
                            // direct-extend pattern — NOT
                            // enqueue_counts_for_visible, whose >=30 skip
                            // gate would drop large deltas) instead of
                            // re-running the counts pipeline.
                            let mut flush_queue = ctx.pending_count_flush;
                            let mut flush_nonce = ctx.count_flush_nonce;
                            enqueue_count_anchors(&mut flush_queue, delta_ids);
                            *flush_nonce.write() += 1;
                        }
                    }
                    if !is_stale() {
                        spawn(async move {
                            prefetch_author_metadata(&with_pending).await;
                        });
                    }
                }
                Err(e) => {
                    if rendered_feed_snapshot(ctx.feed_state).is_empty() {
                        feed_state_sig.set(DataState::Error(e.to_string()));
                    } else {
                        log::warn!("Network error but showing cached data: {}", e);
                    }
                }
            }
        }
        FeedType::RelayFeed { .. } | FeedType::RelaySetFeed { .. } => {
            let urls = current_feed_type.relay_urls();
            let cache_key = FeedCacheKey::RelayFeed {
                urls: urls.join(","),
            };
            let mut fs = ctx.feed_state;
            let result =
                load_relay_feed(urls, None, ctx.cached_cursor, ctx.cached_count).await;
            if is_stale() {
                return;
            }
            match result {
                Ok(page) => {
                    let (newer, rest) =
                        crate::feeds::list::partition_by_cache_top(page.items, cache_top);
                    if !newer.is_empty() {
                        buffer_pending_items(ctx.pending_posts, ctx.feed_state, newer);
                    }
                    // Settle-pass counts delta: the at-or-below-top items
                    // that merged into the rendered set this pass. The
                    // initial pass's live interaction stream covers them;
                    // the settle pass enqueues them into the deferred
                    // count queue instead.
                    let delta_ids: Vec<EventId> = rest.iter().map(|i| i.event().id).collect();
                    let rendered_final = feed_cache::merge_feed_items(
                        rendered_feed_snapshot(ctx.feed_state),
                        rest,
                    );
                    {
                        let timestamps: Vec<u64> = rendered_final
                            .iter()
                            .map(|i| i.sort_timestamp().as_secs())
                            .collect();
                        // Monotonic-deepest cursor (R2 review): pagination
                        // deepens `oldest_timestamp` as pages land (union
                        // `next_until` / per-relay `deepest`); the settle
                        // pass re-runs this phase under the SAME epoch
                        // token and its display-gap cursor can sit ABOVE
                        // the paginated floor — a rewound bound re-serves
                        // the already-paged window, folds
                        // `advanced=false`, and kills `has_more`
                        // permanently. The cursor may only move deeper.
                        let display_cursor =
                            crate::utils::pagination::safe_cursor_from_timestamps(&timestamps);
                        let existing = *oldest_timestamp_sig.read();
                        let next = match (existing, display_cursor) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            (Some(a), None) => Some(a),
                            (None, b) => b,
                        };
                        if next != existing {
                            oldest_timestamp_sig.set(next);
                        }
                    }
                    // `has_more` intentionally NOT re-armed (see the
                    // Following branch note: settled pagination windows
                    // must survive the settle pass's same-epoch re-run).
                    let with_pending = feed_cache::merge_feed_items(
                        rendered_final.clone(),
                        ctx.pending_posts.read().clone(),
                    );
                    set_loaded_with_probe(&mut fs, ctx.feed_reset_generation, rendered_final.clone());
                    if !is_stale() {
                        let cache_key_for_store = cache_key;
                        let store_fs = ctx.feed_state;
                        let store_pending = ctx.pending_posts;
                        let items_for_cache = with_pending.clone();
                        spawn(async move {
                            let snapshot =
                                cache_store_snapshot(store_fs, store_pending, items_for_cache);
                            let _ = feed_cache::store_feed_items(
                                &cache_key_for_store,
                                &snapshot,
                            )
                            .await;
                            let _ = feed_cache::run_eviction_if_needed().await;
                        });
                    }
                    if !is_stale() {
                        if ctx.interaction_stream_handle.peek().is_none() {
                            // Counts cover the rendered set only
                            // (Amethyst's composition-driven model):
                            // banner items load their counts lazily on
                            // accept.
                            let items_for_counts = rendered_final.clone();
                            let ic = ctx.interaction_counts;
                            let il = ctx.interactions_loaded;
                            let ish = ctx.interaction_stream_handle;
                            spawn(async move {
                                fetch_and_stream_interactions(
                                    &items_for_counts,
                                    ic,
                                    stale,
                                    token,
                                    il,
                                    ish,
                                )
                                .await
                            });
                        } else if !ctx.is_first_load {
                            // Settle pass: the initial load's live
                            // interaction stream already covers the
                            // rendered set — only start a fallback stream
                            // above if that subscription never landed.
                            // Enqueue just this pass's rendered delta into
                            // the idle-deferred count queue (pagination's
                            // direct-extend pattern — NOT
                            // enqueue_counts_for_visible, whose >=30 skip
                            // gate would drop large deltas) instead of
                            // re-running the counts pipeline.
                            let mut flush_queue = ctx.pending_count_flush;
                            let mut flush_nonce = ctx.count_flush_nonce;
                            enqueue_count_anchors(&mut flush_queue, delta_ids);
                            *flush_nonce.write() += 1;
                        }
                    }
                    if !is_stale() {
                        spawn(async move {
                            prefetch_author_metadata(&with_pending).await;
                        });
                    }
                }
                Err(e) => {
                    if rendered_feed_snapshot(ctx.feed_state).is_empty() {
                        feed_state_sig.set(DataState::Error(e.to_string()));
                    } else {
                        log::warn!("Network error but showing cached data: {}", e);
                    }
                }
            }
        }
    }
}

/// Run the shared network phase and — for cache-painted (stale-cache)
/// epochs — schedule the one-shot settle pass (see
/// [`SETTLE_PASS_DELAY_MS`]). Schedules exactly once per epoch: the spawn
/// is created here and dies with the epoch's stale token.
async fn phase_with_settle(current_feed_type: &FeedType, ctx: &FeedPhaseCtx) {
    run_feed_network_phase(current_feed_type, ctx).await;
    if ctx.cache_top.is_none() {
        // No stale cache painted (fresh load) or user-initiated refresh:
        // nothing to converge toward — the refresh button IS the warm-
        // pool fetch.
        return;
    }
    let settle_type = current_feed_type.clone();
    let settle_ctx = FeedPhaseCtx {
        is_first_load: false,
        ..*ctx
    };
    let settle_stale = ctx.stale;
    let settle_token = ctx.token;
    crate::platform::spawn::spawn_catch_unwind("feed_settle_pass", async move {
        crate::platform::timer::sleep_ms(SETTLE_PASS_DELAY_MS).await;
        if settle_stale.is_stale(settle_token) {
            log::debug!("Settle pass skipped: epoch superseded");
            return;
        }
        log::info!("Settle pass: re-fetching feed against the warmed relay pool");
        run_feed_network_phase(&settle_type, &settle_ctx).await;
    });
}

#[component]
pub fn Home(list: String) -> Element {
    // The feed list is held in an Rc so a parent render clones a refcount
    // instead of every event, and so `FeedListBody`'s props memoization can
    // use pointer equality (a fresh Rc is only allocated when the list
    // actually changed — see `set_loaded_with_probe` / wave B1).
    let mut feed_state = use_signal(|| DataState::<Rc<Vec<FeedItem>>>::Pending);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    // Backward-pagination cursor session (raw-event floor tracking). See
    // `BackwardPageCursor` — replaces the old gap-walk cursor for pages,
    // which could pin the same `until` forever and re-issue identical REQs.
    let mut backward_cursor = use_signal(BackwardPageCursor::new);
    // Phase-2 per-relay cursor session for author feeds (Following /
    // WithReplies / PeopleList): each connected relay walks its own
    // timeline (Amethyst `RelayLoadingCursors` port). `backward_cursor`
    // remains the cursor for Global (DB-merge) and RelayFeed pages.
    let mut relay_cursors = use_signal(RelayCursorSet::new);
    // 3c-2: idle-deferred pagination count flush. Page merges enqueue
    // their anchors into the queue and bump the nonce; the flush effect
    // near the sentinel coalesces them into ONE bounded backfill fired
    // after the debounce window with no newer enqueue (trailing
    // debounce). This moves the SDK-side per-event parse + unconditional
    // schnorr verify + IndexedDB save burst (300+ events per page,
    // measured 2-4s main-thread stalls mid-scroll) out of the active
    // scroll interaction. Counts keep painting instantly from the local
    // DB and the live stream; the flush is enrichment.
    let mut pending_count_flush = use_signal(Vec::<EventId>::new);
    let mut count_flush_failures = use_signal(|| 0u32);
    let mut count_flush_nonce = use_signal(|| 0u64);
    let mut pagination_loading = use_signal(|| false);
    let interaction_counts = use_signal(HashMap::<String, InteractionCounts>::new);
    let mut interactions_loaded = use_signal(|| false);
    let mut cached_muted_posts: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut cached_blocked_users: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut pending_posts = use_signal(Vec::<FeedItem>::new);
    let pending_count = use_memo(move || pending_posts.read().len());
    // Page-wake staleness epoch: bumped by the web visibility/focus/online
    // listeners (or the safety-net poll) when the user returns to a page
    // whose feed top has gone stale. Read by the main load effect as an
    // epoch source alongside refresh/feed-type/signer/readiness.
    let wake_trigger = use_signal(|| 0u32);
    // Unproductive-epoch backoff (see `wake_min_interval`): consecutive
    // wakes after which the feed top never moved, and the top timestamp
    // captured at the last accepted wake for that comparison.
    #[cfg(feature = "web")]
    let wake_unproductive_streak = use_signal(|| 0u32);
    #[cfg(feature = "web")]
    let wake_top_at_bump: Signal<Option<u64>> = use_signal(|| None);
    #[cfg(feature = "web")]
    let wake_last_bumped: Signal<u64> = use_signal(|| 0);
    // Stored JS listeners, dropped on unmount (settings/home.rs pattern).
    #[cfg(feature = "web")]
    let mut wake_listeners: Signal<
        Option<(EventListener, EventListener, EventListener)>,
    > = use_signal(|| None);
    let mut realtime_started = use_signal(|| false);
    // The CONTACTS_VERSION the current realtime session subscribed with
    // (plan wave B3): a bump (follow/unfollow, or a network refresh that
    // observed a different set) tears the session down and re-keys it
    // against the new author set — previously a mid-session follow was
    // invisible to the home feed until a full reload.
    let mut realtime_contacts_version = use_signal(|| None::<u64>);
    let mut subscription_ids = use_signal(Vec::<nostr::message::SubscriptionId>::new);
    // Guards for dispatcher-backed realtime subscriptions (relay-feed +
    // global); the follows path keeps raw batch sub ids in `subscription_ids`
    // because ONE listener watches N batch subs.
    let mut realtime_guards =
        use_signal(Vec::<subscription_manager::SpawnedSubscription>::new);
    let mut interaction_stream_handle: Signal<Option<InteractionStreamHandle>> =
        use_signal(|| None);
    let mut stale = use_stale_guard();
    let mut realtime_stale = use_stale_guard();
    // (refresh, feed type, signer available, relays applied, wake,
    // pubkey): the pubkey pins each epoch's identity — merge-paints and
    // the accept-time cache store must never cross accounts. Seeded with
    // the boot pubkey so the FIRST epoch isn't tagged as an account
    // switch (a later login/logout flips it, which is exactly when the
    // feed must reload).
    let mut last_loaded_trigger = use_signal(|| {
        (
            0u32,
            FeedType::Following,
            false,
            false,
            0u32,
            auth_store::get_pubkey().unwrap_or_default(),
        )
    });
    let mut feed_reset_generation = use_signal(|| 0u64);
    let mut relay_feed_sub_id: Signal<Option<nostr::message::SubscriptionId>> =
        use_signal(|| None);
    let mut relay_feed_ephemeral_urls = use_signal(Vec::<String>::new);
    let relay_url_input = use_signal(String::new);
    let (all_lists, _lists_loading, _lists_error, _) = use_user_lists();
    let people_lists = use_memo(move || {
        all_lists
            .read()
            .iter()
            .filter(|list| list.kind == crate::utils::list_kinds::NAMED_PEOPLE)
            .cloned()
            .collect::<Vec<_>>()
    });
    let relay_lists = use_memo(move || {
        all_lists
            .read()
            .iter()
            .filter(|list| list.kind == NAMED_RELAYS)
            .cloned()
            .collect::<Vec<_>>()
    });

    // Deep link: set feed type from ?list= parameter
    use_effect({
        let list_param = list.clone();
        move || {
            if list_param.is_empty() {
                return;
            }
            let lists = people_lists.read();
            if lists.is_empty() {
                return;
            }
            if let Some(matching_list) = lists.iter().find(|l| l.identifier == list_param) {
                log::info!("Deep link: Setting feed to list '{}'", matching_list.name);
                feed_type.set(FeedType::PeopleList(Box::new(matching_list.clone())));
            } else {
                log::warn!("Deep link: List with identifier '{}' not found", list_param);
            }
        }
    });

    // Mute/block cache management
    let mut last_mute_pubkey: Signal<Option<String>> = use_signal(|| None);
    let mut last_invalidate_token: Signal<u32> = use_signal(|| 0);
    use_effect(move || {
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let current_pubkey = auth_store::AUTH_STATE.read().pubkey.clone();
        let current_token = *nostr_client::MUTE_BLOCK_INVALIDATE.read();
        if current_token != *last_invalidate_token.peek() {
            log::debug!("Mute/block invalidation detected in home feed, clearing caches");
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            last_invalidate_token.set(current_token);
        }
        if !is_authenticated {
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            last_mute_pubkey.set(None);
            return;
        }
        if let (Some(ref last), Some(ref current)) =
            (last_mute_pubkey.peek().as_ref(), current_pubkey.as_ref())
        {
            if last != current {
                log::debug!("Account switch detected in home feed, clearing mute/block cache");
                cached_muted_posts.set(None);
                cached_blocked_users.set(None);
            }
        }
        last_mute_pubkey.set(current_pubkey.clone());
        if !client_initialized {
            return;
        }
        if current_pubkey.is_none() {
            log::debug!("Skipping mute list fetch - pubkey not yet available");
            return;
        }
        if cached_muted_posts.peek().is_some() && cached_blocked_users.peek().is_some() {
            return;
        }
        let auth_pubkey_snapshot = current_pubkey.clone();
        let invalidate_token_snapshot = current_token;
        spawn(async move {
            match nostr_client::get_mute_list_data().await {
                Ok(data) => {
                    let current_pubkey = auth_store::AUTH_STATE.peek().pubkey.clone();
                    let current_invalidate = *nostr_client::MUTE_BLOCK_INVALIDATE.peek();
                    if current_pubkey == auth_pubkey_snapshot
                        && auth_pubkey_snapshot.is_some()
                        && current_invalidate == invalidate_token_snapshot
                    {
                        cached_muted_posts.set(Some(Rc::new(data.muted_posts)));
                        cached_blocked_users.set(Some(Rc::new(data.blocked_users)));
                    } else {
                        log::debug!("Discarding stale mute list fetch (pubkey or token changed)");
                    }
                }
                Err(e) => {
                    let snapshot_short =
                        auth_pubkey_snapshot.as_ref().map(|s| safe_slice(s, 8));
                    let current_short = auth_store::AUTH_STATE
                        .peek()
                        .pubkey
                        .as_ref()
                        .map(|s| s[..8.min(s.len())].to_string());
                    log::error!(
                        "Failed to fetch mute list: {} (snapshot={:?}, current={:?})",
                        e,
                        snapshot_short,
                        current_short
                    );
                }
            }
        });
    });

    // Page-wake listeners (web): returning to a long-idle page must
    // re-check for new posts — the realtime sockets died with the idle
    // page (browser cull / OS sleep; undetectable on WASM, no pings) and
    // nothing else re-runs on an already-loaded page. The staleness gate
    // lives in `should_wake_reload`; the epoch itself is the main load
    // effect below via `wake_trigger`.
    #[cfg(feature = "web")]
    use_effect(move || {
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };

        let fs_for_wake = feed_state;
        let trig = wake_trigger;
        let bumped = wake_last_bumped;
        let streak = wake_unproductive_streak;
        let top_at_bump = wake_top_at_bump;

        // visibilitychange fires for both transitions; only the
        // became-visible one may wake.
        let visibility_listener = {
            let fs = fs_for_wake;
            let t = trig;
            let b = bumped;
            let s = streak;
            let tb = top_at_bump;
            EventListener::new(&document, "visibilitychange", move |_| {
                if !document_is_hidden() {
                    attempt_wake_reload("visibilitychange", fs, t, b, s, tb);
                }
            })
        };
        // Window focus covers returns that never hid the document (OS
        // slept with the tab visible, another window was focused).
        let focus_listener = {
            let fs = fs_for_wake;
            let t = trig;
            let b = bumped;
            let s = streak;
            let tb = top_at_bump;
            EventListener::new(&window, "focus", move |_| {
                attempt_wake_reload("focus", fs, t, b, s, tb);
            })
        };
        // Network return (sleep wake-up, Wi-Fi switch) — `online` fires
        // even when the document was visible throughout.
        let online_listener = {
            let fs = fs_for_wake;
            let t = trig;
            let b = bumped;
            let s = streak;
            let tb = top_at_bump;
            EventListener::new(&window, "online", move |_| {
                attempt_wake_reload("online", fs, t, b, s, tb);
            })
        };
        wake_listeners.set(Some((
            visibility_listener,
            focus_listener,
            online_listener,
        )));
    });

    // Freeze-detection poll (web): the listeners can miss the return (OS
    // sleep with the tab visible fires no visibilitychange; focus may
    // already be held). The reliable proof of a frozen tab is timer drift:
    // JS timers don't fire while the tab is suspended, so a sleep that was
    // asked for WAKE_POLL_INTERVAL_MS but took ≥ WAKE_FREEZE_MIN_ELAPSED_MS
    // of wall clock means the tab was frozen — wake then. Normal ticks
    // (and hidden-tab throttle skew, ~60-90s for a 30s sleep) never wake,
    // so a quiet-but-alive feed no longer re-epochs (the 60s churn loop
    // behind the home count bounce).
    #[cfg(feature = "web")]
    {
        let fs = feed_state;
        let t = wake_trigger;
        let b = wake_last_bumped;
        let s = wake_unproductive_streak;
        let tb = wake_top_at_bump;
        use_future(move || async move {
            loop {
                let before_ms = crate::platform::timestamp::now_millis();
                crate::platform::timer::sleep_ms(WAKE_POLL_INTERVAL_MS).await;
                let elapsed_ms =
                    crate::platform::timestamp::now_millis().saturating_sub(before_ms);
                if poll_drift_indicates_freeze(elapsed_ms) {
                    log::info!(
                        "Wake poll: {}ms elapsed for a {}ms sleep — tab was frozen; checking staleness epoch",
                        elapsed_ms,
                        WAKE_POLL_INTERVAL_MS
                    );
                    attempt_wake_reload("freeze-detected-poll", fs, t, b, s, tb);
                }
            }
        });
    }

    #[cfg(feature = "web")]
    use_drop(move || {
        wake_listeners.set(None);
    });

    // Main feed loading effect
    use_effect(move || {
        let refresh = *refresh_trigger.read();
        // Tracked read: wake bumps (page return on a stale feed) re-run
        // this effect as a new unattended epoch — see the wake block above.
        let wake = *wake_trigger.read();
        let current_feed_type = feed_type.read().clone();
        let auth_state = auth_store::AUTH_STATE.read();
        let is_authenticated = auth_state.is_authenticated;
        let login_method = auth_state.login_method.clone();
        drop(auth_state);
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = *nostr_client::HAS_SIGNER.read();

        if !client_initialized {
            return;
        }
        // No identity (logged out): the pubkey-scoped feed load would only
        // produce NotAuthenticated. LoginSection renders instead of the feed,
        // and any login flips AUTH_STATE (read tracked above via
        // current_feed_type's auth reads / get_pubkey) to re-run this effect.
        // Read-only (npub) users have a pubkey and proceed as before.
        if auth_store::get_pubkey().is_none() {
            return;
        }
        // Relay readiness gate (canonical pattern, ref routes/dms.rs): the
        // signer attaches fast (local keys) while NIP-65 relay lists lag by
        // network round-trips — loading before USER_RELAYS_APPLIED streams
        // against DEFAULT relays, yielding incomplete Following feeds.
        // Tracked read re-runs this effect when the flag flips; typically ~ms
        // on boot via the disk-seed early flip (auth_store.rs).
        let user_relays_applied = *crate::stores::relay::USER_RELAYS_APPLIED.read();
        if has_signer && !user_relays_applied {
            return;
        }
        let requires_signer = login_method_requires_signer(login_method.as_ref());
        let signer_available = !requires_signer || has_signer;
        let cache_only = is_authenticated && requires_signer && !has_signer;
        let (
            last_refresh,
            last_feed,
            last_signer_available,
            last_relays_applied,
            last_wake,
            last_pubkey,
        ) = last_loaded_trigger.peek().clone();
        let (is_loading, has_data) = {
            let current_state = &*feed_state.peek();
            let loading = matches!(current_state, DataState::Loading);
            let data = matches!(
                current_state,
                DataState::Loaded(items)
                if !items.is_empty()
            );
            (loading, data)
        };
        let feed_type_changed = current_feed_type != last_feed;
        // Identity pin: a different viewer pubkey means the rendered feed
        // belongs to another account — the epoch must replace (never
        // merge-paint), and accept-time cache stores must abort.
        let current_pubkey = auth_store::get_pubkey().unwrap_or_default();
        let pubkey_changed = current_pubkey != last_pubkey;
        let refresh_changed = refresh != last_refresh;
        let signer_changed = signer_available != last_signer_available;
        // Relay readiness transitions (e.g. same-session logout→login flips
        // USER_RELAYS_APPLIED false→true) must force a reload — the
        // previously loaded feed belongs to the prior session's relay set.
        let readiness_changed = user_relays_applied != last_relays_applied;
        // Page-wake epoch: the user returned to a stale, long-idle page.
        // Same epoch semantics as an unattended load (newer-than-cache
        // items buffer into the banner; no skeleton flash since data is
        // already rendered) — see the wake block above.
        let wake_changed = wake != last_wake;
        if is_loading
            && !feed_type_changed
            && !refresh_changed
            && !signer_changed
            && !readiness_changed
            && !wake_changed
            && !pubkey_changed
        {
            log::debug!("Skipping feed re-load: already loading, no intentional change");
            return;
        }
        if has_data
            && !feed_type_changed
            && !refresh_changed
            && !signer_changed
            && !readiness_changed
            && !wake_changed
            && !pubkey_changed
        {
            log::debug!("Skipping feed re-load: data already present, no intentional change");
            return;
        }
        last_loaded_trigger.set((
            refresh,
            current_feed_type.clone(),
            signer_available,
            user_relays_applied,
            wake,
            current_pubkey.clone(),
        ));
        let token = stale.bump();
        // Epoch attribution for the shared cleanup logs below: every
        // epoch source funnels through the same teardown, and "manual
        // refresh" was misleading when a page wake or readiness reload
        // triggered it (observed in the wake-loop debugging).
        let reload_reason = if feed_type_changed {
            "feed type change"
        } else if refresh_changed {
            "manual refresh"
        } else if wake_changed {
            "page wake"
        } else if readiness_changed {
            "readiness reload"
        } else if pubkey_changed {
            "account switch"
        } else {
            "epoch reload"
        };
        if !has_data || feed_type_changed {
            feed_state.set(DataState::Loading);
            // The loading branch unmounts the sentinel: re-attach the
            // infinite-scroll observer to the node that mounts with the new
            // feed's data.
            feed_reset_generation += 1;
        }
        oldest_timestamp.set(None);
        backward_cursor.write().reset();
        relay_cursors.write().reset();
        // 3c-2: drop count anchors queued for the old feed so a flush
        // fired after the switch cannot fetch counts for it. (The nonce
        // is NOT bumped here: a debounce spawn already waiting on the
        // old feed's anchors takes the now-empty queue and no-ops.)
        pending_count_flush.write().clear();
        count_flush_failures.set(0);
        has_more.set(true);
        // The stale guard bumped above invalidates any in-flight pagination
        // task (which must not clear the flag itself when it discards results),
        // so release the flag here to keep the load_more guard usable for the
        // new feed.
        pagination_loading.set(false);

        // Cleanup existing subscriptions. `had_realtime` records whether any
        // live subscription work existed before this cleanup — the
        // `realtime_started` reset below is conditional on it (v3
        // double-start fix: the readiness reload used to reset the flag even
        // when a just-started post-flip build had subscribed nothing yet,
        // killing it via the second effect run's stale bump and rebuilding
        // from scratch — observed as two "Starting batched real-time
        // subscription" lines in the run-3 smoke).
        let ids = subscription_ids.peek().clone();
        let had_ids = !ids.is_empty();
        let had_guards = !realtime_guards.read().is_empty();
        {
            let guards: Vec<_> = realtime_guards.write().drain(..).collect();
            if !guards.is_empty() {
                spawn(async move {
                    for g in guards {
                        g.shutdown().await;
                    }
                });
            }
        }
        if !ids.is_empty() {
            let reason = reload_reason;
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    log::info!(
                        "Cleaning up {} real-time subscriptions due to {reason}",
                        ids.len()
                    );
                    subscription_manager::unsubscribe_all(&client, &ids).await;
                }
            });
        }
        let had_realtime = had_ids || had_guards;
        subscription_ids.write().clear();
        #[cfg(feature = "native")]
        {
            spawn(async move {
                let _ = crate::stores::ndb::subscriptions::unsubscribe(
                    crate::stores::ndb::subscriptions::SubKey::FollowingFeed,
                ).await;
            });
        }
        if let Some(handle) = interaction_stream_handle.peek().clone() {
            let reason = reload_reason;
            spawn(async move {
                log::info!("Cleaning up interaction stream due to {reason}");
                handle.unsubscribe().await;
            });
        }
        interaction_stream_handle.set(None);
        pending_posts.set(Vec::new());
        crate::hooks::use_viewport_engagement::clear_engaged();
        // Only reset the realtime flag when this cleanup actually tore
        // something down (see the v3 double-start note above). Feed-type
        // switches and manual refresh always have live subscriptions, so they
        // still reset + rebuild; logout/user-change resets are handled by the
        // separate unmount effect (see ~line 960). Wake epochs reset
        // unconditionally: the whole point is rebuilding realtime against
        // the revived pool — including the case where the previous session
        // half-started (contacts fetch failed, flag true, zero subs).
        if had_realtime || wake_changed {
            realtime_started.set(false);
        }
        let is_first_load = !*interactions_loaded.peek() || feed_type_changed;
        interactions_loaded.set(false);
        // Wake epochs get pool revival before their fetch (see the spawn
        // below); every epoch source shares this spawn.
        let is_wake_epoch = wake_changed;

        spawn(async move {
            if stale.is_stale(token) {
                log::debug!("Discarding stale feed request {}", token);
                return;
            }
            let is_stale = || stale.is_stale(token);

            // Wake epochs revive the pool before fetching: zombie sockets
            // (web: dead but still "Connected" — no WS pings on WASM) and
            // Idle/Sleeping members would otherwise starve the fetch's
            // connected-relays snapshot and the realtime rebuild. On
            // reconnect the SDK replays saved REQs, so revived relays
            // recover their backlog into the banner automatically.
            if is_wake_epoch {
                if let Some(client) = nostr_client::get_client() {
                    relay::connection::revive_stale_pool(&client).await;
                }
            }

            // Epoch source: only the refresh button bumps `refresh_trigger`,
            // so a changed trigger ⟺ user-initiated refresh, which inserts
            // fetched posts directly (the user asked for fresh content and
            // the button scrolls to top). Every unattended epoch — app
            // open, feed switch, readiness reload — buffers fetched items
            // newer than the painted cache into the pending-posts banner
            // instead, so the feed never jumps (issue 1).
            let buffer_new_items = !refresh_changed;

            // Base context for the shared network phase; each branch fills
            // in the cache-derived fields after painting its cache.
            let base_ctx = FeedPhaseCtx {
                feed_type,
                feed_state,
                pending_posts,
                feed_reset_generation,
                // `has_more` deliberately NOT in the phase ctx: the
                // network phase must never write it (pagination owns
                // settling it; epochs reset it true).
                oldest_timestamp,
                interaction_counts,
                interactions_loaded,
                interaction_stream_handle,
                stale,
                token,
                is_first_load,
                cache_top: None,
                cached_cursor: None,
                cached_count: 0,
                pending_count_flush,
                count_flush_nonce,
            };

            match current_feed_type {
                FeedType::Following => {
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let cache_key = cache_key_for_feed(&current_feed_type, &pubkey_str);
                    let (cached_items, cached_cursor, cached_count) = {
                        let items = feed_cache::load_cached_feed(&cache_key, FEED_LIMIT)
                            .await
                            .unwrap_or_default();
                        let cursor = feed_cache::load_feed_cursor(&cache_key).await;
                        let count = feed_cache::get_cached_item_count(&cache_key).await;
                        (items, cursor, count)
                    };
                    if is_stale() {
                        return;
                    }
                    // Paint the cache + derive the partition threshold
                    // (issue 1): fetched items strictly newer than the
                    // painted newest item buffer into the banner; `None`
                    // when the epoch was user-initiated via the refresh
                    // button (direct insert) or no cache was painted
                    // (first-ever load renders whatever arrives).
                    // Same-feed/same-account epochs MERGE with the
                    // rendered feed so accepted-but-unpersisted banner
                    // posts can never be replaced off the screen — see
                    // `paint_cached_items`.
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for Following feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                    } else {
                        log::info!("No cache, loading Following feed...");
                    }
                    let cache_top = paint_cached_items(
                        feed_state,
                        feed_reset_generation,
                        &cached_items,
                        feed_type_changed,
                        pubkey_changed,
                        has_data,
                        buffer_new_items,
                    );
                    if cache_only {
                        if cached_items.is_empty() {
                            log::info!("Phase 1 cache-only: no cached items for Following, waiting for signer restore");
                        } else {
                            log::info!(
                                "Phase 1 cache-only: showing {} cached items for Following while signer restores",
                                cached_items.len()
                            );
                        }
                        has_more.set(false);
                        return;
                    }
                    let phase_ctx = FeedPhaseCtx {
                        cache_top,
                        cached_cursor,
                        cached_count,
                        ..base_ctx
                    };
                    phase_with_settle(&current_feed_type, &phase_ctx).await;
                }
                FeedType::FollowingWithReplies => {
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let cache_key = cache_key_for_feed(&current_feed_type, &pubkey_str);
                    let (cached_items, cached_cursor, cached_count) = {
                        let items = feed_cache::load_cached_feed(&cache_key, FEED_LIMIT)
                            .await
                            .unwrap_or_default();
                        let cursor = feed_cache::load_feed_cursor(&cache_key).await;
                        let count = feed_cache::get_cached_item_count(&cache_key).await;
                        (items, cursor, count)
                    };
                    if is_stale() {
                        return;
                    }
                    // Paint + partition threshold — see `paint_cached_items`
                    // (merge-paint on same-feed/same-account epochs).
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for FollowingWithReplies feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                    }
                    let cache_top = paint_cached_items(
                        feed_state,
                        feed_reset_generation,
                        &cached_items,
                        feed_type_changed,
                        pubkey_changed,
                        has_data,
                        buffer_new_items,
                    );
                    if cache_only {
                        if cached_items.is_empty() {
                            log::info!("Phase 1 cache-only: no cached items for FollowingWithReplies, waiting for signer restore");
                        } else {
                            log::info!(
                                "Phase 1 cache-only: showing {} cached items for FollowingWithReplies while signer restores",
                                cached_items.len()
                            );
                        }
                        has_more.set(false);
                        return;
                    }
                    let phase_ctx = FeedPhaseCtx {
                        cache_top,
                        cached_cursor,
                        cached_count,
                        ..base_ctx
                    };
                    phase_with_settle(&current_feed_type, &phase_ctx).await;
                }
                FeedType::Global => {
                    let cache_key = cache_key_for_feed(&current_feed_type, "");
                    let (cached_items, cached_cursor, cached_count) = {
                        let items = feed_cache::load_cached_feed(&cache_key, FEED_LIMIT)
                            .await
                            .unwrap_or_default();
                        let cursor = feed_cache::load_feed_cursor(&cache_key).await;
                        let count = feed_cache::get_cached_item_count(&cache_key).await;
                        (items, cursor, count)
                    };
                    if is_stale() {
                        return;
                    }
                    // Paint + partition threshold — see `paint_cached_items`
                    // (merge-paint on same-feed/same-account epochs).
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for Global feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                    }
                    let cache_top = paint_cached_items(
                        feed_state,
                        feed_reset_generation,
                        &cached_items,
                        feed_type_changed,
                        pubkey_changed,
                        has_data,
                        buffer_new_items,
                    );
                    let phase_ctx = FeedPhaseCtx {
                        cache_top,
                        cached_cursor,
                        cached_count,
                        ..base_ctx
                    };
                    phase_with_settle(&current_feed_type, &phase_ctx).await;
                }
                FeedType::PeopleList(_) => {
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let cache_key = cache_key_for_feed(&current_feed_type, &pubkey_str);
                    let (cached_items, cached_cursor, cached_count) = {
                        let items = feed_cache::load_cached_feed(&cache_key, FEED_LIMIT)
                            .await
                            .unwrap_or_default();
                        let cursor = feed_cache::load_feed_cursor(&cache_key).await;
                        let count = feed_cache::get_cached_item_count(&cache_key).await;
                        (items, cursor, count)
                    };
                    if is_stale() {
                        return;
                    }
                    // Paint + partition threshold — see `paint_cached_items`
                    // (merge-paint on same-feed/same-account epochs).
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for PeopleList feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                    }
                    let cache_top = paint_cached_items(
                        feed_state,
                        feed_reset_generation,
                        &cached_items,
                        feed_type_changed,
                        pubkey_changed,
                        has_data,
                        buffer_new_items,
                    );
                    if cache_only {
                        if cached_items.is_empty() {
                            log::info!("Phase 1 cache-only: no cached items for PeopleList, waiting for signer restore");
                        } else {
                            log::info!(
                                "Phase 1 cache-only: showing {} cached items for PeopleList while signer restores",
                                cached_items.len()
                            );
                        }
                        has_more.set(false);
                        return;
                    }
                    let phase_ctx = FeedPhaseCtx {
                        cache_top,
                        cached_cursor,
                        cached_count,
                        ..base_ctx
                    };
                    phase_with_settle(&current_feed_type, &phase_ctx).await;
                }
                FeedType::RelayFeed { .. } | FeedType::RelaySetFeed { .. } => {
                    let cache_key = cache_key_for_feed(&current_feed_type, "");
                    let (cached_items, cached_cursor, cached_count) = {
                        let items = feed_cache::load_cached_feed(&cache_key, FEED_LIMIT)
                            .await
                            .unwrap_or_default();
                        let cursor = feed_cache::load_feed_cursor(&cache_key).await;
                        let count = feed_cache::get_cached_item_count(&cache_key).await;
                        (items, cursor, count)
                    };
                    if is_stale() {
                        return;
                    }
                    // Paint + partition threshold — see `paint_cached_items`
                    // (merge-paint on same-feed/same-account epochs).
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for RelayFeed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                    }
                    let cache_top = paint_cached_items(
                        feed_state,
                        feed_reset_generation,
                        &cached_items,
                        feed_type_changed,
                        pubkey_changed,
                        has_data,
                        buffer_new_items,
                    );
                    let phase_ctx = FeedPhaseCtx {
                        cache_top,
                        cached_cursor,
                        cached_count,
                        ..base_ctx
                    };
                    phase_with_settle(&current_feed_type, &phase_ctx).await;
                }
            }
        });
    });

    // Cleanup subscriptions on feed type change
    use_effect(move || {
        let _ = feed_type.read();
        let ids = subscription_ids.peek().clone();
        {
            let guards: Vec<_> = realtime_guards.write().drain(..).collect();
            if !guards.is_empty() {
                spawn(async move {
                    for g in guards {
                        g.shutdown().await;
                    }
                });
            }
        }
        if !ids.is_empty() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    log::info!(
                        "Cleaning up {} real-time subscriptions due to feed type change",
                        ids.len()
                    );
                    subscription_manager::unsubscribe_all(&client, &ids).await;
                }
            });
        }
        subscription_ids.write().clear();
        realtime_started.set(false);
        #[cfg(feature = "native")]
        {
            spawn(async move {
                let _ = crate::stores::ndb::subscriptions::unsubscribe(
                    crate::stores::ndb::subscriptions::SubKey::FollowingFeed,
                ).await;
            });
        }
        if let Some(handle) = interaction_stream_handle.peek().clone() {
            spawn(async move {
                log::info!("Cleaning up interaction stream due to feed type change");
                handle.unsubscribe().await;
            });
        }
        interaction_stream_handle.set(None);
        if let Some(sub_id) = relay_feed_sub_id.peek().clone() {
            let ephemeral_urls = relay_feed_ephemeral_urls.peek().clone();
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    let _ = client.unsubscribe(&sub_id).await;
                    for url in &ephemeral_urls {
                        let _ = client.remove_relay(url).force().await;
                    }
                }
            });
        }
        relay_feed_sub_id.set(None);
        relay_feed_ephemeral_urls.write().clear();
    });

    // Real-time subscriptions
    use_effect(move || {
        let current_feed_type = feed_type.read().clone();
        let auth_state = auth_store::AUTH_STATE.read();
        let is_authenticated = auth_state.is_authenticated;
        let login_method = auth_state.login_method.clone();
        drop(auth_state);
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = *nostr_client::HAS_SIGNER.read();

        if !client_initialized {
            return;
        }
        let requires_signer = login_method_requires_signer(login_method.as_ref());
        if is_authenticated && requires_signer && !has_signer {
            log::debug!("Waiting for signer restoration before starting realtime...");
            return;
        }
        // v2 boot-stall plan — canonical readiness gate (AGENTS.md "Signer &
        // Relay Readiness Gating"; same pattern as routes/dms.rs and
        // stores/ui/notifications.rs). Without it this effect fires on
        // signer restore, builds the 9-batch following subscription against
        // the pre-NIP-65 pool, and the intentional readiness reload tears it
        // all down and rebuilds (observed in the 98630372 smoke). The
        // signal is guaranteed to flip (disk-seed or the bounded 5s quorum
        // wait in run_post_login_init), and this reactive read re-runs the
        // effect when it does. Logged-out feeds proceed (has_signer false).
        let user_relays_applied = *crate::stores::relay::USER_RELAYS_APPLIED.read();
        if has_signer && !user_relays_applied {
            log::debug!("Waiting for user relays before starting realtime subscription");
            return;
        }
        if *realtime_started.read() {
            // Re-key on follow-set changes: same session, new authors. The
            // teardown below mirrors the feed-type-change effect; the
            // effect then falls through and rebuilds against the fresh
            // contact list.
            let contacts_version = *nostr_client::CONTACTS_VERSION.read();
            if *realtime_contacts_version.read() == Some(contacts_version) {
                return;
            }
            log::info!(
                "Contacts version changed ({}); re-keying realtime subscriptions",
                contacts_version
            );
            let mut rt_rekey_stale = realtime_stale;
            let _ = rt_rekey_stale.bump();
            {
                let guards: Vec<_> = realtime_guards.write().drain(..).collect();
                if !guards.is_empty() {
                    spawn(async move {
                        for g in guards {
                            g.shutdown().await;
                        }
                    });
                }
            }
            let ids = subscription_ids.peek().clone();
            if !ids.is_empty() {
                spawn(async move {
                    if let Some(client) = nostr_client::get_client() {
                        log::info!(
                            "Re-keying: closing {} real-time subscriptions for the new follow set",
                            ids.len()
                        );
                        subscription_manager::unsubscribe_all(&client, &ids).await;
                    }
                });
            }
            subscription_ids.write().clear();
            #[cfg(feature = "native")]
            {
                spawn(async move {
                    let _ = crate::stores::ndb::subscriptions::unsubscribe(
                        crate::stores::ndb::subscriptions::SubKey::FollowingFeed,
                    )
                    .await;
                });
            }
            realtime_started.set(false);
        }
        let since_timestamp = match &*feed_state.peek() {
            DataState::Loaded(ref items) => {
                items
                    .first()
                    .map(|i| i.sort_timestamp())
                    .unwrap_or_else(realtime_since_floor)
            }
            _ => realtime_since_floor(),
        };
        realtime_started.set(true);
        realtime_contacts_version
            .set(Some(*nostr_client::CONTACTS_VERSION.peek()));
        let rt_token = realtime_stale.bump();
        let rt_stale = realtime_stale;
        crate::platform::spawn::spawn_catch_unwind("home_feed", async move {
            if current_feed_type.is_relay_feed() {
                let urls = current_feed_type.relay_urls();
                let client = match nostr_client::get_client() {
                    Some(c) => c,
                    None => return,
                };
                for url in &urls {
                    let relay_url = match nostr::types::url::RelayUrl::parse(url) {
                        Ok(u) => u,
                        Err(_) => continue,
                    };
                    let relays = client.relays().await;
                    if !relays.contains_key(&relay_url) {
                        drop(relays);
                        let _ = client.add_relay(url).capabilities(nostr_sdk::relay::RelayCapabilities::READ).await;
                    }
                    let _ = client.connect_relay(url).await;
                }
                let parsed_urls: Vec<nostr::types::url::RelayUrl> = urls
                    .iter()
                    .filter_map(|u| nostr::types::url::RelayUrl::parse(u).ok())
                    .collect();
                let filter = Filter::new().kinds(feed_kinds()).since(since_timestamp);
                let sub_targets: Vec<_> = parsed_urls
                    .into_iter()
                    .map(|u| (u, vec![filter.clone()]))
                    .collect();
                let mut pending = pending_posts;
                let fstate = feed_state;
                let stale_check = rt_stale;
                let stale_token = rt_token;
                match subscription_manager::spawn_subscription(
                    client.clone(),
                    subscription_manager::SubTarget::Targeted(sub_targets),
                    None,
                    // signals are thread-local; same contract as use_relay_subscription
                    #[allow(clippy::arc_with_non_send_sync)]
                    std::sync::Arc::new(std::sync::Mutex::new(Box::new(move || {
                        stale_check.is_stale(stale_token)
                    }))),
                    #[allow(clippy::arc_with_non_send_sync)]
                    std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                        move |event: &nostr::event::Event| {
                            // Unified classifier, broadcast rules — identical
                            // to `load_relay_feed`'s classification (the
                            // pre-unification code here accepted ALL kind-1111
                            // comments, drifting from the loader's topic
                            // gate).
                            let feed_item_opt = classify_event(
                                event,
                                crate::feeds::classify::FeedRules::BROADCAST,
                            );
                            if let Some(feed_item) = feed_item_opt {
                                let event_id = feed_item.event().id;
                                let already_buffered = pending
                                    .read()
                                    .iter()
                                    .any(|item| item.event().id == event_id);
                                let already_in_feed = match &*fstate.peek() {
                                    DataState::Loaded(ref current_items) => current_items
                                        .iter()
                                        .any(|item| item.event().id == event_id),
                                    _ => false,
                                };
                                if !already_buffered && !already_in_feed {
                                    pending.write().push(feed_item);
                                }
                            } else if let Some(original_ref) =
                                crate::utils::repost::repost_original_ref(event)
                            {
                                // Same NIP-18 empty-content fallback as the
                                // other realtime handlers: hydrate by `e`
                                // tag and buffer when the original resolves.
                                if crate::feeds::repost_hydration::hydration_eligible(
                                    &original_ref,
                                ) {
                                    spawn_repost_hydration_buffer(
                                        event.clone(),
                                        original_ref,
                                        pending,
                                        fstate,
                                        stale_check,
                                        stale_token,
                                    );
                                }
                            }
                        },
                    ))),
                )
                .await
                {
                    Ok(sub) => {
                        log::info!(
                            "Relay feed real-time subscription: {:?}",
                            sub.sub_id()
                        );
                        relay_feed_sub_id.set(Some(sub.sub_id().clone()));
                        relay_feed_ephemeral_urls.write().extend(urls.iter().cloned());
                        subscription_ids.write().push(sub.sub_id().clone());
                        realtime_guards.write().push(sub);
                    }
                    Err(e) => {
                        log::error!("Failed to subscribe to relay feed: {}", e);
                    }
                }
                return;
            }

            // Global feed: one long-lived, authors-free subscription on the
            // READ pool so brand-new posts from anyone surface live via the
            // pending-posts pill. The contacts path below is authors-scoped
            // (kind 3) and can never deliver strangers' posts. Long-lived
            // subs survive relay reconnects (the pool re-issues saved REQs
            // on reconnect), and cleanup happens via `subscription_ids` on
            // feed switch or refresh.
            if matches!(current_feed_type, FeedType::Global) {
                let client = match nostr_client::get_client() {
                    Some(c) => c,
                    None => return,
                };
                let now_secs = Timestamp::now().as_secs();
                let since_secs = since_timestamp
                    .as_secs()
                    .max(now_secs.saturating_sub(GLOBAL_LIVE_SINCE_CLAMP_SECS));
                let filter = Filter::new()
                    .kinds(feed_kinds())
                    .since(Timestamp::from(since_secs))
                    .limit(0);
                let mut pending = pending_posts;
                let fstate = feed_state;
                let stale_check = rt_stale;
                let stale_token = rt_token;
                match subscription_manager::spawn_subscription(
                    client.clone(),
                    subscription_manager::SubTarget::Auto(filter),
                    None,
                    #[allow(clippy::arc_with_non_send_sync)]
                    std::sync::Arc::new(std::sync::Mutex::new(Box::new(move || {
                        stale_check.is_stale(stale_token)
                    }))),
                    #[allow(clippy::arc_with_non_send_sync)]
                    std::sync::Arc::new(std::sync::Mutex::new(Box::new(
                        move |event: &nostr::event::Event| {
                            // Unified classifier, broadcast rules — mirrors
                            // `load_global_feed`: all text notes (replies
                            // included); comments only as topic posts.
                            let feed_item_opt = classify_event(
                                event,
                                crate::feeds::classify::FeedRules::BROADCAST,
                            );
                            if let Some(feed_item) = feed_item_opt {
                                let event_id = feed_item.event().id;
                                let already_buffered = pending
                                    .read()
                                    .iter()
                                    .any(|item| item.event().id == event_id);
                                let already_in_feed = match &*fstate.peek() {
                                    DataState::Loaded(ref current_items) => current_items
                                        .iter()
                                        .any(|item| item.event().id == event_id),
                                    _ => false,
                                };
                                if !already_buffered && !already_in_feed {
                                    let author_pk = feed_item.event().pubkey.to_hex();
                                    spawn(async move {
                                        let _ = crate::stores::profiles::fetch_profile(
                                            author_pk,
                                        )
                                        .await;
                                    });
                                    if let FeedItem::Repost { ref original, .. } = feed_item {
                                        let original_author_pk = original.pubkey.to_hex();
                                        spawn(async move {
                                            let _ = crate::stores::profiles::fetch_profile(
                                                original_author_pk,
                                            )
                                            .await;
                                        });
                                    }
                                    pending.write().push(feed_item);
                                    log::info!(
                                        "Buffered new global post, total pending: {}",
                                        pending.read().len()
                                    );
                                }
                            } else if let Some(original_ref) =
                                crate::utils::repost::repost_original_ref(event)
                            {
                                // Same NIP-18 empty-content fallback as the
                                // Following handler: hydrate by `e` tag and
                                // buffer when the original resolves.
                                if crate::feeds::repost_hydration::hydration_eligible(
                                    &original_ref,
                                ) {
                                    spawn_repost_hydration_buffer(
                                        event.clone(),
                                        original_ref,
                                        pending,
                                        fstate,
                                        stale_check,
                                        stale_token,
                                    );
                                }
                            }
                        },
                    ))),
                )
                .await
                {
                    Ok(sub) => {
                        log::info!(
                            "Global feed real-time subscription: {:?}",
                            sub.sub_id()
                        );
                        subscription_ids.write().push(sub.sub_id().clone());
                        realtime_guards.write().push(sub);
                    }
                    Err(e) => {
                        log::error!("Failed to subscribe to global feed: {}", e);
                    }
                }
                return;
            }

            let pubkey_str = match auth_store::get_pubkey() {
                Some(pk) => pk,
                None => return,
            };
            let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Failed to fetch contacts for real-time subscription: {}", e);
                    return;
                }
            };
            if contacts.is_empty() {
                log::info!("No contacts to subscribe to for real-time updates");
                return;
            }
            let authors: Vec<PublicKey> = contacts
                .iter()
                .filter_map(|contact| PublicKey::parse(contact).ok())
                .collect();
            if authors.is_empty() {
                return;
            }
            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => return,
            };
            const BATCH_SIZE: usize = 50;
            const BATCH_DELAY_MS: u64 = 100;
            let total_authors = authors.len();
            let num_batches = total_authors.div_ceil(BATCH_SIZE);
            log::info!(
                "Starting batched real-time subscription for {} followed users in {} batches using gossip",
                contacts.len(), num_batches
            );

            {
                let sub_ids = subscription_ids;
                let mut pending = pending_posts;
                let fstate = feed_state;
                let ftype = current_feed_type.clone();
                let client_for_listener = client.clone();
                let rt_stale_inner = rt_stale;
                let rt_token_inner = rt_token;
                crate::platform::spawn::spawn_catch_unwind("home_follows", async move {
                    let mut notifications = client_for_listener.notifications();
                    loop {
                        if rt_stale_inner.is_stale(rt_token_inner) {
                            break;
                        }
                        // Stream end (client shut down / dropped) is a genuine
                        // termination signal; the NotificationStream handles
                        // lag internally.
                        let notification = match notifications.next().await {
                            Some(n) => n,
                            None => break,
                        };
                        if let nostr_sdk::client::ClientNotification::Event {
                            subscription_id: event_sub_id,
                            event,
                            ..
                        } = notification
                        {
                            let active_ids = sub_ids.read();
                            if !active_ids.contains(&event_sub_id) {
                                continue;
                            }
                            drop(active_ids);

                            // Unified classifier with this feed type's rules:
                            // the same predicate as the feed's initial load
                            // and pagination paths.
                            let feed_item_opt =
                                classify_event(&event, ftype.feed_rules());
                            if let Some(feed_item) = feed_item_opt {
                                log::info!("New post received in real-time");
                                let event_id = feed_item.event().id;
                                let already_buffered = pending
                                    .read()
                                    .iter()
                                    .any(|item| item.event().id == event_id);
                                let already_in_feed = match &*fstate.peek() {
                                    DataState::Loaded(ref current_items) => current_items
                                        .iter()
                                        .any(|item| item.event().id == event_id),
                                    _ => false,
                                };
                                if !already_buffered && !already_in_feed {
                                    let author_pk = feed_item.event().pubkey.to_hex();
                                    spawn(async move {
                                        let _ = crate::stores::profiles::fetch_profile(
                                            author_pk,
                                        )
                                        .await;
                                    });
                                    if let FeedItem::Repost { ref original, .. } = feed_item {
                                        let original_author_pk = original.pubkey.to_hex();
                                        spawn(async move {
                                            let _ = crate::stores::profiles::fetch_profile(
                                                original_author_pk,
                                            )
                                            .await;
                                        });
                                    }
                                    pending.write().push(feed_item);
                                    log::info!(
                                        "Buffered new post, total pending: {}",
                                        pending.read().len()
                                    );
                                }
                            } else if let Some(original_ref) =
                                crate::utils::repost::repost_original_ref(&event)
                            {
                                // NIP-18 empty-content repost (original
                                // referenced only by the required `e` tag):
                                // the classifier dropped it above, so the
                                // banner would never see it. Hydrate the
                                // original and buffer the reconstructed
                                // repost when it resolves.
                                if crate::feeds::repost_hydration::hydration_eligible(
                                    &original_ref,
                                ) {
                                    spawn_repost_hydration_buffer(
                                        *event,
                                        original_ref,
                                        pending,
                                        fstate,
                                        rt_stale_inner,
                                        rt_token_inner,
                                    );
                                }
                            }
                }
            }

            #[cfg(feature = "native")]
            {
                let ndb_authors: Vec<PublicKey> = contacts
                    .iter()
                    .filter_map(|c| PublicKey::parse(c).ok())
                    .collect();
                let ndb_filter = Filter::new()
                    .kinds(vec![Kind::TextNote, Kind::Repost, Kind::Comment])
                    .authors(ndb_authors)
                    .since(since_timestamp)
                    .limit(0);
                let filter_jsons = crate::stores::ndb::queries::sdk_filters_to_ndb_jsons(&[ndb_filter]);
                if let Err(e) = crate::stores::ndb::subscriptions::subscribe(
                    crate::stores::ndb::subscriptions::SubKey::FollowingFeed,
                    filter_jsons,
                ).await {
                    log::warn!("NDB subscription failed: {}", e);
                } else {
                    log::info!("NDB subscription active for following feed");
                }
            }
        });
            }

            // Once the relay pool hits its capacity cap (WASM only, via gossip
            // expansion), switch remaining batches to a targeted subscribe on
            // connected read relays. This guarantees every batch receives
            // realtime updates instead of being silently dropped when the pool
            // is full. Native is uncapped, so this never triggers there.
            let mut gossip_exhausted = false;
            for (batch_idx, author_batch) in authors.chunks(BATCH_SIZE).enumerate() {
                // Stale gate between batches: the feed-type-change effect
                // resets `realtime_started` and bumps the stale token while
                // this spawn may still be subscribing (login restores the
                // feed type mid-boot, re-arming the realtime effect).
                // Without this check a superseded spawn keeps opening REQs
                // and pushes sub ids into the already-drained signal — the
                // doubled "Batch N/9 subscribed" and relay
                // `too many subscriptions` symptoms.
                if rt_stale.is_stale(rt_token) {
                    log::debug!(
                        "home_follows: stale before batch {}/{}; aborting remaining batches",
                        batch_idx + 1,
                        num_batches
                    );
                    break;
                }
                let batch_authors = author_batch.to_vec();
                let client = client.clone();
                let batch_num = batch_idx + 1;
                if batch_idx > 0 {
                    let delay = (batch_idx as u32) * (BATCH_DELAY_MS as u32);
                    crate::platform::timer::sleep_ms(delay).await;
                }
                let filter = Filter::new()
                    .kinds(vec![Kind::TextNote, Kind::Repost, Kind::Comment])
                    .authors(batch_authors.clone())
                    .since(since_timestamp)
                    .limit(0);
                log::info!(
                    "Subscribing to batch {}/{} ({} authors){}",
                    batch_num,
                    num_batches,
                    batch_authors.len(),
                    if gossip_exhausted {
                        " [connected-relay fallback]"
                    } else {
                        ""
                    }
                );

                // Gossip path: per-author outbox routing (thorough). On a pool
                // cap hit, mark gossip exhausted and fall through to the
                // connected-relay fallback below.
                let gossip_result = if !gossip_exhausted {
                    match client.subscribe(filter.clone()).await {
                        ok @ Ok(_) => Some(ok),
                        Err(e) => {
                            // 0.45: SDK errors are opaque; the pool relay-cap
                            // error surfaces with the `LimitExceeded` kind.
                            if e.kind() == nostr_sdk::error::ErrorKind::LimitExceeded {
                                log::warn!(
                                    "Batch {}/{} hit relay pool cap; switching remaining batches to connected relays",
                                    batch_num,
                                    num_batches
                                );
                                gossip_exhausted = true;
                                None
                            } else if e.kind() == nostr_sdk::error::ErrorKind::NotFound {
                                // A relay resolved by the gossip breakdown was
                                // removed from the pool before the REQ landed
                                // (indexer reconcile, ephemeral cleanup, relay
                                // edits — observed: reconcile_indexer_pool
                                // removing relay.nos.social mid-build). SDK
                                // maps relay-not-found to ErrorKind::NotFound
                                // (error.rs relay_not_found_with_url). Reroute
                                // ONLY this batch to connected relays — unlike
                                // the pool-cap case, gossip stays usable for
                                // later batches.
                                log::warn!(
                                    "Batch {}/{} relay removed from pool mid-build; switching batch to connected relays: {}",
                                    batch_num,
                                    num_batches,
                                    e
                                );
                                None
                            } else {
                                log::error!(
                                    "Failed to subscribe batch {}/{}: {}",
                                    batch_num,
                                    num_batches,
                                    e
                                );
                                continue;
                            }
                        }
                    }
                } else {
                    None
                };

                let result = match gossip_result {
                    Some(ok) => ok,
                    None => {
                        let connected: Vec<nostr::types::url::RelayUrl> = client
                            .relays()
                            .with_capabilities(nostr_sdk::relay::RelayCapabilities::READ)
                            .await
                            .into_keys()
                            .collect();
                        if connected.is_empty() {
                            log::warn!(
                                "Batch {}/{} fallback skipped: no connected read relays",
                                batch_num,
                                num_batches
                            );
                            continue;
                        }
                        let fallback_targets: Vec<(
                            nostr::types::url::RelayUrl,
                            Vec<Filter>,
                        )> = connected
                            .into_iter()
                            .map(|u| (u, vec![filter.clone()]))
                            .collect();
                        // Pool membership shifts between the snapshot above and
                        // the REQ (login-time relay reconciliation removes/adds
                        // relays concurrently); one stale URL makes the whole
                        // targeted subscribe fail with `RelayNotFound` and would
                        // silently skip this batch's authors (observed: batches
                        // 4/9 + 5/9 lost in a live session). `subscribe_target`
                        // intersects with pool members and retries, degrading to
                        // pool-auto when nothing survives.
                        subscription_manager::subscribe_target(
                            &client,
                            subscription_manager::SubTarget::Targeted(fallback_targets),
                            None,
                        )
                        .await
                    }
                };

                match result {
                    Ok(output) => {
                        let subscription_id = output.value;
                        if rt_stale.is_stale(rt_token) {
                            // Stale between subscribe and bookkeeping: close
                            // the REQ just opened instead of leaking it into
                            // the drained id list.
                            log::debug!(
                                "home_follows: stale after batch {}/{} subscribe; closing {:?}",
                                batch_num,
                                num_batches,
                                subscription_id
                            );
                            let _ = client.unsubscribe(&subscription_id).await;
                            break;
                        }
                        log::info!(
                            "Batch {}/{} subscribed: {:?}",
                            batch_num,
                            num_batches,
                            subscription_id
                        );
                        subscription_ids.write().push(subscription_id);
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to subscribe batch {}/{}: {}",
                            batch_num,
                            num_batches,
                            e
                        );
                    }
                }
            }
        });
    });

    // NDB live events poller (native only) — drains events from nostrdb
    // subscriptions and adds matching new posts to pending_posts
    #[cfg(feature = "native")]
    {
        let mut ndb_pending = pending_posts;
        let fstate = feed_state;
        let ftype = feed_type;
        use_future(move || async move {
            loop {
                crate::platform::timer::sleep_ms(500).await;
                let events = crate::stores::ndb::drain_ndb_live_events();
                if events.is_empty() {
                    continue;
                }
                log::debug!("NDB live: {} new events", events.len());
                let existing_ids: HashSet<nostr::event::EventId> = match &*fstate.peek() {
                    DataState::Loaded(items) => items.iter().map(|i| i.event().id).collect(),
                    _ => HashSet::new(),
                };
                let buffered_ids: HashSet<nostr::event::EventId> = ndb_pending
                    .read()
                    .iter()
                    .map(|i| i.event().id)
                    .collect();
                for event in events {
                    if existing_ids.contains(&event.id) || buffered_ids.contains(&event.id) {
                        continue;
                    }
                    // Unified classifier with the live feed type's rules —
                    // same predicate as the loaders and relay-path realtime.
                    let feed_item =
                        classify_event(&event, ftype.read().feed_rules());
                    if let Some(item) = feed_item {
                        ndb_pending.write().push(item);
                    }
                }
            }
        });
    }

    // Scroll position tracking — polls window.scrollY into a Signal so
    // use_drop can read it synchronously (document::eval is async and
    // would return 0 because WebHistory::push already scrolled to top
    // before use_drop fires).
    let last_scroll_y: Signal<f64> = use_signal(|| 0.0);
    {
        let mut sy = last_scroll_y;
        use_future(move || async move {
            loop {
                crate::platform::timer::sleep_ms(200).await;
                let y = scroll_restore::get_scroll_y().await;
                if y > 0.0 {
                    sy.set(y);
                }
            }
        });
    }

    // Save scroll position on unmount — reads the tracked Signal synchronously
    use_drop(move || {
        let scroll_y = *last_scroll_y.read();
        if scroll_y > 0.0 {
            let mut anchor = scroll_restore::HOME_SCROLL_ANCHOR.write();
            anchor.scroll_y = scroll_y;
            anchor.is_set = true;
            anchor.feed_type_label = feed_type.read().label();
            log::debug!("Saved scroll position (sync): y={}", scroll_y);
        }

        let ids = subscription_ids.peek().clone();
        if !ids.is_empty() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    log::info!(
                        "Cleaning up {} real-time subscriptions on unmount",
                        ids.len()
                    );
                    subscription_manager::unsubscribe_all(&client, &ids).await;
                }
            });
        }
        #[cfg(feature = "native")]
        {
            spawn(async move {
                let _ = crate::stores::ndb::subscriptions::unsubscribe(
                    crate::stores::ndb::subscriptions::SubKey::FollowingFeed,
                )
                .await;
            });
        }
        if let Some(handle) = interaction_stream_handle.peek().clone() {
            spawn(async move {
                log::info!("Cleaning up interaction stream on unmount");
                handle.unsubscribe().await;
            });
        }
        if let Some(sub_id) = relay_feed_sub_id.peek().clone() {
            let ephemeral_urls = relay_feed_ephemeral_urls.peek().clone();
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    let _ = client.unsubscribe(&sub_id).await;
                    for url in &ephemeral_urls {
                        let _ = client.remove_relay(url).force().await;
                    }
                }
            });
        }
    });

    // Restore scroll position after feed content loads (only for popstate/back navigation)
    use_effect(move || {
        if let DataState::Loaded(items) = &*feed_state.read() {
            if !items.is_empty() {
                let anchor = scroll_restore::HOME_SCROLL_ANCHOR.read();
                if anchor.is_set {
                    let label = anchor.feed_type_label.clone();
                    let scroll_y = anchor.scroll_y;
                    drop(anchor);
                    let current_label = feed_type.read().label();
                    if label == current_label {
                        spawn(async move {
                            if scroll_restore::was_popstate_nav().await {
                                crate::platform::timer::sleep_ms(100).await;
                                scroll_restore::set_scroll_y(scroll_y).await;
                                log::debug!("Restored scroll position (popstate): y={}", scroll_y);
                            }
                            scroll_restore::HOME_SCROLL_ANCHOR.write().is_set = false;
                        });
                    }
                }
            }
        }
    });

    // Viewport-aware engagement (Phase 4)
    {
        use crate::hooks::use_viewport_engagement;
        let viewport_pending = pending_count_flush;
        let viewport_nonce = count_flush_nonce;
        use_future(move || {
            async move {
                // v3 render debounce: accumulate unengaged visible ids across
                // ticks and enqueue ONCE per ~1s window instead of one
                // fetch→extend→full-re-render per 300ms tick (the smokes
                // showed ~7 single-event extends at boot, each a full
                // ~72-card re-render — dioxus-signals 0.7.9 `extend` is one
                // write/notify per call). Ids are marked engaged at
                // COLLECTION time so ticks don't re-collect the same pending
                // ids; a failed batch is not retried (same accepted tradeoff
                // as the enqueue path's mark-before-queue, and the global
                // processor + live stream still cover the ids).
                //
                // 3c-4: the batch joins the idle-deferred count queue
                // (3c-2) instead of issuing its own shared-limits fetch —
                // the old path overlapped the pagination flush with a
                // 28-anchor/300-event burst mid-scroll.
                let mut pending: Vec<String> = Vec::new();
                let mut ticks_since_flush: u32 = 0;
                loop {
                    crate::platform::timer::sleep_ms(300).await;
                    ticks_since_flush += 1;
                    let engaged = use_viewport_engagement::ENGAGED_IDS.read().clone();
                    let script = r#"
                        const els = document.querySelectorAll('[data-event-id]');
                        const vh = window.innerHeight;
                        const results = [];
                        for (const el of els) {
                            const rect = el.getBoundingClientRect();
                            if (rect.bottom >= -200 && rect.top <= vh + 200) {
                                results.push(el.getAttribute('data-event-id'));
                            }
                            if (results.length >= 30) break;
                        }
                        return JSON.stringify(results);
                    "#;
                    let result = match document::eval(script).await {
                        Ok(v) => v.as_str().unwrap_or_default().to_string(),
                        Err(_) => continue,
                    };
                    let visible_ids: Vec<String> = match serde_json::from_str(&result) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let unengaged: Vec<String> = visible_ids
                        .into_iter()
                        .filter(|id| !engaged.contains(id))
                        .collect();
                    if !unengaged.is_empty() {
                        use_viewport_engagement::ENGAGED_IDS
                            .write()
                            .extend(unengaged.iter().cloned());
                        for id in unengaged {
                            if !pending.contains(&id) {
                                pending.push(id);
                            }
                        }
                    }
                    // ~1s window (3 ticks) or an early flush once a decent
                    // batch has accumulated (the JS caps visibility at 30).
                    if !pending.is_empty() && (ticks_since_flush >= 3 || pending.len() >= 20) {
                        let batch = std::mem::take(&mut pending);
                        ticks_since_flush = 0;
                        log::debug!(
                            "Viewport engagement: debounced enqueue for {} events",
                            batch.len()
                        );
                        use_viewport_engagement::enqueue_counts_for_visible(
                            batch,
                            viewport_pending,
                            viewport_nonce,
                        )
                        .await;
                    }
                }
            }
        });
    }

    // Pagination (load more)

    /// Phase-1 union page for any feed type: one global `until` for the
    /// whole relay union. Used for Global (DB-merge) and RelayFeed pages,
    /// and as the fallback when the Phase-2 per-relay path fails before
    /// issuing pages (no client, no connected relays, contacts failure —
    /// this loader owns the global-feed fallback chain).
    async fn union_page_for_feed(
        feed_type: &FeedType,
        until: Option<u64>,
    ) -> Result<FeedPage, NostrBlueError> {
        match feed_type {
            FeedType::Following => match load_following_feed(until, None, 0).await {
                Ok((page, false)) => Ok(page),
                Ok((_, true)) => Err(NostrBlueError::Other(
                    "Contact fetch failed during pagination".to_string(),
                )),
                Err(e) => Err(e),
            },
            FeedType::FollowingWithReplies => {
                match load_following_with_replies(until, None, 0).await {
                    Ok((page, false)) => Ok(page),
                    Ok((_, true)) => Err(NostrBlueError::Other(
                        "Contact fetch failed during pagination".to_string(),
                    )),
                    Err(e) => Err(e),
                }
            }
            FeedType::Global => load_paginated_global_feed(until).await,
            FeedType::PeopleList(list) => load_people_list_feed(list, until, None, 0).await,
            FeedType::RelayFeed { .. } | FeedType::RelaySetFeed { .. } => {
                load_relay_feed(feed_type.relay_urls(), until, None, 0).await
            }
        }
    }

    let load_more = move || {
        log::info!(
            "load_more called - pagination_loading: {}, has_more: {}",
            *pagination_loading.peek(),
            *has_more.peek()
        );
        if *pagination_loading.peek() || !*has_more.peek() {
            log::info!("load_more blocked by guards");
            return;
        }
        log::info!("load_more setting pagination_loading to true and spawning");
        pagination_loading.set(true);
        // Capture the stale-guard generation at request time. The main load
        // effect bumps it on feed-type switch / refresh; if that happens while
        // this fetch is in flight, the results below are discarded before any
        // signal write (merging old-feed items would corrupt the new feed's
        // list and cursor, and an empty old-feed page would kill `has_more`).
        let stale_guard = stale;
        let token = stale_guard.current();
        spawn(async move {
            let mut watchdog_loading = pagination_loading;
            let stale_watchdog = stale_guard;
            spawn(async move {
                crate::platform::timer::sleep_ms(30_000).await;
                // Only release the flag if this pagination request is still
                // current: after a feed switch the reset block already cleared
                // it, and a newer fetch may hold it true again.
                if !stale_watchdog.is_stale(token) && *watchdog_loading.peek() {
                    log::warn!("load_more timed out after 30s, resetting pagination_loading");
                    watchdog_loading.set(false);
                }
            });
            let until = *oldest_timestamp.read();
            let current_feed_type = feed_type.read().clone();
            log::info!(
                "load_more spawn executing - until: {:?}, feed_type: {:?}",
                until,
                current_feed_type
            );
            // ── Shared page-completion helpers ──────────────────────────────
            // Merge core shared by both pagination paths (Phase-1 union and
            // Phase-2 per-relay): dedup against the current feed, probe-
            // reload, and fan out metadata/interaction prefetch. An empty
            // items list is a no-op (a classified-away page).
            //
            // R2 chunked merge paints: a pagination page can land ~60 new
            // cards at once, and dioxus renders synchronously on the wasm
            // thread — one signal write with all of them is a multi-second
            // render turn (= main-thread stall). When the unique batch is
            // large, paint it in `PROGRESSIVE_FIRST_PAINT_CARDS` chunks
            // separated by a task turn (same pattern as the first-load
            // progressive paint), each chunk merging into whatever is live
            // so realtime items that land mid-paint are never clobbered.
            // Metadata/interaction prefetch fires immediately on the full
            // batch — only the paint is chunked.
            let mut merge_page_items = move |new_items: Vec<FeedItem>| {
                if new_items.is_empty() {
                    return;
                }
                let fetched_count = new_items.len();
                let current_state = feed_state.read().clone();
                if let DataState::Loaded(current) = current_state {
                    let (updated, unique_items, _merge_cursor) =
                        merge_paginated_feed_items(current.to_vec(), new_items);
                    log::info!(
                        "Deduplication: {} total, {} unique items after filtering",
                        fetched_count,
                        unique_items.len()
                    );
                    if unique_items.is_empty() {
                        return;
                    }
                    let prefetch_items = unique_items.clone();
                    spawn(async move {
                        prefetch_author_metadata(&prefetch_items).await;
                    });
                    // 3c-2: enqueue for the idle-deferred flush instead of
                    // fetching counts immediately. The old path fired a
                    // 300-event parse+verify+save backfill per page
                    // mid-scroll (MAIN THREAD STALL 2-4s, lag up to
                    // 6.5s/10s); the flush effect coalesces consecutive
                    // pages and fires once scrolling pauses.
                    let count_ids: Vec<EventId> =
                        unique_items.iter().map(|item| item.event().id).collect();
                    enqueue_count_anchors(&mut pending_count_flush, count_ids);
                    *count_flush_nonce.write() += 1;
                    if unique_items.len() < PROGRESSIVE_MIN_BATCH {
                        set_loaded_with_probe(&mut feed_state, feed_reset_generation,updated);
                        return;
                    }
                    // Chunked paint: first chunk synchronously (the user is
                    // staring at the sentinel), the rest across task turns
                    // with a paint opportunity between each write.
                    let mut remaining = unique_items;
                    let first_chunk: Vec<FeedItem> = {
                        let take = PROGRESSIVE_FIRST_PAINT_CARDS.min(remaining.len());
                        remaining.drain(..take).collect()
                    };
                    let merged_first = match &*feed_state.peek() {
                        DataState::Loaded(current) => {
                            feed_cache::merge_feed_items(current.to_vec(), first_chunk)
                        }
                        _ => first_chunk,
                    };
                    let first_count = merged_first.len();
                    {
                        let _span = crate::platform::perf::begin("feed_flush");
                        feed_state.set(DataState::Loaded(std::rc::Rc::new(merged_first)));
                    }
                    probe_render_turn(first_count);
                    if remaining.is_empty() {
                        return;
                    }
                    // Epoch captured before the head write: a feed switch /
                    // reset during the paint aborts the remaining chunks
                    // (same guard as the progressive-paint tail).
                    let generation_at_write = *feed_reset_generation.read();
                    let generation_signal = feed_reset_generation;
                    // Signals are Copy: hand the chunk task its own handle.
                    let mut fs = feed_state;
                    spawn(async move {
                        while !remaining.is_empty() {
                            crate::platform::timer::sleep_ms(0).await;
                            if *generation_signal.read() != generation_at_write {
                                log::debug!("Chunked paint aborted: feed epoch changed");
                                return;
                            }
                            let take = PROGRESSIVE_FIRST_PAINT_CARDS.min(remaining.len());
                            let chunk: Vec<FeedItem> = remaining.drain(..take).collect();
                            // Merge into whatever is live: realtime items may
                            // have landed between the turns.
                            let merged = match &*fs.peek() {
                                DataState::Loaded(current) => {
                                    feed_cache::merge_feed_items(current.to_vec(), chunk)
                                }
                                _ => chunk,
                            };
                            let count = merged.len();
                            {
                                let _span = crate::platform::perf::begin("feed_flush");
                                fs.set(DataState::Loaded(std::rc::Rc::new(merged)));
                            }
                            probe_render_turn(count);
                        }
                    });
                }
            };
            // Phase-1 union-page completion (also the per-relay fallback):
            // fold the global raw floor cursor, settle on non-advance.
            let mut finish_union_page = move |page: FeedPage, request_bound: Option<u64>| {
                // Fold the raw (pre-classification) floor FIRST: the
                // cursor must reflect what the relays actually delivered,
                // and a page with no event strictly older than the
                // running floor settles the window instead of looping.
                // This replaces the old gap-walk cursor, which re-derived
                // the same `until` forever once classification/dedup
                // opened a ≥1h gap inside the page boundary.
                let advanced = backward_cursor.write().fold_page(page.raw_floor, request_bound);
                let next_until = backward_cursor.read().next_until();
                if !advanced {
                    log::info!(
                        "Pagination window settled (floor: {:?}, bound: {:?}, oldest reached: {:?}) — nothing strictly older; ending pagination",
                        page.raw_floor,
                        request_bound,
                        backward_cursor.read().reached()
                    );
                    has_more.set(false);
                    pagination_loading.set(false);
                    return;
                }
                log::info!(
                    "Pagination advanced: floor {:?} -> next until {:?}",
                    page.raw_floor,
                    next_until
                );
                oldest_timestamp.set(next_until);
                merge_page_items(page.items);
                pagination_loading.set(false);
            };

            // ── Phase 2: per-relay planning for author feeds ───────────────
            // Following / WithReplies / PeopleList page over the connected
            // relay snapshot with one cursor per relay (Amethyst-style
            // `RelayLoadingCursors`); Global (DB-merge) and RelayFeed keep
            // the Phase-1 union cursor. Planning only READS the cursor
            // state — folding happens after the stale guard below.
            enum PerRelayPlan {
                NotApplicable,
                Settled,
                Fetch {
                    urls: Vec<nostr::types::url::RelayUrl>,
                    untils: Vec<(nostr::types::url::RelayUrl, Option<u64>)>,
                },
            }
            let plan = match &current_feed_type {
                FeedType::Following | FeedType::FollowingWithReplies | FeedType::PeopleList(_) => {
                    match nostr_client::get_client() {
                        Some(client) => {
                            let urls = nostr_client::connected_read_relay_urls(
                                &client,
                                "load_more_per_relay",
                            )
                            .await;
                            if urls.is_empty() {
                                // No connected relays (e.g. offline): the
                                // union loader's fallback chain applies.
                                PerRelayPlan::NotApplicable
                            } else {
                                // 3c-1 follow-up (R2 review): strike-out
                                // retirement is no longer permanent. The
                                // strike heuristic measures OUR page
                                // deadline, not relay liveness — a relay
                                // that EOSEs just past the deadline took
                                // one strike per page and was excluded for
                                // the whole session, silently dropping its
                                // (possibly unique) events. Re-admit
                                // retired relays after the cooldown; their
                                // `reached` floor is preserved so they
                                // resume from their own depth.
                                let re_armed = relay_cursors.write().rearm_retired(
                                    Timestamp::now().as_secs(),
                                    crate::utils::pagination::STALL_RETIRE_COOLDOWN_SECS,
                                );
                                if re_armed > 0 {
                                    log::info!(
                                        "Re-admitted {re_armed} strike-out retired relay(s) after cooldown"
                                    );
                                }
                                let snapshot = relay_cursors.read().clone();
                                let untils: Vec<_> = urls
                                    .iter()
                                    .filter(|url| !snapshot.is_done(url))
                                    .map(|url| (url.clone(), snapshot.until_for(url, until)))
                                    .collect();
                                if untils.is_empty() {
                                    PerRelayPlan::Settled
                                } else {
                                    PerRelayPlan::Fetch { urls, untils }
                                }
                            }
                        }
                        None => PerRelayPlan::NotApplicable,
                    }
                }
                _ => PerRelayPlan::NotApplicable,
            };

            enum PageFetch {
                Union(Result<FeedPage, NostrBlueError>),
                PerRelay(
                    Result<PerRelayPage, NostrBlueError>,
                    Vec<nostr::types::url::RelayUrl>,
                    Vec<(nostr::types::url::RelayUrl, Option<u64>)>,
                ),
                PreSettled,
            }
            let fetch: PageFetch = match plan {
                PerRelayPlan::NotApplicable => {
                    PageFetch::Union(union_page_for_feed(&current_feed_type, until).await)
                }
                PerRelayPlan::Settled => PageFetch::PreSettled,
                PerRelayPlan::Fetch { urls, untils } => {
                    let loaded = match &current_feed_type {
                        FeedType::Following => load_following_feed_per_relay(&untils).await,
                        FeedType::FollowingWithReplies => {
                            load_following_with_replies_per_relay(&untils).await
                        }
                        FeedType::PeopleList(list) => {
                            load_people_list_feed_per_relay(list, &untils).await
                        }
                        _ => unreachable!("per-relay plan is only built for author feeds"),
                    };
                    PageFetch::PerRelay(loaded, urls, untils)
                }
            };
            // Discard stale results before touching any signal (including
            // `pagination_loading` — the reset block owns clearing it after a
            // bump; clearing here could break a newer fetch's guard).
            if stale_guard.is_stale(token) {
                log::debug!(
                    "Discarding stale pagination result (token {}); feed switched mid-fetch",
                    token
                );
                return;
            }
            match fetch {
                PageFetch::PreSettled => {
                    // Every live relay bottomed out on an earlier page
                    // (e.g. the last folds finished them while items were
                    // still merged). Terminal until refresh/feed-switch.
                    log::info!(
                        "Per-relay pagination settled before page issue (all live relays done)"
                    );
                    has_more.set(false);
                    pagination_loading.set(false);
                }
                PageFetch::Union(result) => match result {
                    Ok(page) => finish_union_page(page, until),
                    Err(e) => {
                        log::error!("Failed to load more events: {}", e);
                        pagination_loading.set(false);
                    }
                },
                PageFetch::PerRelay(Ok(per_relay), urls, untils) => {
                    // Fold each relay's own page into its own cursor. The
                    // request bound per relay is the `until` THIS page
                    // carried for it (from the pre-fetch snapshot).
                    let bounds: std::collections::HashMap<
                        nostr::types::url::RelayUrl,
                        Option<u64>,
                    > = untils.into_iter().collect();
                    let mut any_advanced = false;
                    {
                        let mut cursors = relay_cursors.write();
                        // Prune relays that churned out of the pool while
                        // the page was in flight; the fold then (re)creates
                        // entries only for relays this page actually asked.
                        // R2 review: `urls` is already refusal-filtered
                        // (connected_read_relay_urls), but a refused relay
                        // never left the pool — pruning its cursor entry
                        // makes it re-arm at the GLOBAL bound once the
                        // refusal TTL lapses, re-fetching an already-paged
                        // window. Keep refused-but-connected relays' entries
                        // alive alongside the asked set (exhaustion checks
                        // below still consult only `urls`).
                        let mut live = urls.clone();
                        live.extend(
                            crate::stores::relay::refusal::recently_refused_urls(),
                        );
                        live.sort_unstable();
                        live.dedup();
                        cursors.retain_live(&live);
                        for (url, floor, settled, handshake_slow) in &per_relay.folds {
                            let bound = bounds.get(url).copied().flatten();
                            if cursors.fold_page(url, *floor, bound, *settled, *handshake_slow) {
                                any_advanced = true;
                            }
                        }
                    }
                    let deepest = relay_cursors.read().deepest_reached();
                    let exhausted = relay_cursors.read().exhausted(&urls);
                    let done_count = relay_cursors.read().done_count(&urls);
                    log::info!(
                        "Per-relay page folded: advanced={}, done={}/{} relays, deepest reached: {:?}, exhausted: {}",
                        any_advanced,
                        done_count,
                        urls.len(),
                        deepest,
                        exhausted
                    );
                    if deepest.is_some() {
                        oldest_timestamp.set(deepest);
                    }
                    if exhausted || per_relay.folds.is_empty() {
                        // All live relays answered empty or non-advancing
                        // (or the loader produced no folds, e.g. an empty
                        // people list): the window is settled.
                        log::info!(
                            "Per-relay pagination window settled — every live relay answered empty or non-advancing"
                        );
                        merge_page_items(per_relay.page.items);
                        has_more.set(false);
                        pagination_loading.set(false);
                    } else {
                        // An items-empty page is NOT exhaustion while any
                        // relay still advances (e.g. all replies on this
                        // page, classified away) — keep paging. Stalled
                        // relays also keep the window open: they are
                        // retried on the next sentinel trigger, so a
                        // network blip never settles the feed permanently.
                        merge_page_items(per_relay.page.items);
                        pagination_loading.set(false);
                    }
                }
                PageFetch::PerRelay(Err(e), _urls, _untils) => {
                    log::warn!(
                        "Per-relay pagination failed ({}); falling back to the union loader",
                        e
                    );
                    match union_page_for_feed(&current_feed_type, until).await {
                        Ok(page) => {
                            // Second await after the stale check: re-check
                            // before the fallback's signal writes.
                            if stale_guard.is_stale(token) {
                                log::debug!(
                                    "Discarding stale pagination fallback (token {})",
                                    token
                                );
                                return;
                            }
                            finish_union_page(page, until);
                        }
                        Err(e2) => {
                            log::error!("Failed to load more events: {}", e2);
                            pagination_loading.set(false);
                        }
                    }
                }
            }
        });
    };

    let sentinel_id =
        use_infinite_scroll_with_generation(load_more, has_more, pagination_loading, feed_reset_generation);

    // 3c-2: flush the idle-deferred pagination count queue. Every page
    // merge bumps the nonce; the effect re-runs per bump and its spawn
    // waits out the debounce window. A superseded spawn (a newer enqueue
    // landed meanwhile) exits, so only the newest spawn fires — a
    // trailing debounce that keeps deferring while pages keep arriving
    // and flushes once scrolling pauses. Same pattern as the profile
    // queue drain in `routes/mod.rs`.
    let flush_nonce = *count_flush_nonce.read();
    use_effect(use_reactive(
        &flush_nonce,
        move |nonce: u64| {
            let mut nonce_signal = count_flush_nonce;
            let mut counts_signal = interaction_counts;
            let mut pending = pending_count_flush;
            let mut failures_signal = count_flush_failures;
            spawn(async move {
                crate::platform::timer::sleep_ms(COUNT_FLUSH_DEBOUNCE_MS).await;
                if *nonce_signal.read() != nonce {
                    // Superseded: a newer enqueue's spawn owns the flush.
                    return;
                }
                // Single-flight (3c-6): acquire BEFORE reading the queue so
                // take-and-drain is atomic w.r.t. other rounds. Busy → a
                // round is already in flight; its completion re-arm covers
                // everything enqueued since, so just stand down.
                let Ok(round_permit) = COUNT_FLUSH_GATE.try_acquire() else {
                    return;
                };
                let ids: Vec<EventId> = {
                    let queue = pending.read();
                    let take = queue.len().min(COUNT_FLUSH_MAX_ANCHORS);
                    queue.iter().take(take).cloned().collect()
                };
                if ids.is_empty() {
                    return;
                }
                let round = crate::services::aggregation::flush_paginated_interaction_counts(
                    ids.clone(),
                    std::time::Duration::from_secs(5),
                )
                .await;
                match round {
                    Ok(new_counts) => {
                        failures_signal.set(0);
                        // Drain-on-success: the taken anchors' fetch
                        // completed (counts found or verified absent), so
                        // remove exactly those from the queue. Without this
                        // the 3c-4 re-arm below sees them still queued and
                        // re-flushes the same anchors every debounce window
                        // forever (observed on Android: identical
                        // "Flushing coalesced pagination counts for 3
                        // events" at 1.5-2s intervals, cache-hit rounds).
                        {
                            let taken: std::collections::HashSet<EventId> =
                                ids.iter().cloned().collect();
                            let mut queue = pending.write();
                            queue.retain(|id| !taken.contains(id));
                        }
                        if !new_counts.is_empty() {
                            // Monotonic merge, not extend: HashMap::extend
                            // replaces per-anchor entries, so a truncated
                            // (`complete: false`) round repaints lagging
                            // L2/DB baselines over live-incremented
                            // counters (the count-flicker regression the
                            // stream path already guards against).
                            let mut map = counts_signal.write();
                            crate::services::aggregation::merge_counts_monotonic(
                                &mut map, new_counts,
                            );
                        }
                    }
                    Err(e) => {
                        // Error round: anchors stay queued and the re-arm
                        // below retries them — after the failure backoff.
                        *failures_signal.write() += 1;
                        log::warn!("Coalesced pagination count flush failed: {}", e)
                    }
                }
                // Bounded rounds (3c-4): a fling can queue far more anchors
                // than one bounded fetch should carry. Whatever remains
                // (beyond this round's cap, or errored) re-arms the debounce
                // for the next round; nothing is dropped.
                if !pending.read().is_empty() {
                    let failures = *failures_signal.read();
                    if failures > 0 {
                        // R2 review: exponential backoff on consecutive
                        // failures, applied AFTER releasing the round
                        // permit — sleeping with the gate held would park
                        // every other count path (micro path, shared
                        // entry) behind a doomed 5s retry loop.
                        drop(round_permit);
                        let backoff = COUNT_FLUSH_DEBOUNCE_MS
                            .saturating_mul(1u32 << failures.min(4))
                            .min(COUNT_FLUSH_MAX_BACKOFF_MS);
                        crate::platform::timer::sleep_ms(backoff).await;
                        // A newer enqueue's spawn may own the flush now —
                        // only re-arm if this round is still current.
                        if *nonce_signal.read() == nonce {
                            *nonce_signal.write() += 1;
                        }
                        return;
                    }
                    *nonce_signal.write() += 1;
                }
            });
        },
    ));

    use_effect(move || {
        let optimistic = feed_cache::OPTIMISTIC_FEED_INSERTS.read().clone();
        if optimistic.is_empty() {
            return;
        }
        // Take only what the notes feed renders; leave kind 30023 articles
        // queued for the articles feed's drain effect.
        let drained = feed_cache::drain_optimistic_feed_items_matching(|item| {
            !matches!(item, FeedItem::OriginalPost(e) if e.kind == Kind::LongFormTextNote)
        });
        if !drained.is_empty() {
            pending_posts.write().extend(drained);
        }
    });

    let mut refresh_and_scroll_to_top = move || {
        let current = *refresh_trigger.read();
        refresh_trigger.set(current + 1);
        spawn(async move {
            scroll_restore::set_scroll_y(0.0).await;
        });
    };

    let mut accept_pending_posts = move || {
        // Loaded-check BEFORE the drain: accepting while a load epoch
        // holds the feed in Loading/Pending would drain the banner and
        // silently discard the batch — those posts exist nowhere else
        // yet. Leave the banner intact instead; the epoch's fetch
        // re-delivers and the user can accept again.
        let current = match feed_state.read().clone() {
            DataState::Loaded(items) => items.to_vec(),
            _ => return,
        };
        let pending: Vec<FeedItem> = pending_posts.write().drain(..).collect();
        if pending.is_empty() {
            return;
        }
        // Incremental merge (plan wave B1): dioxus signals re-render on
        // every write (no equality short-circuit), so skip the write
        // entirely when every pending post was already in the feed — and
        // cap the live list, dropping the oldest tail only here (the user
        // is at the top by definition of accepting the pill), never during
        // pagination.
        let mut list = crate::feeds::list::FeedList::from_sorted_items(current.clone());
        let landed_items = list.insert_batch_returning(pending);
        if landed_items.is_empty() {
            return;
        }
        // Lazy counts for accepted banner items (composition-driven, the
        // Amethyst model): direct-extend the idle-deferred bounded count
        // queue — NOT enqueue_counts_for_visible, whose >=30 skip gate
        // would drop exactly these large accept backlogs — so cards paint
        // instantly from local-DB counts (use_global_interaction) and
        // accurate numbers arrive after the debounce window.
        {
            let mut queue = pending_count_flush.write();
            for item in &landed_items {
                if !queue.contains(&item.event().id) {
                    queue.push(item.event().id);
                }
            }
        }
        *count_flush_nonce.write() += 1;
        spawn(async move {
            scroll_restore::set_scroll_y(0.0).await;
        });
        // Persist the accepted batch (the vanish-and-rebuffer fix):
        // posts accepted after the epoch's last cache store exist only in
        // memory, so the NEXT unattended epoch's cache paint would
        // replace them off the screen and its re-fetch would feed them
        // back into the banner. Debounced like the phase store (one idle
        // beat), identity-guarded: a feed/account switch during the
        // window aborts, and the key was captured at accept time so the
        // store can never cross feeds.
        {
            let feed_type_at_accept = feed_type.read().clone();
            let pubkey_at_accept = auth_store::get_pubkey().unwrap_or_default();
            let cache_key = cache_key_for_feed(&feed_type_at_accept, &pubkey_at_accept);
            let generation_at_accept = *feed_reset_generation.read();
            let store_fs = feed_state;
            let store_pending = pending_posts;
            let generation_signal = feed_reset_generation;
            let feed_type_signal = feed_type;
            spawn(async move {
                crate::platform::timer::sleep_ms(1500).await;
                if *generation_signal.read() != generation_at_accept
                    || *feed_type_signal.read() != feed_type_at_accept
                    || auth_store::get_pubkey().unwrap_or_default() != pubkey_at_accept
                {
                    log::debug!("Accept-store skipped: feed epoch or account changed");
                    return;
                }
                // Snapshot live at fire time: rendered now contains the
                // accepted posts, and anything realtime buffered since is
                // covered too (the same union the phase store persists).
                let snapshot = cache_store_snapshot(store_fs, store_pending, Vec::new());
                if let Err(e) = feed_cache::store_feed_items(&cache_key, &snapshot).await {
                    log::warn!("Failed to store accepted posts to cache: {}", e);
                }
                if let Err(e) = feed_cache::run_eviction_if_needed().await {
                    log::warn!("Failed to run cache eviction: {}", e);
                }
            });
        }
        // Chunked paint: a stale-cache reopen can buffer hundreds of items
        // in the banner, and one signal write = one synchronous render
        // turn (the multi-second stall pattern documented on
        // PROGRESSIVE_FIRST_PAINT_CARDS). Small batches render in one
        // write; large ones paint the first chunk immediately and land the
        // rest across task turns, each merging into whatever is live.
        if landed_items.len() < PROGRESSIVE_MIN_BATCH {
            list.trim_oldest(crate::feeds::list::DEFAULT_FEED_CAP);
            set_loaded_with_probe(&mut feed_state, feed_reset_generation, list.into_items());
            return;
        }
        let mut remaining = landed_items;
        let first_take = PROGRESSIVE_FIRST_PAINT_CARDS.min(remaining.len());
        let first_chunk: Vec<FeedItem> = remaining.drain(..first_take).collect();
        let mut first_list = crate::feeds::list::FeedList::from_sorted_items(current);
        first_list.insert_batch(first_chunk);
        first_list.trim_oldest(crate::feeds::list::DEFAULT_FEED_CAP);
        let first_count = first_list.len();
        set_loaded_with_probe(
            &mut feed_state,
            feed_reset_generation,
            first_list.into_items(),
        );
        probe_render_turn(first_count);
        if remaining.is_empty() {
            return;
        }
        // Epoch captured before the first write: a feed switch / refresh
        // during the paint aborts the remaining chunks (same guard as the
        // pagination chunked paint).
        let generation_at_write = *feed_reset_generation.read();
        let generation_signal = feed_reset_generation;
        let mut fs = feed_state;
        spawn(async move {
            while !remaining.is_empty() {
                crate::platform::timer::sleep_ms(0).await;
                if *generation_signal.read() != generation_at_write {
                    log::debug!("Accept-paint aborted: feed epoch changed");
                    return;
                }
                let take = PROGRESSIVE_FIRST_PAINT_CARDS.min(remaining.len());
                let chunk: Vec<FeedItem> = remaining.drain(..take).collect();
                let merged = match &*fs.peek() {
                    DataState::Loaded(current) => {
                        let mut list = crate::feeds::list::FeedList::from_sorted_items(
                            current.to_vec(),
                        );
                        list.insert_batch(chunk);
                        list.trim_oldest(crate::feeds::list::DEFAULT_FEED_CAP);
                        list.into_items()
                    }
                    _ => chunk,
                };
                let count = merged.len();
                {
                    let _span = crate::platform::perf::begin("feed_flush");
                    fs.set(DataState::Loaded(Rc::new(merged)));
                }
                probe_render_turn(count);
            }
        });
    };

    let auth = auth_store::AUTH_STATE.read();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    if auth.is_authenticated {
                        div { class: "relative",
                            button {
                                class: "text-xl font-bold flex items-center gap-2 hover:bg-accent px-3 py-1 rounded-lg transition",
                                onclick: move |_| {
                                    let current = *show_dropdown.read();
                                    show_dropdown.set(!current);
                                },
                                "{feed_type.read().label()}"
                                span { class: "text-sm",
                                    if *show_dropdown.read() {
                                        "▲"
                                    } else {
                                        "▼"
                                    }
                                }
                            }
                            if *show_dropdown.read() {
                                div { class: "absolute top-full left-0 mt-2 bg-card border border-border rounded-lg shadow-lg min-w-[200px] overflow-hidden z-30",
                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::Following);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div { class: "font-medium", "Following" }
                                            div { class: "text-xs text-muted-foreground",
                                                "Top level posts only"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::Following {
                                            span { "✓" }
                                        }
                                    }
                                    div { class: "border-t border-border" }
                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::FollowingWithReplies);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div { class: "font-medium", "Following + Replies" }
                                            div { class: "text-xs text-muted-foreground",
                                                "All posts including replies"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::FollowingWithReplies {
                                            span { "✓" }
                                        }
                                    }
                                    div { class: "border-t border-border" }
                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::Global);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div { class: "font-medium", "Global" }
                                            div { class: "text-xs text-muted-foreground",
                                                "Posts from everyone"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::Global {
                                            span { "✓" }
                                        }
                                    }
                                    if !people_lists.read().is_empty() {
                                        div { class: "border-t border-border" }
                                        div { class: "px-4 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wide",
                                            "Your Lists"
                                        }
                                        for list in people_lists.read().iter() {
                                            {
                                                let list_for_select = list.clone();
                                                let list_for_check = list.clone();
                                                let display_count = match list.total_member_count {
                                                    Some(count) => count.to_string(),
                                                    None if list.has_private_content => {
                                                        format!("{}+", get_item_count(&list.tags))
                                                    }
                                                    None => get_item_count(&list.tags).to_string(),
                                                };
                                                rsx! {
                                                    button {
                                                        key: "{list.id}",
                                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                                        onclick: move |_| {
                                                            feed_type.set(FeedType::PeopleList(Box::new(list_for_select.clone())));
                                                            show_dropdown.set(false);
                                                        },
                                                        div {
                                                            div { class: "font-medium flex items-center gap-2",
                                                                "👥 {list.name}"
                                                                if list.has_private_content {
                                                                    span { class: "text-sm", title: "Has private members", "🔒" }
                                                                }
                                                            }
                                                            div { class: "text-xs text-muted-foreground", "{display_count} members" }
                                                        }
                                                        if matches!(
                                                            feed_type.read().clone(),
                                                            FeedType::PeopleList(ref l)
                                                            if l.id == list_for_check.id
                                                        )
                                                        {
                                                            span { "✓" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    {
                                        let favorite_relays = {
                                            use dioxus::prelude::ReadableExt;
                                            let relays = relay::nip65::FAVORITE_RELAYS.peek().clone();
                                            if relays.is_empty() {
                                                relay::nip65::default_favorite_relays()
                                            } else {
                                                relays
                                            }
                                        };
                                        let rlists = relay_lists.read().clone();
                                        let has_relay_entries = !favorite_relays.is_empty() || !rlists.is_empty();
                                        rsx! {
                                            if has_relay_entries {
                                                div { class: "border-t border-border" }
                                                div { class: "px-4 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wide",
                                                    "Relay Feeds"
                                                }
                                            }
                                            for url in &favorite_relays {
                                                {
                                                    let url_for_click = url.clone();
                                                    let url_for_check = url.clone();
                                                    let domain = url
                                                        .trim_start_matches("wss://")
                                                        .trim_start_matches("ws://")
                                                        .trim_end_matches('/')
                                                        .to_string();
                                                    rsx! {
                                                        button {
                                                            key: "fav-{url}",
                                                            class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                                            onclick: move |_| {
                                                                feed_type.set(FeedType::RelayFeed {
                                                                    url: url_for_click.clone(),
                                                                    name: domain.clone(),
                                                                });
                                                                show_dropdown.set(false);
                                                            },
                                                            div {
                                                                div { class: "font-medium", "📡 {domain}" }
                                                                div { class: "text-xs text-muted-foreground truncate", "{url}" }
                                                            }
                                                            if matches!(
                                                                feed_type.read().clone(),
                                                                FeedType::RelayFeed { ref url, .. } if url == &url_for_check
                                                            ) {
                                                                span { "✓" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            for list in rlists.iter() {
                                                {
                                                    let list_relay_urls: Vec<String> = list.tags.iter()
                                                        .filter_map(|tag| {
                                                            let s = tag.as_slice();
                                                            if s.first().map(|v| v.as_str()) == Some("relay") {
                                                                s.get(1).map(|v| v.to_string())
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                        .collect();
                                                    let list_name_for_set = list.name.clone();
                                                    let list_name_for_check = list.name.clone();
                                                    let list_urls_for_set = list_relay_urls.clone();
                                                    let list_urls_for_check = list_relay_urls.clone();
                                                    let relay_count = list_relay_urls.len();
                                                    rsx! {
                                                        button {
                                                            key: "rlist-{list.id}",
                                                            class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                                            onclick: move |_| {
                                                                if !list_urls_for_set.is_empty() {
                                                                    feed_type.set(FeedType::RelaySetFeed {
                                                                        name: list_name_for_set.clone(),
                                                                        urls: list_urls_for_set.clone(),
                                                                    });
                                                                    show_dropdown.set(false);
                                                                }
                                                            },
                                                            div {
                                                                div { class: "font-medium", "📡 {list.name}" }
                                                                div { class: "text-xs text-muted-foreground", "{relay_count} relays" }
                                                            }
                                                            if matches!(
                                                                feed_type.read().clone(),
                                                                FeedType::RelaySetFeed { ref name, ref urls, .. }
                                                                if name == &list_name_for_check && urls == &list_urls_for_check
                                                            ) {
                                                                span { "✓" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            {
                                                let mut input_url = relay_url_input;
                                                rsx! {
                                                    div { class: "border-t border-border" }
                                                    div { class: "px-4 py-2 flex gap-2",
                                                        input {
                                                            class: "flex-1 text-sm px-3 py-2 bg-muted border border-border rounded-lg focus:outline-none focus:ring-1 focus:ring-primary",
                                                            r#type: "text",
                                                            placeholder: "wss://relay.example.com",
                                                            value: "{input_url}",
                                                            oninput: move |e| {
                                                                input_url.set(e.value());
                                                            },
                                                        }
                                                        button {
                                                            class: "px-3 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                                            onclick: move |_| {
                                                                let url = input_url.read().clone();
                                                                if !url.is_empty() {
                                                                    let domain = url
                                                                        .trim_start_matches("wss://")
                                                                        .trim_start_matches("ws://")
                                                                        .trim_end_matches('/')
                                                                        .to_string();
                                                                    feed_type.set(FeedType::RelayFeed {
                                                                        url: url.clone(),
                                                                        name: domain,
                                                                    });
                                                                    show_dropdown.set(false);
                                                                    input_url.set(String::new());
                                                                }
                                                            },
                                                            "Go"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        h2 { class: "text-xl font-bold", "Home" }
                    }
                    if auth.is_authenticated {
                        button {
                            class: "p-2 hover:bg-accent rounded-lg transition disabled:opacity-50",
                            disabled: feed_state.read().is_loading(),
                            onclick: move |_| refresh_and_scroll_to_top(),
                            title: "Refresh feed",
                            if feed_state.read().is_loading() {
                                span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                            } else {
                                "🔄"
                            }
                        }
                    }
                }
            }
            if auth.is_authenticated {
                NoteComposer { mode: NoteMode::Inline }
            }
            if !auth.is_authenticated {
                div { class: "border-b border-border p-6 bg-blue-50 dark:bg-blue-900/20",
                    div { class: "max-w-md mx-auto text-center",
                        h3 { class: "text-lg font-bold mb-2", "Welcome to nostr.blue" }
                        p { class: "text-muted-foreground mb-4",
                            "Connect your account to see your feed"
                        }
                    }
                }
            }
            div {
                if !auth.is_authenticated {
                    LoginSection {}
                } else if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if feed_state.read().is_pending() || feed_state.read().is_loading() {
                    div { class: "divide-y divide-border",
                        for i in 0..5 {
                            NoteCardSkeleton { key: "{i}" }
                        }
                    }
                } else if let Some(err) = feed_state.read().error() {
                    div { class: "p-6 text-center",
                        div { class: "max-w-md mx-auto",
                            div { class: "text-4xl mb-2", "⚠️" }
                            p { class: "text-red-600 dark:text-red-400", "Error loading feed: {err}" }
                        }
                    }
                } else if let Some(feed_items) = feed_state.read().data() {
                    if feed_items.is_empty() {
                        div { class: "p-6 text-center text-gray-500 dark:text-gray-400",
                            div { class: "max-w-md mx-auto space-y-4",
                                div { class: "text-4xl mb-2", "📝" }
                                h3 { class: "text-lg font-semibold text-gray-700 dark:text-gray-300",
                                    "No posts yet"
                                }
                                p { class: "text-sm", "Posts from the network will appear here" }
                            }
                        }
                    } else {
                        if *pending_count.read() > 0 {
                            {
                                let count = *pending_count.read();
                                let post_text = if count == 1 { "post" } else { "posts" };
                                rsx! {
                                    div {
                                        class: "sticky top-[57px] z-10 border-b border-border bg-blue-500 hover:bg-blue-600 transition-colors cursor-pointer",
                                        onclick: move |_| accept_pending_posts(),
                                        div { class: "px-4 py-3 text-center",
                                            span { class: "text-white font-medium", "Show {count} new {post_text}" }
                                        }
                                    }
                                }
                            }
                        }
                        // Isolated list body (plan wave B2 + R3a): the
                        // counts signal is passed through unread — each
                        // NoteCard's counts-sync leaf subscribes to it
                        // directly, so live interaction updates re-render
                        // only those tiny leaves, not this subtree's card
                        // bodies.
                        FeedListBody {
                            items: feed_items.clone(),
                            counts_signal: interaction_counts,
                            cached_muted_posts: cached_muted_posts.read().clone(),
                            cached_blocked_users: cached_blocked_users.read().clone(),
                        }
                        if *has_more.read() {
                            div {
                                id: "{sentinel_id}",
                                class: "p-8 flex justify-center",
                                if *pagination_loading.read() {
                                    span { class: "flex items-center gap-2 text-muted-foreground",
                                        span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                        "Loading more..."
                                    }
                                }
                            }
                        } else {
                            div { class: "p-8 text-center text-muted-foreground",
                                "You've reached the end"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Normal visible-tab ticks (~30s) and hidden-tab throttle skew
    /// (~60-90s for a 30s sleep) must never fire a freeze wake — only a
    /// genuinely suspended tab exceeds the threshold.
    #[test]
    fn poll_drift_ignores_normal_ticks_and_hidden_throttle() {
        assert!(!poll_drift_indicates_freeze(WAKE_POLL_INTERVAL_MS as u64));
        assert!(!poll_drift_indicates_freeze(60_000));
        assert!(!poll_drift_indicates_freeze(90_000));
        assert!(!poll_drift_indicates_freeze(150_000));
    }

    /// A sleep that took ≥ WAKE_FREEZE_MIN_ELAPSED_MS of wall clock is
    /// proof the tab was frozen (JS timers do not fire while suspended).
    #[test]
    fn poll_drift_wakes_on_frozen_tab() {
        assert!(poll_drift_indicates_freeze(WAKE_FREEZE_MIN_ELAPSED_MS));
        assert!(poll_drift_indicates_freeze(WAKE_FREEZE_MIN_ELAPSED_MS + 1));
        assert!(poll_drift_indicates_freeze(20 * 60_000));
    }

    #[test]
    fn wake_reload_skips_hidden_document() {
        // Returning is the trigger; a still-hidden page must not epoch.
        assert!(!should_wake_reload(
            true,
            Some(WAKE_FEED_STALE_SECS * 10),
            10_000,
            0,
            WAKE_MIN_INTERVAL_SECS
        ));
    }

    #[test]
    fn wake_reload_skips_fresh_or_missing_feed() {
        // Fresh feed: realtime is alive, an epoch would be pure churn.
        assert!(!should_wake_reload(
            false,
            Some(0),
            10_000,
            0,
            WAKE_MIN_INTERVAL_SECS
        ));
        assert!(!should_wake_reload(
            false,
            Some(WAKE_FEED_STALE_SECS - 1),
            10_000,
            0,
            WAKE_MIN_INTERVAL_SECS
        ));
        // Nothing rendered: the initial-load epoch (or a fresh mount) owns it.
        assert!(!should_wake_reload(
            false,
            None,
            10_000,
            0,
            WAKE_MIN_INTERVAL_SECS
        ));
    }

    #[test]
    fn wake_reload_rate_limits_rapid_returns() {
        // Focus + online + poll can all fire within the same return; only
        // the first may epoch (grace window, Amethyst-style).
        let now = 10_000;
        let first = should_wake_reload(false, Some(600), now, 0, WAKE_MIN_INTERVAL_SECS);
        assert!(first, "stale feed + never woken must epoch");
        let second = should_wake_reload(
            false,
            Some(600),
            now + (WAKE_MIN_INTERVAL_SECS - 1),
            now,
            WAKE_MIN_INTERVAL_SECS,
        );
        assert!(!second, "second wake inside the grace window must be ignored");
    }

    #[test]
    fn wake_reload_accepts_stale_feed_after_grace_window() {
        let now = 10_000;
        assert!(should_wake_reload(
            false,
            Some(600),
            now,
            0,
            WAKE_MIN_INTERVAL_SECS
        ));
        // Long-idle return well past the last wake is accepted again.
        assert!(should_wake_reload(
            false,
            Some(600),
            now + WAKE_MIN_INTERVAL_SECS,
            now,
            WAKE_MIN_INTERVAL_SECS
        ));
    }

    /// The backoff ladder: consecutive unproductive epochs (the feed top
    /// never moved) stretch the minimum wake interval so a quiet follow
    /// graph doesn't pay a full epoch on every tab glance.
    #[test]
    fn wake_min_interval_escalates_on_unproductive_streaks() {
        // Fresh state (and a single unproductive epoch — no new posts in
        // one minute is normal): base interval.
        assert_eq!(wake_min_interval(0), WAKE_MIN_INTERVAL_SECS);
        assert_eq!(wake_min_interval(1), WAKE_MIN_INTERVAL_SECS);
        // Sustained quiet: 5-minute spacing…
        assert_eq!(wake_min_interval(2), 5 * 60);
        assert_eq!(wake_min_interval(3), 5 * 60);
        // …capped at 15 minutes so a possibly-dead pool is still checked.
        assert_eq!(wake_min_interval(4), 15 * 60);
        assert_eq!(wake_min_interval(400), 15 * 60);
    }

    /// The escalated interval actually gates the wake decision: a return
    /// past the base interval but inside the backoff window is ignored.
    #[test]
    fn wake_reload_honors_backoff_interval() {
        let now = 10_000;
        let interval = wake_min_interval(2);
        assert!(interval > WAKE_MIN_INTERVAL_SECS);
        assert!(!should_wake_reload(
            false,
            Some(600),
            now + WAKE_MIN_INTERVAL_SECS,
            now,
            interval
        ));
        assert!(should_wake_reload(false, Some(600), now + interval, now, interval));
    }

    use nostr::event::{EventBuilder, FinalizeEvent};
    use nostr_sdk::prelude::Keys;

    fn post_at(secs: u64, content: &str) -> FeedItem {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, content)
            .custom_created_at(Timestamp::from(secs))
            .finalize(&keys)
            .expect("signed test note");
        FeedItem::OriginalPost(event)
    }

    /// The threshold is the max of cache and rendered tops: posts already
    /// on screen must partition as "not newer" so their re-delivery
    /// merges directly instead of re-entering the banner.
    #[test]
    fn effective_cache_top_lifts_to_rendered_top() {
        assert_eq!(effective_cache_top(Some(100), Some(200)), Some(200));
        assert_eq!(effective_cache_top(Some(300), Some(200)), Some(300));
        assert_eq!(effective_cache_top(Some(100), None), Some(100));
        assert_eq!(effective_cache_top(None, Some(200)), Some(200));
        assert_eq!(effective_cache_top(None, None), None);
    }

    /// Only same-feed, same-account epochs with rendered content may
    /// merge-paint; switches must replace and empty feeds have nothing
    /// worth keeping.
    #[test]
    fn merge_paint_requires_same_feed_account_and_data() {
        assert!(should_merge_paint(false, false, true));
        assert!(!should_merge_paint(true, false, true), "feed switch replaces");
        assert!(!should_merge_paint(false, true, true), "account switch replaces");
        assert!(!should_merge_paint(false, false, false), "nothing rendered");
    }

    /// Accepted-but-unpersisted posts (the rendered set) survive the
    /// epoch paint: the union keeps everything, newest-first.
    #[test]
    fn merge_and_cap_keeps_accepted_items() {
        let rendered = vec![post_at(300, "accepted"), post_at(100, "rendered old")];
        let cached = vec![post_at(200, "cached only"), post_at(50, "cached deep")];
        let ts: Vec<u64> = merge_and_cap(cached, rendered)
            .iter()
            .map(|i| i.sort_timestamp().as_secs())
            .collect();
        assert_eq!(ts, vec![300, 200, 100, 50]);
    }

    /// The same note on both sides lands once (visible-event-id dedup).
    #[test]
    fn merge_and_cap_dedups_by_visible_id() {
        let shared = post_at(200, "same note");
        let merged = merge_and_cap_with(
            vec![shared.clone()],
            vec![shared, post_at(100, "other")],
            10,
        );
        assert_eq!(merged.len(), 2);
    }

    /// The union is capped, dropping the oldest tail — accepted newer
    /// posts are never the ones trimmed.
    #[test]
    fn merge_and_cap_drops_oldest_tail_beyond_cap() {
        let cached = vec![post_at(30, "c3"), post_at(20, "c2"), post_at(10, "c1")];
        let rendered = vec![post_at(40, "accepted"), post_at(5, "oldest")];
        let ts: Vec<u64> = merge_and_cap_with(cached, rendered, 4)
            .iter()
            .map(|i| i.sort_timestamp().as_secs())
            .collect();
        assert_eq!(ts, vec![40, 30, 20, 10]);
    }
}
