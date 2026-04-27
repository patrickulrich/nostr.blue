use crate::platform::{timer, timestamp};
use crate::stores::settings_store;
use crate::stores::{
    auth_store, nostr_client, relay,
    ui::sync_store::{self, SyncPhase, SyncProgressState, SyncTarget},
};
use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_relay_pool::relay::SyncProgress;
use nostr_relay_pool::RelayStatus as PoolRelayStatus;
use nostr_sdk::{Client, Filter, Kind, PublicKey, RelayUrl, SyncDirection, SyncOptions, Timestamp};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

const FOLLOWING_SYNC_WINDOW_SECS: u64 = 86_400;
const SYNC_POLL_INTERVAL: Duration = Duration::from_secs(1);
const SYNC_INITIAL_TIMEOUT: Duration = Duration::from_secs(5);
const USER_RELAY_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

pub fn use_sync_service() {
    use_future(move || async move {
        let mut last_connected = false;
        let mut startup_attempted_for_pubkey: Option<String> = None;
        let mut next_due_at: Option<u64> = None;

        loop {
            let now = timestamp::now_secs();
            let client_initialized = *nostr_client::CLIENT_INITIALIZED.peek();
            let is_authenticated = auth_store::is_authenticated();
            let connected = *relay::RELAY_CONNECTED.peek();
            let current_pubkey = auth_store::get_pubkey();
            let settings = settings_store::SETTINGS.peek().clone();

            let trigger = if !client_initialized {
                startup_attempted_for_pubkey = None;
                next_due_at = None;
                set_waiting_state("Waiting for client initialization", None);
                None
            } else if !is_authenticated || current_pubkey.is_none() {
                startup_attempted_for_pubkey = None;
                next_due_at = None;
                set_waiting_state("Login required for sync", None);
                None
            } else if !settings.enable_negentropy_sync {
                next_due_at = None;
                set_waiting_state("Disabled in settings", None);
                None
            } else if !connected {
                set_waiting_state("Waiting for relay connection", next_due_at);
                None
            } else if startup_attempted_for_pubkey.as_ref() != current_pubkey.as_ref() {
                startup_attempted_for_pubkey = current_pubkey.clone();
                Some("startup")
            } else if !last_connected && connected {
                Some("reconnect")
            } else if next_due_at.map(|due| now >= due).unwrap_or(true) {
                Some("interval")
            } else {
                {
                    let mut state = sync_store::SYNC_SERVICE_STATE.write();
                    state.next_scheduled_at = next_due_at;
                    if matches!(state.phase, SyncPhase::Idle) {
                        state.phase = SyncPhase::Waiting;
                        state.waiting_reason = Some("Waiting for next scheduled sync".to_string());
                    }
                }
                None
            };

            if let Some(reason) = trigger {
                match run_sync_cycle(reason, settings.negentropy_sync_interval_minutes).await {
                    Ok(next_due) => next_due_at = Some(next_due),
                    Err(e) => {
                        log::warn!("Negentropy sync cycle failed: {}", e);
                        next_due_at = Some(
                            timestamp::now_secs()
                                + u64::from(settings.negentropy_sync_interval_minutes.max(1)) * 60,
                        );
                    }
                }
            }

            last_connected = connected;
            timer::sleep(SYNC_POLL_INTERVAL).await;
        }
    });
}

