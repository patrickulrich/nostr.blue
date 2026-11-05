//! Station favorites (kind 30078) — WaveFunc interop shape.
//!
//! Each user keeps a favorites list as an addressable kind 30078 event with
//! `l` = `wavefunc_user_favourite_list`, plain `["a", "31237:pubkey:d"]`
//! entry tags (WaveFunc reads only `tag[1]` — no relay/petname/added_at),
//! a `name` tag, and a content JSON `{name}`. The default list ("My Favorite
//! Stations") is auto-created on the first favorite, mirroring WaveFunc's
//! `useFavorites.ts`.
//!
//! Loading goes through the facade's client-parameterized fetch
//! (`nostr_client::fetch_with_client`, quorum-EOSE — same relay reach as the
//! rest of the radio baseline since the radio relays carry READ flags);
//! publishing goes through the publish queue with
//! `target_relays: None` after `ensure_radio_relays_connected_bounded` — the
//! radio relays carry the default READ|WRITE flags, so the queue's fast-path
//! delivers to them alongside the user's write relays (identical to
//! `publish_station`). Kind 30078 is addressable, so rapid heart-toggles
//! coalesce.
//!
//! # Load lifecycle
//!
//! The load is triggered once per login from `run_post_login_init`
//! (detached `spawn_forever` context) and may be re-triggered idempotently
//! from the UI (favorites tab, hearts, station viewer). The status is a
//! state machine (`RadioFavoritesStatus`), not booleans:
//!
//! - Concurrent callers coalesce on `LOAD_LOCK`: at most one fetch runs and
//!   the rest observe its result. The previous boolean guard treated
//!   "another load is in flight" as a failure, surfacing spurious
//!   "Couldn't load your favorites" errors.
//! - The load is cancel-safe: component-scoped spawns are cancelled on
//!   unmount (Dioxus drops a component's futures), so `LoadingGuard`
//!   demotes `Loading` back to `Idle` when the future is dropped
//!   mid-flight. Without it, `loading` stranded `true` forever and every
//!   later attempt early-returned into the error path.
//! - A fetch against a client that a login/logout rebuild swapped
//!   mid-flight is retried once on the current client (same
//!   Arc-identity guard as `nostr_client::fetch_radio_events`) — a dead
//!   pool's relay map is force-cleared, so `subscribe` errors out.

use crate::stores::nostr_client;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;

/// `l` tag label identifying the WaveFunc favorites list
pub const FAVORITES_LIST_LABEL: &str = "wavefunc_user_favourite_list";
/// Default list name (WaveFunc parity)
const DEFAULT_LIST_NAME: &str = "My Favorite Stations";

/// Load lifecycle for the favorites list.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum RadioFavoritesStatus {
    /// No completed load: never attempted, or a previous attempt was
    /// cancelled mid-flight (see `LoadingGuard`).
    #[default]
    Idle,
    /// A load is in flight.
    Loading,
    /// Load completed — "no list exists on relays" counts, so the first
    /// favorite may safely publish a fresh list.
    Loaded,
    /// The fetch failed; retried by the next `load()` call. The string is
    /// the failure reason (for logs/UI).
    Failed(String),
}

impl RadioFavoritesStatus {
    /// Whether a load has completed successfully.
    pub fn is_loaded(&self) -> bool {
        matches!(self, Self::Loaded)
    }
}

/// Favorites list state
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RadioFavoritesState {
    /// Load lifecycle (replaces the old `loading`/`loaded` booleans, which
    /// conflated "in flight" with "failed").
    pub status: RadioFavoritesStatus,
    /// Favorite station coordinates (`31237:pubkey:d`)
    pub favorites: HashSet<String>,
    /// Coordinate of the user's list event (`30078:pubkey:d`)
    pub list_coordinate: Option<String>,
}

/// Global favorites state
pub static RADIO_FAVORITES: GlobalSignal<RadioFavoritesState> = Signal::global(Default::default);

