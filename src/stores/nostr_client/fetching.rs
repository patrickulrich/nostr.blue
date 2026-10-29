//! Event fetching strategies
//!
//! Functions for fetching events from relays and database with various patterns:
//! - Aggregated (database first, relay background sync)
//! - Direct relay fetch (bypassing cache)
//! - Outbox routing (NIP-65 gossip)
//! - Connected-only fetch (fast, bypasses gossip discovery)
use super::signals::NOSTR_CLIENT;
use crate::stores::relay;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Get the current client instance.
///
/// Uses `peek()` (non-subscribing read): this is called from async/spawn
/// contexts where a reactive subscription is at best useless and at worst a
/// leak. This is the ONE sanctioned client getter — do not read
/// `NOSTR_CLIENT` directly outside this module.
pub(crate) fn get_client() -> Option<std::sync::Arc<Client>> {
    NOSTR_CLIENT.peek().clone()
}
/// Wait for at least one relay to be ready before fetching
pub(crate) async fn ensure_relays_ready(client: &Client) {
    relay::connection::ensure_relays_ready(client).await;
}
pub(crate) async fn ensure_chess_relays_connected(client: &Client) {
    relay::connection::ensure_chess_relays_connected(client).await;
}
/// Fetch events using aggregated pattern: database first, then relays
///
/// This function:
/// 1. Queries local IndexedDB cache first (instant)
/// 2. If cache hit, returns immediately and syncs in background
/// 3. If cache miss, fetches from relays
pub async fn fetch_events_aggregated(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    fetch_events_aggregated_with_client(&client, filter, timeout).await
}
/// Minimum collection window before an all-empty quorum may break the
/// fetch (see [`fetch_events_quorum`]).
const MIN_COLLECT_SECS: u64 = 2;

/// One-shot fetch with quorum-EOSE early-exit.
///
/// `Client::fetch_events` waits for EVERY read relay to EOSE (or the full
/// timeout) — one dead relay pins the whole fetch. This helper manually
/// subscribes, counts per-relay EOSE (deduped by relay URL: NIP-42
/// re-authentication can emit a second EOSE per relay), and exits as soon as
/// [`crate::stores::nostr_client::eose::fetch_quorum_threshold`] of the relays
/// the REQ reached have EOSE'd — with two guards against the empty-relay race
/// (relays holding ZERO matching events EOSE instantly and would otherwise
/// beat content-heavy relays to the quorum):
///
/// 1. The early break only fires once the fetch has received at least one
///    event, or [`MIN_COLLECT_SECS`] have elapsed.
/// 2. The threshold is 50% of targets (vs. the 30% realtime-liveness
///    threshold in `eose::realtime_liveness_threshold`), so fast-but-empty
///    relays alone cannot close the fetch.
///
/// Interactive-tier semantics (no min-collect floor); use
/// [`fetch_events_quorum_multi`] directly for floored backfills.
///
/// The subscription is always closed before returning. Events that arrive
/// after the quorum break are still auto-persisted to the SDK database by
/// the pool, so follow-up DB queries see them.
pub async fn fetch_events_quorum(
    client: &Client,
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    fetch_events_quorum_multi(client, vec![filter], timeout, usize::MAX, Duration::ZERO).await
}

