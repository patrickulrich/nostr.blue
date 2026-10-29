//! Observation layer: derive state from the SDK database, refreshed by live
//! subscription signals (Amethyst's cache-as-bus pattern, built on nostr-sdk
//! primitives — the DB is pull-only, so the dispatcher provides the
//! invalidation signal).
//!
//! `observe_with_client` = one immediate DB query + a live subscription whose
//! events trigger a debounced re-query. Consumers read from the converging
//! DB instead of holding private fetch snapshots; the negentropy sync
//! (`services/sync.rs`) keeps the DB converging even without an open sub.
//!
//! **No third store** (plan guardrail): reads the SDK DB only. Optimistic
//! inserts (local echo of the user's own signed events) belong in the SDK DB
//! via `database().save_event`, never a side cache — see the plan's Phase 6
//! feed_cache bridge decision.
//!
//! Status: primitive landed, first consumer wiring (plan: video_card
//! interaction counts / notification badge) is the Phase 6 follow-up and
//! needs live `dx serve` validation of the live-refresh path.
#![allow(dead_code)] // plan phase 6: first consumer wiring pending
use std::sync::Arc;
use std::time::Duration;

use nostr::filter::Filter;
use nostr_sdk::client::Client;

use crate::stores::subscription_manager::{
    spawn_subscription_forever, OnEvent, SpawnedSubscription, SubTarget,
};

/// Debounce window between a matching live event and the DB re-query: feeds
/// burst (backfills, negentropy syncs), and one re-query per burst is enough.
const REQUERY_DEBOUNCE_MS: u64 = 200;

/// Guard for an observation: holds the live subscription that drives
/// re-queries. Drop (or `shutdown`) to stop observing; the DB data stays.
pub struct ObserveGuard {
    sub: SpawnedSubscription,
}

impl ObserveGuard {
    #[allow(dead_code)] // first consumer wiring is plan phase 6 follow-up
    pub async fn shutdown(self) {
        self.sub.shutdown().await;
    }
}

/// Query-filter factory: re-derived for every read so time-window
/// derivations (e.g. unread-since-last-checked) pick up window resets that
/// happen while observing.
pub type MakeQueryFilter = Arc<dyn Fn() -> Filter + Send + Sync>;

/// Immediate DB read → `on_events`. Runtime-free core (unit-testable).
pub async fn query_and_emit(
    client: &Client,
    filter: &Filter,
    on_events: &Arc<dyn Fn(Vec<nostr::event::Event>) + Send + Sync>,
) {
    match client.database().query(filter.clone()).await {
        Ok(events) => on_events(events.into_iter().collect()),
        Err(e) => log::warn!("observe: initial DB query failed: {e}"),
    }
}