/// Serializes loads: at most one fetch runs; concurrent callers wait on the
/// lock and observe the winner's result ("join an in-flight load" — the old
/// boolean guard turned this case into a spurious failure).
static LOAD_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn load_lock() -> &'static tokio::sync::Mutex<()> {
    LOAD_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Serializes toggles so the read-modify-write cycle (read set → sign →
/// publish → write set) can't interleave: two rapid toggles would otherwise
/// both compute from the same base set and the later publish would silently
/// drop the first station.
static TOGGLE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn toggle_lock() -> &'static tokio::sync::Mutex<()> {
    TOGGLE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Whether a station coordinate is favorited.
pub fn is_favorite(coordinate: &str) -> bool {
    RADIO_FAVORITES.read().favorites.contains(coordinate)
}

/// Current favorites coordinate set, read non-subscribing (for callers like
/// the /radio effect that must not re-run when the set changes — they peek
/// to decide cache staleness instead of subscribing).
pub fn favorites_snapshot() -> HashSet<String> {
    RADIO_FAVORITES.peek().favorites.clone()
}

/// Cancel-safe `Loading` marker: when the load future is dropped before
/// writing a terminal state (`Loaded`/`Failed`), demote the status back to
/// `Idle`. Dioxus cancels component-owned futures on unmount ("ensure that
/// the state is restored when the future is dropped" — Cancel Safety, Dioxus
/// async docs); without this guard a mid-load unmount stranded
/// `loading = true` forever, after which every favorites attempt surfaced
/// "Couldn't load your favorites from relays" until logout.
struct LoadingGuard;

impl LoadingGuard {
    fn arm() -> Self {
        RADIO_FAVORITES.write().status = RadioFavoritesStatus::Loading;
        Self
    }
}

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        // `try_write_unchecked` (never panics on a dropped global): during
        // runtime teardown this task may be dropped along with the Runtime,
        // in which case the global's storage is already gone and there is
        // nothing to restore.
        if let Ok(mut state) = RADIO_FAVORITES.try_write_unchecked() {
            if state.status == RadioFavoritesStatus::Loading {
                state.status = RadioFavoritesStatus::Idle;
            }
        }
    }
}

/// Load the user's favorites list from relays.
///
/// - No-op (`Ok`) when already `Loaded`.
/// - Concurrent callers coalesce on `LOAD_LOCK` and observe the in-flight
///   attempt's result.
/// - Cancel-safe: a caller dropped mid-attempt demotes `Loading` → `Idle`
///   (retryable) instead of stranding the latch.
/// - Client-rebuild aware: retries once on the current client when the
///   captured one was swapped out by a login/logout rebuild mid-flight.
///
/// Returns `Err` only when a fetch genuinely failed (status left `Failed`;
/// the next call retries). Not-yet-signed-in callers also return `Err`
/// while leaving the status at `Idle` — the UI distinguishes that case via
/// `HAS_SIGNER` before calling.
pub async fn load() -> Result<(), String> {
    if RADIO_FAVORITES.read().status.is_loaded() {
        return Ok(());
    }
    let _guard = load_lock().lock().await;
    // Re-check after acquiring: the previous holder may have completed.
    if RADIO_FAVORITES.read().status.is_loaded() {
        return Ok(());
    }
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("Not signed in".to_string());
    }
    let Ok(pubkey) = nostr_client::get_cached_pubkey() else {
        return Err("No cached pubkey".to_string());
    };

    crate::stores::relay::wait_for_user_relays(Duration::from_secs(10), "radio favorites").await;

    let filter = Filter::new()
        .kind(Kind::from(30078))
        .author(pubkey)
        .custom_tag(
            SingleLetterTag::from_char('l').unwrap(),
            FAVORITES_LIST_LABEL,
        )
        .limit(5);

    let _loading = LoadingGuard::arm();

    // A login/logout rebuild swaps `NOSTR_CLIENT` and force-clears the old
    // pool; every SDK call on the dead client then fails (most visibly
    // `subscribe`, whose gossip filter breakdown yields "broken down
    // filters are empty"). Retry once on the *current* client when the one
    // we captured went stale (Arc identity check — no error-string
    // matching), mirroring `nostr_client::fetch_radio_events`.
    let mut last_err = String::new();
    for _attempt in 0..2 {
        let Some(client) = nostr_client::get_client() else {
            last_err = "Client not initialized".to_string();
            break;
        };
        // Bounded (~5s): the unbounded `ensure_radio_relay_connected`
        // waits up to 30s per radio relay sequentially, widening the
        // cancel/stale-client window for no gain — REQs queue on
        // still-Connecting relays and later fetches pick them up.
        crate::stores::relay::ensure_radio_relays_connected_bounded(
            &client,
            Duration::from_secs(5),
        )
        .await;

        let result = nostr_client::fetch_with_client(
            &client,
            nostr_client::FetchTarget::Auto(vec![filter.clone()]),
            nostr_client::FetchProfile::Std,
        )
        .await;

        match result {
            Ok(events) => {
                let newest = events.into_iter().max_by_key(|e| e.created_at);
                let mut state = RADIO_FAVORITES.write();
                state.status = RadioFavoritesStatus::Loaded;
                if let Some(event) = newest {
                    apply_list_event(&mut state, &event);
                }
                return Ok(());
            }
            Err(e) => {
                let err = e.to_string();
                let stale = nostr_client::get_client()
                    .as_ref()
                    .map(|current| !std::sync::Arc::ptr_eq(current, &client))
                    .unwrap_or(false);
                last_err = err;
                if !stale {
                    // Live client: the error is real (relays down).
                    break;
                }
                log::info!(
                    "radio favorites: client rebuilt mid-flight ({last_err}), retrying on current client"
                );
            }
        }
    }

    // A failed fetch is NOT "no list exists" — leaving the status at
    // `Failed` (not `Loaded`) means the next toggle/visit retries instead
    // of publishing a fresh list (new random d-tag) that orphans the
    // user's previously published favorites.
    log::warn!("Failed to load radio favorites (will retry on next attempt): {last_err}");
    RADIO_FAVORITES.write().status = RadioFavoritesStatus::Failed(last_err.clone());
    Err(last_err)
}

