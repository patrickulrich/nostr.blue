use super::persistence;
use super::types::{
    PublishQueueStoreStoreExt, QueueEventStatus, QueueEventType, QueuedEvent,
};
use super::PUBLISH_QUEUE;
use dioxus::prelude::*;

static PROCESSOR_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

static PROCESSING_LOCK: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

const PROCESSING_TIMEOUT_SECS: u64 = 60;
const SEND_TIMEOUT_SECS: u64 = 20;
const MAX_EVENTS_PER_CYCLE: usize = 5;
const STUCK_PUBLISHING_THRESHOLD_SECS: i64 = 60;

/// Routed send with the queue's target-relay logic, factored out of
/// `process_once` so the corruption escalation path can retry with the
/// local-DB save disabled.
///
/// `skip_local_save` turns off the SDK's pre-broadcast
/// `database().save_event()` (`SendEvent::save_into_database(false)`). The
/// SDK runs that save — and propagates its error — BEFORE contacting any
/// relay, so a corrupt local cache would otherwise abort every broadcast
/// despite the relays being perfectly healthy. The local cache is a
/// relay-rehydratable store; the broadcast is the point.
///
/// Returns `Err(String)` on failure or timeout (message is the rendered
/// error, used by the corruption classifier).
async fn send_routed(
    client: &nostr_sdk::client::Client,
    event: &QueuedEvent,
    event_obj: &nostr::event::Event,
    skip_local_save: bool,
    send_timeout: std::time::Duration,
) -> std::result::Result<nostr_sdk::client::SendEventOutput, String> {
    let inner = async {
        let current_write: Vec<nostr::types::url::RelayUrl> = client
            .relays()
            .with_capabilities(nostr_sdk::relay::RelayCapabilities::WRITE)
            .await
            .keys()
            .cloned()
            .collect();
        let mut sender = client.send_event(event_obj);
        if let Some(ref urls) = event.target_relays {
            let pool_urls: Vec<nostr::types::url::RelayUrl> = urls
                .iter()
                .filter_map(|u| nostr::types::url::RelayUrl::parse(u).ok())
                .filter(|u| current_write.contains(u))
                .collect();
            if pool_urls.is_empty() {
                if current_write.is_empty() {
                    sender = sender.broadcast();
                } else {
                    sender = sender.to(current_write);
                }
            } else {
                sender = sender.to(pool_urls);
            }
        } else if current_write.is_empty() {
            // No WRITE relays: plain send resolves URLs via gossip routing.
            log::warn!(
                "[PQ] No WRITE relays, falling back to gossip path for {}",
                event.event_id
            );
        } else {
            log::debug!(
                "[PQ] Fast-path send to {} WRITE relays for {}",
                current_write.len(),
                event.event_id
            );
            sender = sender.broadcast();
        }
        if skip_local_save {
            sender = sender.save_into_database(false);
        }
        sender.ok_timeout(send_timeout).await
    };
    match with_timeout(send_timeout, inner).await {
        Some(result) => result.map_err(|e| e.to_string()),
        None => Err(format!("Send timed out after {}s", SEND_TIMEOUT_SECS)),
    }
}

/// Escalate unrecoverable local-store corruption: queue a cache rebuild at
/// next boot, tell the user, and reload. On web the reload re-runs
/// `initialize_client`, which consumes the recovery flag and deletes both
/// SQLite cache files before opening them (deletion requires a closed
/// database, so it cannot be done in-place mid-session). Native builds
/// surface the notice only — the boot probe rebuilds on next launch.
///
/// R4 review circuit breaker: if the cache-file deletion keeps failing
/// (OPFS quota, a second tab holding the SAH pool handles exclusively),
/// the reload re-opens the still-corrupt store, the persisted publish
/// queue re-escalates, and the app cycles forever — roughly one
/// uninterruptible reload every few seconds. A persisted attempt counter
/// caps the cycle: past MAX_RECOVERY_ATTEMPTS the escalation degrades
/// to a notice (publishes that can skip the local save still succeed via
/// the `skip_local_save` retry; full caching returns on the next launch's
/// boot probe). The counter is reset at boot when the recovery actually
/// succeeded or none is pending.
async fn schedule_corruption_recovery(error: &str) {
    static RECOVERY_SCHEDULED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    if RECOVERY_SCHEDULED
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        return; // already scheduled — don't stack reloads
    }
    log::error!(
        "[PQ] local store corruption is blocking publishes ({}); scheduling cache rebuild",
        error
    );
    #[cfg(target_arch = "wasm32")]
    {
        let attempts = crate::utils::db_corruption::recovery_attempts();
        if attempts >= crate::utils::db_corruption::MAX_RECOVERY_ATTEMPTS {
            log::error!(
                "[PQ] cache-rebuild reload failed {attempts} times (deletion likely blocked) — \
                 degrading to notice-only recovery; publishes continue without local caching"
            );
            super::push_notice(super::PublishNotice::CacheRecoveryDegraded {
                error: error.to_string(),
            });
            return;
        }
        crate::utils::db_corruption::set_recovery_attempts(attempts + 1);
    }
    super::push_notice(super::PublishNotice::CacheRecovery {
        error: error.to_string(),
    });
    #[cfg(target_arch = "wasm32")]
    {
        let _ = crate::platform::storage::set_string(
            crate::utils::db_corruption::DB_RECOVERY_FLAG_KEY,
            "1",
        );
        // Let the toast drainer render before the reload tears down the page.
        crate::platform::timer::sleep_ms(2500).await;
        if let Some(window) = web_sys::window() {
            let _ = window.location().reload();
        }
    }
}

