//! Workouts feed (kind 1301, NIP-101e) — Polls feed pattern.
use crate::components::{ClientInitializing, WorkoutCard};
use crate::hooks::use_infinite_scroll_with_generation;
use crate::services::aggregation::{
    fetch_interaction_counts_batch, stream_interaction_counts, InteractionCounts,
    InteractionStreamHandle,
};
use crate::stores::nostr_client::stream_events_immediate;
use crate::stores::{auth_store, nostr_client};
use crate::utils::debounced_collector::DebouncedCollector;
use crate::utils::nips::nip101e::KIND_WORKOUT;
use dioxus::prelude::*;
use nostr_sdk::{Event, EventId, Filter, Kind, PublicKey, Timestamp};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

// Amethyst pulls 200/relay; 100 per aggregated page is a good density
// for rich cards while keeping the REQ light.
const PAGE_SIZE: usize = 100;
/// Global workout feeds only look back one week (Amethyst's default
/// `since` for the global builder).
const GLOBAL_LOOKBACK_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Clone, Copy, PartialEq, Debug)]
enum FeedType {
    Following,
    Global,
}
impl FeedType {
    fn label(&self) -> &'static str {
        match self {
            FeedType::Following => "Following",
            FeedType::Global => "Global",
        }
    }
}

#[component]
pub fn WorkoutsHome() -> Element {
    let mut events = use_signal(Vec::<Event>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut last_event_id = use_signal(|| None::<nostr_sdk::EventId>);
    let mut interaction_counts = use_signal(HashMap::<String, InteractionCounts>::new);
    let mut interaction_stream_handles: Signal<Vec<InteractionStreamHandle>> = use_signal(Vec::new);
    let mut all_streamed_ids = use_signal(HashSet::<EventId>::new);
    let mut request_id = use_signal(|| 0u64);
    let mut feed_reset_generation = use_signal(|| 0u64);
    use_effect(move || {
        let _ = refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        // Signer + relay readiness gate (canonical pattern, ref
        // routes/dms.rs): without it the stream below fires before the
        // NIP-65 relays land and targets DEFAULT relays, yielding
        // incomplete Following feeds.
        let has_signer = *nostr_client::HAS_SIGNER.read();
        let user_relays_applied = *crate::stores::relay::USER_RELAYS_APPLIED.read();
        if has_signer && !user_relays_applied {
            return;
        }
        loading.set(true);
        error.set(None);
        events.set(Vec::new());
        feed_reset_generation += 1;
        oldest_timestamp.set(None);
        last_event_id.set(None);
        has_more.set(true);
        let old_handles = interaction_stream_handles.peek().clone();
        interaction_stream_handles.set(Vec::new());
        interaction_counts.set(HashMap::new());
        all_streamed_ids.set(HashSet::new());
        request_id.with_mut(|v| *v = v.wrapping_add(1));
        let current_id = *request_id.peek();
        spawn(async move {
            for handle in old_handles {
                handle.unsubscribe().await;
            }
            let filter = match current_feed_type {
                FeedType::Following => {
                    let pubkey_str = match auth_store::get_pubkey() {
                        Some(pk) => pk,
                        None => {
                            error.set(Some(
                                "Not authenticated. Please sign in to view your following feed."
                                    .to_string(),
                            ));
                            loading.set(false);
                            return;
                        }
                    };
                    let contacts = match nostr_client::fetch_contacts(pubkey_str).await {
                        Ok(c) => c,
                        Err(e) => {
                            error.set(Some(e));
                            loading.set(false);
                            return;
                        }
                    };
                    let authors: Vec<PublicKey> = contacts
                        .iter()
                        .filter_map(|c| PublicKey::parse(c).ok())
                        .collect();
                    if authors.is_empty() {
                        loading.set(false);
                        return;
                    }
                    Filter::new()
                        .kind(Kind::from(KIND_WORKOUT))
                        .authors(authors)
                        .limit(PAGE_SIZE)
                }
                FeedType::Global => Filter::new()
                    .kind(Kind::from(KIND_WORKOUT))
                    .limit(PAGE_SIZE)
                    .since(Timestamp::from(
                        Timestamp::now().as_secs().saturating_sub(GLOBAL_LOOKBACK_SECS),
                    )),
            };
            // Stream events for fast time-to-first-post
            let mut seen_ids = HashSet::new();
            let collector = DebouncedCollector::<Event>::new(50);
            let result = stream_events_immediate(filter, Duration::from_secs(10), |event| {
                if *request_id.peek() != current_id {
                    return;
                }
                if seen_ids.insert(event.id) {
                    collector.extend([event], {
                        let mut events = events;
                        move |batch| {
                            if *request_id.peek() != current_id {
                                return;
                            }
                            let mut current = events.peek().clone();
                            current.extend(batch);
                            current.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                            events.set(current);
                        }
                    });
                }
            })
            .await;
            if *request_id.peek() == current_id {
                let tail = collector.drain();
                if !tail.is_empty() {
                    let mut current = events.peek().clone();
                    current.extend(tail);
                    current.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                    events.set(current);
                }
            }
            if *request_id.peek() != current_id {
                loading.set(false);
                return;
            }
            match result {
                Ok(count) => {
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }
                    let current_events = events.read();
                    if let Some(last_event) = current_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                        last_event_id.set(Some(last_event.id));
                    }
                    has_more.set(count > 0);
                    // NOTE: unlike the polls feed, exhaustion is NOT
                    // decided here. `count` only reflects events that
                    // arrived inside the 10s streaming window; a slow
                    // relay delivering after the window would make a
                    // partial page look like the end of the feed
                    // ("No more workouts to load" while Amethyst keeps
                    // showing content). Instead, keep the feed
                    // scrollable whenever anything streamed and let the
                    // `load_more` probe (an EOSE-based aggregated fetch)
                    // decide exhaustion authoritatively.
                    let event_ids: Vec<EventId> = current_events.iter().map(|e| e.id).collect();
                    drop(current_events);
                    all_streamed_ids.set(event_ids.iter().copied().collect());
                    if !event_ids.is_empty() {
                        match fetch_interaction_counts_batch(
                            event_ids.clone(),
                            Duration::from_secs(5),
                        )
                        .await
                        {
                            Ok(counts) => {
                                if *request_id.peek() != current_id {
                                    loading.set(false);
                                    return;
                                }
                                interaction_counts.set(counts);
                            }
                            Err(e) => {
                                log::warn!("Failed to fetch interaction counts: {e}");
                            }
                        }
                        if *request_id.peek() != current_id {
                            loading.set(false);
                            return;
                        }
                        match stream_interaction_counts(event_ids, interaction_counts, Some(600))
                            .await
                        {
                            Ok(handle) => {
                                if *request_id.peek() == current_id
                                    && *feed_type.read() == current_feed_type
                                {
                                    interaction_stream_handles
                                        .with_mut(|handles| handles.push(handle));
                                } else {
                                    handle.unsubscribe().await;
                                }
                            }
                            Err(e) => {
                                log::error!(
                                    "Failed to start interaction stream for workouts: {}",
                                    e
                                );
                            }
                        }
                    }
                    loading.set(false);
                }
                Err(e) => {
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }
                    error.set(Some(format!("Failed to fetch workouts: {}", e)));
                    loading.set(false);
                }
            }
        });
    });
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }
        if *nostr_client::HAS_SIGNER.peek() && !*crate::stores::relay::USER_RELAYS_APPLIED.peek()
        {
            return;
        }
        let until = *oldest_timestamp.read();
        let last_id = *last_event_id.read();
        let current_feed_type = *feed_type.read();
        loading.set(true);
        let rid = *request_id.peek();
        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_workouts(until, last_id).await,
                FeedType::Global => load_global_workouts(until, last_id).await,
            };
            match result {
                Ok(new_events) => {
                    if *request_id.peek() != rid {
                        loading.set(false);
                        return;
                    }
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }
                    let raw_count = new_events.len();
                    let existing_ids: std::collections::HashSet<_> = {
                        let current = events.read();
                        current.iter().map(|e| e.id).collect()
                    };
                    let unique_new: Vec<_> = new_events
                        .into_iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .collect();
                    if unique_new.is_empty() {
                        loading.set(false);
                        has_more.set(false);
                        return;
                    }
                    if let Some(last_event) = unique_new.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                        last_event_id.set(Some(last_event.id));
                    }
                    has_more.set(raw_count >= PAGE_SIZE);
                    let new_event_ids: Vec<EventId> = unique_new.iter().map(|e| e.id).collect();
                    events.with_mut(|current| {
                        current.extend(unique_new);
                    });
                    if !new_event_ids.is_empty() {
                        if let Ok(counts) = fetch_interaction_counts_batch(
                            new_event_ids.clone(),
                            Duration::from_secs(5),
                        )
                        .await
                        {
                            if *request_id.peek() != rid {
                                loading.set(false);
                                return;
                            }
                            interaction_counts.with_mut(|existing| {
                                existing.extend(counts);
                            });
                        } else {
                            log::warn!(
                                "Failed to fetch interaction counts for load_more (rid={}, events={})",
                                rid,
                                new_event_ids.len()
                            );
                        }
                        if *request_id.peek() != rid {
                            loading.set(false);
                            return;
                        }
                        let truly_new_ids: Vec<EventId> = new_event_ids
                            .into_iter()
                            .filter(|id| !all_streamed_ids.peek().contains(id))
                            .collect();
                        all_streamed_ids.with_mut(|ids| {
                            ids.extend(truly_new_ids.iter().copied());
                        });
                        if !truly_new_ids.is_empty() {
                            match stream_interaction_counts(
                                truly_new_ids,
                                interaction_counts,
                                Some(600),
                            )
                            .await
                            {
                                Ok(handle) => {
                                    if *request_id.peek() == rid
                                        && *feed_type.read() == current_feed_type
                                    {
                                        interaction_stream_handles
                                            .with_mut(|handles| handles.push(handle));
                                    } else {
                                        handle.unsubscribe().await;
                                    }
                                }
                                Err(e) => {
                                    log::error!(
                                        "Failed to start interaction stream for new workouts: {}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                    loading.set(false);
                }
                Err(e) => {
                    if *feed_type.read() != current_feed_type {
                        loading.set(false);
                        return;
                    }
                    log::error!("Failed to load more workouts: {}", e);
                    loading.set(false);
                }
            }
        });
    };
    let sentinel_id =
        use_infinite_scroll_with_generation(load_more, has_more, loading, feed_reset_generation);
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    div { class: "relative",
                        button {
                            class: "text-xl font-bold flex items-center gap-2 p-2 hover:bg-accent rounded-lg transition",
                            onclick: move |_| {
                                let current = *show_dropdown.read();
                                show_dropdown.set(!current);
                            },
                            "\u{1F3C3} {feed_type.read().label()}"
                            span { class: "text-sm", "\u{25BC}" }
                        }
                        if *show_dropdown.read() {
                            div {
                                class: "fixed inset-0 z-40",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_dropdown.set(false);
                                },
                            }
                            div { class: "absolute top-full left-0 mt-1 bg-card border border-border rounded-lg shadow-lg overflow-hidden z-40 min-w-[150px]",
                                button {
                                    class: "w-full px-4 py-2 text-left hover:bg-accent transition",
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Following);
                                        show_dropdown.set(false);
                                        refresh_trigger.with_mut(|v| *v += 1);
                                    },
                                    "Following"
                                }
                                button {
                                    class: "w-full px-4 py-2 text-left hover:bg-accent transition",
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Global);
                                        show_dropdown.set(false);
                                        refresh_trigger.with_mut(|v| *v += 1);
                                    },
                                    "Global"
                                }
                            }
                        }
                    }
                    div { class: "flex items-center gap-2",
                        button {
                            class: "px-4 py-2 text-sm rounded-lg hover:bg-accent transition",
                            onclick: move |_| {
                                refresh_trigger.with_mut(|v| *v += 1);
                            },
                            "\u{21BB} Refresh"
                        }
                        Link {
                            to: crate::routes::Route::WorkoutNew {},
                            class: "px-4 py-2 text-sm rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition font-medium",
                            "Log Workout"
                        }
                    }
                }
            }
            div { class: "max-w-2xl mx-auto",
                if !*nostr_client::CLIENT_INITIALIZED.read()
                    || (*nostr_client::HAS_SIGNER.read()
                        && !*crate::stores::relay::USER_RELAYS_APPLIED.read())
                {
                    ClientInitializing {}
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "\u{26A0}\u{FE0F}" }
                        h3 { class: "text-xl font-semibold mb-2", "Error" }
                        p { class: "text-muted-foreground", "{err}" }
                        button {
                            class: "mt-4 px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            onclick: move |_| {
                                refresh_trigger.with_mut(|v| *v += 1);
                            },
                            "Try Again"
                        }
                    }
                } else if events.read().is_empty() && !*loading.read() {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "\u{1F3C3}" }
                        h3 { class: "text-xl font-semibold mb-2", "No workouts yet" }
                        p { class: "text-muted-foreground",
                            if *feed_type.read() == FeedType::Following {
                                "Workouts from people you follow will appear here"
                            } else {
                                "Workouts from the last week will appear here"
                            }
                        }
                        Link {
                            to: crate::routes::Route::WorkoutNew {},
                            class: "inline-block mt-4 px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Log a Workout"
                        }
                    }
                } else {
                    div { class: "divide-y divide-border",
                        for event in events.read().iter() {
                            WorkoutCard {
                                key: "{event.id}",
                                event: event.clone(),
                                precomputed_counts: interaction_counts.read().get(&event.id.to_hex()).cloned(),
                            }
                        }
                    }
                    div { id: "{sentinel_id}", class: "p-8 flex justify-center",
                        if *loading.read() {
                            div { class: "flex items-center gap-3 text-muted-foreground",
                                span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading more..."
                            }
                        } else if !*has_more.read() {
                            p { class: "text-muted-foreground text-sm", "No more workouts to load" }
                        }
                    }
                }
            }
        }
    }
}

