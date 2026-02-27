use crate::stores::{auth_store, nostr_client, settings_store, subscription_manager};
use crate::utils::notification_nip78;
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use crate::platform::storage;
use nostr_sdk::{Filter, FromBech32, Kind, PublicKey, SubscriptionId};
const NOTIFICATIONS_CHECKED_AT_KEY: &str = "notifications_checked_at";
/// Minimum interval between NIP-78 publishes (10 minutes)
const PUBLISH_THROTTLE_SECONDS: i64 = 10 * 60;
/// Global signal to track unread notification count
pub static UNREAD_COUNT: GlobalSignal<usize> = Signal::global(|| 0);
/// Track the current real-time subscription ID
pub static SUBSCRIPTION_ID: GlobalSignal<Option<SubscriptionId>> = Signal::global(|| {
    None
});
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
    log::debug!("Loaded notifications checked_at from localStorage: {}", timestamp);
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
    let settings = settings_store::SETTINGS.read();
    if !settings.sync_notifications {
        log::debug!("NIP-78 sync disabled, skipping publish");
        return;
    }
    drop(settings);
    if !auth_store::is_authenticated() {
        log::debug!("Not authenticated, skipping NIP-78 publish");
        return;
    }
    let last_published = *LAST_PUBLISHED_AT.read();
    let time_since_last = timestamp - last_published;
    if last_published > 0 && time_since_last < PUBLISH_THROTTLE_SECONDS {
        log::debug!(
            "Throttled NIP-78 publish (last: {} seconds ago, need: {})", time_since_last,
            PUBLISH_THROTTLE_SECONDS
        );
        return;
    }
    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::error!("No client available for NIP-78 publish");
            return;
        }
    };
    let builder = notification_nip78::create_checked_at_event(timestamp);
    match client.send_event_builder(builder).await {
        Ok(output) => {
            log::info!("Published notification checked_at to NIP-78: {}", output.id());
            *LAST_PUBLISHED_AT.write() = timestamp;
        }
        Err(e) => {
            log::error!("Failed to publish checked_at to NIP-78: {}", e);
        }
    }
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
    match client.fetch_events(filter, std::time::Duration::from_secs(5)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                if let Some(relay_timestamp) = notification_nip78::parse_checked_at_event(
                    &event,
                ) {
                    let local_timestamp = get_checked_at();
                    let merged_timestamp = relay_timestamp.max(local_timestamp);
                    log::info!(
                        "Fetched NIP-78 checked_at: relay={}, local={}, merged={}",
                        relay_timestamp, local_timestamp, merged_timestamp
                    );
                    if merged_timestamp > local_timestamp {
                        *NOTIFICATIONS_CHECKED_AT.write() = merged_timestamp;
                        if let Err(e) = storage::set(
                            NOTIFICATIONS_CHECKED_AT_KEY,
                            &merged_timestamp,
                        ) {
                            log::error!("Failed to save merged checked_at: {}", e);
                        }
                    }
                } else {
                    log::warn!("Failed to parse NIP-78 checked_at event");
                }
            } else {
                log::info!("No NIP-78 checked_at event found on relays");
            }
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
        std::time::Duration::from_secs(5), "start_realtime_subscription"
    ).await;
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
    let filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost, Kind::Reaction, Kind::ZapReceipt])
        .pubkey(my_pubkey)
        .limit(20);
    log::info!("Starting real-time notification subscription using gossip (limit: 20)");
    let subscription_result = subscription_manager::subscribe_realtime(
            &client,
            filter.clone(),
            Some(600),
        )
        .await;
    match subscription_result {
        Ok(sub_id) => {
            SUBSCRIPTION_ID.write().replace(sub_id.clone());
            log::info!("Real-time notification subscription started: {:?}", sub_id);
            let my_pubkey_clone = my_pubkey;
            spawn(async move {
                let mut notifications = client.notifications();
                while let Ok(notification) = notifications.recv().await {
                    if let nostr_sdk::RelayPoolNotification::Event {
                        subscription_id,
                        event,
                        ..
                    } = notification {
                        if subscription_id != sub_id {
                            continue;
                        }
                        if event.pubkey == my_pubkey_clone {
                            continue;
                        }
                        let checked_at = get_checked_at();
                        let event_timestamp = event.created_at.as_secs() as i64;
                        if event_timestamp > checked_at {
                            log::debug!(
                                "New notification received: kind={}, from={}, created_at={}",
                                event.kind, event.pubkey, event_timestamp
                            );
                            increment_unread_count();
                        }
                    }
                }
                log::warn!(
                    "Notification listener loop ended - clearing subscription for reconnect"
                );
                *SUBSCRIPTION_ID.write() = None;
            });
        }
        Err(e) => {
            log::error!("Failed to start notification subscription: {}", e);
        }
    }
}
/// Stop real-time notification subscription
pub async fn stop_realtime_subscription() {
    let sub_id = SUBSCRIPTION_ID.read().clone();
    if let Some(id) = sub_id {
        if let Some(client) = nostr_client::get_client() {
            log::info!("Stopping real-time notification subscription: {:?}", id);
            subscription_manager::unsubscribe(&client, &id).await;
        }
        *SUBSCRIPTION_ID.write() = None;
    }
}
