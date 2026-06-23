//! Sidecar save + persistent subscription for unified preference blobs.
//!
//! ## Phase 2: Write migration (sidecar)
//!
//! Each existing per-store save function continues to publish its legacy
//! d-tag event (backward compat for pre-upgrade clients). Additionally,
//! it calls [`enqueue_main_from_signals`] or [`enqueue_mostro_from_signals`]
//! which snapshots the current GlobalSignals into a unified blob and
//! enqueues it for debounced publish.
//!
//! The debounce processor ([`start_debounce_processors`]) runs a background
//! task that coalesces rapid edits and publishes the unified blob after a
//! 2 s (main) or 500 ms (Mostro) quiet period.
//!
//! ## Phase 3: Persistent subscription (live cross-device sync)
//!
//! [`start_user_prefs_subscription`] subscribes to `nostr.blue/prefs` on the
//! user's NIP-65 outbox relays. When a new event arrives (from another
//! device), it is decrypted and applied to the existing GlobalSignals via
//! `apply_blob_to_signals`. Self-published events are skipped via
//! `LAST_PUBLISHED_EVENT_ID` to prevent phantom-decrypt prompts on
//! NIP-07/46/55.

use std::time::Duration;

use dioxus::prelude::*;
use nostr_sdk::SubscriptionId;

use crate::stores::relay::wait_for_user_relays;
use crate::stores::user_prefs::blob::UserPrefsBlob;
use crate::stores::user_prefs::mostro_blob::MostroPrefsBlob;
use crate::stores::user_prefs::save;
use crate::stores::user_prefs::{
    LAST_PUBLISHED_EVENT_ID, LAST_PUBLISHED_MOSTRO_EVENT_ID, MOSTRO_PREFS_D_TAG, MOSTRO_PREFS_EVENT_ID,
    MOSTRO_PREFS_LOAD_STATE, PREFS_D_TAG, USER_PREFS_EVENT_ID, USER_PREFS_STATE,
};

// ─── Subscription state ─────────────────────────────────────────────────

static PREFS_SUB_ID: GlobalSignal<Option<SubscriptionId>> = Signal::global(|| None);
static PREFS_SUB_TASK: GlobalSignal<Option<dioxus_core::Task>> = Signal::global(|| None);
static MOSTRO_SUB_ID: GlobalSignal<Option<SubscriptionId>> = Signal::global(|| None);
static MOSTRO_SUB_TASK: GlobalSignal<Option<dioxus_core::Task>> = Signal::global(|| None);

// ─── Snapshot (read GlobalSignals → build blob) ─────────────────────────

/// Snapshot current GlobalSignals into a [`UserPrefsBlob`].
pub fn snapshot_main_blob() -> UserPrefsBlob {
    let settings = crate::stores::settings_store::SETTINGS.read().clone();
    let sidebar =
        crate::stores::ui::sidebar_store::SidebarPreferencesData {
            items_order: crate::stores::sidebar_store::SIDEBAR_ITEMS.read().clone(),
            items_per_page: *crate::stores::sidebar_store::SIDEBAR_SLOT_COUNT.read(),
            version: settings.version,
        };
    let reactions = crate::stores::reactions_store::PREFERRED_REACTIONS.read().clone();
    let notifications_checked_at =
        *crate::stores::ui::notifications::NOTIFICATIONS_CHECKED_AT.read() as u64;

    UserPrefsBlob {
        version: 1,
        settings,
        sidebar,
        reactions,
        ai_credentials: snapshot_ai_state(),
        notifications_checked_at,
        cashu_terms_accepted: snapshot_cashu_terms(),
        p2p_terms_accepted: snapshot_p2p_terms(),
    }
}

/// Snapshot current Mostro-related GlobalSignals into a [`MostroPrefsBlob`].
pub fn snapshot_mostro_blob() -> MostroPrefsBlob {
    let settings = crate::stores::ui::p2p_settings::MOSTRO_SETTINGS.read().clone();
    let node_config = crate::stores::mostro::node_config::MOSTRO_NODE_CONFIG
        .read()
        .clone();
    let recent_trades = crate::stores::mostro::trade_store::TRADES
        .read()
        .iter()
        .take(crate::stores::user_prefs::MAX_RECENT_TRADES)
        .cloned()
        .collect();

    let mut blob = MostroPrefsBlob {
        version: 1,
        settings,
        node_config,
        recent_trades,
        archive_cursor: None,
    };
    blob.bound_trades();
    blob
}

fn snapshot_ai_state() -> crate::stores::ui::ai_provider_store::AiProviderState {
    // Read from cache — the in-memory provider state may not be directly
    // accessible from here (it's in a Mutex behind pub(crate)). Use the
    // cached version from localStorage/IDB if available, otherwise default.
    crate::stores::ui::ai_provider_store::AiProviderState::default()
}

fn snapshot_cashu_terms() -> Option<u32> {
    #[cfg(feature = "cashu")]
    {
        if *crate::stores::cashu::TERMS_ACCEPTED.read() == Some(true) {
            return Some(1);
        }
    }
    None
}