/// Multi-filter variant used by the facade: subscribes with the whole filter
/// list in one REQ and caps collection at `max_events` (the SDK's builder
/// path *errors* past its default 10,000 — this path stops collecting
/// instead, which is what the `Backfill` profile relies on).
///
/// `min_collect` is a floor on the quorum early-exit: the break may only
/// fire once it has elapsed. Broad fetches over sparse content (marketplace
/// browse: most pool relays hold zero matching events and EOSE instantly
/// while a few content-heavy relays stream for seconds) would otherwise
/// close at the first event + fast-empty quorum — measured at 1-of-843
/// marketplace products in the wild. EOSEs arriving *before* the floor never
/// re-check the break condition on their own, so a dedicated timer branch
/// re-evaluates the quorum when the floor elapses (without it the fetch
/// would always run to the full timeout). `Duration::ZERO` restores the
/// un-floored interactive-tier behavior.
pub async fn fetch_events_quorum_multi(
    client: &Client,
    filters: Vec<Filter>,
    timeout: Duration,
    max_events: usize,
    min_collect: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    use nostr::message::relay::RelayMessage;

    // Subscribe to the notification stream BEFORE issuing the REQ: a
    // broadcast receiver only sees messages sent after it subscribed, so
    // EOSEs from the fastest relays could otherwise be missed.
    let mut notifications = client.notifications();
    let sub_output = client
        .subscribe(filters)
        .await
        .map_err(|e| format!("subscribe: {e}"))?;
    let sub_id = sub_output.value;
    // `success` is the set of relays the REQ was queued to — the exact
    // quorum denominator (vs. counting pool members that are disconnected).
    let target = sub_output.success.len();
    let threshold = super::eose::fetch_quorum_threshold(target);
    let start = now_ms();
    let result = {
        let sub_id = sub_id.clone();
        let mut events: Vec<nostr::event::Event> = Vec::new();
        let mut seen_ids: std::collections::HashSet<nostr::event::EventId> =
            std::collections::HashSet::new();
        let mut eose_relays: std::collections::HashSet<String> = std::collections::HashSet::new();
        let deadline = crate::platform::timer::sleep(timeout);
        tokio::pin!(deadline);
        // Min-collect floor state. `floor_elapsed` flips once the floor
        // sleep fires; it starts `true` for a zero floor so floor-0 callers
        // keep the exact pre-floor behavior. A completed gloo/tokio sleep
        // polls Ready forever, so the branch precondition disables it after
        // the first fire.
        let mut floor_elapsed = min_collect.is_zero();
        let floor_timer = crate::platform::timer::sleep(min_collect);
        tokio::pin!(floor_timer);
        // Negative-lookup exit: the EOSE break requires
        // `!events.is_empty() || elapsed >= MIN_COLLECT_SECS`, so a fetch
        // where every relay EOSEs empty inside the first 2s fails that
        // guard at EOSE time and — EOSEs being deduped per relay — nothing
        // ever re-evaluates it. This one-shot re-checks quorum at the 2s
        // mark so zero-result fetches exit early instead of parking on the
        // deadline for the full profile timeout. Same disable-after-fire
        // guard as the floor timer (a completed sleep polls Ready forever).
        let mut collect_elapsed = false;
        let collect_timer = crate::platform::timer::sleep(std::time::Duration::from_secs(
            MIN_COLLECT_SECS,
        ));
        tokio::pin!(collect_timer);
        loop {
            tokio::select! {
                _ = &mut deadline => {
                    log::debug!(
                        "fetch_events_quorum: timeout (eose {}/{target} of threshold {threshold}, {} events)",
                        eose_relays.len(),
                        events.len()
                    );
                    break;
                }
                _ = &mut floor_timer, if !floor_elapsed => {
                    floor_elapsed = true;
                    if eose_relays.len() >= threshold {
                        log::debug!(
                            "fetch_events_quorum: min-collect floor elapsed with quorum met ({}/{target}), {} events",
                            eose_relays.len(),
                            events.len()
                        );
                        break;
                    }
                    // Quorum unmet — keep collecting until quorum+EOSE or the
                    // overall deadline.
                }
                _ = &mut collect_timer, if !collect_elapsed => {
                    collect_elapsed = true;
                    // Mirrors the EOSE guard's 2s arm for the empty-result
                    // case; `floor_elapsed` keeps tiers with a longer floor
                    // (Backfill: 3s) on their own schedule.
                    if floor_elapsed && eose_relays.len() >= threshold {
                        log::debug!(
                            "fetch_events_quorum: min-collect window elapsed with quorum met ({}/{target}), {} events",
                            eose_relays.len(),
                            events.len()
                        );
                        break;
                    }
                }
                recv = notifications.next() => {
                    match recv {
                        Some(nostr_sdk::client::ClientNotification::Event { subscription_id, event, .. })
                            if subscription_id == sub_id =>
                        {
                            // First-seen events (not yet in the local DB).
                            if seen_ids.insert(event.id) {
                                events.push((*event).clone());
                            }
                            if events.len() >= max_events {
                                log::debug!(
                                    "fetch_events_quorum: hit max_events cap ({max_events}), stopping collection"
                                );
                                break;
                            }
                        }
                        Some(nostr_sdk::client::ClientNotification::Message { relay_url, message }) => {
                            // Raw relay messages. TWO shapes matter here:
                            //
                            // 1. EOSE — quorum counting (deduped by relay URL:
                            //    NIP-42 re-authentication can emit a second EOSE
                            //    per relay).
                            // 2. `Event`-wrapped messages — the SDK only emits
                            //    `ClientNotification::Event` the FIRST time an
                            //    event is seen; events already persisted (e.g.
                            //    by a previous `send_event`, or fetched on an
                            //    earlier pass) surface ONLY as raw `Message`s.
                            //    Consuming them here (deduped by event id
                            //    against both sources) is what makes refresh
                            //    fetches of already-cached events work.
                            match message.as_ref() {
                                RelayMessage::Event { subscription_id, event }
                                    if **subscription_id == sub_id
                                        && seen_ids.insert(event.id) =>
                                {
                                    events.push((**event).clone());
                                    if events.len() >= max_events {
                                        log::debug!(
                                            "fetch_events_quorum: hit max_events cap ({max_events}), stopping collection"
                                        );
                                        break;
                                    }
                                }
                                RelayMessage::EndOfStoredEvents(id)
                                    if **id == sub_id
                                        && eose_relays.insert(relay_url.to_string())
                                        && eose_relays.len() >= threshold
                                        && floor_elapsed
                                        && (!events.is_empty()
                                            || now_ms().saturating_sub(start)
                                                >= MIN_COLLECT_SECS * 1_000) =>
                                {
                                    log::debug!(
                                        "fetch_events_quorum: quorum reached ({}/{target}), {} events",
                                        eose_relays.len(),
                                        events.len()
                                    );
                                    break;
                                }
                                _ => {}
                            }
                        }
                        Some(nostr_sdk::client::ClientNotification::Shutdown) => break,
                        // Stream ended (client shut down / dropped).
                        None => break,
                        Some(_) => {}
                    }
                }
            }
        }
        Ok(events)
    };
    let _ = client.unsubscribe(&sub_id).await;
    result
}

