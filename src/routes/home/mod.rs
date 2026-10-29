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
/// Maximum anchors per flush round (3c-4). A long fling queues ~58 anchors
/// per page; flushing everything at pause would re-create one giant burst.
/// Rounds are bounded to one chunk's worth (INTERACTION_EVENT_BATCH_SIZE =
/// 100 anchors ≈ one pagination-capped fetch of ≤150 events) and chain:
/// leftovers re-arm the debounce for the next round 1.5s later.
const COUNT_FLUSH_MAX_ANCHORS: usize = 100;
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
    let mut pending_count_flush = use_signal(HashSet::<EventId>::new);
    let mut count_flush_nonce = use_signal(|| 0u64);
    let mut pagination_loading = use_signal(|| false);
    let interaction_counts = use_signal(HashMap::<String, InteractionCounts>::new);
    let mut interactions_loaded = use_signal(|| false);
    let mut cached_muted_posts: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut cached_blocked_users: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut pending_posts = use_signal(Vec::<FeedItem>::new);
    let pending_count = use_memo(move || pending_posts.read().len());
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
    let mut last_loaded_trigger = use_signal(|| (0u32, FeedType::Following, false, false));
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

    // Main feed loading effect
    use_effect(move || {
        let refresh = *refresh_trigger.read();
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
        let (last_refresh, last_feed, last_signer_available, last_relays_applied) =
            last_loaded_trigger.peek().clone();
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
        let refresh_changed = refresh != last_refresh;
        let signer_changed = signer_available != last_signer_available;
        // Relay readiness transitions (e.g. same-session logout→login flips
        // USER_RELAYS_APPLIED false→true) must force a reload — the
        // previously loaded feed belongs to the prior session's relay set.
        let readiness_changed = user_relays_applied != last_relays_applied;
        if is_loading && !feed_type_changed && !refresh_changed && !signer_changed && !readiness_changed
        {
            log::debug!("Skipping feed re-load: already loading, no intentional change");
            return;
        }
        if has_data && !feed_type_changed && !refresh_changed && !signer_changed && !readiness_changed
        {
            log::debug!("Skipping feed re-load: data already present, no intentional change");
            return;
        }
        last_loaded_trigger.set((
            refresh,
            current_feed_type.clone(),
            signer_available,
            user_relays_applied,
        ));
        let token = stale.bump();
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
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    log::info!(
                        "Cleaning up {} real-time subscriptions due to manual refresh",
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
            spawn(async move {
                log::info!("Cleaning up interaction stream due to refresh");
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
        // separate unmount effect (see ~line 960).
        if had_realtime {
            realtime_started.set(false);
        }
        let is_first_load = !*interactions_loaded.peek() || feed_type_changed;
        interactions_loaded.set(false);

        spawn(async move {
            if stale.is_stale(token) {
                log::debug!("Discarding stale feed request {}", token);
                return;
            }
            let is_stale = || stale.is_stale(token);

            match current_feed_type {
                FeedType::Following => {
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let cache_key = FeedCacheKey::Following {
                        pubkey: pubkey_str.clone(),
                    };
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
                    let mut accumulated_items = if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for Following feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                        set_loaded_with_probe(&mut feed_state, feed_reset_generation,cached_items.clone());
                        cached_items
                    } else {
                        log::info!("No cache, loading Following feed...");
                        Vec::new()
                    };
                    if cache_only {
                        if accumulated_items.is_empty() {
                            log::info!("Phase 1 cache-only: no cached items for Following, waiting for signer restore");
                        } else {
                            log::info!(
                                "Phase 1 cache-only: showing {} cached items for Following while signer restores",
                                accumulated_items.len()
                            );
                        }
                        has_more.set(false);
                        return;
                    }
                    let stream_stale = stale;
                    let stream_token = token;
                    let collector = DebouncedCollector::<FeedItem>::new(50);
                    let accumulated_clone: Rc<RefCell<Vec<FeedItem>>> =
                        Rc::new(RefCell::new(accumulated_items.clone()));
                    let feed_state_clone = feed_state;
                    let result = load_following_feed_streaming(None, cached_cursor, cached_count, |batch_items| {
                        if stream_stale.is_stale(stream_token) {
                            log::debug!("Discarding stale streaming batch");
                            return;
                        }
                        collector.extend(batch_items, {
                            let acc = accumulated_clone.clone();
                            let mut fs = feed_state_clone;
                            move |items| {
                                let mut acc_guard = acc.borrow_mut();
                                *acc_guard =
                                    feed_cache::merge_feed_items(acc_guard.clone(), items);
                                fs.set(DataState::Loaded(Rc::new(acc_guard.clone())));
                            }
                        });
                    })
                    .await;
                    accumulated_items = accumulated_clone.borrow().clone();
                    let tail = collector.drain();
                    if !tail.is_empty() {
                        accumulated_items =
                            feed_cache::merge_feed_items(accumulated_items.clone(), tail);
                        set_loaded_with_probe(&mut feed_state, feed_reset_generation,accumulated_items.clone());
                    }
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok((page, did_fallback)) => {
                            let feed_items = page.items;
                            let effective_cache_key = if did_fallback {
                                log::info!("No contacts, switched to Global feed");
                                feed_type.set(FeedType::Global);
                                FeedCacheKey::Global
                            } else {
                                cache_key.clone()
                            };
                            has_more.set(true);
                            accumulated_items = feed_cache::merge_feed_items(accumulated_items, feed_items.clone());
                            {
                                let timestamps: Vec<u64> = accumulated_items.iter().map(|i| i.sort_timestamp().as_secs()).collect();
                                oldest_timestamp.set(crate::utils::pagination::safe_cursor_from_timestamps(&timestamps));
                            }
                            prefetch_author_metadata(&accumulated_items).await;
                            set_loaded_with_probe(&mut feed_state, feed_reset_generation,accumulated_items.clone());
                            if !is_stale() {
                                let cache_key_for_store = effective_cache_key;
                                let items_for_cache = accumulated_items.clone();
                                spawn(async move {
                                    // v3 render deferral: keep the cache_store
                                    // serialization (per-item serde + IndexedDB
                                    // puts) off the render-adjacent turn — its
                                    // span overlapped every >2s render turn in
                                    // the smokes. One idle beat is harmless:
                                    // the cache only serves the NEXT boot.
                                    crate::platform::timer::sleep_ms(1500).await;
                                    if let Err(e) = feed_cache::store_feed_items(
                                        &cache_key_for_store,
                                        &items_for_cache,
                                    )
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
                                let items_for_counts = feed_items.clone();
                                let ic = interaction_counts;
                                let il = interactions_loaded;
                                let ish = interaction_stream_handle;
                                spawn(async move {
                                    fetch_and_stream_interactions(
                                        &items_for_counts,
                                        is_first_load,
                                        ic,
                                        stale,
                                        token,
                                        il,
                                        ish,
                                    )
                                    .await
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata_with_relays(&feed_items).await;
                                });
                            }
                            if !is_stale() && !did_fallback {
                                let pk_for_sync = pubkey_str.clone();
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
                            if accumulated_items.is_empty() {
                                feed_state.set(DataState::Error(e.to_string()));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
                FeedType::FollowingWithReplies => {
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let pk_for_sync = pubkey_str.clone();
                    let cache_key = FeedCacheKey::FollowingWithReplies { pubkey: pubkey_str };
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
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for FollowingWithReplies feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                        set_loaded_with_probe(&mut feed_state, feed_reset_generation,cached_items.clone());
                    }
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
                    let result = load_following_with_replies(None, cached_cursor, cached_count).await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok((page, did_fallback)) => {
                            let feed_items = page.items;
                            let effective_cache_key = if did_fallback {
                                log::info!("No contacts, switched to Global feed");
                                feed_type.set(FeedType::Global);
                                FeedCacheKey::Global
                            } else {
                                cache_key.clone()
                            };
                            has_more.set(true);
                            let merged = feed_cache::merge_feed_items(cached_items, feed_items.clone());
                            {
                                let timestamps: Vec<u64> = merged.iter().map(|i| i.sort_timestamp().as_secs()).collect();
                                oldest_timestamp.set(crate::utils::pagination::safe_cursor_from_timestamps(&timestamps));
                            }
                            set_loaded_with_probe(&mut feed_state, feed_reset_generation,merged.clone());
                            if !is_stale() {
                                let cache_key_for_store = effective_cache_key;
                                let items_for_cache = merged;
                                spawn(async move {
                                    let _ = feed_cache::store_feed_items(
                                        &cache_key_for_store,
                                        &items_for_cache,
                                    )
                                    .await;
                                    let _ = feed_cache::run_eviction_if_needed().await;
                                });
                            }
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let ic = interaction_counts;
                                let il = interactions_loaded;
                                let ish = interaction_stream_handle;
                                spawn(async move {
                                    fetch_and_stream_interactions(
                                        &items_for_counts,
                                        is_first_load,
                                        ic,
                                        stale,
                                        token,
                                        il,
                                        ish,
                                    )
                                    .await
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                            if !is_stale() && !did_fallback {
                                let pk_sync = pk_for_sync.clone();
                                spawn(async move {
                                    if let Ok(contacts) = nostr_client::fetch_contacts(pk_sync).await {
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
                            if cached_items.is_empty() {
                                feed_state.set(DataState::Error(e.to_string()));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
                FeedType::Global => {
                    let cache_key = FeedCacheKey::Global;
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
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for Global feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                        set_loaded_with_probe(&mut feed_state, feed_reset_generation,cached_items.clone());
                    }
                    let result = load_global_feed(None, cached_cursor, cached_count).await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok(page) => {
                            let feed_items = page.items;
                            {
                                let timestamps: Vec<u64> = feed_items.iter().map(|i| i.sort_timestamp().as_secs()).collect();
                                oldest_timestamp.set(crate::utils::pagination::safe_cursor_from_timestamps(&timestamps));
                            }
                            has_more.set(true);
                            set_loaded_with_probe(&mut feed_state, feed_reset_generation,feed_items.clone());
                            if !is_stale() {
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    let _ = feed_cache::store_feed_items(
                                        &FeedCacheKey::Global,
                                        &items_for_cache,
                                    )
                                    .await;
                                    let _ = feed_cache::run_eviction_if_needed().await;
                                });
                            }
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let ic = interaction_counts;
                                let il = interactions_loaded;
                                let ish = interaction_stream_handle;
                                spawn(async move {
                                    fetch_and_stream_interactions(
                                        &items_for_counts,
                                        is_first_load,
                                        ic,
                                        stale,
                                        token,
                                        il,
                                        ish,
                                    )
                                    .await
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            if cached_items.is_empty() {
                                feed_state.set(DataState::Error(e.to_string()));
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
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for PeopleList feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                        set_loaded_with_probe(&mut feed_state, feed_reset_generation,cached_items.clone());
                    }
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
                    let result = load_people_list_feed(&list, None, cached_cursor, cached_count).await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok(page) => {
                            let feed_items = page.items;
                            {
                                let timestamps: Vec<u64> = feed_items.iter().map(|i| i.sort_timestamp().as_secs()).collect();
                                oldest_timestamp.set(crate::utils::pagination::safe_cursor_from_timestamps(&timestamps));
                            }
                            has_more.set(true);
                            set_loaded_with_probe(&mut feed_state, feed_reset_generation,feed_items.clone());
                            if !is_stale() {
                                let cache_key_for_store = cache_key.clone();
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    let _ = feed_cache::store_feed_items(
                                        &cache_key_for_store,
                                        &items_for_cache,
                                    )
                                    .await;
                                    let _ = feed_cache::run_eviction_if_needed().await;
                                });
                            }
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let ic = interaction_counts;
                                let il = interactions_loaded;
                                let ish = interaction_stream_handle;
                                spawn(async move {
                                    fetch_and_stream_interactions(
                                        &items_for_counts,
                                        is_first_load,
                                        ic,
                                        stale,
                                        token,
                                        il,
                                        ish,
                                    )
                                    .await
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            if cached_items.is_empty() {
                                feed_state.set(DataState::Error(e.to_string()));
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
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for RelayFeed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                        set_loaded_with_probe(&mut feed_state, feed_reset_generation,cached_items.clone());
                    }
                    let result = load_relay_feed(urls.clone(), None, cached_cursor, cached_count).await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok(page) => {
                            let feed_items = page.items;
                            {
                                let timestamps: Vec<u64> = feed_items.iter().map(|i| i.sort_timestamp().as_secs()).collect();
                                oldest_timestamp.set(crate::utils::pagination::safe_cursor_from_timestamps(&timestamps));
                            }
                            has_more.set(true);
                            set_loaded_with_probe(&mut feed_state, feed_reset_generation,feed_items.clone());
                            if !is_stale() {
                                let cache_key_for_store = cache_key;
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    let _ = feed_cache::store_feed_items(
                                        &cache_key_for_store,
                                        &items_for_cache,
                                    )
                                    .await;
                                    let _ = feed_cache::run_eviction_if_needed().await;
                                });
                            }
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let ic = interaction_counts;
                                let il = interactions_loaded;
                                let ish = interaction_stream_handle;
                                spawn(async move {
                                    fetch_and_stream_interactions(
                                        &items_for_counts,
                                        is_first_load,
                                        ic,
                                        stale,
                                        token,
                                        il,
                                        ish,
                                    )
                                    .await
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            if cached_items.is_empty() {
                                feed_state.set(DataState::Error(e.to_string()));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
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
                    pending_count_flush.write().extend(count_ids);
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
                        cursors.retain_live(&urls);
                        for (url, floor, settled) in &per_relay.folds {
                            let bound = bounds.get(url).copied().flatten();
                            if cursors.fold_page(url, *floor, bound, *settled) {
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
                let Ok(_round_permit) = COUNT_FLUSH_GATE.try_acquire() else {
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
                match crate::services::aggregation::flush_paginated_interaction_counts(
                    ids.clone(),
                    std::time::Duration::from_secs(5),
                )
                .await
                {
                    Ok(new_counts) => {
                        // Drain-on-success: the taken anchors' fetch
                        // completed (counts found or verified absent), so
                        // remove exactly those from the queue. Without this
                        // the 3c-4 re-arm below sees them still queued and
                        // re-flushes the same anchors every debounce window
                        // forever (observed on Android: identical
                        // "Flushing coalesced pagination counts for 3
                        // events" at 1.5-2s intervals, cache-hit rounds).
                        {
                            let mut queue = pending.write();
                            for id in &ids {
                                queue.remove(id);
                            }
                        }
                        if !new_counts.is_empty() {
                            counts_signal.extend(new_counts);
                        }
                    }
                    Err(e) => {
                        // Error round: anchors stay queued and the re-arm
                        // below retries them next round.
                        log::warn!("Coalesced pagination count flush failed: {}", e)
                    }
                }
                // Bounded rounds (3c-4): a fling can queue far more anchors
                // than one bounded fetch should carry. Whatever remains
                // (beyond this round's cap, or errored) re-arms the debounce
                // for the next round; nothing is dropped.
                if !pending.read().is_empty() {
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
        let pending: Vec<FeedItem> = pending_posts.write().drain(..).collect();
        if pending.is_empty() {
            return;
        }
        let current = match feed_state.read().clone() {
            DataState::Loaded(items) => items.to_vec(),
            _ => return,
        };
        // Incremental merge (plan wave B1): dioxus signals re-render on
        // every write (no equality short-circuit), so skip the write
        // entirely when every pending post was already in the feed — and
        // cap the live list, dropping the oldest tail only here (the user
        // is at the top by definition of accepting the pill), never during
        // pagination.
        let mut list = crate::feeds::list::FeedList::from_sorted_items(current);
        let landed = list.insert_batch(pending);
        if landed == 0 {
            return;
        }
        list.trim_oldest(crate::feeds::list::DEFAULT_FEED_CAP);
        set_loaded_with_probe(&mut feed_state, feed_reset_generation,list.into_items());
        spawn(async move {
            scroll_restore::set_scroll_y(0.0).await;
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
                                        class: "sticky top-[57px] z-20 border-b border-border bg-blue-500 hover:bg-blue-600 transition-colors cursor-pointer",
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