struct ProcessingGuard;

impl ProcessingGuard {
    fn try_acquire() -> Option<Self> {
        use std::sync::atomic::Ordering;
        PROCESSING_LOCK
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| Self)
    }
}

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        PROCESSING_LOCK.store(false, Ordering::SeqCst);
        log::debug!("ProcessingGuard dropped, lock released");
    }
}

async fn with_timeout<F: std::future::Future>(
    dur: std::time::Duration,
    f: F,
) -> Option<F::Output> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::timeout(dur, f).await.ok()
    }
    #[cfg(target_arch = "wasm32")]
    {
        use futures::future::{select, Either};
        use futures::pin_mut;
        let timeout = crate::platform::timer::sleep_ms(dur.as_millis() as u32);
        pin_mut!(f);
        pin_mut!(timeout);
        match select(f, timeout).await {
            Either::Left((result, _)) => Some(result),
            Either::Right(_) => None,
        }
    }
}

pub fn start_publish_queue_processor() {
    use std::sync::atomic::Ordering;
    if PROCESSOR_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        log::debug!("Publish queue processor already running");
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        use gloo_timers::future::TimeoutFuture;
        spawn(async {
            loop {
                TimeoutFuture::new(2_000).await;
                if !PROCESSOR_RUNNING.load(Ordering::SeqCst) {
                    log::info!("Publish queue processor stopped");
                    break;
                }
                if let Err(e) = process_once_guarded().await {
                    log::error!("Publish queue processor error: {}", e);
                }
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        dioxus_core::spawn_forever(async {
            loop {
                if !PROCESSOR_RUNNING.load(Ordering::SeqCst) {
                    log::info!("Publish queue processor stopped");
                    break;
                }
                if let Err(e) = process_once_guarded().await {
                    log::error!("Publish queue processor error: {}", e);
                }
                crate::stores::nostr_client::platform_sleep_ms(2_000).await;
            }
        });
    }
    log::info!("Started publish queue processor");
}

pub async fn process_once_guarded() -> Result<(), String> {
    let _guard = match ProcessingGuard::try_acquire() {
        Some(g) => g,
        None => {
            log::debug!("process_once_guarded: already processing, skipping");
            return Ok(());
        }
    };
    log::debug!("process_once_guarded: acquired lock, calling process_once");
    match with_timeout(
        std::time::Duration::from_secs(PROCESSING_TIMEOUT_SECS),
        process_once(),
    )
    .await
    {
        Some(result) => result,
        None => {
            log::error!(
                "[PQ] process_once timed out after {}s",
                PROCESSING_TIMEOUT_SECS
            );
            Err("Processing timed out".to_string())
        }
    }
}

pub async fn process_once() -> Result<(), String> {
    recover_stuck_publishing().await;

    let pending: Vec<QueuedEvent> = {
        let queue = PUBLISH_QUEUE.read();
        queue
            .events()
            .read()
            .iter()
            .filter(|e| matches!(e.status, QueueEventStatus::Pending))
            .take(MAX_EVENTS_PER_CYCLE)
            .cloned()
            .collect()
    };
    if pending.is_empty() {
        return Ok(());
    }
    log::info!("Processing {} pending publish queue events", pending.len());
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized for publish queue")?;
    crate::stores::relay::connection::ensure_relays_ready(&client).await;

    let send_timeout = std::time::Duration::from_secs(SEND_TIMEOUT_SECS);

    for event in pending {
        log::debug!("[PQ] setting event {} to Publishing", event.event_id);
        set_status(&event.id, QueueEventStatus::Publishing).await;
        set_metadata(
            &event.id,
            "publishing_since",
            &chrono::Utc::now().timestamp().to_string(),
        )
        .await;

        let event_obj: nostr::event::Event = match serde_json::from_str(&event.event_json) {
            Ok(e) => e,
            Err(e) => {
                let msg = format!("Failed to deserialize event: {}", e);
                log::error!("{}", msg);
                set_status(&event.id, QueueEventStatus::Failed { error: msg.clone() })
                    .await;
                continue;
            }
        };

        // First attempt with the SDK defaults (event saved into the local
        // DB before broadcasting). Corruption-class failures escalate below.
        let mut result = send_routed(&client, &event, &event_obj, false, send_timeout).await;

        // Corruption escalation ladder: the SDK runs `database().save_event`
        // and `gossip.store().process` BEFORE contacting relays and
        // propagates their errors, so a corrupt local cache write would
        // otherwise hold every broadcast hostage (reactions enqueue fine
        // but never reach a relay). Retry once without the local save; if
        // the corruption failure persists it is the gossip store (not
        // skippable per-call) — schedule a cache rebuild + reload.
        if let Err(msg) = &result {
            if crate::utils::db_corruption::is_corruption_error(msg) {
                log::warn!(
                    "[PQ] corruption-class failure for {}: {} — retrying without local DB save",
                    event.event_id,
                    msg
                );
                result = send_routed(&client, &event, &event_obj, true, send_timeout).await;
                if let Err(retry_msg) = &result {
                    if crate::utils::db_corruption::is_corruption_error(retry_msg) {
                        schedule_corruption_recovery(retry_msg).await;
                    }
                }
            }
        }

        match result {
            Ok(output) => {
                let success_count = output.success.len();
                let fail_count = output.failed.len();
                if fail_count > 0 && success_count == 0 {
                    for (relay, err) in &output.failed {
                        log::warn!(
                            "Event {} rejected by {}: {}",
                            event.event_id,
                            relay,
                            err
                        );
                    }
                    let first_err = output
                        .failed
                        .values()
                        .next()
                        .cloned()
                        .unwrap_or_else(|| "Unknown error".to_string());
                    handle_failure(&event, &first_err).await;
                } else {
                    if fail_count > 0 {
                        log::warn!(
                            "Event {} partial: {}/{} relays",
                            event.event_id,
                            success_count,
                            success_count + fail_count
                        );
                    } else {
                        log::info!(
                            "Event {} published to {} relays",
                            event.event_id,
                            success_count
                        );
                    }

                    let has_p_tags = event_obj.tags.public_keys().next().is_some();
                    let needs_gossip = has_p_tags && event.target_relays.is_none();

                    set_status(&event.id, QueueEventStatus::Success).await;

                    if needs_gossip {
                        log::info!(
                            "[PQ] Event {} has p-tags, scheduling background gossip propagation",
                            event.event_id
                        );
                        set_metadata(&event.id, "gossip_propagation", "pending").await;
                        let auto_remove_ts = chrono::Utc::now().timestamp() + 600;
                        set_metadata(
                            &event.id,
                            "auto_remove_after",
                            &auto_remove_ts.to_string(),
                        )
                        .await;
                        spawn_gossip_task(event_obj, event.id.clone());
                    } else {
                        set_metadata(&event.id, "gossip_propagation", "skipped").await;
                        schedule_auto_remove(&event.id, 30);
                    }
                }
            }
            Err(msg) => {
                log::error!("Event {} send_event failed: {}", event.event_id, msg);
                handle_failure(&event, &msg).await;
            }
        }
    }
    Ok(())
}

async fn recover_stuck_publishing() {
    let stuck_ids: Vec<String> = {
        let queue = PUBLISH_QUEUE.read();
        let now_ts = chrono::Utc::now().timestamp();
        queue
            .events()
            .read()
            .iter()
            .filter(|e| {
                if !matches!(e.status, QueueEventStatus::Publishing) {
                    return false;
                }
                e.metadata
                    .get("publishing_since")
                    .and_then(|s| s.parse::<i64>().ok())
                    .map(|since| now_ts - since > STUCK_PUBLISHING_THRESHOLD_SECS)
                    .unwrap_or(true)
            })
            .map(|e| e.id.clone())
            .collect()
    };
    for id in stuck_ids {
        log::warn!(
            "[PQ] Resetting stuck Publishing event to Pending: {}",
            id
        );
        set_status(&id, QueueEventStatus::Pending).await;
    }
}

fn spawn_gossip_task(event_obj: nostr::event::Event, queue_id: String) {
    #[cfg(target_arch = "wasm32")]
    {
        spawn(async move {
            propagate_via_gossip(&event_obj, &queue_id).await;
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        dioxus_core::spawn_forever(async move {
            propagate_via_gossip(&event_obj, &queue_id).await;
        });
    }
}

async fn propagate_via_gossip(event_obj: &nostr::event::Event, queue_id: &str) {
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => {
            log::warn!(
                "[PQ] Gossip propagation: client not available for {}",
                queue_id
            );
            set_metadata(queue_id, "gossip_propagation", "done").await;
            set_metadata(queue_id, "gossip_error", "client unavailable").await;
            schedule_auto_remove(queue_id, 30);
            return;
        }
    };

    log::info!(
        "[PQ] Starting gossip propagation for event {}",
        event_obj.id.to_hex()
    );

    match client.send_event(event_obj).await {
        Ok(output) => {
            let success_urls: Vec<String> = output
                .success
                .keys()
                .map(|u| u.to_string())
                .collect();
            let fail_count = output.failed.len();
            log::info!(
                "[PQ] Gossip propagation complete for {}: {}/{} relays succeeded",
                event_obj.id.to_hex(),
                output.success.len(),
                output.success.len() + fail_count
            );
            set_metadata(queue_id, "gossip_propagation", "done").await;
            set_metadata(queue_id, "gossip_success_relays", &success_urls.join(",")).await;
            set_metadata(queue_id, "gossip_fail_count", &fail_count.to_string()).await;
        }
        Err(e) => {
            log::warn!(
                "[PQ] Gossip propagation failed for {}: {}",
                event_obj.id.to_hex(),
                e
            );
            set_metadata(queue_id, "gossip_propagation", "done").await;
            set_metadata(queue_id, "gossip_fail_count", "all").await;
            set_metadata(queue_id, "gossip_error", &e.to_string()).await;
        }
    }

    schedule_auto_remove(queue_id, 30);
}

pub fn resume_gossip_propagation() {
    spawn(async move {
        let events: Vec<QueuedEvent> = {
            let queue = PUBLISH_QUEUE.read();
            queue
                .events()
                .read()
                .iter()
                .filter(|e| matches!(e.status, QueueEventStatus::Success))
                .cloned()
                .collect()
        };

        let now_ts = chrono::Utc::now().timestamp();

        for event in events {
            let gossip_state = event
                .metadata
                .get("gossip_propagation")
                .cloned()
                .unwrap_or_default();

            match gossip_state.as_str() {
                "pending" => {
                    let event_obj: nostr::event::Event =
                        match serde_json::from_str(&event.event_json) {
                            Ok(e) => e,
                            Err(_) => {
                                log::warn!(
                                    "[PQ] Resume gossip: failed to deserialize event {}",
                                    event.event_id
                                );
                                deque(&event.id).await;
                                continue;
                            }
                        };

                    if event_obj.tags.public_keys().next().is_some() {
                        log::info!(
                            "[PQ] Resuming gossip propagation for event {}",
                            event.event_id
                        );
                        spawn_gossip_task(event_obj, event.id.clone());
                    } else {
                        set_metadata(&event.id, "gossip_propagation", "skipped").await;
                        schedule_auto_remove(&event.id, 30);
                    }
                }
                "done" | "skipped" => {
                    if let Some(ts_str) = event.metadata.get("auto_remove_after") {
                        if let Ok(ts) = ts_str.parse::<i64>() {
                            if now_ts >= ts {
                                deque(&event.id).await;
                                continue;
                            }
                        }
                    }
                    schedule_auto_remove(&event.id, 30);
                }
                _ => {
                    deque(&event.id).await;
                }
            }
        }
    });
}

async fn handle_failure(event: &QueuedEvent, error: &str) {
    let new_retry = event.retry_count + 1;
    if new_retry >= event.max_retries {
        log::error!(
            "Event {} exceeded max retries ({})",
            event.event_id,
            event.max_retries
        );
        set_status(
            &event.id,
            QueueEventStatus::MaxRetriesExceeded {
                error: error.to_string(),
            },
        )
        .await;
        // Enqueue-time success hides relay-side failures from the UI, so a
        // terminal failure of a user-facing action is surfaced as a toast.
        if is_user_facing(&event.event_type) {
            super::push_notice(super::PublishNotice::Failed {
                label: event_type_label(&event.event_type),
                error: error.to_string(),
            });
        }
    } else {
        log::warn!(
            "Event {} publish failed (retry {}/{}): {}",
            event.event_id,
            new_retry,
            event.max_retries,
            error
        );
        let queue = PUBLISH_QUEUE.write();
        let mut events = queue.events();
        let mut events_guard = events.write();
        if let Some(queued) = events_guard.iter_mut().find(|e| e.id == event.id) {
            queued.retry_count = new_retry;
            queued.status = QueueEventStatus::Pending;
            queued.last_retry_at = Some(chrono::Utc::now().timestamp() as u64);
            let updated = queued.clone();
            drop(events_guard);
            drop(queue);
            let _ = persistence::update_queued_event(&updated).await;
        }
    }
}

/// Event types whose publish failures the user should see as toasts —
/// direct social actions whose silent loss is most confusing. Structured
/// flows (Cashu, Mostro, Git hosting, …) report errors through their own UI.
fn is_user_facing(event_type: &QueueEventType) -> bool {
    matches!(
        event_type,
        QueueEventType::Note
            | QueueEventType::Reaction
            | QueueEventType::Repost
            | QueueEventType::DirectMessage
    )
}

/// Human-readable label for a toast about a terminally failed event.
fn event_type_label(event_type: &QueueEventType) -> String {
    match event_type {
        QueueEventType::Note => "Note".to_string(),
        QueueEventType::Reaction => "Reaction".to_string(),
        QueueEventType::Repost => "Repost".to_string(),
        QueueEventType::DirectMessage => "Message".to_string(),
        _ => "Event".to_string(),
    }
}

async fn set_status(id: &str, status: QueueEventStatus) {
    let queue = PUBLISH_QUEUE.write();
    let mut events = queue.events();
    let mut events_guard = events.write();
    if let Some(event) = events_guard.iter_mut().find(|e| e.id == id) {
        event.status = status.clone();
        let updated = event.clone();
        drop(events_guard);
        drop(queue);
        let _ = persistence::update_queued_event(&updated).await;
    }
}

async fn set_metadata(id: &str, key: &str, value: &str) {
    let queue = PUBLISH_QUEUE.write();
    let mut events = queue.events();
    let mut events_guard = events.write();
    if let Some(event) = events_guard.iter_mut().find(|e| e.id == id) {
        event.metadata.insert(key.to_string(), value.to_string());
        let updated = event.clone();
        drop(events_guard);
        drop(queue);
        let _ = persistence::update_queued_event(&updated).await;
    }
}

fn schedule_auto_remove(id: &str, delay_secs: u64) {
    let id = id.to_string();
    let auto_remove_ts = chrono::Utc::now().timestamp() + delay_secs as i64;
    let _ = auto_remove_ts;

    {
        let queue = PUBLISH_QUEUE.write();
        let mut events = queue.events();
        let mut events_guard = events.write();
        if let Some(event) = events_guard.iter_mut().find(|e| e.id == id) {
            event.metadata.insert(
                "auto_remove_after".to_string(),
                auto_remove_ts.to_string(),
            );
            let updated = event.clone();
            drop(events_guard);
            drop(queue);
            spawn(async move {
                let _ = persistence::update_queued_event(&updated).await;
            });
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        use gloo_timers::future::TimeoutFuture;
        spawn(async move {
            TimeoutFuture::new((delay_secs * 1000) as u32).await;
            deque(&id).await;
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        dioxus_core::spawn_forever(async move {
            crate::stores::nostr_client::platform_sleep_ms(delay_secs * 1000).await;
            deque(&id).await;
        });
    }
}

pub async fn deque(id: &str) {
    let queue = PUBLISH_QUEUE.write();
    queue.events().write().retain(|e| e.id != id);
    drop(queue);
    let _ = persistence::remove_queued_event(id).await;
}

pub fn stop_processor() {
    use std::sync::atomic::Ordering;
    PROCESSOR_RUNNING.store(false, Ordering::SeqCst);
}