/// Populate state from a kind 30078 list event (pure; testable without a
/// Dioxus runtime).
fn apply_list_event(state: &mut RadioFavoritesState, event: &nostr::event::Event) {
    let d = event
        .tags
        .identifier()
        .map(|s| s.to_string())
        .unwrap_or_default();
    state.list_coordinate = Some(format!("30078:{}:{}", event.pubkey.to_hex(), d));
    state.favorites = event
        .tags
        .iter()
        .filter_map(|t| {
            let slice = t.as_slice();
            if slice.first().map(|s| s.as_str()) == Some("a") {
                slice.get(1).map(|s| s.to_string())
            } else {
                None
            }
        })
        .filter(|coord| coord.starts_with("31237:"))
        .collect();
}

/// Build the list event tags: `d`, `l` label, `name`, NIP-31 `alt`, then
/// plain 2-element `a` entries sorted for determinism (WaveFunc reads only
/// `tag[1]`). Pure; testable without a Dioxus runtime.
fn build_list_tags(d_tag: &str, name: &str, favorites: &HashSet<String>) -> Vec<Tag> {
    let mut tags = vec![
        Tag::custom("d", vec![d_tag]),
        Tag::custom("l", vec![FAVORITES_LIST_LABEL]),
        Tag::custom("name", vec![name]),
        Tag::custom("alt", vec!["Radio station favorites list"]),
    ];
    let mut coords: Vec<&String> = favorites.iter().collect();
    coords.sort();
    for coord in coords {
        tags.push(Tag::custom("a", vec![coord.clone()]));
    }
    tags
}