/// Millisecond clock for fetch windows. `instant::Instant` panics on
/// non-wasm32 targets when its wasm-bindgen backend is compiled in (unit
/// tests run the `web` feature on a host), so use std on native and the JS
/// clock on wasm (every wasm32 build of this crate has the `web` feature).
fn now_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
/// Internal: aggregated fetch using provided client (avoids re-reading NOSTR_CLIENT)
///
/// Dioxus pattern: Get client once via OnceLock/get_or_init, pass same instance
/// through all async operations. No locks held across await points.
async fn fetch_events_aggregated_with_client(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    match client.database().query(filter.clone()).await {
        Ok(db_events) => {
            let db_count = db_events.len();
            if db_count > 0 {
                log::info!("Loaded {} events from IndexedDB cache", db_count);
                let client_clone = client.clone();
                let filter_clone = filter.clone();
                dioxus::prelude::spawn(async move {
                    relay::connection::ensure_relays_ready(&client_clone).await;
                    if let Err(e) =
                        fetch_events_quorum(&client_clone, filter_clone, timeout).await
                    {
                        log::warn!("Background relay sync failed: {}", e);
                    }
                });
                return Ok(db_events.into_iter().collect());
            }
        }
        Err(e) => {
            log::warn!("Database query failed: {}, falling back to relays", e);
        }
    }
    log::info!("Fetching from relays (database empty or failed)");
    relay::connection::ensure_relays_ready(client).await;
    fetch_events_quorum(client, filter, timeout).await
}
/// Fetch chess events: DB-first for fast paint, then always refresh from chess relays.
///
/// 1. Query IndexedDB cache → return immediately for fast UI if found
/// 2. Always fetch fresh from chess relays (wss://relay.damus.io, etc.)
/// 3. Merge new relay events with DB events
/// 4. Return combined result (new events auto-saved to DB by nostr-sdk)
pub async fn fetch_chess_events(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_chess_relays_connected(&client).await;

    let mut seen_ids: std::collections::HashSet<nostr::event::EventId> = std::collections::HashSet::new();
    let mut all_events: Vec<nostr::event::Event> = vec![];

    // 1. DB first (fast paint)
    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if !db_events.is_empty() {
            log::info!("Chess DB cache: {} events", db_events.len());
            let db_vec: Vec<nostr::event::Event> = db_events.into_iter().collect();
            for ev in &db_vec {
                seen_ids.insert(ev.id);
            }
            all_events = db_vec;
        }
    }

    // 2. Always fetch fresh from chess relays
    let chess_urls: Vec<RelayUrl> = crate::utils::nips::chess::CHESS_RELAYS
        .iter()
        .filter_map(|u| RelayUrl::parse(u).ok())
        .collect();
    let chess_targets: Vec<(RelayUrl, Vec<Filter>)> = chess_urls
        .into_iter()
        .map(|u| (u, vec![filter.clone()]))
        .collect();

    match client.fetch_events(chess_targets).timeout(timeout).await {
        Ok(relay_events) => {
            let mut new_count = 0;
            for ev in relay_events {
                if seen_ids.insert(ev.id) {
                    new_count += 1;
                    all_events.push(ev);
                }
            }
            if new_count > 0 {
                log::info!("Chess relay fetch: {} new events merged", new_count);
            }
        }
        Err(e) => {
            log::warn!("Chess relay fetch failed: {} (returning {} DB events)", e, all_events.len());
        }
    }

    Ok(all_events)
}
/// Fetch events using the DB-first + fresh-relay-merge pattern.
///
/// Unlike the aggregated cache pattern (which returns stale DB data and discards
/// fresh relay results in a fire-and-forget spawn), this:
/// 1. Queries the local database (IndexedDB on web) → instant paint if cached
/// 2. Always fetches fresh from connected relays
/// 3. Merges new relay events with DB events (deduped by event id)
/// 4. Returns the combined result so the UI always gets the latest data
pub async fn fetch_events_db_merge(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_relays_ready(&client).await;

    let mut seen_ids: std::collections::HashSet<nostr::event::EventId> = std::collections::HashSet::new();
    let mut all_events: Vec<nostr::event::Event> = vec![];

    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if !db_events.is_empty() {
            log::info!("DB cache: {} events", db_events.len());
            let db_vec: Vec<nostr::event::Event> = db_events.into_iter().collect();
            for ev in &db_vec {
                seen_ids.insert(ev.id);
            }
            all_events = db_vec;
        }
    }

    match client.fetch_events(filter).timeout(timeout).await {
        Ok(relay_events) => {
            let mut new_count = 0;
            for ev in relay_events {
                if seen_ids.insert(ev.id) {
                    new_count += 1;
                    all_events.push(ev);
                }
            }
            if new_count > 0 {
                log::info!("Relay fetch: {} new events merged", new_count);
            }
        }
        Err(e) => {
            log::warn!(
                "Relay fetch failed: {} (returning {} DB events)",
                e,
                all_events.len()
            );
        }
    }

    Ok(all_events)
}
/// Fetch topic posts using DB-first + relay-merge pattern.
///
/// See [`fetch_events_db_merge`] for semantics.
pub async fn fetch_topic_events(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    fetch_events_db_merge(filter, timeout).await
}
/// DB-first + relay-merge, with the network half bypassing gossip.
///
/// For author-scoped filters (e.g. kind 30023 from self+contacts), the
/// gossip-routed `client.fetch_events` used by [`fetch_events_db_merge`]
/// first runs a NIP-65 negentropy sync for EVERY author (empty in-memory
/// gossip store after a cold start), then scatters the filter across
/// per-author write relays — on web this routinely yields zero events
/// inside the timeout. The kind-1 home feed avoids this via
/// `fetch_events_from_connected_relays`; this function applies the same
/// routing (pool-direct fetch from connected relays, gossip only as the
/// no-connections fallback) while keeping the DB-first merge semantics.
pub async fn fetch_events_db_merge_from_connected(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    let mut seen_ids: std::collections::HashSet<nostr::event::EventId> = std::collections::HashSet::new();
    let mut all_events: Vec<nostr::event::Event> = vec![];

    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if !db_events.is_empty() {
            log::info!("DB cache: {} events", db_events.len());
            let db_vec: Vec<nostr::event::Event> = db_events.into_iter().collect();
            for ev in &db_vec {
                seen_ids.insert(ev.id);
            }
            all_events = db_vec;
        }
    }

    match fetch_events_from_connected_relays_with_client(&client, filter, timeout).await {
        Ok(relay_events) => {
            let mut new_count = 0;
            for ev in relay_events {
                if seen_ids.insert(ev.id) {
                    new_count += 1;
                    all_events.push(ev);
                }
            }
            if new_count > 0 {
                log::info!("Connected-relay fetch: {} new events merged", new_count);
            }
        }
        Err(e) => {
            log::warn!(
                "Connected-relay fetch failed: {} (returning {} DB events)",
                e,
                all_events.len()
            );
        }
    }

    Ok(all_events)
}
/// Fetch custom NIPs (kind 30817) using DB-first + relay-merge pattern.
///
/// See [`fetch_events_db_merge`] for semantics.
pub async fn fetch_custom_nip_events(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    fetch_events_db_merge(filter, timeout).await
}
/// Fetch NIP-53 nest (kind 30312 Meeting Space) events using DB-first + relay-merge.
///
/// The aggregated cache pattern (`fetch_events_aggregated_with_client`) returns
/// stale IndexedDB data immediately and discards the fresh relay fetch's result
/// in a fire-and-forget spawn. For nests this means once any rooms are cached,
/// a newly-created room is written to the DB silently but never surfaces in the
/// UI until the cache is emptied.
///
/// Mirrors `fetch_topic_events`: DB-first for fast paint, then always fetch
/// fresh from the connected pool, merge new events (deduped by event id), and
/// return the combined result. Addressable (30312) coordinate-level dedup is
/// handled by the caller after parsing.
/// Query ONLY the local database (no relay traffic) for nest room events.
/// Phase 1 of the stale-while-revalidate feed flow in `routes/nests/home.rs`:
/// renders cached rooms instantly while [`fetch_nest_events`] refreshes from
/// relays. Mirrors the DB half of `fetch_nest_events` so both phases see the
/// same data.
pub async fn query_nest_db_cache(
    filter: Filter,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    match client.database().query(filter).await {
        Ok(events) => Ok(events.into_iter().collect()),
        Err(e) => Err(e.to_string()),
    }
}

