//! Kind-33401 exercise-template cache.
//!
//! Strength-form workouts reference exercise templates by coordinate
//! (`33401:pubkey:d-tag`). Cards render the slug-derived name immediately
//! and swap in the template's real title once fetched (gossip-routed to
//! the template author's write relays).
//!
//! Follows the PROFILE_CACHE two-signal pattern: the LruCache mutation
//! does not notify subscribers, so a companion version signal is bumped
//! on every insert for consumers to re-evaluate against.
use crate::utils::nips::nip101e::{self, ExerciseTemplate};
use dioxus::prelude::*;
use lru::LruCache;
use nostr_sdk::prelude::*;
use std::num::NonZeroUsize;
use std::time::Duration;

pub static WORKOUT_TEMPLATE_CACHE: GlobalSignal<LruCache<String, Option<ExerciseTemplate>>> =
    Signal::global(|| LruCache::new(NonZeroUsize::new(500).unwrap()));

/// Bumped on every cache insert (LruCache mutation alone is not reactive).
pub static WORKOUT_TEMPLATE_CACHE_VERSION: GlobalSignal<u64> = Signal::global(|| 0);

/// In-flight fetch dedup so concurrent cards don't double-fetch.
static IN_FLIGHT: std::sync::LazyLock<tokio::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(|| {
        tokio::sync::Mutex::new(std::collections::HashSet::new())
    });

/// Best-effort display title for a template coordinate: the `title` tag,
/// else the slug prettified. `None` when not yet cached (caller falls
/// back to the workout-side slug).
pub fn cached_title(reference: &str) -> Option<String> {
    let cache = WORKOUT_TEMPLATE_CACHE.read();
    cache.peek(reference).and_then(|opt| {
        opt.as_ref()
            .and_then(|t| t.title.clone().or_else(|| Some(nip101e::slug_to_title(&t.d_tag))))
    })
}

/// Fetch a kind-33401 template by coordinate and cache it. A template
/// that genuinely does not exist is cached as `None`; network errors
/// leave the entry uncached so a later mount can retry.
pub async fn fetch_template(reference: String) {
    if WORKOUT_TEMPLATE_CACHE.read().contains(&reference) {
        return;
    }
    {
        let mut in_flight = IN_FLIGHT.lock().await;
        if !in_flight.insert(reference.clone()) {
            return;
        }
    }
    let coordinate = match Coordinate::parse(&reference) {
        Ok(c) => c,
        Err(_) => {
            insert(reference.clone(), None).await;
            IN_FLIGHT.lock().await.remove(&reference);
            return;
        }
    };
    let filter = Filter::new()
        .kind(coordinate.kind)
        .author(coordinate.public_key)
        .identifier(coordinate.identifier)
        .limit(1);
    match crate::stores::nostr_client::fetch_events_aggregated_outbox(
        filter,
        Duration::from_secs(10),
    )
    .await
    {
        Ok(events) => {
            let template = events
                .first()
                .and_then(|e| nip101e::parse_exercise_template(e).ok());
            insert(reference.clone(), template).await;
        }
        Err(e) => {
            log::debug!("Exercise template fetch failed (will retry later): {}", e);
        }
    }
    IN_FLIGHT.lock().await.remove(&reference);
}

async fn insert(reference: String, template: Option<ExerciseTemplate>) {
    {
        let mut cache = WORKOUT_TEMPLATE_CACHE.write();
        cache.put(reference, template);
    }
    WORKOUT_TEMPLATE_CACHE_VERSION.with_mut(|v| *v = v.wrapping_add(1));
}
