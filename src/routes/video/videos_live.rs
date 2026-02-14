use crate::components::{ClientInitializing, MiniLiveStreamCard};
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
use dioxus_core::use_drop;
use nostr_sdk::{Event, Filter, Kind, PublicKey, RelayPoolNotification, SubscriptionId, TagKind, Timestamp};
use std::collections::HashSet;
use std::time::Duration;
#[derive(Clone, Copy, PartialEq, Debug)]
enum StatusFilter {
    Live,
    Upcoming,
    All,
}
#[component]
pub fn VideosLive() -> Element {
    let mut following_streams = use_signal(Vec::<Event>::new);
    let mut loading_following = use_signal(|| false);
    let mut has_more_following = use_signal(|| true);
    let mut oldest_timestamp_following = use_signal(|| None::<u64>);
    let mut error_following = use_signal(|| None::<String>);
    let mut global_streams = use_signal(Vec::<Event>::new);
    let mut loading_global = use_signal(|| false);
    let mut has_more_global = use_signal(|| true);
    let mut oldest_timestamp_global = use_signal(|| None::<u64>);
    let mut error_global = use_signal(|| None::<String>);
    let mut status_filter = use_signal(|| StatusFilter::Live);
    let mut refresh_trigger = use_signal(|| 0);
    let mut request_id_following = use_signal(|| 0u32);
    let mut request_id_global = use_signal(|| 0u32);
    let mut last_loaded_following = use_signal(|| (0u32, StatusFilter::Live));
    let mut last_loaded_global = use_signal(|| (0u32, StatusFilter::Live));
    let mut stream_sub_id: Signal<Option<SubscriptionId>> = use_signal(|| None);
    let mut followed_pubkeys_cache: Signal<Option<HashSet<String>>> = use_signal(|| None);
    use_effect(move || {
        let refresh = *refresh_trigger.read();
        let current_status = *status_filter.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let (last_refresh, last_status) = *last_loaded_following.peek();
        let has_data = !following_streams.peek().is_empty();
        let status_changed = current_status != last_status;
        let refresh_changed = refresh != last_refresh;
        if has_data && !status_changed && !refresh_changed {
            log::debug!("Skipping following streams re-load: data already present");
            return;
        }
        last_loaded_following.set((refresh, current_status));
        let current_id = *request_id_following.peek() + 1;
        request_id_following.set(current_id);
        if !has_data {
            loading_following.set(true);
        }
        error_following.set(None);
        oldest_timestamp_following.set(None);
        has_more_following.set(true);
        spawn(async move {
            if *request_id_following.peek() != current_id {
                log::debug!("Discarding stale following streams request {}", current_id);
                return;
            }
            match load_following_streams(None, current_status).await {
                Ok((events, next_until, hit_limit, did_fallback, followed_pks)) => {
                    if did_fallback {
                        log::info!("No contacts, hiding Following streams section");
                        following_streams.set(Vec::new());
                    } else {
                        oldest_timestamp_following.set(next_until);
                        has_more_following.set(hit_limit);
                        following_streams.set(events);
                    }
                    // Cache followed pubkeys for real-time streaming
                    if let Some(pks) = followed_pks {
                        followed_pubkeys_cache.set(Some(pks));
                    }
                    loading_following.set(false);
                }
                Err(e) => {
                    error_following.set(Some(e));
                    loading_following.set(false);
                }
            }
        });
    });
    use_effect(move || {
        let refresh = *refresh_trigger.read();
        let current_status = *status_filter.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let (last_refresh, last_status) = *last_loaded_global.peek();
        let has_data = !global_streams.peek().is_empty();
        let status_changed = current_status != last_status;
        let refresh_changed = refresh != last_refresh;
        if has_data && !status_changed && !refresh_changed {
            log::debug!("Skipping global streams re-load: data already present");
            return;
        }
        last_loaded_global.set((refresh, current_status));
        let current_id = *request_id_global.peek() + 1;
        request_id_global.set(current_id);
        if !has_data {
            loading_global.set(true);
        }
        error_global.set(None);
        oldest_timestamp_global.set(None);
        has_more_global.set(true);
        spawn(async move {
            if *request_id_global.peek() != current_id {
                log::debug!("Discarding stale global streams request {}", current_id);
                return;
            }
            match load_global_streams(None, current_status).await {
                Ok((events, next_until, hit_limit)) => {
                    oldest_timestamp_global.set(next_until);
                    has_more_global.set(hit_limit);
                    global_streams.set(events);
                    loading_global.set(false);
                }
                Err(e) => {
                    error_global.set(Some(e));
                    loading_global.set(false);
                }
            }
        });
    });
    // Set up real-time subscription for stream updates
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        // Only set up once
        if stream_sub_id.peek().is_some() {
            return;
        }

        spawn(async move {
            if let Some(client) = nostr_client::get_client() {
                let filter = Filter::new()
                    .kind(Kind::Custom(30311))
                    .since(Timestamp::now())
                    .limit(0);

                match client.subscribe(filter, None).await {
                    Ok(output) => {
                        let subscription_id = output.val;
                        stream_sub_id.set(Some(subscription_id.clone()));
                        log::debug!("Subscribed for live stream updates");

                        let mut notifications = client.notifications();
                        while let Ok(notification) = notifications.recv().await {
                            if let RelayPoolNotification::Event {
                                subscription_id: sub_id,
                                event,
                                ..
                            } = notification
                            {
                                if sub_id == subscription_id {
                                    let current_filter = *status_filter.peek();
                                    let followed = followed_pubkeys_cache.peek().clone();

                                    // Upsert into global (always)
                                    upsert_stream_event(
                                        &mut global_streams,
                                        (*event).clone(),
                                        current_filter,
                                    );

                                    // Upsert into following (if followed)
                                    if let Some(ref followed_set) = followed {
                                        if followed_set.contains(&event.pubkey.to_hex()) {
                                            upsert_stream_event(
                                                &mut following_streams,
                                                (*event).clone(),
                                                current_filter,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => log::error!("Failed to subscribe for stream updates: {}", e),
                }
            }
        });
    });
    // Clean up subscription on unmount
    use_drop(move || {
        if let Some(sub_id) = stream_sub_id.peek().clone() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    client.unsubscribe(&sub_id).await;
                    log::debug!("Cleaned up stream subscription");
                }
            });
        }
    });
    let mut load_more_following = move || {
        if *loading_following.read() || !*has_more_following.read() {
            return;
        }
        let until = *oldest_timestamp_following.read();
        let current_status = *status_filter.read();
        loading_following.set(true);
        spawn(async move {
            match load_following_streams(until, current_status).await {
                Ok((new_events, next_until, hit_limit, _did_fallback, _followed_pks)) => {
                    let existing_ids: std::collections::HashSet<_> = {
                        let current = following_streams.read();
                        current.iter().map(|e| e.id).collect()
                    };
                    let unique_events: Vec<_> = new_events
                        .into_iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .collect();
                    oldest_timestamp_following.set(next_until);
                    has_more_following.set(hit_limit);
                    if !unique_events.is_empty() {
                        let mut current = following_streams.read().clone();
                        current.extend(unique_events);
                        following_streams.set(current);
                    }
                    loading_following.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more following streams: {}", e);
                    loading_following.set(false);
                }
            }
        });
    };
    let mut load_more_global = move || {
        if *loading_global.read() || !*has_more_global.read() {
            return;
        }
        let until = *oldest_timestamp_global.read();
        let current_status = *status_filter.read();
        loading_global.set(true);
        spawn(async move {
            match load_global_streams(until, current_status).await {
                Ok((new_events, next_until, hit_limit)) => {
                    let existing_ids: std::collections::HashSet<_> = {
                        let current = global_streams.read();
                        current.iter().map(|e| e.id).collect()
                    };
                    let unique_events: Vec<_> = new_events
                        .into_iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .collect();
                    oldest_timestamp_global.set(next_until);
                    has_more_global.set(hit_limit);
                    if !unique_events.is_empty() {
                        let mut current = global_streams.read().clone();
                        current.extend(unique_events);
                        global_streams.set(current);
                    }
                    loading_global.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more global streams: {}", e);
                    loading_global.set(false);
                }
            }
        });
    };
    rsx! {
        div { class: "min-h-screen bg-background",
            div { class: "sticky top-0 z-20 bg-background/95 backdrop-blur-sm border-b border-border",
                div { class: "px-6 py-4 flex items-center justify-between max-w-[1600px] mx-auto",
                    h1 { class: "text-2xl font-bold flex items-center gap-3",
                        svg {
                            class: "w-7 h-7",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z",
                            }
                        }
                        "Live Streams"
                    }
                    div { class: "flex items-center gap-3",
                        if auth_store::AUTH_STATE.read().is_authenticated {
                            Link {
                                to: Route::LiveStreamNew {},
                                class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white font-medium rounded-lg transition",
                                "Create Stream"
                            }
                        }
                        button {
                            class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                            disabled: *loading_following.read() || *loading_global.read(),
                            onclick: move |_| {
                                let current = *refresh_trigger.read();
                                refresh_trigger.set(current + 1);
                            },
                            title: "Refresh",
                            if *loading_following.read() || *loading_global.read() {
                                span { class: "inline-block w-5 h-5 border-2 border-foreground border-t-transparent rounded-full animate-spin" }
                            } else {
                                crate::components::icons::RefreshIcon { class: "w-5 h-5" }
                            }
                        }
                    }
                }
            }
            div { class: "max-w-[1600px] mx-auto px-6 py-6",
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else {
                    div { class: "flex gap-2 mb-6",
                        button {
                            class: if *status_filter.read() == StatusFilter::Live { "px-4 py-2 bg-red-600 text-white font-medium rounded-lg" } else { "px-4 py-2 bg-accent hover:bg-accent/80 font-medium rounded-lg transition" },
                            onclick: move |_| status_filter.set(StatusFilter::Live),
                            "🔴 Live"
                        }
                        button {
                            class: if *status_filter.read() == StatusFilter::Upcoming { "px-4 py-2 bg-blue-600 text-white font-medium rounded-lg" } else { "px-4 py-2 bg-accent hover:bg-accent/80 font-medium rounded-lg transition" },
                            onclick: move |_| status_filter.set(StatusFilter::Upcoming),
                            "Upcoming"
                        }
                        button {
                            class: if *status_filter.read() == StatusFilter::All { "px-4 py-2 bg-primary text-white font-medium rounded-lg" } else { "px-4 py-2 bg-accent hover:bg-accent/80 font-medium rounded-lg transition" },
                            onclick: move |_| status_filter.set(StatusFilter::All),
                            "All"
                        }
                    }
                    if *loading_following.read() || !following_streams.read().is_empty()
                        || error_following.read().is_some()
                    {
                        div { class: "mb-12",
                            h2 { class: "text-xl font-bold mb-4", "Following" }
                            if *loading_following.read() && following_streams.read().is_empty() {
                                div { class: "flex items-center justify-center py-20",
                                    div { class: "w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" }
                                }
                            } else if let Some(err) = error_following.read().as_ref() {
                                div { class: "text-center py-20 text-muted-foreground",
                                    "Error loading streams: {err}"
                                }
                            } else {
                                div { class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                                    for event in following_streams.read().iter() {
                                        MiniLiveStreamCard {
                                            key: "{event.id}",
                                            event: event.clone(),
                                        }
                                    }
                                }
                                if *loading_following.read() {
                                    div { class: "flex items-center justify-center py-8",
                                        div { class: "w-6 h-6 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" }
                                    }
                                }
                                if *has_more_following.read() && !*loading_following.read() {
                                    div { class: "flex justify-center mt-6",
                                        button {
                                            class: "px-6 py-3 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                            onclick: move |_| load_more_following(),
                                            "Load More"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div {
                        h2 { class: "text-xl font-bold mb-4", "Global" }
                        if *loading_global.read() && global_streams.read().is_empty() {
                            div { class: "flex items-center justify-center py-20",
                                div { class: "w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" }
                            }
                        } else if let Some(err) = error_global.read().as_ref() {
                            div { class: "text-center py-20 text-muted-foreground",
                                "Error loading streams: {err}"
                            }
                        } else if global_streams.read().is_empty() {
                            div { class: "text-center py-20 text-muted-foreground",
                                "No global streams found"
                            }
                        } else {
                            div { class: "grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                                for event in global_streams.read().iter() {
                                    MiniLiveStreamCard {
                                        key: "{event.id}",
                                        event: event.clone(),
                                    }
                                }
                            }
                            if *loading_global.read() {
                                div { class: "flex items-center justify-center py-8",
                                    div { class: "w-6 h-6 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" }
                                }
                            }
                            if *has_more_global.read() && !*loading_global.read() {
                                div { class: "flex justify-center mt-6",
                                    button {
                                        class: "px-6 py-3 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                        onclick: move |_| load_more_global(),
                                        "Load More"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Returns (events, next_until, hit_limit, did_fallback, followed_pubkeys)
async fn load_following_streams(
    until: Option<u64>,
    status: StatusFilter,
) -> Result<(Vec<Event>, Option<u64>, bool, bool, Option<HashSet<String>>), String> {
    let pubkey_str = auth_store::AUTH_STATE
        .read()
        .pubkey
        .clone()
        .ok_or("Not authenticated")?;
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            let (events, next_until, hit_limit) = load_global_streams(until, status)
                .await?;
            return Ok((events, next_until, hit_limit, true, None));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global streams");
        let (events, next_until, hit_limit) = load_global_streams(until, status).await?;
        return Ok((events, next_until, hit_limit, true, None));
    }
    let followed_pubkeys: std::collections::HashSet<String> = contacts
        .iter()
        .filter_map(|contact| PublicKey::parse(contact).ok().map(|pk| pk.to_string()))
        .collect();
    if followed_pubkeys.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let (events, next_until, hit_limit) = load_global_streams(until, status).await?;
        return Ok((events, next_until, hit_limit, true, None));
    }
    let mut filter = Filter::new().kind(Kind::Custom(30311)).limit(100);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let mut events = Vec::new();
    nostr_client::stream_events_immediate(
        filter,
        Duration::from_secs(10),
        |event| {
            events.push(event);
        },
    )
    .await
    .map_err(|e| format!("Failed to fetch streams: {}", e))?;
    let mut seen = std::collections::HashSet::new();
    events.retain(|e| seen.insert(e.id));
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let next_until = events.last().map(|e| e.created_at.as_secs());
    let hit_limit = events.len() >= 100;
    let following_events: Vec<Event> = events
        .into_iter()
        .filter(|event| {
            if followed_pubkeys.contains(&event.pubkey.to_string()) {
                return true;
            }
            for tag in event.tags.iter() {
                let tag_vec = tag.clone().to_vec();
                if tag_vec.first().map(|s| s.as_str()) == Some("p") {
                    if let Some(creator_pk) = tag_vec.get(1) {
                        if followed_pubkeys.contains(creator_pk) {
                            return true;
                        }
                    }
                    break;
                }
            }
            false
        })
        .collect();
    let filtered_events = filter_by_status(following_events, status);
    Ok((filtered_events, next_until, hit_limit, false, Some(followed_pubkeys)))
}
async fn load_global_streams(
    until: Option<u64>,
    status: StatusFilter,
) -> Result<(Vec<Event>, Option<u64>, bool), String> {
    let mut filter = Filter::new().kind(Kind::Custom(30311)).limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let mut all_events = Vec::new();
    nostr_client::stream_events_immediate(
        filter,
        Duration::from_secs(10),
        |event| {
            all_events.push(event);
        },
    )
    .await
    .map_err(|e| format!("Failed to fetch streams: {}", e))?;
    let mut seen = std::collections::HashSet::new();
    all_events.retain(|e| seen.insert(e.id));
    all_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let next_until = all_events.last().map(|e| e.created_at.as_secs());
    let hit_limit = all_events.len() >= 50;
    let filtered_events = filter_by_status(all_events, status);
    Ok((filtered_events, next_until, hit_limit))
}
fn filter_by_status(events: Vec<Event>, status: StatusFilter) -> Vec<Event> {
    match status {
        StatusFilter::All => events,
        StatusFilter::Live => {
            events
                .into_iter()
                .filter(|event| {
                    event
                        .tags
                        .iter()
                        .any(|tag| {
                            let tag_vec = tag.clone().to_vec();
                            tag_vec.first().map(|s| s.as_str()) == Some("status")
                                && tag_vec.get(1).map(|s| s.to_lowercase())
                                    == Some("live".to_string())
                        })
                })
                .collect()
        }
        StatusFilter::Upcoming => {
            events
                .into_iter()
                .filter(|event| {
                    event
                        .tags
                        .iter()
                        .any(|tag| {
                            let tag_vec = tag.clone().to_vec();
                            tag_vec.first().map(|s| s.as_str()) == Some("status")
                                && tag_vec.get(1).map(|s| s.to_lowercase())
                                    == Some("planned".to_string())
                        })
                })
                .collect()
        }
    }
}

fn get_stream_d_tag(event: &Event) -> Option<String> {
    event
        .tags
        .iter()
        .find(|tag| tag.kind() == TagKind::d())
        .and_then(|tag| tag.content().map(|s| s.to_string()))
}

fn upsert_stream_event(
    streams: &mut Signal<Vec<Event>>,
    new_event: Event,
    status_filter: StatusFilter,
) {
    // First check if it passes the current filter
    let filtered_events = filter_by_status(vec![new_event.clone()], status_filter);
    let filtered_event = match filtered_events.into_iter().next() {
        Some(e) => e,
        None => {
            // Event doesn't pass filter - remove it if it exists (status changed)
            let new_d_tag = get_stream_d_tag(&new_event);
            let new_pubkey = new_event.pubkey;
            streams.write().retain(|e| {
                !(e.pubkey == new_pubkey && get_stream_d_tag(e) == new_d_tag)
            });
            return;
        }
    };

    let new_d_tag = get_stream_d_tag(&filtered_event);
    let new_pubkey = filtered_event.pubkey;

    let mut current = streams.write();
    // Find existing event with same pubkey + d-tag (replaceable event identity)
    if let Some(idx) = current
        .iter()
        .position(|e| e.pubkey == new_pubkey && get_stream_d_tag(e) == new_d_tag)
    {
        // Replace only if newer
        if filtered_event.created_at > current[idx].created_at {
            current[idx] = filtered_event;
            log::debug!("Updated existing stream");
        }
    } else {
        // New stream
        current.push(filtered_event);
        log::debug!("Added new stream via streaming");
    }
}