/// Observe `filter`: immediate DB query + live re-queries.
///
/// `on_events` receives the full (deduplicated, SDK-ordered) result set each
/// time — initial query plus one per debounced burst of matching live
/// events. Callbacks use the dispatcher-safe shape (Dioxus signals are
/// thread-local).
///
/// App-lifetime scope (`spawn_forever` — requires the app's Dioxus runtime;
/// the live path is exercised via `dx serve`, the initial read via unit
/// tests). Guards must be stored by the caller (a store or component signal)
/// and shut down explicitly on unmount/logout.
pub async fn observe_with_client(
    client: Arc<Client>,
    // Static, wide window for the live signal subscription (relay load is
    // bounded by the idle auto-close; the DB keeps whatever arrives).
    sub_filter: Filter,
    // Re-derived per read — the window the `on_events` result set reflects.
    make_query: MakeQueryFilter,
    on_events: Arc<dyn Fn(Vec<nostr::event::Event>) + Send + Sync>,
) -> Result<ObserveGuard, String> {
    // Initial read: DB only, no network.
    let initial = make_query();
    query_and_emit(&client, &initial, &on_events).await;

    // Live signal: a subscription with the same filter drives re-queries.
    // NO idle auto-close: `spawn_subscription_forever` subscribes exactly
    // once (the "forever" is the listener's scope, not resubscription), so
    // an auto-closing REQ would silently freeze the guard's invalidation
    // signal for the rest of the session ~10min after EOSE. Observations
    // are app-lifetime: one standing REQ, relay load bounded by the
    // single-subscription footprint, the DB keeps whatever arrives.
    let db_client = client.clone();
    // Trailing-edge debounce state: a shared "signal pending" flag
    // collapses bursts — without it every matching event spawned its own
    // task and its own full DB re-query (a negentropy sync delivering N
    // events meant N simultaneous main-thread queries).
    #[allow(clippy::arc_with_non_send_sync)] // dispatcher-safe callback shape
    let requery_pending: Arc<std::sync::atomic::AtomicBool> =
        Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[allow(clippy::arc_with_non_send_sync)] // dispatcher-safe callback shape
    let on_event: OnEvent = {
        let db_client = db_client.clone();
        let requery_pending = requery_pending.clone();
        Arc::new(std::sync::Mutex::new(Box::new(
            move |_event: &nostr::event::Event| {
                use std::sync::atomic::Ordering;
                // Record the signal; ensure exactly ONE debounce task is in
                // flight per guard (bursts collapse into one re-query).
                if !requery_pending.swap(true, Ordering::AcqRel) {
                    let db_client = db_client.clone();
                    let on_events = on_events.clone();
                    let make_query = make_query.clone();
                    let pending = requery_pending.clone();
                    dioxus::prelude::spawn(async move {
                        loop {
                            crate::stores::nostr_client::platform_sleep_ms(
                                REQUERY_DEBOUNCE_MS,
                            )
                            .await;
                            if !pending.load(std::sync::atomic::Ordering::Acquire) {
                                // No signal since the last query — quiescent.
                                break;
                            }
                            // Consume this round's signals; anything that
                            // arrives during the query restarts the window
                            // on the next iteration.
                            pending.store(false, std::sync::atomic::Ordering::Release);
                            let filter = make_query();
                            match db_client.database().query(filter).await {
                                Ok(events) => on_events(events.into_iter().collect()),
                                Err(e) => log::debug!("observe: re-query failed: {e}"),
                            }
                        }
                    });
                }
            },
        )))
    };

    let sub = spawn_subscription_forever(
        client,
        SubTarget::Auto(sub_filter),
        // None: no auto-close (see above).
        None,
        // Lifecycle is the guard's; no per-event stale check.
        #[allow(clippy::arc_with_non_send_sync)]
        Arc::new(std::sync::Mutex::new(Box::new(|| false))),
        on_event,
    )
    .await?;

    Ok(ObserveGuard { sub })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn observe_initial_read_emits_db_results() {
        use nostr::event::{EventBuilder, FinalizeEvent, Kind};
        use nostr::key::Keys;

        let relay = nostr_sdk::local_relay::MockRelay::run().await.unwrap();
        let url = relay.url().await.to_string();
        // Client::default() uses MemoryEventsTracker (a seen-ID tracker that
        // does NOT store events); observe reads a real database, so build
        // the client with the full in-memory one — mirroring the app, whose
        // clients always carry a real (sqlite/ndb) database.
        let client = Client::builder()
            .database(nostr_memory::MemoryDatabase::unbounded())
            .build();
        client.add_relay(&url).await.unwrap();
        client.connect().await;

        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "observed")
            .finalize(&keys)
            .unwrap();
        // Seed the DB (send_event auto-persists).
        client
            .send_event(&event)
            .to([&url])
            .ok_timeout(Duration::from_secs(5))
            .await
            .unwrap();

        let seen = Arc::new(std::sync::Mutex::new(Vec::<nostr::event::EventId>::new()));
        let seen_cb = seen.clone();
        let on_events: Arc<dyn Fn(Vec<nostr::event::Event>) + Send + Sync> =
            Arc::new(move |events| {
                let mut guard = seen_cb.lock().unwrap();
                for e in events {
                    guard.push(e.id);
                }
            });

        let filter = Filter::new().kind(Kind::TextNote).limit(10);
        // Runtime-free core: the initial DB read observe_with_client performs
        // (with a re-derivable query window).
        let make_query: crate::stores::nostr_client::observe::MakeQueryFilter =
            Arc::new(move || filter.clone());
        let query = make_query();
        query_and_emit(&client, &query, &on_events).await;

        assert!(
            seen.lock().unwrap().contains(&event.id),
            "seeded event emitted from the DB read"
        );
    }

    #[allow(dead_code)] // live-refresh path doc: requires the app Dioxus runtime
    async fn _observe_full_path_needs_app_runtime() {
        // The full observe_with_client (live subscription + debounced
        // re-queries) requires the app's Dioxus runtime for
        // spawn_subscription_forever; exercise it via `dx serve` (plan
        // phase 6 first-consumer wiring), not unit tests.
    }
}