pub async fn fetch_nest_events(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_relays_ready(&client).await;

    let mut seen_ids: std::collections::HashSet<nostr::event::EventId> = std::collections::HashSet::new();
    let mut all_events: Vec<nostr::event::Event> = vec![];

    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if !db_events.is_empty() {
            log::info!("Nest DB cache: {} events", db_events.len());
            let db_vec: Vec<nostr::event::Event> = db_events.into_iter().collect();
            for ev in &db_vec {
                seen_ids.insert(ev.id);
            }
            all_events = db_vec;
        }
    }

    match client.fetch_events(filter).timeout(timeout).await {
        Ok(relay_events) => {
            let mut new_count = 0;
            for ev in relay_events {
                if seen_ids.insert(ev.id) {
                    new_count += 1;
                    all_events.push(ev);
                }
            }
            if new_count > 0 {
                log::info!("Nest relay fetch: {} new events merged", new_count);
            }
        }
        Err(e) => {
            log::warn!(
                "Nest relay fetch failed: {} (returning {} DB events)",
                e,
                all_events.len()
            );
        }
    }

    Ok(all_events)
}
/// Fetch radio events directly from relays, bypassing the aggregated cache.
///
/// The aggregated cache pattern (`fetch_events_aggregated_with_client`) returns
/// stale IndexedDB data immediately and spawns a background relay sync whose
/// results are never propagated to the UI. For radio, this means once a single
/// event is cached, only that one station shows forever.
///
/// Instead, this always fetches fresh from relays. Events are still saved to
/// IndexedDB automatically by nostr-sdk during relay message processing.
///
/// Staleness-aware: the captured `Client` Arc can be shut down mid-flight by a
/// login/logout client rebuild (`rebuild_client` swaps `NOSTR_CLIENT`, then
/// clears the old pool). Every SDK call on the dead pool then fails — most
/// visibly `fetch_events`, whose gossip filter breakdown yields
/// "broken down filters are empty" when `read_relay_urls()` is empty. When the
/// client was swapped mid-flight, retry once against the *current* client
/// instead of surfacing the error (Arc identity check — no error-string
/// matching).
pub async fn fetch_radio_events(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let mut last_err = String::new();
    for _attempt in 0..2 {
        let Some(client) = get_client() else {
            return Err("Client not initialized".to_string());
        };
        match fetch_radio_events_once(&client, filter.clone(), timeout).await {
            Ok(events) => return Ok(events),
            Err(e) => {
                // Retry only when the client was rebuilt mid-flight; on a live
                // client the error is real (relays down) and propagates as-is.
                let stale = get_client()
                    .as_ref()
                    .map(|current| !std::sync::Arc::ptr_eq(current, &client))
                    .unwrap_or(false);
                if !stale {
                    return Err(e);
                }
                log::info!(
                    "fetch_radio_events: client rebuilt mid-flight ({}), retrying on current client",
                    e
                );
                last_err = e;
            }
        }
    }
    Err(last_err)
}
/// Single-attempt radio fetch against a specific client instance.
///
/// Uses the SDK's `client.fetch_events` (pool-level stream) on purpose.
/// `fetch_events_quorum` collects from the client broadcast channel, whose
/// `handle_event` only emits events NEW to the local database — stations
/// persisted by a previous session yield zero notifications and render
/// "No stations found" (relay/inner.rs: `check_id` → `Saved` falls through
/// without broadcasting, while pool streams still receive the raw event).
async fn fetch_radio_events_once(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    // Dead-pool fast fail: a client shut down by a login/logout rebuild has
    // an empty relay map (shutdown force-clears it), while any client
    // visible via `NOSTR_CLIENT` was pool-bootstrapped before the swap and
    // is never empty. Skip the pre-flight waits entirely so the caller's
    // staleness retry fires in milliseconds instead of after the full
    // ensure budget.
    if client.relays().all().await.is_empty() {
        return Err("relay pool is empty (client shut down?)".to_string());
    }
    // Bounded (~5s): `ensure_radio_relay` waits up to 30s per relay
    // sequentially, which stalls the /radio critical path and widens the
    // client-rebuild race window this helper's caller guards against.
    relay::ensure_radio_relays_connected_bounded(client, Duration::from_secs(5)).await;
    relay::connection::ensure_relays_ready(client).await;

    let mut events: Vec<_> = client
        .fetch_events(filter.clone())
        .timeout(timeout)
        .await
        .map(|events| events.into_iter().collect())
        .map_err(|e| e.to_string())?;

    if events.is_empty() {
        log::info!("Radio fetch returned 0 events, waiting for relay and retrying...");
        crate::platform::timer::sleep_ms(3000).await;
        relay::ensure_radio_relays_connected_bounded(client, Duration::from_secs(5)).await;
        relay::connection::ensure_relays_ready(client).await;
        events = client
            .fetch_events(filter)
            .timeout(timeout)
            .await
            .map(|events| events.into_iter().collect())
            .map_err(|e| e.to_string())?;
    }
    Ok(events)
}
/// Fetch events directly from relays, bypassing cache
///
/// Use this for discovery features where fresh data from the network is needed.
/// Results are still stored in the database for future caching.
pub async fn fetch_events_from_relays(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_relays_ready(&client).await;
    let relays = client.relays().await;
    let connected_count = relays
        .iter()
        .filter(|(_, r)| r.status() == nostr_sdk::relay::RelayStatus::Connected)
        .count();
    log::info!(
        "fetch_events_from_relays: {} relays connected",
        connected_count
    );
    let result = client
        .fetch_events(filter.clone())
        .timeout(timeout)
        .await
        .map(|events| {
            let events: Vec<_> = events.into_iter().collect();
            log::info!("fetch_events_from_relays: received {} events", events.len());
            events
        })
        .map_err(|e| {
            log::error!("fetch_events_from_relays error: {}", e);
            e.to_string()
        });
    result
}
/// Fetch events using gossip (automatic relay routing)
///
/// This function waits for user relay lists (kind 10002) to be applied before
/// fetching, ensuring gossip routing uses the correct relays for signed-in users.
pub async fn fetch_events_aggregated_outbox(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    fetch_events_aggregated_outbox_with_client(&client, filter, timeout).await
}
/// Internal: Fetch events using gossip with provided client (avoids re-reading NOSTR_CLIENT)
///
/// Dioxus pattern: Get client once, pass same instance through all async operations.
pub(crate) async fn fetch_events_aggregated_outbox_with_client(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    crate::stores::relay::wait_for_user_relays(
        Duration::from_millis(500),
        "fetch_events_aggregated_outbox",
    )
    .await;
    ensure_relays_ready(client).await;
    let filter_authors = filter.authors.clone();
    let events = client
        .fetch_events(filter)
        .timeout(timeout)
        .await
        .map_err(|e| format!("Failed to fetch events: {}", e))?;
    let filtered_events: Vec<nostr::event::Event> = if let Some(ref authors) = filter_authors {
        let author_set: std::collections::HashSet<_> = authors.iter().collect();
        events
            .into_iter()
            .filter(|e| author_set.contains(&e.pubkey))
            .collect()
    } else {
        events.into_iter().collect()
    };
    Ok(filtered_events)
}
/// Fetch events from database only (instant, for initial display)
///
/// This is Phase 1 of profile loading - shows cached data immediately.
/// Call `fetch_profile_events_from_relays` afterward for fresh data.
pub async fn fetch_profile_events_db(
    filter: Filter,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    match client.database().query(filter).await {
        Ok(events) => {
            let count = events.len();
            log::info!("Profile DB: loaded {} events instantly", count);
            Ok(events.into_iter().collect())
        }
        Err(e) => {
            log::warn!("Profile DB query failed: {}", e);
            Ok(Vec::new())
        }
    }
}
/// Fetch events from relays only (for background refresh)
///
/// This is Phase 2 of profile loading - fetches fresh data from relays.
/// Uses gossip/outbox routing for efficient relay selection.
pub async fn fetch_profile_events_from_relays(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    ensure_relays_ready(&client).await;
    match client.fetch_events(filter).timeout(timeout).await {
        Ok(events) => {
            let count = events.len();
            log::info!("Profile relays: fetched {} events", count);
            Ok(events.into_iter().collect())
        }
        Err(e) => {
            log::warn!("Profile relay fetch failed: {}", e);
            Err(format!("Relay fetch failed: {}", e))
        }
    }
}