/// Load workouts from followed users
async fn load_following_workouts(
    until: Option<u64>,
    _last_event_id: Option<nostr_sdk::EventId>,
) -> Result<Vec<Event>, String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated. Please sign in to view your following feed.")?;
    let contacts = nostr_client::fetch_contacts(pubkey_str).await?;
    let authors: Vec<PublicKey> = contacts
        .iter()
        .filter_map(|c| PublicKey::parse(c).ok())
        .collect();
    if authors.is_empty() {
        return Ok(Vec::new());
    }
    let mut filter = Filter::new()
        .kind(Kind::from(KIND_WORKOUT))
        .authors(authors)
        .limit(PAGE_SIZE);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch workouts: {}", e))?;
    let mut event_vec: Vec<Event> = events.into_iter().collect();
    event_vec.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(event_vec)
}

/// Load workouts from everyone (global feed, last week)
async fn load_global_workouts(
    until: Option<u64>,
    _last_event_id: Option<nostr_sdk::EventId>,
) -> Result<Vec<Event>, String> {
    let mut filter = Filter::new()
        .kind(Kind::from(KIND_WORKOUT))
        .limit(PAGE_SIZE)
        .since(Timestamp::from(
            Timestamp::now().as_secs().saturating_sub(GLOBAL_LOOKBACK_SECS),
        ));
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch workouts: {}", e))?;
    let mut event_vec: Vec<Event> = events.into_iter().collect();
    event_vec.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(event_vec)
}
