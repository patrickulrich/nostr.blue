use dioxus::prelude::*;
use nostr_sdk::{Filter, Kind, SubscriptionId};
use crate::stores::{auth_store, nostr_client};

/// Global signal to track unread notification count
pub static UNREAD_COUNT: GlobalSignal<usize> = Signal::global(|| 0);

/// Track the current real-time subscription ID
pub static SUBSCRIPTION_ID: GlobalSignal<Option<SubscriptionId>> = Signal::global(|| None);

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

/// Start real-time notification subscription (limit: 20 for recent notifications only)
/// This should be called once when the app initializes and user is authenticated
pub async fn start_realtime_subscription() {
    let is_authenticated = auth_store::is_authenticated();
    let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

    if !is_authenticated || !client_initialized {
        log::warn!("Cannot start notification subscription: not authenticated or client not initialized");
        return;
    }

    // Check if already subscribed
    if SUBSCRIPTION_ID.read().is_some() {
        log::debug!("Real-time notification subscription already active");
        return;
    }

    let my_pubkey = match auth_store::get_pubkey() {
        Some(pk) => pk,
        None => {
            log::error!("Cannot start notification subscription: no pubkey");
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

    // Subscribe with limit 20 for real-time updates only
    // Use #p tag to match events that mention/tag our pubkey
    let filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,      // For mentions and replies
            Kind::Repost,        // Reposts
            Kind::Reaction,      // Reactions (likes)
            Kind::ZapReceipt,    // Zap receipts
        ])
        .custom_tag(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::P),
            my_pubkey
        )
        .limit(20);              // Only recent events for real-time updates

    log::info!("Starting real-time notification subscription (limit: 20)");

    match client.subscribe(filter, None).await {
        Ok(output) => {
            let sub_id = output.val;
            SUBSCRIPTION_ID.write().replace(sub_id.clone());
            log::info!("Real-time notification subscription started: {:?}", sub_id);
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
            client.unsubscribe(&id).await;
        }
        *SUBSCRIPTION_ID.write() = None;
    }
}
