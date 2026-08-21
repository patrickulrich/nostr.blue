//! Station favorites (kind 30078) — WaveFunc interop shape.
//!
//! Each user keeps a favorites list as an addressable kind 30078 event with
//! `l` = `wavefunc_user_favourite_list`, plain `["a", "31237:pubkey:d"]`
//! entry tags (WaveFunc reads only `tag[1]` — no relay/petname/added_at),
//! a `name` tag, and a content JSON `{name}`. The default list ("My Favorite
//! Stations") is auto-created on the first favorite, mirroring WaveFunc's
//! `useFavorites.ts`.
//!
//! Loading uses the plain SDK `client.fetch_events` (same as the rest of the
//! radio baseline); publishing goes through the publish queue with
//! `target_relays: None` after `ensure_radio_relay_connected` — the radio
//! relays carry the default READ|WRITE|PING flags, so the queue's fast-path
//! delivers to them alongside the user's write relays (identical to
//! `publish_station`). Kind 30078 is addressable, so rapid heart-toggles
//! coalesce.

use crate::stores::nostr_client;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::time::Duration;

/// `l` tag label identifying the WaveFunc favorites list
pub const FAVORITES_LIST_LABEL: &str = "wavefunc_user_favourite_list";
/// Default list name (WaveFunc parity)
const DEFAULT_LIST_NAME: &str = "My Favorite Stations";

/// Favorites list state
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RadioFavoritesState {
    /// Load completed (successfully or with "no list yet")
    pub loaded: bool,
    /// Load in progress
    pub loading: bool,
    /// Favorite station coordinates (`31237:pubkey:d`)
    pub favorites: HashSet<String>,
    /// Coordinate of the user's list event (`30078:pubkey:d`)
    pub list_coordinate: Option<String>,
}

/// Global favorites state
pub static RADIO_FAVORITES: GlobalSignal<RadioFavoritesState> = Signal::global(Default::default);

/// Whether a station coordinate is favorited.
pub fn is_favorite(coordinate: &str) -> bool {
    RADIO_FAVORITES.read().favorites.contains(coordinate)
}

/// Load the user's favorites list from relays.
///
/// No-op for logged-out users; for authenticated users, waits (bounded) for
/// the user's relay list to be applied before fetching. Idempotent —
/// concurrent/subsequent calls no-op while a load is in flight or after one
/// has completed.
pub async fn load() {
    if !*nostr_client::HAS_SIGNER.read() {
        return;
    }
    let Ok(pubkey) = nostr_client::get_cached_pubkey() else {
        return;
    };
    let Some(client) = nostr_client::get_client() else {
        return;
    };
    {
        let state = RADIO_FAVORITES.read();
        if state.loading || state.loaded {
            return;
        }
    }
    RADIO_FAVORITES.write().loading = true;

    crate::stores::relay::wait_for_user_relays(Duration::from_secs(10), "radio favorites").await;
    crate::stores::relay::ensure_radio_relay_connected(&client).await;

    let filter = Filter::new()
        .kind(Kind::from(30078))
        .author(pubkey)
        .custom_tag(
            SingleLetterTag::lowercase(Alphabet::L),
            FAVORITES_LIST_LABEL,
        )
        .limit(5);

    let result = client.fetch_events(filter, Duration::from_secs(10)).await;

    match result {
        Ok(events) => {
            let newest = events.into_iter().max_by_key(|e| e.created_at);
            let mut state = RADIO_FAVORITES.write();
            state.loading = false;
            state.loaded = true;
            if let Some(event) = newest {
                apply_list_event(&mut state, &event);
            }
        }
        Err(e) => {
            log::warn!("Failed to load radio favorites: {e}");
            let mut state = RADIO_FAVORITES.write();
            state.loading = false;
            state.loaded = true;
        }
    }
}

/// Populate state from a kind 30078 list event (pure; testable without a
/// Dioxus runtime).
fn apply_list_event(state: &mut RadioFavoritesState, event: &nostr::Event) {
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
        Tag::custom(TagKind::d(), vec![d_tag]),
        Tag::custom(TagKind::custom("l"), vec![FAVORITES_LIST_LABEL]),
        Tag::custom(TagKind::custom("name"), vec![name]),
        Tag::from_standardized_without_cell(TagStandard::Alt(
            "Radio station favorites list".to_string(),
        )),
    ];
    let mut coords: Vec<&String> = favorites.iter().collect();
    coords.sort();
    for coord in coords {
        tags.push(Tag::custom(TagKind::custom("a"), vec![coord.clone()]));
    }
    tags
}

/// Toggle a station in the user's favorites list.
///
/// Ensures the existing list is loaded first (so the first toggle in a
/// session doesn't clobber a previously published list), creates the
/// default list on the first favorite, and returns the station's new
/// favorite state (`true` = now favorited).
pub async fn toggle_favorite(
    station: &crate::utils::radio::RadioStation,
) -> std::result::Result<bool, String> {
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("Sign in to favorite stations".to_string());
    }
    if !RADIO_FAVORITES.read().loaded {
        load().await;
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
        crate::stores::relay::ensure_radio_relay_connected(&client).await;
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
    state.loaded = true;
    Ok(now_favorite)
}

/// Reset state (logout).
pub fn reset() {
    *RADIO_FAVORITES.write() = RadioFavoritesState::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_event(d: &str, coords: &[&str], created_at: u64) -> nostr::Event {
        let mut tags = vec![
            Tag::custom(TagKind::d(), vec![d]),
            Tag::custom(TagKind::custom("l"), vec![FAVORITES_LIST_LABEL]),
            Tag::custom(TagKind::custom("name"), vec![DEFAULT_LIST_NAME]),
        ];
        for c in coords {
            tags.push(Tag::custom(TagKind::custom("a"), vec![c.to_string()]));
        }
        // Non-station `a` entry must be ignored by the parser
        tags.push(Tag::custom(
            TagKind::custom("a"),
            vec!["30078:aaaa:other-kind-list".to_string()],
        ));
        let keys = nostr::key::Keys::generate();
        EventBuilder::new(Kind::from(30078), r#"{"name":"My Favorite Stations"}"#)
            .custom_created_at(Timestamp::from(created_at))
            .tags(tags)
            .sign_with_keys(&keys)
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
}