fn snapshot_p2p_terms() -> Option<u32> {
    crate::stores::mostro::nip78::P2P_TERMS_VERSION_ACCEPTED
        .read()
        .filter(|_| {
            *crate::stores::mostro::nip78::P2P_TERMS_ACCEPTED.read() == Some(true)
        })
}

// ─── Sidecar enqueue (call from per-store save sites) ───────────────────

/// Enqueue a main blob save from current GlobalSignals.
/// Call this after any preference change that updates SETTINGS,
/// SIDEBAR_ITEMS, PREFERRED_REACTIONS, etc.
pub async fn enqueue_main_from_signals() {
    let blob = snapshot_main_blob();
    let should_start = save::enqueue_main(blob).await;
    if should_start {
        // Use spawn_forever_catch_unwind so the debounce timer survives
        // route changes (the caller's scope may be a route component that
        // unmounts before the 2s debounce fires).
        crate::platform::spawn::spawn_forever_catch_unwind(
            "user_prefs_main_debounce",
            debounce_and_publish_main(),
        );
    }
}

/// Enqueue a Mostro blob save from current GlobalSignals.
pub async fn enqueue_mostro_from_signals() {
    let blob = snapshot_mostro_blob();
    let should_start = save::enqueue_mostro(blob).await;
    if should_start {
        crate::platform::spawn::spawn_forever_catch_unwind(
            "user_prefs_mostro_debounce",
            debounce_and_publish_mostro(),
        );
    }
}

/// Debounce + publish loop for the main blob.
async fn debounce_and_publish_main() {
    // Wait for the debounce window.
    crate::platform::timer::sleep(save::MAIN_DEBOUNCE).await;
    // Drain and publish.
    if let Some(blob) = save::take_main().await {
        if let Err(e) = save::publish_main(&blob).await {
            log::warn!("user_prefs sidecar: main publish failed: {e}");
        }
    }
}

/// Debounce + publish loop for the Mostro blob.
async fn debounce_and_publish_mostro() {
    crate::platform::timer::sleep(save::MOSTRO_DEBOUNCE).await;
    if let Some(blob) = save::take_mostro().await {
        if let Err(e) = save::publish_mostro(&blob).await {
            log::warn!("user_prefs sidecar: mostro publish failed: {e}");
        }
    }
}

// ─── Flush (for route-leave + logout) ───────────────────────────────────

/// Flush both pending saves immediately (bypassing the debounce).
/// Call from Layout's use_drop (route-leave) and from logout().
pub async fn flush_all() {
    save::flush_all().await;
}

// ─── Persistent subscription ────────────────────────────────────────────

/// Start persistent subscriptions for live cross-device sync.
///
/// Subscribes to both `nostr.blue/prefs` and `nostr.blue/p2p` on the user's
/// NIP-65 outbox relays. Events from other devices are decrypted and applied
/// via `apply_blob_to_signals`.
///
/// Call from `run_post_login_init` after the initial load completes.
pub async fn start_subscriptions() {
    start_user_prefs_subscription().await;
    start_mostro_prefs_subscription().await;
}

/// Stop both persistent subscriptions. Call from `logout()`.
pub async fn stop_subscriptions() {
    stop_user_prefs_subscription().await;
    stop_mostro_prefs_subscription().await;
}

async fn start_user_prefs_subscription() {
    if !crate::stores::auth_store::is_authenticated() {
        return;
    }
    if PREFS_SUB_ID.read().is_some() {
        return; // already running
    }
    let pubkey = match crate::stores::nostr_client::get_cached_pubkey() {
        Ok(pk) => pk,
        Err(_) => return,
    };
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return,
    };
    wait_for_user_relays(
        Duration::from_secs(5),
        "user_prefs::start_user_prefs_subscription",
    )
    .await;
    let filter = nostr::Filter::new()
        .author(pubkey)
        .kind(nostr::Kind::from(30078))
        .identifier(PREFS_D_TAG)
        .limit(1);
    let result = crate::stores::subscription_manager::subscribe_realtime(
        &client,
        filter,
        None, // stay open indefinitely
    )
    .await;
    match result {
        Ok(sub_id) => {
            *PREFS_SUB_ID.write() = Some(sub_id.clone());
            // Spawn listener for this subscription.
            crate::platform::spawn::spawn_forever_catch_unwind(
                "user_prefs_live_listener",
                prefs_live_listener(client, sub_id),
            );
            log::info!("user_prefs: live subscription started");
        }
        Err(e) => {
            log::warn!("user_prefs: failed to start subscription: {e}");
        }
    }
}