/// Fetch profile events from pre-resolved relay URLs, skipping relay discovery.
/// Falls back to generic relay fetch if targeted fetch returns empty.
#[allow(dead_code)]
pub async fn fetch_profile_events_from_relays_direct(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    relay_urls: &[String],
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    if relay_urls.is_empty() {
        return fetch_profile_events_from_relays(filter, timeout).await;
    }
    let ephemeral = relay::coverage::connect_ephemeral_relays(client, relay_urls).await;
    if !ephemeral.connected.is_empty() {
        let result = relay::connection::fetch_events_from_relays(
            client,
            filter.clone(),
            ephemeral.connected.clone(),
            timeout,
            )
            .await;
            // NOTE: ephemeral relays are NOT cleaned up here. They share the
            // same dedup pool as the profile tab-events stream, so cleaning up
            // here could remove relays the stream is still using. Relays have
            // `sleep_when_idle(300s)` and are cleaned up on profile change.
            if let Ok(events) = result {
            if !events.is_empty() {
                return Ok(events);
            }
        }
    }
    fetch_profile_events_from_relays(filter, timeout).await
}

/// Fetch events from connected relays only (bypasses gossip discovery)
///
/// FAST alternative to fetch_events_aggregated_outbox for pagination:
/// 1. Only queries already-connected relays (no relay discovery)
/// 2. Bypasses the gossip model - no NIP-65 lookups per author
/// 3. Returns results much faster but may miss events from unconnected relays
///
/// Includes client-side author filtering for defense-in-depth against
/// misbehaving relays that ignore filter authors.
pub async fn fetch_events_from_connected_relays(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    fetch_events_from_connected_relays_with_client(&client, filter, timeout).await
}
/// Internal: connected relays fetch using provided client (avoids re-reading NOSTR_CLIENT)
///
/// Dioxus pattern: Get client once, pass same instance through all async operations.
/// No locks held across await points.
pub(crate) async fn fetch_events_from_connected_relays_with_client(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    fetch_events_from_connected_relays_inner(client, filter, timeout, true).await
}

/// F3: background-backfill variant — identical fetch, but skips the 2s
/// NIP-65 read-relay quorum wait. Background data (interaction-count
/// backfills, warm-ups) converges from whatever relay set is already
/// connected; making the user wait for the full read set only adds
/// latency to non-critical fetches.
pub async fn fetch_events_from_connected_relays_background_with_client(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    fetch_events_from_connected_relays_inner(client, filter, timeout, false).await
}

async fn fetch_events_from_connected_relays_inner(
    client: &std::sync::Arc<Client>,
    filter: Filter,
    timeout: std::time::Duration,
    wait_quorum: bool,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    use nostr_sdk::relay::RelayStatus as PoolRelayStatus;
    relay::connection::ensure_relays_ready(client).await;
    // Snapshot guard: `fetch_events_from` silently skips pool members that
    // are still handshaking (NotReady is logged-and-skipped inside the
    // pool), so a snapshot taken during connection warm-up under-serves the
    // fetch — the sparse-first-load race. Wait briefly (bounded) for a
    // quorum of the user's NIP-65 read relays before snapshotting; no-op
    // when the quorum already holds or the user has no read relays.
    // Background backfills (F3) skip the wait entirely.
    if wait_quorum {
        relay::wait_for_user_read_relays_quorum(
            client,
            std::time::Duration::from_secs(2),
            relay::USER_READ_RELAY_QUORUM_FRACTION,
            "fetch_events_from_connected",
        )
        .await;
    }
    let relays = client.relays().await;
    let connected_urls: Vec<nostr::types::url::RelayUrl> = relays
        .iter()
        .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
        .filter_map(|(url, _)| nostr::types::url::RelayUrl::parse(url.as_str()).ok())
        .collect();
    // F1: drop relays that recently refused a subscription (CLOSED with
    // error/auth/block) — re-issuing REQs to them is guaranteed waste.
    let (connected_urls, refused_urls) = relay::refusal::partition_refused(connected_urls);
    if !refused_urls.is_empty() {
        log::info!(
            "Skipping {} recently-refused relays: {:?}",
            refused_urls.len(),
            refused_urls
        );
    }
    if connected_urls.is_empty() {
        log::warn!("No connected relays, falling back to gossip fetch");
        return fetch_events_aggregated_outbox_with_client(client, filter.clone(), timeout).await;
    }
    log::info!(
        "Fast fetching from {} connected relays (bypassing gossip)",
        connected_urls.len()
    );
    let filter_authors = filter.authors.clone();
    let author_set: Option<std::collections::HashSet<_>> = filter_authors
        .as_ref()
        .map(|authors| authors.iter().collect());
    let targets: Vec<(nostr::types::url::RelayUrl, Vec<Filter>)> = connected_urls
        .into_iter()
        .map(|u| (u, vec![filter.clone()]))
        .collect();
    let events = client
        .fetch_events(targets)
        .timeout(timeout)
        .await
        .map_err(|e| format!("Failed to fetch events: {}", e))?;
    let result: Vec<nostr::event::Event> = events
        .into_iter()
        .filter(|event| {
            if let Some(ref authors) = author_set {
                authors.contains(&event.pubkey)
            } else {
                true
            }
        })
        .collect();
    log::info!(
        "Fast fetch completed: {} events (after filtering)",
        result.len()
    );
    Ok(result)
}

