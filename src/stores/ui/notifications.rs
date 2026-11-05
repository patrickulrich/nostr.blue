use crate::platform::storage;
use crate::stores::{auth_store, nostr_client, settings_store, subscription_manager};
use crate::utils::notification_nip78;
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_sdk::prelude::{Filter, FromBech32, Kind, PublicKey, SingleLetterTag, SubscriptionId, EventId};
use std::sync::Arc;
const NOTIFICATIONS_CHECKED_AT_KEY: &str = "notifications_checked_at";
/// Minimum interval between NIP-78 publishes (10 minutes)
const PUBLISH_THROTTLE_SECONDS: i64 = 10 * 60;
/// Global signal to track unread notification count
pub static UNREAD_COUNT: GlobalSignal<usize> = Signal::global(|| 0);
/// Track the current real-time subscription ID
pub static SUBSCRIPTION_ID: GlobalSignal<Option<SubscriptionId>> = Signal::global(|| None);
static REALTIME_SUB: GlobalSignal<
    Option<crate::stores::subscription_manager::SpawnedSubscription>,
> = Signal::global(|| None);
/// Phase 6 staged rollout: when true, the unread badge is DERIVED from the
/// SDK database via `nostr_client::observe` (initial catch-up of offline
/// events + debounced live re-queries through a re-derivable
/// unread-since-last-checked window) instead of increment-only live
/// counting. Flip to `true` and `dx serve` to validate; leave off until the
/// live-refresh path is confirmed (plan phase 6 first consumer).
const OBSERVE_BADGE_ENABLED: bool = false;
static OBSERVE_BADGE_GUARD: GlobalSignal<
    Option<crate::stores::nostr_client::observe::ObserveGuard>,