async fn run_sync_cycle(trigger: &str, interval_minutes: u32) -> Result<u64, String> {
    let started_at = timestamp::now_secs();
    {
        let mut state = sync_store::SYNC_SERVICE_STATE.write();
        state.phase = SyncPhase::Running;
        state.active_target = None;
        state.progress = None;
        state.waiting_reason = Some(format!("Triggered by {}", trigger));
        state.last_started_at = Some(started_at);
        state.last_finished_at = None;
        state.last_error = None;
        state.next_scheduled_at = None;
    }

    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let result: Result<(), String> = async {
        relay::wait_for_user_relays(USER_RELAY_WAIT_TIMEOUT, "negentropy sync").await;
        let relay_urls = resolve_sync_relays(&client).await?;

        let authors = load_following_authors().await?;
        if authors.is_empty() {
            mark_target_skipped(SyncTarget::FollowingFeed);
        } else {
            let filter = build_following_sync_filter(authors.clone());
            run_sync_target(
                client.clone(),
                relay_urls.clone(),
                SyncTarget::FollowingFeed,
                filter,
                "following feed",
            )
            .await?;
        }

        let pubkey = current_pubkey()?;
        let relay_list_filter = Filter::new().author(pubkey).kind(Kind::RelayList).limit(1);
        run_sync_target(
            client.clone(),
            relay_urls.clone(),
            SyncTarget::RelayList,
            relay_list_filter,
            "relay list",
        )
        .await?;

        // Native-only extended sync: pre-populate nostrdb with more event types
        #[cfg(feature = "native")]
        {
            let pk = current_pubkey()?;

            let identity_filter = Filter::new()
                .author(pk)
                .kinds(vec![
                    Kind::Metadata,
                    Kind::ContactList,
                    Kind::RelayList,
                    Kind::MuteList,
                    Kind::PinList,
                    Kind::Reporting,
                ])
                .limit(10);
            run_sync_target(
                client.clone(),
                relay_urls.clone(),
                SyncTarget::OwnIdentity,
                identity_filter,
                "own identity",
            )
            .await?;

            let own_content_filter = Filter::new()
                .author(pk)
                .kinds(vec![
                    Kind::TextNote,
                    Kind::Repost,
                    Kind::Reaction,
                    Kind::Custom(20),
                    Kind::LongFormTextNote,
                    Kind::Custom(1010),
                ])
                .limit(200);
            run_sync_target(
                client.clone(),
                relay_urls.clone(),
                SyncTarget::OwnContent,
                own_content_filter,
                "own content",
            )
            .await?;

            if !authors.is_empty() {
                let profile_filter = Filter::new()
                    .authors(authors.clone())
                    .kind(Kind::Metadata)
                    .limit(authors.len() as usize);
                run_sync_target(
                    client.clone(),
                    relay_urls.clone(),
                    SyncTarget::FollowedProfiles,
                    profile_filter,
                    "followed profiles",
                )
                .await?;
            }

            let notification_filter = Filter::new()
                .pubkey(pk)
                .kinds(vec![
                    Kind::TextNote,
                    Kind::Repost,
                    Kind::Reaction,
                    Kind::ZapReceipt,
                ])
                .limit(200);
            run_sync_target(
                client,
                relay_urls,
                SyncTarget::Notifications,
                notification_filter,
                "notifications",
            )
            .await?;
        }

        #[cfg(not(feature = "native"))]
        {
            let _ = (client, relay_urls, pubkey, authors);
        }

        Ok(())
    }
    .await;

    let finished_at = timestamp::now_secs();
    let next_due_at = finished_at + u64::from(interval_minutes.max(1)) * 60;

    match result {
        Ok(()) => {
            let mut state = sync_store::SYNC_SERVICE_STATE.write();
            state.phase = SyncPhase::Succeeded;
            state.active_target = None;
            state.progress = None;
            state.waiting_reason = Some("Waiting for next scheduled sync".to_string());
            state.last_finished_at = Some(finished_at);
            state.last_success_at = Some(finished_at);
            state.next_scheduled_at = Some(next_due_at);
            Ok(next_due_at)
        }
        Err(error) => {
            let mut state = sync_store::SYNC_SERVICE_STATE.write();
            state.phase = SyncPhase::Failed;
            state.active_target = None;
            state.progress = None;
            state.waiting_reason = Some("Waiting for next scheduled sync".to_string());
            state.last_finished_at = Some(finished_at);
            state.last_error = Some(error.clone());
            state.next_scheduled_at = Some(next_due_at);
            Err(error)
        }
    }
}