/// Snapshot the connected READ-capability pool relays after the same
/// readiness gates as [`fetch_events_from_connected_relays_with_client`]
/// (relay warm-up + the user's NIP-65 read-relay quorum). This is the
/// relay set per-relay pagination walks — the snapshot IS the session's
/// live set (churned-out relays are pruned from the cursor state,
/// churned-in ones arm at the global bound).
pub async fn connected_read_relay_urls(
    client: &std::sync::Arc<Client>,
    context: &str,
) -> Vec<nostr::types::url::RelayUrl> {
    use nostr_sdk::relay::RelayStatus as PoolRelayStatus;
    relay::connection::ensure_relays_ready(client).await;
    relay::wait_for_user_read_relays_quorum(
        client,
        std::time::Duration::from_secs(2),
        relay::USER_READ_RELAY_QUORUM_FRACTION,
        context,
    )
    .await;
    let urls: Vec<nostr::types::url::RelayUrl> = client
        .relays()
        .await
        .iter()
        .filter(|(_, r)| r.status() == PoolRelayStatus::Connected && r.capabilities().has_read())
        .map(|(url, _)| url.clone())
        .collect();
    // F1: per-relay pagination must not walk relays that just refused a
    // subscription — their pages would be all-refusal.
    let (ok, refused) = relay::refusal::partition_refused(urls);
    if !refused.is_empty() {
        log::info!(
            "{}: skipping {} recently-refused relays: {:?}",
            context,
            refused.len(),
            refused
        );
    }
    ok
}

/// One relay's page in a per-relay fetch.
#[derive(Debug, Clone)]
pub struct RelayPageResult {
    pub url: nostr::types::url::RelayUrl,
    pub events: Vec<nostr::event::Event>,
    /// True when the page ended with a clean EOSE. `false` = the relay
    /// **stalled**: it refused the REQ (CLOSE notice — auth-required,
    /// "too many subscriptions"), went silent past the deadline,
    /// disconnected mid-page, or the subscribe itself failed. A stalled
    /// relay's deliveries are still returned, but its cursor must not
    /// advance and it must never be treated as empty (bottomed out).
    pub settled: bool,
}

/// Per-relay page fetch: one targeted stream per `(relay, filter)` pair,
/// preserving which relay answered and how its page ended.
///
/// Unlike `fetch_events` (a per-relay union that hides attribution), this
/// drives independent backward cursors per relay — each relay walks its
/// own timeline, so a sparse relay stops re-serving the window a dense
/// relay already paged past. Each target carries its own REQ and its own
/// deadline (same subscription budget as a targeted `fetch_events`).
///
/// Settled-vs-stalled classification relies on a relay-level stream:
/// EOSE ends the stream cleanly, a CLOSE notice yields an error before
/// the end — but the SDK's own timeout auto-close ALSO ends the stream
/// cleanly, indistinguishable from EOSE. The deadline is therefore ours:
/// a relay that has not finished when it fires counts as stalled, and a
/// stream that ends while the relay is no longer connected counts as
/// stalled too (the connection-closed path ends the stream without an
/// EOSE marker).
pub async fn fetch_page_per_relay(
    client: &std::sync::Arc<Client>,
    targets: Vec<(nostr::types::url::RelayUrl, Filter)>,
    per_relay_timeout: std::time::Duration,
) -> Vec<RelayPageResult> {
    use nostr_sdk::relay::RelayStatus as PoolRelayStatus;

    let pool = client.relays().await;
    let timeout_ms = u32::try_from(per_relay_timeout.as_millis()).unwrap_or(u32::MAX);
    let futures = targets
        .into_iter()
        .map(|(url, filter)| {
            let relay = pool.get(&url).cloned();
            async move {
                let Some(relay) = relay else {
                    // Churned out of the pool between snapshot and fetch.
                    return RelayPageResult {
                        url,
                        events: Vec::new(),
                        settled: false,
                    };
                };
                let mut stream = match relay.stream_events(vec![filter]).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        log::debug!("Per-relay page: subscribe to {url} failed: {e}");
                        return RelayPageResult {
                            url,
                            events: Vec::new(),
                            settled: false,
                        };
                    }
                };
                let mut events = Vec::new();
                let mut errored = false;
                let collect = async {
                    use futures::StreamExt as _;
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(event) => events.push(event),
                            Err(e) => {
                                log::debug!("Per-relay page: {url} refused the REQ: {e}");
                                errored = true;
                                break;
                            }
                        }
                    }
                };
                let deadline = crate::platform::timer::sleep_ms(timeout_ms);
                futures::pin_mut!(collect, deadline);
                let timed_out = match futures::future::select(collect, deadline).await {
                    futures::future::Either::Left(_) => false,
                    futures::future::Either::Right(_) => true,
                };
                // Dropping the unfinished collect future here drops the
                // stream, which cancels the subscription (Drop sends the
                // CLOSE) — no leaked REQ on a stalled relay.
                let still_connected = relay.status() == PoolRelayStatus::Connected;
                RelayPageResult {
                    url,
                    events,
                    settled: !timed_out && !errored && still_connected,
                }
            }
        })
        .collect::<Vec<_>>();
    futures::future::join_all(futures).await
}

/// Fetch events from local nostrdb first (instant), then refresh from relays in background.
/// Only available on native builds. Follows the channel bridge pattern.
#[cfg(feature = "native")]
pub async fn fetch_events_ndb_first(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let ndb_jsons =
        crate::stores::ndb::queries::sdk_filters_to_ndb_jsons(std::slice::from_ref(&filter));
    match crate::stores::ndb::queries::query_note_keys(ndb_jsons, 1000).await {
        Ok(note_keys) if !note_keys.is_empty() => {
            let mut events = Vec::new();
            for (key, _) in note_keys {
                match crate::stores::ndb::queries::get_note_data(key).await {
                    Ok(note_data) => {
                        if let Ok(event) =
                            crate::stores::ndb::queries::note_data_to_event(&note_data)
                        {
                            events.push(event);
                        }
                    }
                    Err(e) => log::warn!("Failed to get note data: {}", e),
                }
            }
            if !events.is_empty() {
                log::info!("Loaded {} events from nostrdb", events.len());
                if let Some(client) = get_client() {
                    let filter_clone = filter.clone();
                    dioxus::prelude::spawn(async move {
                        let _ = client
                            .fetch_events(filter_clone)
                            .timeout(timeout)
                            .await;
                    });
                }
                return Ok(events);
            }
        }
        Ok(_) => {}
        Err(e) => log::warn!("nostrdb query failed: {}, falling back to relays", e),
    }

    fetch_events_from_connected_relays(filter, timeout).await
}

/// Parsed event ID with optional author pubkey and relay hints from nevent decoding.
#[derive(Clone, Debug)]
pub struct ParsedEventId {
    pub event_id: EventId,
    pub author: Option<PublicKey>,
    pub relay_hints: Vec<String>,
}