> = Signal::global(|| None);
/// Global signal to track when notifications were last checked
/// Timestamp in Unix seconds (i64)
pub static NOTIFICATIONS_CHECKED_AT: GlobalSignal<i64> = Signal::global(|| 0);
/// Track when we last published a NIP-78 event (for throttling)
pub static LAST_PUBLISHED_AT: GlobalSignal<i64> = Signal::global(|| 0);
/// Set the unread notification count
#[allow(dead_code)]
pub fn set_unread_count(count: usize) {
    *UNREAD_COUNT.write() = count;
}
/// Get the current unread count
pub fn get_unread_count() -> usize {
    *UNREAD_COUNT.read()
}
/// Clear the unread notification count (when user views notifications)
pub fn clear_unread_count() {
    *UNREAD_COUNT.write() = 0;
}
/// Increment unread count
#[allow(dead_code)]
pub fn increment_unread_count() {
    *UNREAD_COUNT.write() += 1;
}
/// Load the last checked timestamp from localStorage and update the signal
pub fn load_checked_at() {
    let timestamp = storage::get::<i64>(NOTIFICATIONS_CHECKED_AT_KEY).unwrap_or(0);
    *NOTIFICATIONS_CHECKED_AT.write() = timestamp;
    log::debug!(
        "Loaded notifications checked_at from localStorage: {}",
        timestamp
    );
}
/// Get the current checked_at timestamp
pub fn get_checked_at() -> i64 {
    *NOTIFICATIONS_CHECKED_AT.read()
}
/// Set the checked_at timestamp, updating both the signal and localStorage
/// Optionally publishes to NIP-78 if sync is enabled (throttled to once per 10 min)
pub fn set_checked_at(timestamp: i64) {
    *NOTIFICATIONS_CHECKED_AT.write() = timestamp;
    if let Err(e) = storage::set(NOTIFICATIONS_CHECKED_AT_KEY, &timestamp) {
        log::error!("Failed to save checked_at to localStorage: {}", e);
    }
    log::debug!("Set notifications checked_at: {}", timestamp);
    clear_unread_count();
    spawn(async move {
        publish_checked_at_if_enabled(timestamp).await;
    });
}
/// Publish checked_at to NIP-78 if sync is enabled and throttle allows
async fn publish_checked_at_if_enabled(timestamp: i64) {
    let settings = settings_store::SETTINGS.peek();
    if !settings.sync_notifications {
        log::debug!("NIP-78 sync disabled, skipping publish");
        return;
    }
    drop(settings);
    if !auth_store::is_authenticated() {
        log::debug!("Not authenticated, skipping NIP-78 publish");
        return;
    }
    let last_published = *LAST_PUBLISHED_AT.peek();
    let time_since_last = timestamp - last_published;
    if last_published > 0 && time_since_last < PUBLISH_THROTTLE_SECONDS {
        log::debug!(
            "Throttled NIP-78 publish (last: {} seconds ago, need: {})",
            time_since_last,
            PUBLISH_THROTTLE_SECONDS
        );
        return;
    }
    let _client = match nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::error!("No client available for NIP-78 publish");
            return;
        }
    };
    let builder = notification_nip78::create_checked_at_event(timestamp);
    let event = match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
        Ok(e) => e,
        Err(e) => {
            log::error!("Failed to sign checked_at: {}", e);
            return;
        }
    };
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("notification".to_string()),
        None,
        std::collections::HashMap::new(),
    ).await;
    *LAST_PUBLISHED_AT.write() = timestamp;
}
/// Fetch and merge notification checked_at from NIP-78 relays
/// Merges relay timestamp with localStorage (uses maximum of both)
/// Should be called on login after load_checked_at()
pub async fn fetch_and_merge_from_nip78() {
    let settings = settings_store::SETTINGS.read();
    if !settings.sync_notifications {
        log::debug!("NIP-78 sync disabled, skipping fetch");
        return;
    }
    drop(settings);
    if !auth_store::is_authenticated() {
        log::debug!("Not authenticated, skipping NIP-78 fetch");
        return;
    }
    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::error!("No client available for NIP-78 fetch");
            return;
        }
    };
    let my_pubkey_str = match auth_store::get_pubkey() {
        Some(pk) => pk,
        None => {
            log::error!("Cannot fetch NIP-78: no pubkey");
            return;
        }
    };
    let my_pubkey = match PublicKey::from_bech32(&my_pubkey_str)
        .or_else(|_| PublicKey::from_hex(&my_pubkey_str))
    {
        Ok(pk) => pk,
        Err(e) => {
            log::error!("Invalid pubkey for NIP-78 fetch: {}", e);
            return;
        }
    };
    let filter = Filter::new()
        .author(my_pubkey)
        .kind(Kind::from(30078_u16))
        .identifier("notifications_checked_at")
        .limit(1);
    nostr_client::ensure_relays_ready(&client).await;
    match crate::stores::user_prefs::fetch::fetch_newest_with_quorum(
        &client,
        filter,
        std::time::Duration::from_secs(6),
    )
    .await
    {
        Ok(Some(event)) => {
            if let Some(relay_timestamp) = notification_nip78::parse_checked_at_event(&event) {
                let local_timestamp = get_checked_at();
                let merged_timestamp = relay_timestamp.max(local_timestamp);
                log::info!(
                    "Fetched NIP-78 checked_at: relay={}, local={}, merged={}",
                    relay_timestamp,
                    local_timestamp,
                    merged_timestamp
                );
                if merged_timestamp > local_timestamp {
                    *NOTIFICATIONS_CHECKED_AT.write() = merged_timestamp;
                    if let Err(e) = storage::set(NOTIFICATIONS_CHECKED_AT_KEY, &merged_timestamp)
                    {
                        log::error!("Failed to save merged checked_at: {}", e);
                    }
                }
            } else {
                log::warn!("Failed to parse NIP-78 checked_at event");
            }
        }
        Ok(None) => {
            log::info!("No NIP-78 checked_at event found on relays");
        }
        Err(e) => {
            log::warn!("Failed to fetch NIP-78 checked_at: {}", e);
        }
    }
}
/// Start real-time notification subscription (limit: 20 for recent notifications only)
/// This should be called once when the app initializes and user is authenticated
pub async fn start_realtime_subscription() {
    let is_authenticated = auth_store::is_authenticated();
    let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
    if !is_authenticated || !client_initialized {
        log::warn!(
            "Cannot start notification subscription: not authenticated or client not initialized"
        );
        return;
    }
    // End the Write borrow at the semicolon: an if-let scrutinee temporary
    // lives to the end of the statement (edition 2021), which would hold the
    // signal guard across the `shutdown().await` below and panic with
    // `AlreadyBorrowed` if a concurrent task touches REALTIME_SUB.
    let old_sub = REALTIME_SUB.write().take();
    if let Some(old_sub) = old_sub {
        log::info!("Shutting down old notification subscription");
        old_sub.shutdown().await;
    }
    if SUBSCRIPTION_ID.read().is_some() {
        log::debug!("Real-time notification subscription already active");
        return;
    }
    let my_pubkey_str = match auth_store::get_pubkey() {
        Some(pk) => pk,
        None => {
            log::error!("Cannot start notification subscription: no pubkey");
            return;
        }
    };
    let my_pubkey = match PublicKey::from_bech32(&my_pubkey_str)
        .or_else(|_| PublicKey::from_hex(&my_pubkey_str))
    {
        Ok(pk) => pk,
        Err(e) => {
            log::error!("Invalid pubkey for notification subscription: {}", e);
            return;
        }
    };
    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::error!("Cannot start notification subscription: no client");
            return;
        }
    };
    // Wait for user relay lists before subscribing (prevents empty subscriptions on NIP-46 mobile)
    crate::stores::relay::wait_for_user_relays(
        std::time::Duration::from_secs(5),
        "start_realtime_subscription",
    )
    .await;
    // Re-check after await: another call may have started, or user may have logged out
    if SUBSCRIPTION_ID.read().is_some() {
        log::debug!("Notification subscription started by another call during relay wait");
        return;
    }
    if auth_store::get_pubkey().as_deref() != Some(my_pubkey_str.as_str()) {
        log::warn!("Pubkey changed during relay wait, aborting notification subscription");
        return;
    }
    nostr_client::ensure_relays_ready(&client).await;
    // Bound the backfill: events older than `checked_at` are ignored by the
    // unread counter anyway (fetched-then-discarded today), and without a
    // `since` they are still persisted into the shared event database where
    // paginated home-feed reads can surface them. Cap the lookback at 7 days
    // so fresh installs (checked_at = 0) don't pull the full mention history.
    let now_secs = crate::platform::timestamp::now_secs();
    let checked_at = get_checked_at().max(0) as u64;
    let since_secs = now_secs.saturating_sub(7 * 86400).max(checked_at);
    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Repost,
            Kind::Reaction,
            Kind::ZapReceipt,
        ])
        .pubkey(my_pubkey)
        .since(nostr::types::time::Timestamp::from(since_secs))
        .limit(20);
    // Phase 6 first consumer (staged): DB-derived unread badge via observe.
    // Same event universe as the live counter below (mention kinds, p-tag =
    // me, own-author events excluded in the callback), but the count comes
    // from a re-derivable unread-since-last-checked DB query — so offline
    // arrivals are counted on boot and "mark checked" resets the window.
    if OBSERVE_BADGE_ENABLED {
        let my_pk = my_pubkey;
        let kinds = vec![
            Kind::TextNote,
            Kind::Repost,
            Kind::Reaction,
            Kind::ZapReceipt,
        ];
        // Live signal: static, wide (7-day clamped) window.
        let signal_filter = Filter::new()
            .kinds(kinds.clone())
            .pubkey(my_pubkey)
            .since(nostr::types::time::Timestamp::from(since_secs));
        let make_query: crate::stores::nostr_client::observe::MakeQueryFilter = Arc::new(
            move || {
                let checked = crate::platform::timestamp::now_secs()
                    .saturating_sub(7 * 86400)
                    .max(get_checked_at().max(0) as u64);
                Filter::new()
                    .kinds(kinds.clone())
                    .pubkey(my_pk)
                    .since(nostr::types::time::Timestamp::from(checked))
            },
        );
        let my_pk_count = my_pubkey;
        let on_events: Arc<dyn Fn(Vec<nostr::event::Event>) + Send + Sync> = Arc::new(
            move |events: Vec<nostr::event::Event>| {
                // Mirror the live counter's own-author exclusion.
                let count = events
                    .iter()
                    .filter(|e| e.pubkey != my_pk_count)
                    .count();
                set_unread_count(count);
            },
        );
        match crate::stores::nostr_client::observe::observe_with_client(
            client.clone(),
            signal_filter,
            make_query,
            on_events,
        )
        .await
        {
            Ok(guard) => {
                log::info!("Notification badge observing (phase 6 staged)");
                *OBSERVE_BADGE_GUARD.write() = Some(guard);
                // Still run the e/q-tag helper subscriptions (client is
                // consumed here; the live-counter path below only runs when
                // observe failed, in which case this spawn never happened).
                let client_for_etag = client.clone();
                spawn(async move {
                    spawn_etag_qtag_subscriptions(client_for_etag, my_pubkey).await;
                });
            }            Err(e) => {
                log::error!("observe badge failed, falling back to live counter: {e}");
                // Fall through to the live-counter path below.
            }
        }
        if OBSERVE_BADGE_GUARD.read().is_some() {
            return;
        }
    }

    log::info!("Starting real-time notification subscription using gossip (limit: 20)");
    // Same idle auto-close as the previous subscribe_realtime(Some(600)).
    let close_opts = nostr_sdk::relay::SubscribeAutoCloseOptions::default().exit_policy(
        nostr_sdk::relay::ReqExitPolicy::WaitDurationAfterEOSE(std::time::Duration::from_secs(
            600,
        )),
    );
    let my_pubkey_clone = my_pubkey;
    let client_for_etag = client.clone();
    let result = subscription_manager::spawn_subscription_forever_with_end(
        client,
        subscription_manager::SubTarget::Auto(filter),
        Some(close_opts),
        // App-lifetime: lifecycle owned by start/stop below.
        #[allow(clippy::arc_with_non_send_sync)]
        std::sync::Arc::new(std::sync::Mutex::new(Box::new(|| false))),
        #[allow(clippy::arc_with_non_send_sync)]
        std::sync::Arc::new(std::sync::Mutex::new(Box::new(
            move |event: &nostr::event::Event| {
                if event.pubkey == my_pubkey_clone {
                    return;
                }
                let checked_at = get_checked_at();
                let event_timestamp = event.created_at.as_secs() as i64;
                if event_timestamp > checked_at {
                    log::debug!(
                        "New notification received: kind={}, from={}, created_at={}",
                        event.kind,
                        event.pubkey,
                        event_timestamp
                    );
                    increment_unread_count();
                }
            },
        ))),
        // Stream-end hook: clear the sub-id so the reconnect logic in the
        // app shell re-arms the subscription (same contract the raw loop's
        // exit path had).
        #[allow(clippy::arc_with_non_send_sync)]
        std::sync::Arc::new(std::sync::Mutex::new(Box::new(|| {
            log::warn!(
                "Notification listener ended - clearing subscription for reconnect"
            );
            *SUBSCRIPTION_ID.write() = None;
            *REALTIME_SUB.write() = None;
        }))),
    )
    .await;
    match result {
        Ok(sub) => {
            SUBSCRIPTION_ID.write().replace(sub.sub_id().clone());
            log::info!(
                "Real-time notification subscription started: {:?}",
                sub.sub_id()
            );
            *REALTIME_SUB.write() = Some(sub);

            spawn(async move {
                spawn_etag_qtag_subscriptions(client_for_etag, my_pubkey).await;
            });
        }
        Err(e) => {
            log::error!("Failed to start notification subscription: {}", e);
        }
    }
}
/// Stop real-time notification subscription
pub async fn stop_realtime_subscription() {
    // Guard dropped at the semicolon (see start_realtime_subscription).
    let sub = REALTIME_SUB.write().take();
    if let Some(sub) = sub {
        log::info!("Stopping real-time notification subscription");
        sub.shutdown().await;
    }
    let guard = OBSERVE_BADGE_GUARD.write().take();
    if let Some(guard) = guard {
        log::info!("Stopping observe-based notification badge");
        guard.shutdown().await;
    }
    *SUBSCRIPTION_ID.write() = None;
}