/// Toggle a station in the user's favorites list.
///
/// Ensures the existing list is loaded first (so the first toggle in a
/// session doesn't clobber a previously published list), creates the
/// default list on the first favorite, and returns the station's new
/// favorite state (`true` = now favorited). Toggles are serialized through
/// an async mutex so concurrent toggles can't compute from a stale base
/// set.
pub async fn toggle_favorite(
    station: &crate::utils::radio::RadioStation,
) -> std::result::Result<bool, String> {
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("Sign in to favorite stations".to_string());
    }
    // Hold the lock for the whole read-modify-write cycle, and re-read the
    // list state after acquiring so a serialized second toggle builds on
    // the first one's result.
    let _guard = toggle_lock().lock().await;
    if !RADIO_FAVORITES.read().status.is_loaded() {
        let _ = load().await;
    }
    if !RADIO_FAVORITES.read().status.is_loaded() {
        return Err("Couldn't load your favorites from relays — try again".to_string());
    }
    let pubkey = nostr_client::get_cached_pubkey()?;

    let (d_tag, mut favorites, currently_favorite) = {
        let state = RADIO_FAVORITES.read();
        let d = state
            .list_coordinate
            .as_ref()
            .and_then(|c| c.split(':').nth(2).map(|s| s.to_string()))
            .unwrap_or_default();
        let currently = state.favorites.contains(&station.coordinate);
        (d, state.favorites.clone(), currently)
    };

    let now_favorite = if currently_favorite {
        favorites.remove(&station.coordinate);
        false
    } else {
        favorites.insert(station.coordinate.clone());
        true
    };

    // WaveFunc uses a random uuid as the list d-tag; keep ours on edit.
    let d_tag = if d_tag.is_empty() {
        uuid::Uuid::new_v4().to_string()
    } else {
        d_tag
    };

    let content = serde_json::json!({ "name": DEFAULT_LIST_NAME }).to_string();
    let builder = EventBuilder::new(Kind::from(30078), content)
        .tags(build_list_tags(&d_tag, DEFAULT_LIST_NAME, &favorites));
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign: {e}"))?;

    if let Some(client) = nostr_client::get_client() {
        crate::stores::relay::ensure_radio_relays_connected_bounded(
            &client,
            Duration::from_secs(5),
        )
        .await;
    }
    crate::stores::publish_queue::enqueue_and_await(
        event,
        crate::stores::publish_queue::types::QueueEventType::Other("radio".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await?;

    let mut state = RADIO_FAVORITES.write();
    state.favorites = favorites;
    state.list_coordinate = Some(format!("30078:{pubkey}:{d_tag}"));
    state.status = RadioFavoritesStatus::Loaded;
    Ok(now_favorite)
}

/// Reset state (logout). Also drops the session radio list cache so a
/// different account never sees the previous user's pages.
pub fn reset() {
    *RADIO_FAVORITES.write() = RadioFavoritesState::default();
    crate::stores::audio::radio_cache::clear_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_event(d: &str, coords: &[&str], created_at: u64) -> nostr::event::Event {
        let mut tags = vec![
            Tag::custom("d", vec![d]),
            Tag::custom("l", vec![FAVORITES_LIST_LABEL]),
            Tag::custom("name", vec![DEFAULT_LIST_NAME]),
        ];
        for c in coords {
            tags.push(Tag::custom("a", vec![c.to_string()]));
        }
        // Non-station `a` entry must be ignored by the parser
        tags.push(Tag::custom("a",
            vec!["30078:aaaa:other-kind-list".to_string()],
        ));
        let keys = nostr::key::Keys::generate();
        EventBuilder::new(Kind::from(30078), r#"{"name":"My Favorite Stations"}"#)
            .custom_created_at(Timestamp::from(created_at))
            .tags(tags)
            .finalize(&keys)
            .unwrap()
    }

    #[test]
    fn test_apply_list_event_filters_to_station_coords() {
        let mut state = RadioFavoritesState::default();
        let event = list_event("uuid-1", &["31237:aa:st-1", "31237:bb:st-2"], 100);
        apply_list_event(&mut state, &event);
        assert_eq!(state.favorites.len(), 2);
        assert!(state.favorites.contains("31237:aa:st-1"));
        assert!(state.favorites.contains("31237:bb:st-2"));
        assert_eq!(
            state.list_coordinate.as_deref(),
            Some(format!("30078:{}:uuid-1", event.pubkey.to_hex()).as_str())
        );
    }

    #[test]
    fn test_build_list_tags_shape() {
        let mut favorites = HashSet::new();
        favorites.insert("31237:aa:z".to_string());
        favorites.insert("31237:aa:a".to_string());
        let tags = build_list_tags("d-1", DEFAULT_LIST_NAME, &favorites);
        let slices: Vec<Vec<String>> = tags
            .iter()
            .map(|t| t.as_slice().iter().map(|s| s.to_string()).collect())
            .collect();
        // d, l, name, alt, then sorted plain a-tags
        assert_eq!(slices[0], vec!["d".to_string(), "d-1".to_string()]);
        assert_eq!(
            slices[1],
            vec!["l".to_string(), FAVORITES_LIST_LABEL.to_string()]
        );
        assert_eq!(
            slices[2],
            vec!["name".to_string(), DEFAULT_LIST_NAME.to_string()]
        );
        assert_eq!(slices[3][0], "alt");
        assert_eq!(slices[4], vec!["a".to_string(), "31237:aa:a".to_string()]);
        assert_eq!(slices[5], vec!["a".to_string(), "31237:aa:z".to_string()]);
    }

    #[test]
    fn test_status_is_loaded_only_for_loaded() {
        assert!(!RadioFavoritesStatus::Idle.is_loaded());
        assert!(!RadioFavoritesStatus::Loading.is_loaded());
        assert!(RadioFavoritesStatus::Loaded.is_loaded());
        assert!(!RadioFavoritesStatus::Failed("err".into()).is_loaded());
    }
}