async fn run_sync_target(
    client: Arc<Client>,
    relay_urls: Vec<RelayUrl>,
    target: SyncTarget,
    filter: Filter,
    label: &str,
) -> Result<(), String> {
    let started_at = timestamp::now_secs();
    {
        let mut state = sync_store::SYNC_SERVICE_STATE.write();
        state.active_target = Some(target);
        state.progress = Some(SyncProgressState::default());
        state.waiting_reason = Some(format!("Syncing {}", label));
        let stats = state.target_stats_mut(target);
        stats.last_started_at = Some(started_at);
        stats.last_finished_at = None;
    }

    let (progress_tx, mut progress_rx) = SyncProgress::channel();
    spawn(async move {
        while progress_rx.changed().await.is_ok() {
            let progress = *progress_rx.borrow_and_update();
            let mut state = sync_store::SYNC_SERVICE_STATE.write();
            if state.phase != SyncPhase::Running || state.active_target != Some(target) {
                continue;
            }
            state.progress = Some(SyncProgressState {
                current: progress.current,
                total: progress.total,
            });
        }
    });

    let opts = SyncOptions::new()
        .direction(SyncDirection::Down)
        .initial_timeout(SYNC_INITIAL_TIMEOUT)
        .progress(progress_tx);

    let output = client
        .sync_with(relay_urls.iter().cloned(), filter, &opts)
        .await
        .map_err(|e| format!("{} sync failed: {}", label, e))?;

    let summary = output.val;
    let finished_at = timestamp::now_secs();
    let send_failure_count = summary
        .send_failures
        .values()
        .map(|events| events.len())
        .sum::<usize>();

    {
        let mut state = sync_store::SYNC_SERVICE_STATE.write();
        state.progress = None;
        let stats = state.target_stats_mut(target);
        stats.last_finished_at = Some(finished_at);
        stats.last_success_at = Some(finished_at);
        stats.local_count = summary.local.len();
        stats.remote_count = summary.remote.len();
        stats.sent_count = summary.sent.len();
        stats.received_count = summary.received.len();
        stats.send_failure_count = send_failure_count;
    }

    Ok(())
}

async fn resolve_sync_relays(client: &Arc<Client>) -> Result<Vec<RelayUrl>, String> {
    relay::ensure_relays_ready(client).await;
    let relays = client.relays().await;
    let connected_urls: Vec<RelayUrl> = relays
        .iter()
        .filter(|(_, relay)| relay.status() == PoolRelayStatus::Connected)
        .filter_map(|(url, _)| RelayUrl::parse(url.as_str()).ok())
        .collect();

    if connected_urls.is_empty() {
        return Err("No connected relays available for negentropy sync".to_string());
    }

    let preferred_read_relays: HashSet<String> = relay::get_read_relays().into_iter().collect();
    if preferred_read_relays.is_empty() {
        return Ok(connected_urls);
    }

    let preferred: Vec<RelayUrl> = connected_urls
        .iter()
        .filter(|url| preferred_read_relays.contains(url.to_string().as_str()))
        .cloned()
        .collect();

    if preferred.is_empty() {
        Ok(connected_urls)
    } else {
        Ok(preferred)
    }
}

async fn load_following_authors() -> Result<Vec<PublicKey>, String> {
    let pubkey = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let contacts = nostr_client::fetch_contacts(pubkey).await?;

    Ok(contacts
        .into_iter()
        .filter_map(|contact| PublicKey::parse(&contact).ok())
        .collect())
}

fn build_following_sync_filter(authors: Vec<PublicKey>) -> Filter {
    Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost, Kind::Comment])
        .authors(authors)
        .since(Timestamp::now() - Duration::from_secs(FOLLOWING_SYNC_WINDOW_SECS))
        .limit(50)
}

fn current_pubkey() -> Result<PublicKey, String> {
    let pubkey = auth_store::get_pubkey().ok_or("No current pubkey")?;
    PublicKey::parse(&pubkey).map_err(|e| format!("Invalid pubkey: {}", e))
}

fn mark_target_skipped(target: SyncTarget) {
    let now = timestamp::now_secs();
    let mut state = sync_store::SYNC_SERVICE_STATE.write();
    let stats = state.target_stats_mut(target);
    stats.last_started_at = Some(now);
    stats.last_finished_at = Some(now);
    stats.last_success_at = Some(now);
    stats.local_count = 0;
    stats.remote_count = 0;
    stats.sent_count = 0;
    stats.received_count = 0;
    stats.send_failure_count = 0;
}

fn set_waiting_state(reason: &str, next_due_at: Option<u64>) {
    let mut state = sync_store::SYNC_SERVICE_STATE.write();
    state.phase = SyncPhase::Waiting;
    state.active_target = None;
    state.progress = None;
    state.waiting_reason = Some(reason.to_string());
    state.next_scheduled_at = next_due_at;
}