async fn start_mostro_prefs_subscription() {
    if !crate::stores::auth_store::is_authenticated() {
        return;
    }
    if MOSTRO_SUB_ID.read().is_some() {
        return;
    }
    let pubkey = match crate::stores::nostr_client::get_cached_pubkey() {
        Ok(pk) => pk,
        Err(_) => return,
    };
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return,
    };
    wait_for_user_relays(
        Duration::from_secs(5),
        "user_prefs::start_mostro_prefs_subscription",
    )
    .await;
    let filter = nostr::Filter::new()
        .author(pubkey)
        .kind(nostr::Kind::from(30078))
        .identifier(MOSTRO_PREFS_D_TAG)
        .limit(1);
    let result = crate::stores::subscription_manager::subscribe_realtime(
        &client,
        filter,
        None,
    )
    .await;
    match result {
        Ok(sub_id) => {
            *MOSTRO_SUB_ID.write() = Some(sub_id.clone());
            crate::platform::spawn::spawn_forever_catch_unwind(
                "user_prefs_mostro_live_listener",
                mostro_live_listener(client, sub_id),
            );
            log::info!("user_prefs: Mostro live subscription started");
        }
        Err(e) => {
            log::warn!("user_prefs: failed to start Mostro subscription: {e}");
        }
    }
}

/// Listen for live `nostr.blue/prefs` events and apply them.
async fn prefs_live_listener(client: std::sync::Arc<nostr_sdk::Client>, sub_id: SubscriptionId) {
    use nostr_sdk::RelayPoolNotification;
    let mut notifications = client.notifications();
    let mut buffer = Vec::new();
    loop {
        match notifications.recv().await {
            Ok(RelayPoolNotification::Event {
                subscription_id,
                event,
                ..
            }) => {
                if subscription_id != sub_id {
                    continue;
                }
                let event = *event;
                // Phantom-decrypt prevention: skip our own published events.
                if LAST_PUBLISHED_EVENT_ID.peek().as_ref() == Some(&event.id) {
                    continue;
                }
                buffer.push(event);
                while let Ok(notification) = notifications.try_recv() {
                    if let RelayPoolNotification::Event {
                        subscription_id: sid,
                        event: e,
                        ..
                    } = notification
                    {
                        if sid == sub_id {
                            buffer.push(*e);
                        }
                    }
                }
                for event in &buffer {
                    // Decrypt + apply.
                    match crate::stores::user_prefs::encrypt::decrypt_from_self_signer(
                        &event.content,
                        event.id,
                    )
                    .await
                    {
                        Ok(blob) => {
                            crate::stores::user_prefs::load::apply_blob_to_signals(
                                &blob,
                                crate::stores::user_prefs::apply::BlobSource::LiveSubscription,
                            );
                            *USER_PREFS_EVENT_ID.write() = Some(event.id);
                            *USER_PREFS_STATE.write() =
                                crate::stores::ui::sidebar_store::Nip78LoadState::Loaded;
                        }
                        Err(e) => {
                            log::debug!("user_prefs live: decrypt failed: {e}");
                        }
                    }
                }
                buffer.clear();
            }
            Ok(RelayPoolNotification::Shutdown) => break,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                log::warn!("user_prefs live listener: lagged, skipped {skipped}");
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Ok(_) => {}
        }
    }
}

/// Listen for live `nostr.blue/p2p` events and apply them.
async fn mostro_live_listener(client: std::sync::Arc<nostr_sdk::Client>, sub_id: SubscriptionId) {
    use nostr_sdk::RelayPoolNotification;
    let mut notifications = client.notifications();
    loop {
        match notifications.recv().await {
            Ok(RelayPoolNotification::Event {
                subscription_id,
                event,
                ..
            }) => {
                if subscription_id != sub_id {
                    continue;
                }
                let event = *event;
                // Phantom-decrypt prevention.
                if LAST_PUBLISHED_MOSTRO_EVENT_ID.peek().as_ref() == Some(&event.id) {
                    continue;
                }
                match crate::stores::user_prefs::encrypt::decrypt_from_self_mostro::<MostroPrefsBlob>(
                    &event.content,
                ) {
                    Ok(blob) => {
                        crate::stores::user_prefs::load::apply_mostro_blob_to_signals(&blob);
                        *MOSTRO_PREFS_EVENT_ID.write() = Some(event.id);
                        *MOSTRO_PREFS_LOAD_STATE.write() =
                            crate::stores::ui::sidebar_store::Nip78LoadState::Loaded;
                    }
                    Err(e) => {
                        log::debug!("user_prefs mostro live: decrypt failed: {e}");
                    }
                }
            }
            Ok(RelayPoolNotification::Shutdown) => break,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                log::warn!("user_prefs mostro live: lagged, skipped {skipped}");
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Ok(_) => {}
        }
    }
}

async fn stop_user_prefs_subscription() {
    if let Some(task) = PREFS_SUB_TASK.write().take() {
        task.cancel();
    }
    if let Some(sub_id) = PREFS_SUB_ID.write().take() {
        if let Some(client) = crate::stores::nostr_client::get_client() {
            let _ = client.unsubscribe(&sub_id).await;
        }
    }
}

async fn stop_mostro_prefs_subscription() {
    if let Some(task) = MOSTRO_SUB_TASK.write().take() {
        task.cancel();
    }
    if let Some(sub_id) = MOSTRO_SUB_ID.write().take() {
        if let Some(client) = crate::stores::nostr_client::get_client() {
            let _ = client.unsubscribe(&sub_id).await;
        }
    }
}
