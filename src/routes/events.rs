//! Events Discovery Page
//!
//! NIP-52 Calendar Events + NIP-53 Live Activities discovery page

use dioxus::prelude::*;

use std::collections::HashSet;
use crate::stores::{nostr_client, calendar_store};
use crate::stores::calendar_store::{UnifiedEvent, EventFilterState, TimeFilter, LocationFilter, EventTypeFilter};
use crate::components::{ClientInitializing, EventCard, EventCardSkeleton, EventMap};
use crate::hooks::use_infinite_scroll;

/// Debounce delay for NIP-50 search (milliseconds)
const SEARCH_DEBOUNCE_MS: u32 = 300;

/// View mode for events display
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum ViewMode {
    #[default]
    Grid,
    Map,
}

#[component]
pub fn Events() -> Element {
    // State
    let mut events = use_signal(Vec::<UnifiedEvent>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut view_mode = use_signal(|| ViewMode::Grid);
    let mut filters = use_signal(EventFilterState::default);
    let mut selected_hashtag = use_signal(|| None::<String>);

    // NIP-50 search state
    let mut search_results = use_signal(|| None::<Vec<UnifiedEvent>>);
    let mut searching = use_signal(|| false);
    let mut search_debounce_id = use_signal(|| 0u32);
    let mut show_more_filters = use_signal(|| false);
    // Track whether user is actively in search mode (separate from having results)
    let mut search_mode_active = use_signal(|| false);

    // Track previous search term to avoid redundant NIP-50 calls
    let mut last_search_term = use_signal(String::new);

    // Pagination state
    let mut has_more = use_signal(|| true);
    let mut pagination_loading = use_signal(|| false);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Load events on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        log::info!("[Events] use_effect triggered, client_initialized={}", client_initialized);

        if !client_initialized {
            log::info!("[Events] Client not ready, returning early");
            return;
        }

        log::info!("[Events] Client ready, spawning fetch task");

        spawn(async move {
            log::info!("[Events] Fetch task started");
            loading.set(true);
            error.set(None);

            match calendar_store::fetch_all_events_paginated(200, None).await {
                Ok((fetched_events, oldest_ts)) => {
                    log::info!("[Events] Fetched {} events, oldest_ts={:?}", fetched_events.len(), oldest_ts);
                    events.set(fetched_events.clone());
                    oldest_timestamp.set(oldest_ts);
                    has_more.set(fetched_events.len() >= 200);
                }
                Err(e) => {
                    log::error!("[Events] Failed to fetch events: {}", e);
                    error.set(Some(e));
                }
            }

            log::info!("[Events] Setting loading to false");
            loading.set(false);
        });
    });

    // Debounced NIP-50 search effect
    use_effect(move || {
        let search_term = filters.read().search_term.clone();
        let prev_term = last_search_term.read().clone();

        // Skip if search term hasn't changed (prevents redundant NIP-50 calls when other filters change)
        if search_term == prev_term {
            return;
        }

        // Update last search term
        last_search_term.set(search_term.clone());

        // Clear search results if search term is too short
        // Use chars().count() for proper Unicode handling (Dioxus pattern)
        let search_term_trimmed = search_term.trim();
        if search_term_trimmed.chars().count() < 2 {
            // Increment debounce ID to invalidate any in-flight searches
            let current_id = *search_debounce_id.peek() + 1;
            search_debounce_id.set(current_id);
            search_results.set(None);
            searching.set(false);
            // Exit search mode and reset pagination
            search_mode_active.set(false);
            has_more.set(true);
            return;
        }

        // Enter search mode
        search_mode_active.set(true);
        // Increment debounce ID to cancel previous searches
        // Use peek() to avoid creating a reactive dependency (would cause infinite loop)
        let current_id = *search_debounce_id.peek() + 1;
        search_debounce_id.set(current_id);
        searching.set(true);

        spawn(async move {
            // Wait for debounce delay
            #[cfg(target_family = "wasm")]
            {
                gloo_timers::future::TimeoutFuture::new(SEARCH_DEBOUNCE_MS).await;
            }
            #[cfg(not(target_family = "wasm"))]
            {
                use std::time::Duration;
                tokio::time::sleep(Duration::from_millis(SEARCH_DEBOUNCE_MS as u64)).await;
            }

            // Check if this search is still current (use peek to avoid re-subscription)
            if *search_debounce_id.peek() != current_id {
                return;
            }

            log::info!("[Events] NIP-50 search for: {}", search_term);

            match calendar_store::search_all_events(&search_term, 100).await {
                Ok(results) => {
                    // Check again if search is still current
                    if *search_debounce_id.peek() == current_id {
                        log::info!("[Events] NIP-50 search returned {} results", results.len());
                        search_results.set(Some(results));
                    }
                }
                Err(e) => {
                    log::error!("[Events] NIP-50 search failed: {}", e);
                    // Fall back to client-side filtering by clearing search results
                    if *search_debounce_id.peek() == current_id {
                        search_results.set(None);
                    }
                }
            }

            if *search_debounce_id.peek() == current_id {
                searching.set(false);
            }
        });
    });

    // Load more callback for infinite scroll
    let load_more = move || {
        log::info!("[Events] load_more called - pagination_loading: {}, has_more: {}",
                   *pagination_loading.peek(), *has_more.peek());

        if *pagination_loading.peek() || !*has_more.peek() {
            log::info!("[Events] load_more blocked by guards");
            return;
        }

        log::info!("[Events] load_more setting pagination_loading to true");
        pagination_loading.set(true);

        spawn(async move {
            let until = (*oldest_timestamp.peek()).map(|ts| ts.saturating_sub(1));
            log::info!("[Events] load_more fetching with until={:?}", until);

            match calendar_store::fetch_all_events_paginated(200, until).await {
                Ok((new_events, oldest_ts)) => {
                    log::info!("[Events] load_more fetched {} events", new_events.len());

                    if new_events.is_empty() {
                        has_more.set(false);
                    } else {
                        // Append new events, avoiding duplicates
                        let mut current = events.write();
                        for event in new_events {
                            if !current.iter().any(|e| e.coordinate() == event.coordinate()) {
                                current.push(event);
                            }
                        }
                        oldest_timestamp.set(oldest_ts);
                    }
                }
                Err(e) => {
                    log::error!("[Events] load_more failed: {}", e);
                }
            }

            pagination_loading.set(false);
        });
    };

    // Setup infinite scroll
    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);

    // Compute filtered events - use search results when available
    let filtered_events = use_memo(move || {
        let current_filters = filters.read().clone();
        let hashtag = selected_hashtag.read().clone();

        // Use search results if available (NIP-50 search is active)
        // Use search_mode_active to determine if we're filtering (not just presence of results)
        let from_nip50 = *search_mode_active.read();
        let base_events = if let Some(ref results) = *search_results.read() {
            log::info!("[Events] Using NIP-50 search results: {} events", results.len());
            results.clone()
        } else {
            let all_events = events.read();
            log::info!("[Events] Using all events: {} events", all_events.len());
            all_events.clone()
        };

        // Skip client-side search filter when using NIP-50 results (relay already filtered)
        let mut result = calendar_store::filter_events_with_nip50(&base_events, &current_filters, from_nip50);
        log::info!("[Events] filtered_events memo: after filter = {}", result.len());

        if let Some(ref tag) = hashtag {
            result.retain(|e| e.hashtags().contains(&tag.as_str()));
        }

        calendar_store::sort_events_for_display(&mut result);
        log::info!("[Events] filtered_events memo: final result = {}", result.len());
        result
    });

    // Compute hashtags from events
    let all_hashtags = use_memo(move || {
        let all_events = events.read();
        let mut tag_set = HashSet::new();
        for event in all_events.iter() {
            for tag in event.hashtags() {
                tag_set.insert(tag.to_string());
            }
        }
        let mut tags: Vec<String> = tag_set.into_iter().collect();
        tags.sort();
        tags.truncate(20);
        tags
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/95 backdrop-blur border-b border-border",
                div {
                    class: "px-4 py-3",

                    // Title and view toggle
                    div {
                        class: "flex items-center justify-between mb-3",
                        h1 {
                            class: "text-xl font-bold",
                            "Discover Events"
                        }

                        // View mode toggle
                        div {
                            class: "flex items-center bg-muted rounded-lg p-1",
                            button {
                                class: if *view_mode.read() == ViewMode::Grid {
                                    "px-3 py-1.5 text-sm font-medium bg-background rounded shadow-sm"
                                } else {
                                    "px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                                },
                                onclick: move |_| view_mode.set(ViewMode::Grid),
                                "Grid"
                            }
                            button {
                                class: if *view_mode.read() == ViewMode::Map {
                                    "px-3 py-1.5 text-sm font-medium bg-background rounded shadow-sm"
                                } else {
                                    "px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                                },
                                onclick: move |_| view_mode.set(ViewMode::Map),
                                "Map"
                            }
                        }
                    }

                    // Event type filter tabs
                    div {
                        class: "flex items-center bg-muted rounded-lg p-1 mb-3",
                        button {
                            class: if filters.read().event_type_filter == EventTypeFilter::All {
                                "flex-1 px-3 py-1.5 text-sm font-medium bg-background rounded shadow-sm"
                            } else {
                                "flex-1 px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| {
                                filters.write().event_type_filter = EventTypeFilter::All;
                            },
                            "All"
                        }
                        button {
                            class: if filters.read().event_type_filter == EventTypeFilter::Calendar {
                                "flex-1 px-3 py-1.5 text-sm font-medium bg-background rounded shadow-sm"
                            } else {
                                "flex-1 px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| {
                                filters.write().event_type_filter = EventTypeFilter::Calendar;
                            },
                            "Calendar"
                        }
                        button {
                            class: if filters.read().event_type_filter == EventTypeFilter::Meetings {
                                "flex-1 px-3 py-1.5 text-sm font-medium bg-background rounded shadow-sm"
                            } else {
                                "flex-1 px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground transition"
                            },
                            onclick: move |_| {
                                filters.write().event_type_filter = EventTypeFilter::Meetings;
                            },
                            "Meetings"
                        }
                    }

                    // Search input
                    div {
                        class: "relative mb-3",
                        // Search icon or loading spinner
                        if *searching.read() {
                            svg {
                                class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground animate-spin",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                circle {
                                    class: "opacity-25",
                                    cx: "12",
                                    cy: "12",
                                    r: "10",
                                    stroke: "currentColor",
                                    stroke_width: "4"
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                                }
                            }
                        } else {
                            svg {
                                class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
                                }
                            }
                        }
                        input {
                            r#type: "text",
                            placeholder: "Search events...",
                            class: "w-full pl-10 pr-4 py-2 bg-muted rounded-lg text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary",
                            value: "{filters.read().search_term}",
                            oninput: move |evt| {
                                filters.write().search_term = evt.value();
                            }
                        }
                    }

                    // Filter chips
                    div {
                        class: "flex items-center gap-2 overflow-x-auto pb-2 scrollbar-hide",

                        // Time filter dropdown
                        select {
                            class: "text-sm bg-muted border-none rounded-lg px-3 py-1.5 focus:outline-none focus:ring-2 focus:ring-primary",
                            value: "{filters.read().time_filter:?}",
                            onchange: move |evt| {
                                let value = evt.value();
                                let time_filter = match value.as_str() {
                                    "Today" => TimeFilter::Today,
                                    "ThisWeek" => TimeFilter::ThisWeek,
                                    "ThisMonth" => TimeFilter::ThisMonth,
                                    "Upcoming" => TimeFilter::Upcoming,
                                    "Past" => TimeFilter::Past,
                                    _ => TimeFilter::All,
                                };
                                filters.write().time_filter = time_filter;
                            },
                            option { value: "All", "All times" }
                            option { value: "Today", "Today" }
                            option { value: "ThisWeek", "This week" }
                            option { value: "ThisMonth", "This month" }
                            option { value: "Upcoming", "Upcoming" }
                            option { value: "Past", "Past" }
                        }

                        // Location filter dropdown
                        select {
                            class: "text-sm bg-muted border-none rounded-lg px-3 py-1.5 focus:outline-none focus:ring-2 focus:ring-primary",
                            value: "{filters.read().location_filter:?}",
                            onchange: move |evt| {
                                let value = evt.value();
                                let loc_filter = match value.as_str() {
                                    "InPerson" => LocationFilter::InPerson,
                                    "Online" => LocationFilter::Online,
                                    _ => LocationFilter::All,
                                };
                                filters.write().location_filter = loc_filter;
                            },
                            option { value: "All", "All locations" }
                            option { value: "InPerson", "In-Person" }
                            option { value: "Online", "Online" }
                        }

                        // More filters toggle button
                        button {
                            class: if *show_more_filters.read() {
                                "text-sm bg-primary/10 text-primary border border-primary/30 rounded-lg px-3 py-1.5 flex items-center gap-1"
                            } else {
                                "text-sm bg-muted rounded-lg px-3 py-1.5 flex items-center gap-1 hover:bg-accent transition"
                            },
                            onclick: move |_| {
                                let current = *show_more_filters.read();
                                show_more_filters.set(!current);
                            },
                            "More"
                            svg {
                                class: if *show_more_filters.read() { "w-3 h-3 rotate-180 transition-transform duration-200" } else { "w-3 h-3 transition-transform duration-200" },
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M19 9l-7 7-7-7"
                                }
                            }
                        }

                        // Clear filters button
                        if !filters.read().is_empty() || selected_hashtag.read().is_some() {
                            button {
                                class: "text-sm text-muted-foreground hover:text-foreground flex items-center gap-1 px-2",
                                onclick: move |_| {
                                    filters.write().clear();
                                    selected_hashtag.set(None);
                                },
                                svg {
                                    class: "w-4 h-4",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M6 18L18 6M6 6l12 12"
                                    }
                                }
                                "Clear"
                            }
                        }
                    }

                    // Hashtag chips
                    if !all_hashtags.read().is_empty() {
                        div {
                            class: "flex items-center gap-2 overflow-x-auto pb-1 scrollbar-hide",
                            for tag in all_hashtags.read().iter() {
                                button {
                                    key: "{tag}",
                                    class: if selected_hashtag.read().as_ref() == Some(tag) {
                                        "px-3 py-1 text-xs font-medium bg-primary text-primary-foreground rounded-full whitespace-nowrap"
                                    } else {
                                        "px-3 py-1 text-xs font-medium bg-muted text-muted-foreground hover:bg-accent rounded-full whitespace-nowrap transition"
                                    },
                                    onclick: {
                                        let tag = tag.clone();
                                        move |_| {
                                            let current = selected_hashtag.read().clone();
                                            if current.as_ref() == Some(&tag) {
                                                selected_hashtag.set(None);
                                            } else {
                                                selected_hashtag.set(Some(tag.clone()));
                                            }
                                        }
                                    },
                                    "#{tag}"
                                }
                            }
                        }
                    }

                    // Slide-down filter section
                    if *show_more_filters.read() {
                        div {
                            class: "mt-2 pt-3 border-t border-border",
                            div {
                                class: "flex flex-wrap items-center gap-3",
                                // Show ended events toggle
                                button {
                                    class: if !filters.read().hide_ended {
                                        "flex items-center gap-2 px-3 py-1.5 text-sm bg-primary/10 text-primary border border-primary/30 rounded-lg transition"
                                    } else {
                                        "flex items-center gap-2 px-3 py-1.5 text-sm bg-muted text-muted-foreground rounded-lg hover:bg-accent transition"
                                    },
                                    onclick: move |_| {
                                        let current = filters.read().hide_ended;
                                        filters.write().hide_ended = !current;
                                    },
                                    // Checkmark icon when active
                                    if !filters.read().hide_ended {
                                        svg {
                                            class: "w-4 h-4",
                                            xmlns: "http://www.w3.org/2000/svg",
                                            fill: "none",
                                            view_box: "0 0 24 24",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            path {
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                d: "M5 13l4 4L19 7"
                                            }
                                        }
                                    }
                                    "Show ended events"
                                }
                            }
                        }
                    }
                }

                // Stats bar
                div {
                    class: "px-4 py-2 bg-muted/50 text-sm text-muted-foreground flex items-center justify-between",
                    span {
                        "{filtered_events.read().len()} events"
                    }
                    if *loading.read() {
                        span {
                            class: "flex items-center gap-1",
                            svg {
                                class: "w-4 h-4 animate-spin",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                circle {
                                    class: "opacity-25",
                                    cx: "12",
                                    cy: "12",
                                    r: "10",
                                    stroke: "currentColor",
                                    stroke_width: "4"
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                                }
                            }
                            "Loading..."
                        }
                    }
                }
            }

            // Content - matches calendar.rs pattern with direct .read() calls
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if *loading.read() {
                div {
                    class: "p-4 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                    for _ in 0..6 {
                        EventCardSkeleton {}
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-8 text-center",
                    div {
                        class: "text-4xl mb-4",
                        "❌"
                    }
                    h3 {
                        class: "text-lg font-medium mb-2",
                        "Failed to load events"
                    }
                    p {
                        class: "text-red-500 mb-4",
                        "{err}"
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                        onclick: move |_| {
                            spawn(async move {
                                loading.set(true);
                                error.set(None);
                                if let Ok((fetched, oldest_ts)) = calendar_store::fetch_all_events_paginated(200, None).await {
                                    events.set(fetched.clone());
                                    oldest_timestamp.set(oldest_ts);
                                    has_more.set(fetched.len() >= 200);
                                }
                                loading.set(false);
                            });
                        },
                        "Retry"
                    }
                }
            } else if filtered_events.read().is_empty() {
                div {
                    class: "p-8 text-center",
                    div {
                        class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                        svg {
                            class: "w-8 h-8 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "1.5",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M6.75 3v2.25M17.25 3v2.25M3 18.75V7.5a2.25 2.25 0 012.25-2.25h13.5A2.25 2.25 0 0121 7.5v11.25m-18 0A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75m-18 0v-7.5A2.25 2.25 0 015.25 9h13.5A2.25 2.25 0 0121 11.25v7.5"
                            }
                        }
                    }
                    h3 {
                        class: "text-lg font-medium mb-2",
                        "No events found"
                    }
                    p {
                        class: "text-muted-foreground mb-4",
                        "No events match your current filters. Try adjusting the filters or check back later."
                    }
                    if !filters.read().is_empty() || selected_hashtag.read().is_some() {
                        button {
                            class: "px-4 py-2 bg-muted hover:bg-accent text-foreground rounded-lg transition",
                            onclick: move |_| {
                                filters.write().clear();
                                selected_hashtag.set(None);
                            },
                            "Clear filters"
                        }
                    }
                }
            } else {
                match *view_mode.read() {
                    ViewMode::Grid => {
                        rsx! {
                            div {
                                class: "p-4 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                                for event in filtered_events.read().iter() {
                                    EventCard {
                                        key: "{event.coordinate()}",
                                        event: event.clone(),
                                        from: Some("events".to_string())
                                    }
                                }
                            }
                        }
                    }
                    ViewMode::Map => {
                        rsx! {
                            div {
                                class: "p-4",
                                EventMap {
                                    events: filtered_events.read().clone(),
                                    height: "calc(100vh - 220px)".to_string()
                                }
                            }
                        }
                    }
                }
            }

            // Infinite scroll sentinel (only for Grid view, disabled during NIP-50 search)
            // Only show infinite scroll when not in search mode (search has its own result set)
            if *view_mode.read() == ViewMode::Grid && *has_more.read() && !*loading.read() && !filtered_events.read().is_empty() && !*search_mode_active.peek() {
                div {
                    id: "{sentinel_id}",
                    class: "p-8 flex justify-center",
                    if *pagination_loading.read() {
                        div {
                            class: "flex items-center gap-2 text-muted-foreground",
                            svg {
                                class: "w-5 h-5 animate-spin",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                circle {
                                    class: "opacity-25",
                                    cx: "12",
                                    cy: "12",
                                    r: "10",
                                    stroke: "currentColor",
                                    stroke_width: "4"
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                                }
                            }
                            span { "Loading more events..." }
                        }
                    }
                }
            }

            // End of results message
            if !*has_more.read() && !filtered_events.read().is_empty() {
                div {
                    class: "p-4 text-center text-muted-foreground text-sm",
                    "You've reached the end"
                }
            }
        }
    }
}
