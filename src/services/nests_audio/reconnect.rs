//! Reconnection support for nest audio sessions.
//!
//! Tracks each active publisher's connection config and subscriptions so that
//! a session can be torn down and re-established (by the cliff detector, the
//! JWT refresh timer, or the network-change handler) without losing track of
//! which speaker broadcasts the listener was subscribed to.
//!
//! Reference reconnect semantics for live rooms (subscriptions
//! survive session swaps) and the reference impl's `declinedPublish`
//! persistence.
//!
//! All state is process-global and keyed by `publisher_id` (one entry per
//! concurrent nest — in practice there's only one).

use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Connection configuration captured at join time so we can reconnect without
/// the caller having to pass everything again.
#[derive(Clone, Debug)]
pub struct NestConfig {
    pub auth_url: String,
    pub relay_url: String,
    pub namespace: String,
    pub my_pubkey: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DesiredState {
    Off,
    Listener,
    Speaker,
}

struct SessionState {
    cfg: Option<NestConfig>,
    desired: DesiredState,
    /// Speaker pubkeys we're subscribed to. Re-issued after a reconnect.
    subs: HashSet<String>,
    /// Set to true when the user has been demoted from speaker. Prevents
    /// auto-republish on reconnect. Cleared when the host re-promotes.
    declined_publish: AtomicBool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            cfg: None,
            desired: DesiredState::Off,
            subs: HashSet::new(),
            declined_publish: AtomicBool::new(false),
        }
    }
}

static REGISTRY: Lazy<Mutex<HashMap<String, SessionState>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn with_state<F, R>(publisher_id: &str, f: F) -> R
where
    F: FnOnce(&mut SessionState) -> R,
{
    let mut map = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    let state = map.entry(publisher_id.to_string()).or_default();
    f(state)
}

/// Record the connection config and desired state after a successful join.
/// Called by `use_nest_audio::join_room`.
pub fn track_join(publisher_id: &str, cfg: NestConfig, publish: bool) {
    with_state(publisher_id, |s| {
        s.cfg = Some(cfg);
        s.desired = if publish {
            DesiredState::Speaker
        } else {
            DesiredState::Listener
        };
    });
}

/// Record a subscription so it can be re-issued after a reconnect.
/// Called by `use_nest_audio::subscribe_to_participant`.
pub fn track_subscribe(publisher_id: &str, participant_pubkey: &str) {
    with_state(publisher_id, |s| {
        s.subs.insert(participant_pubkey.to_string());
    });
}

/// Remove a subscription from tracking.
/// Called by `use_nest_audio::unsubscribe_from_participant`.
pub fn track_unsubscribe(publisher_id: &str, participant_pubkey: &str) {
    with_state(publisher_id, |s| {
        s.subs.remove(participant_pubkey);
    });
}

/// Set / clear the declined-publish flag. When true, `recycle_as_speaker` will
/// refuse and fall back to listener mode. Cleared by Phase 1.4's use_effect
/// when the host re-promotes the user.
pub fn set_declined_publish(publisher_id: &str, value: bool) {
    with_state(publisher_id, |s| {
        s.declined_publish.store(value, Ordering::Relaxed);
    });
}

/// Snapshot of whether the user is currently in speaker mode (per the last
/// successful join). The cliff detector and JWT refresh use this to decide
/// whether to recycle as listener or speaker.
pub fn is_speaker(publisher_id: &str) -> bool {
    with_state(publisher_id, |s| s.desired == DesiredState::Speaker)
}

/// Remove all state for a publisher. Called by `use_nest_audio::leave_room`.
pub fn clear(publisher_id: &str) {
    let mut map = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
    map.remove(publisher_id);
}

/// Tear down the current session and re-establish it with the same desired
/// state and subscriptions. Uses exponential backoff (1s, 2s, 4s ... 30s cap)
/// matching the existing `join_room_with_retry` policy.
///
/// Called by:
/// - Phase 2.2 cliff detector (when audio frames stop arriving for a speaker)
/// - Phase 2.4 JWT refresh timer (proactive 540s recycle)
/// - Phase 2.5 network-change handler
///
/// Returns Ok(()) if the session was re-established (or was already Off),
/// Err with the last error otherwise.
pub async fn recycle(publisher_id: &str) -> Result<(), String> {
    let (cfg, desired, declined, subs) = with_state(publisher_id, |s| {
        (
            s.cfg.clone(),
            s.desired,
            s.declined_publish.load(Ordering::Relaxed),
            s.subs.clone(),
        )
    });
    let Some(cfg) = cfg else {
        return Ok(()); // nothing to recycle
    };
    if desired == DesiredState::Off {
        return Ok(());
    }

    // Tear down the old session.
    let _ = super::js_disconnect(publisher_id).await;

    // Decide whether to come back as listener or speaker.
    let want_publish = desired == DesiredState::Speaker && !declined;

    // Re-join with retry (exponential backoff is handled inside).
    super::js_init(publisher_id).await?;
    let jwt = super::authenticate_with_nest(&cfg.auth_url, &cfg.namespace, want_publish).await?;
    super::js_connect(
        publisher_id,
        &cfg.auth_url,
        &cfg.relay_url,
        &cfg.namespace,
        &jwt,
        &cfg.my_pubkey,
    )
    .await?;

    // If speaker mode, restart publishing.
    if want_publish {
        super::js_publish_audio(publisher_id).await?;
    }

    // Re-issue all subscriptions.
    for pk in &subs {
        let _ = super::js_subscribe_audio(publisher_id, pk).await;
    }

    // Update desired state if we fell back from speaker to listener.
    if desired == DesiredState::Speaker && !want_publish {
        with_state(publisher_id, |s| s.desired = DesiredState::Listener);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_and_clear() {
        let pid = "test-track-and-clear";
        track_subscribe(pid, "pk1");
        track_subscribe(pid, "pk2");
        let count = with_state(pid, |s| s.subs.len());
        assert_eq!(count, 2);
        clear(pid);
        let exists = with_state(pid, |s| s.cfg.is_some());
        assert!(!exists);
    }

    #[test]
    fn test_declined_publish_persists() {
        let pid = "test-declined-publish";
        set_declined_publish(pid, true);
        let v = with_state(pid, |s| s.declined_publish.load(Ordering::Relaxed));
        assert!(v);
        set_declined_publish(pid, false);
        let v = with_state(pid, |s| s.declined_publish.load(Ordering::Relaxed));
        assert!(!v);
        clear(pid);
    }

    #[test]
    fn test_desired_state_tracking() {
        let pid = "test-desired-state";
        let cfg = NestConfig {
            auth_url: "https://auth.example.com".into(),
            relay_url: "https://relay.example.com".into(),
            namespace: "nests/30312:abc:def".into(),
            my_pubkey: "aa".repeat(32),
        };
        track_join(pid, cfg, true);
        assert!(is_speaker(pid));
        track_join(pid, NestConfig {
            auth_url: "https://auth.example.com".into(),
            relay_url: "https://relay.example.com".into(),
            namespace: "nests/30312:abc:def".into(),
            my_pubkey: "aa".repeat(32),
        }, false);
        assert!(!is_speaker(pid));
        clear(pid);
    }

    #[test]
    fn test_recycle_returns_ok_when_off() {
        let pid = "test-recycle-off";
        let result = futures::executor::block_on(recycle(pid));
        assert!(result.is_ok());
    }
}