async fn spawn_etag_qtag_subscriptions(client: std::sync::Arc<nostr_sdk::client::Client>, my_pubkey: PublicKey) {
    let my_event_ids: Vec<EventId> = {
        let filter = Filter::new()
            .author(my_pubkey)
            .kinds(vec![Kind::TextNote, Kind::Custom(20), Kind::LongFormTextNote])
            .limit(500);
        match crate::stores::nostr_client::fetch_with_client(
            &client,
            crate::stores::nostr_client::FetchTarget::Auto(vec![filter]),
            crate::stores::nostr_client::FetchProfile::Ui,
        )
        .await
        {
            Ok(events) => events.into_iter().map(|e| e.id).collect(),
            Err(e) => {
                log::warn!("Failed to fetch own event IDs for e-tag/q-tag subscriptions: {}", e);
                return;
            }
        }
    };

    if my_event_ids.is_empty() {
        return;
    }

    let since = get_checked_at();
    let since_ts = nostr::types::time::Timestamp::from(since as u64);

    for chunk in my_event_ids.chunks(50) {
        let reply_filter = Filter::new()
            .kind(Kind::TextNote)
            .events(chunk.to_vec())
            .since(since_ts)
            .limit(100);
        if let Err(e) = subscription_manager::subscribe_realtime(&client, reply_filter, Some(600))
            .await
        {
            log::debug!("Failed to subscribe for e-tag replies: {}", e);
        }

        let quote_filter = Filter::new()
            .kind(Kind::TextNote)
            .custom_tags(
                SingleLetterTag::from_char('q').unwrap(),
                chunk.iter().map(|id| id.to_hex()),
            )
            .since(since_ts)
            .limit(100);
        if let Err(e) = subscription_manager::subscribe_realtime(&client, quote_filter, Some(600))
            .await
        {
            log::debug!("Failed to subscribe for q-tag quotes: {}", e);
        }
    }
    log::info!(
        "Started e-tag/q-tag subscriptions for {} own events",
        my_event_ids.len()
    );
}