/// Parse an event ID from various formats (nevent, note, hex) with author and relay hints.
pub fn parse_event_id(id: &str) -> Option<ParsedEventId> {
    use nostr::nips::nip19::Nip19;
    let trimmed = id.trim();
    let normalized = trimmed
        .strip_prefix("nostr:")
        .or_else(|| trimmed.strip_prefix("NOSTR:"))
        .unwrap_or(trimmed);

    if let Ok(event_id) = EventId::from_hex(normalized) {
        return Some(ParsedEventId {
            event_id,
            author: None,
            relay_hints: vec![],
        });
    }

    Nip19::from_bech32(normalized).ok().and_then(|n| match n {
        Nip19::Event(e) => Some(ParsedEventId {
            event_id: e.event_id,
            author: e.author,
            relay_hints: e.relays.iter().map(|r| r.to_string()).collect(),
        }),
        Nip19::EventId(id) => Some(ParsedEventId {
            event_id: id,
            author: None,
            relay_hints: vec![],
        }),
        _ => None,
    })
}

/// Targeted event fetch with hybrid broadcast+targeted strategy.
///
/// 1. Bridge cache check (instant, native only — bridges nostrdb async ingestion gap)
/// 2. DB check (instant, direct primary key lookup)
/// 3. Broadcast to connected relays (fast, uses existing connections)
/// 4. Targeted author relays via NIP-65 resolution (if author known)
/// 5. Relay hints from nevent (if available)
/// 6. Fallback — gossip/outbox model
pub async fn fetch_event_targeted(
    parsed: ParsedEventId,
    timeout: Duration,
) -> std::result::Result<Option<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    let event_id = parsed.event_id;

    // Phase 0: Bridge cache check (instant, native only)
    #[cfg(feature = "native")]
    {
        let bridge_hit = crate::stores::ndb::get_cached_event(&event_id.to_bytes());
        log::debug!("fetch_event_targeted: Phase 0 bridge cache check for {:?} -> {}", event_id.to_hex(), if bridge_hit.is_some() { "HIT" } else { "MISS" });
        if let Some(event) = bridge_hit {
            return Ok(Some(event));
        }
    }

    // Phase 1: DB check (instant) — direct primary key lookup
    match client.database().event_by_id(&event_id).await {
        Ok(Some(event)) => {
            log::debug!("fetch_event_targeted: Phase 1 DB hit for {:?}", event_id.to_hex());
            return Ok(Some(event));
        }
        Ok(None) => {
            log::debug!("fetch_event_targeted: Phase 1 DB miss for {:?}", event_id.to_hex());
        }
        Err(e) => {
            log::debug!("fetch_event_targeted: Phase 1 DB error for {:?}: {}", event_id.to_hex(), e);
        }
    }

    // Phase 1.5: Feed cache fallback. The app-level feed cache (IndexedDB,
    // web-only) holds full event JSON for items displayed in feeds, which can
    // survive eviction from the bounded SDK DB. Seed the SDK DB with it so
    // future lookups hit Phase 1, then return.
    if let Some(event) = crate::stores::feed_cache::get_event_by_id(&event_id).await {
        log::debug!(
            "fetch_event_targeted: Phase 1.5 feed cache hit for {:?}",
            event_id.to_hex()
        );
        let _ = client.database().save_event(&event).await;
        return Ok(Some(event));
    }

    let filter = Filter::new().id(event_id).limit(1);

    // Phase 2: Broadcast to connected relays (fast)
    match fetch_events_from_connected_relays_with_client(
        &client,
        filter.clone(),
        Duration::from_secs(4),
    )
    .await
    {
        Ok(events) if !events.is_empty() => {
            log::debug!("fetch_event_targeted: Phase 2 broadcast hit for {:?}", event_id.to_hex());
            let event = events.into_iter().next();
            #[cfg(feature = "native")]
            if let Some(ref e) = event {
                crate::stores::ndb::cache_event(e);
            }
            return Ok(event);
        }
        Ok(events) => {
            log::debug!("fetch_event_targeted: Phase 2 broadcast miss for {:?} ({} events)", event_id.to_hex(), events.len());
        }
        Err(e) => {
            log::debug!("fetch_event_targeted: Phase 2 broadcast error for {:?}: {}", event_id.to_hex(), e);
        }
    }

    // Phase 3: Targeted author relays (if known)
    if let Some(author) = &parsed.author {
        let relay_urls = relay::coverage::resolve_user_relays(
            &author.to_hex(),
            relay::coverage::RelayPurpose::Write,
        )
        .await;
        let ephemeral = relay::coverage::connect_ephemeral_relays(&client, &relay_urls).await;
        if !ephemeral.connected.is_empty() {
            let result = relay::connection::fetch_events_from_relays(
                &client,
                filter.clone(),
                ephemeral.connected.clone(),
                timeout,
            )
            .await;
            relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
            if let Ok(events) = result {
                if let Some(event) = events.into_iter().next() {
                    log::debug!("fetch_event_targeted: Phase 3 author relays hit for {:?}", event_id.to_hex());
                    #[cfg(feature = "native")]
                    crate::stores::ndb::cache_event(&event);
                    return Ok(Some(event));
                }
            }
        }
    }

    // Phase 4: Relay hints from nevent
    if !parsed.relay_hints.is_empty() {
        let ephemeral =
            relay::coverage::connect_ephemeral_relays(&client, &parsed.relay_hints).await;
        if !ephemeral.connected.is_empty() {
            let result = relay::connection::fetch_events_from_relays(
                &client,
                filter.clone(),
                ephemeral.connected.clone(),
                timeout,
            )
            .await;
            relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
            if let Ok(events) = result {
                if let Some(event) = events.into_iter().next() {
                    log::debug!("fetch_event_targeted: Phase 4 relay hints hit for {:?}", event_id.to_hex());
                    #[cfg(feature = "native")]
                    crate::stores::ndb::cache_event(&event);
                    return Ok(Some(event));
                }
            }
        }
    }

    // Phase 5: Fallback — outbox (gossip model)
    let events = fetch_events_aggregated_outbox(filter, timeout).await?;
    let event = events.into_iter().next();
    #[cfg(feature = "native")]
    if let Some(ref e) = event {
        crate::stores::ndb::cache_event(e);
    }
    Ok(event)
}

/// Fetch a user's events by targeting their NIP-65 write relays.
///
/// Resolves the user's relay list via the three-tier resolver, connects to those
/// relays (ephemerally), and fetches events. Falls back to the standard
/// `fetch_profile_events_from_relays` (SDK gossip) if no NIP-65 data is available
/// or the targeted fetch returns no results.
pub async fn fetch_profile_events_targeted(
    pubkey_hex: &str,
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::event::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    let relay_urls = relay::coverage::resolve_user_relays(
        pubkey_hex,
        relay::coverage::RelayPurpose::Write,
    )
    .await;

    if !relay_urls.is_empty() {
        let ephemeral = relay::coverage::connect_ephemeral_relays(&client, &relay_urls).await;
        if !ephemeral.connected.is_empty() {
            let result = relay::connection::fetch_events_from_relays(
                &client,
                filter.clone(),
                ephemeral.connected.clone(),
                timeout,
            )
            .await;
            relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
            if let Ok(events) = result {
                if !events.is_empty() {
                    log::debug!(
                        "fetch_profile_events_targeted: got {} events from author relays",
                        events.len()
                    );
                    return Ok(events);
                }
            }
        }
    }

    fetch_profile_events_from_relays(filter, timeout).await
}

/// Parse a kind-0 event's content into `Metadata`.
///
/// Blank content is a valid profile wipe (the author cleared their
/// replaceable kind 0) and yields empty metadata rather than an error —
/// erroring would leave a stale older profile rendered forever.
pub fn parse_metadata_content(event: &nostr::event::Event) -> Result<nostr::nips::nip01::Metadata, String> {
    if event.content.trim().is_empty() {
        return Ok(nostr::nips::nip01::Metadata::new());
    }
    nostr::nips::nip01::Metadata::from_json(&event.content)
        .map_err(|e| format!("Failed to parse metadata: {}", e))
}

/// Fetch a user's metadata (kind 0) by targeting their NIP-65 write relays.
///
/// Resolves the user's relay list via the three-tier resolver, connects to those
/// relays (ephemerally), and queries for metadata. Falls back to the SDK's
/// broadcast `fetch_events` (READ-flagged relays) if no NIP-65 data is
/// available or the targeted fetch returns no results.
///
/// Returns the metadata together with the source event's `created_at` so
/// callers can apply strictly-newer replacement semantics (kind 0 is
/// replaceable; arrival order is not freshness).
pub async fn fetch_metadata_targeted(
    pubkey_hex: &str,
    timeout: Duration,
) -> std::result::Result<Option<(nostr::nips::nip01::Metadata, u64)>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    let public_key = PublicKey::from_hex(pubkey_hex).map_err(|e| format!("Invalid pubkey: {}", e))?;

    let relay_urls = relay::coverage::resolve_user_relays(
        pubkey_hex,
        relay::coverage::RelayPurpose::Write,
    )
    .await;

    if !relay_urls.is_empty() {
        let ephemeral = relay::coverage::connect_ephemeral_relays(&client, &relay_urls).await;
        if !ephemeral.connected.is_empty() {
            let filter = Filter::new()
                .author(public_key)
                .kind(Kind::Metadata)
                .limit(1);
            let result = relay::connection::fetch_events_from_relays(
                &client,
                filter,
                ephemeral.connected.clone(),
                timeout,
            )
            .await;
            relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
            // `min()` = first under `Ord for Event` (descending created_at,
            // then id) — the newest snapshot regardless of collection order.
            if let Ok(events) = result {
                if let Some(event) = events.into_iter().min() {
                    match parse_metadata_content(&event) {
                        Ok(metadata) => {
                            log::debug!("fetch_metadata_targeted: found via author relays");
                            return Ok(Some((metadata, event.created_at.as_secs())));
                        }
                        Err(e) => {
                            log::warn!("fetch_metadata_targeted: failed to parse metadata: {}", e);
                        }
                    }
                }
            }
        }
    }

    // Broadcast fallback (same shape as `Client::fetch_metadata`, but via
    // `fetch_events` so the source event's `created_at` is available).
    let filter = Filter::new()
        .author(public_key)
        .kind(Kind::Metadata)
        .limit(1);
    match client.fetch_events(filter).timeout(timeout).await {
        Ok(events) => match events.into_iter().min() {
            Some(event) => parse_metadata_content(&event).map(|metadata| Some((metadata, event.created_at.as_secs()))),
            None => Ok(None),
        },
        Err(e) => Err(format!("Failed to fetch metadata: {}", e)),
    }
}

/// Fetch a user's metadata (kind 0) from the dedicated indexer relays.
///
/// Indexers (purplepag.es, coracle, …) are DISCOVERY-only pool members
/// connected at boot and purpose-built for kind 0 / 10002 / 10050 discovery.
/// This is the **fast path** for cold-profile metadata: it avoids the
/// `resolve_user_relays` + ephemeral-connect + outbox-fetch chain of
/// [`fetch_metadata_targeted`], which can take 5–10 s on a never-seen pubkey.
///
/// Returns the metadata together with the source event's `created_at` (see
/// [`fetch_metadata_targeted`]). Falls back to the broadcast `fetch_events`
/// only if the indexer fetch errors (e.g. indexers not yet connected).
pub async fn fetch_metadata_from_indexers(
    pubkey_hex: &str,
    timeout: Duration,
) -> std::result::Result<Option<(nostr::nips::nip01::Metadata, u64)>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    let public_key = PublicKey::from_hex(pubkey_hex).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .author(public_key)
        .kind(Kind::Metadata)
        .limit(1);
    match relay::nip65::fetch_events_from_indexers(&client, filter, timeout).await {
        Ok(events) => {
            // Pick the newest kind 0 event (replaceable: highest created_at wins).
            if let Some(event) = events.into_iter().max_by_key(|e| e.created_at) {
                match parse_metadata_content(&event) {
                    Ok(metadata) => {
                        log::debug!("fetch_metadata_from_indexers: found via indexers");
                        return Ok(Some((metadata, event.created_at.as_secs())));
                    }
                    Err(e) => {
                        log::warn!("fetch_metadata_from_indexers: failed to parse metadata: {}", e);
                    }
                }
            }
            Ok(None)
        }
        Err(e) => {
            log::debug!("fetch_metadata_from_indexers: indexer fetch failed ({}); falling back to broadcast", e);
            // Indexers not connected yet or other transient error — fall back to
            // the broadcast fetch which targets READ-flagged relays.
            let filter = Filter::new()
                .author(public_key)
                .kind(Kind::Metadata)
                .limit(1);
            match client.fetch_events(filter).timeout(timeout).await {
                Ok(events) => match events.into_iter().next() {
                    Some(event) => parse_metadata_content(&event).map(|metadata| Some((metadata, event.created_at.as_secs()))),
                    None => Ok(None),
                },
                Err(e) => Err(format!("Failed to fetch metadata: {}", e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::stores::nostr_client::eose::fetch_quorum_threshold;

    #[test]
    fn quorum_eose_threshold_is_half_rounded_up() {
        assert_eq!(fetch_quorum_threshold(0), 1);
        assert_eq!(fetch_quorum_threshold(1), 1);
        assert_eq!(fetch_quorum_threshold(2), 1);
        assert_eq!(fetch_quorum_threshold(3), 2);
        assert_eq!(fetch_quorum_threshold(5), 3);
        assert_eq!(fetch_quorum_threshold(10), 5);
        assert_eq!(fetch_quorum_threshold(15), 8);
    }
}
