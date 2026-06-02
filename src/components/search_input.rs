use crate::routes::Route;
use crate::services::profile_search::{
    get_contact_pubkeys, search_cached_profiles, search_profiles, ProfileSearchResult,
};
use crate::services::search::query_parser;
use crate::stores::ui::search_history;
use dioxus::prelude::Event as DioxusEvent;
use dioxus::prelude::*;
use dioxus_core::Task;
use nostr_sdk::prelude::*;

#[component]
pub fn SearchInput() -> Element {
    let mut query = use_signal(String::new);
    let mut show_dropdown = use_signal(|| false);
    let mut search_results = use_signal(Vec::<ProfileSearchResult>::new);
    let mut selected_index = use_signal(|| 0usize);
    let mut is_searching = use_signal(|| false);
    let mut relay_search_task = use_signal(|| None::<Task>);
    let mut contact_pubkeys = use_signal(Vec::<PublicKey>::new);
    let navigator = navigator();
    use_effect(move || {
        spawn(async move {
            let contacts = get_contact_pubkeys().await;
            contact_pubkeys.set(contacts);
        });
    });
    let cached_results = use_memo(move || {
        let q = query.read().clone();
        if q.is_empty() {
            return Vec::<ProfileSearchResult>::new();
        }
        let contacts = contact_pubkeys.read().clone();
        search_cached_profiles(&q, 10, &contacts, &[])
    });
    use_effect(move || {
        let results = cached_results.read().clone();
        search_results.set(results);
    });
    let handle_input = move |evt: DioxusEvent<FormData>| {
        let new_value = evt.value().clone();
        query.set(new_value.clone());
        if new_value.is_empty() {
            show_dropdown.set(true);
            search_results.set(Vec::new());
            selected_index.set(0);
            return;
        }
        show_dropdown.set(true);
        selected_index.set(0);
        if new_value.len() >= 2 && cached_results.read().len() < 5 {
            is_searching.set(true);
            if let Some(task) = relay_search_task.read().as_ref() {
                task.cancel();
            }
            let query_snapshot = new_value.clone();
            let new_task = spawn(async move {
                crate::platform::timer::sleep_ms(300).await;
                let query_relays = query_snapshot.len() >= 3;
                match search_profiles(&query_snapshot, 10, query_relays).await {
                    Ok(results) => {
                        if query.read().as_str() == query_snapshot.as_str() {
                            search_results.set(results);
                            is_searching.set(false);
                        }
                    }
                    Err(e) => {
                        log::error!("Profile search failed: {}", e);
                        if query.read().as_str() == query_snapshot.as_str() {
                            is_searching.set(false);
                        }
                    }
                }
            });
            relay_search_task.set(Some(new_task));
        } else {
            is_searching.set(false);
        }
    };
    let handle_keydown = move |evt: DioxusEvent<KeyboardData>| {
        let key = evt.key();
        let q = query.read().clone();
        let has_profiles = !search_results.read().is_empty();
        let is_empty_query = q.is_empty();
        let has_history = !search_history::get_items().is_empty();
        let extra_items = count_extra_items(&q, is_empty_query, has_history);
        let total_items = search_results.read().len() + extra_items;

        if *show_dropdown.read() {
            match key {
                Key::ArrowDown => {
                    evt.prevent_default();
                    let current = *selected_index.read();
                    let max = total_items.saturating_sub(1);
                    if current < max {
                        selected_index.set(current + 1);
                    }
                }
                Key::ArrowUp => {
                    evt.prevent_default();
                    let current = *selected_index.read();
                    if current > 0 {
                        selected_index.set(current - 1);
                    }
                }
                Key::Enter => {
                    evt.prevent_default();
                    let idx = *selected_index.read();
                    if idx < extra_items {
                        handle_extra_item_select(idx, &q, is_empty_query, has_history);
                        query.set(String::new());
                        show_dropdown.set(false);
                    } else if has_profiles {
                        let profile_idx = idx - extra_items;
                        let results = search_results.read();
                        if let Some(profile) = results.get(profile_idx) {
                            let pubkey_hex = profile.pubkey.to_hex();
                            search_history::add_profile(
                                pubkey_hex.clone(),
                                profile.get_display_name(),
                            );
                            navigator.push(Route::AddressViewer {
                                address: crate::utils::nip19_urls::profile_route_id(&pubkey_hex),
                            });
                            query.set(String::new());
                            show_dropdown.set(false);
                        }
                    } else if !is_empty_query {
                        search_history::add_query(q.clone());
                        navigator.push(Route::Search { q });
                        query.set(String::new());
                        show_dropdown.set(false);
                    }
                }
                Key::Escape => {
                    show_dropdown.set(false);
                }
                _ => {}
            }
        } else if key == Key::Enter {
            evt.prevent_default();
            if !is_empty_query {
                search_history::add_query(q.clone());
                navigator.push(Route::Search { q });
                query.set(String::new());
            }
        }
    };
    let close_dropdown = move |_| {
        show_dropdown.set(false);
    };
    rsx! {
        div { class: "relative",
            input {
                r#type: "text",
                placeholder: "Search Nostr...",
                value: "{query}",
                class: "w-full px-4 py-2 pr-10 bg-muted border border-border rounded-full text-sm focus:outline-hidden focus:ring-2 focus:ring-ring",
                oninput: handle_input,
                onkeydown: handle_keydown,
                onblur: close_dropdown,
            }
            div { class: "absolute right-2 top-1/2 -translate-y-1/2 p-1.5", "🔍" }
            if *show_dropdown.read() {
                {
                    render_dropdown(
                        &search_results.read(),
                        *selected_index.read(),
                        *is_searching.read(),
                        query,
                        show_dropdown,
                    )
                }
            }
        }
    }
}

fn count_extra_items(query: &str, is_empty: bool, has_history: bool) -> usize {
    if is_empty {
        if has_history { 1 } else { 0 }
    } else {
        1 + if has_bech32_direct_nav(query) { 1 } else { 0 }
    }
}

fn has_bech32_direct_nav(query: &str) -> bool {
    matches!(
        query_parser::detect_search_type(query),
        query_parser::SearchType::ProfileLookup { .. }
            | query_parser::SearchType::NoteLookup { .. }
            | query_parser::SearchType::AddressLookup { .. }
    )
}

fn handle_extra_item_select(
    _idx: usize,
    query: &str,
    is_empty: bool,
    _has_history: bool,
) {
    if is_empty {
        return;
    }
    let nav = navigator();
    if _idx == 0 {
        search_history::add_query(query.to_string());
        nav.push(Route::Search {
            q: query.to_string(),
        });
    }
}

fn render_dropdown(
    results: &[ProfileSearchResult],
    selected_index: usize,
    is_searching: bool,
    mut query: Signal<String>,
    mut show_dropdown: Signal<bool>,
) -> Element {
    let navigator = navigator();
    let q = query.read().clone();
    let is_empty = q.is_empty();
    let history_items = search_history::get_items();
    let has_history = !history_items.is_empty();

    rsx! {
        div { class: "absolute top-full left-0 right-0 mt-2 bg-white dark:bg-gray-800 shadow-lg rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden z-50 max-h-96",
            div { class: "overflow-y-auto max-h-96",
                if is_empty {
                    if has_history {
                        div { class: "px-4 py-2 flex items-center justify-between",
                            span { class: "text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase", "Recent" }
                            button {
                                class: "text-xs text-gray-400 hover:text-gray-600 dark:hover:text-gray-300",
                                onclick: move |_| {
                                    search_history::clear_all();
                                },
                                "Clear all"
                            }
                        }
                        for (i, item) in history_items.iter().enumerate() {
                            {
                                let item_clone = item.clone();
                                let is_selected = i == selected_index;
                                rsx! {
                                    button {
                                        key: "history-{i}",
                                        class: if is_selected { "w-full px-4 py-2 flex items-center gap-3 bg-blue-50 dark:bg-blue-900 cursor-pointer transition text-left" } else { "w-full px-4 py-2 flex items-center gap-3 hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer transition text-left" },
                                        onclick: move |_| {
                                            match &item_clone {
                                                search_history::RecentSearchItem::Query(q) => {
                                                    navigator.push(Route::Search { q: q.clone() });
                                                }
                                                search_history::RecentSearchItem::Profile { pubkey, .. } => {
                                                    navigator.push(Route::AddressViewer {
                                                        address: crate::utils::nip19_urls::profile_route_id(pubkey),
                                                    });
                                                }
                                            }
                                            query.set(String::new());
                                            show_dropdown.set(false);
                                        },
                                        {match &item {
                                            search_history::RecentSearchItem::Query(q) => rsx! {
                                                span { class: "text-sm text-gray-700 dark:text-gray-300 truncate",
                                                    "🔍 {q}"
                                                }
                                            },
                                            search_history::RecentSearchItem::Profile { display_name, .. } => rsx! {
                                                span { class: "text-sm text-gray-700 dark:text-gray-300 truncate",
                                                    "👤 {display_name}"
                                                }
                                            },
                                        }}
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "px-4 py-3 text-sm text-gray-500 dark:text-gray-400", "Type to search" }
                    }
                } else {
                    if !q.is_empty() {
                        {
                            let is_selected = 0 == selected_index;
                            let q_for_click = q.clone();
                            rsx! {
                                button {
                                    class: if is_selected { "w-full px-4 py-2 flex items-center gap-3 bg-blue-50 dark:bg-blue-900 cursor-pointer transition text-left" } else { "w-full px-4 py-2 flex items-center gap-3 hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer transition text-left" },
                                    onclick: move |_| {
                                        search_history::add_query(q_for_click.clone());
                                        navigator.push(Route::Search { q: q_for_click.clone() });
                                        query.set(String::new());
                                        show_dropdown.set(false);
                                    },
                                    span { class: "text-sm text-gray-600 dark:text-gray-400",
                                        "🔍 Search posts for \"{q}\""
                                    }
                                }
                            }
                        }
                    }
                    if has_bech32_direct_nav(&q) {
                        {
                            let nav_idx = 1;
                            let is_selected = nav_idx == selected_index;
                            let q_for_click = q.clone();
                            rsx! {
                                button {
                                    class: if is_selected { "w-full px-4 py-2 flex items-center gap-3 bg-blue-50 dark:bg-blue-900 cursor-pointer transition text-left" } else { "w-full px-4 py-2 flex items-center gap-3 hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer transition text-left" },
                                    onclick: move |_| {
                                        navigator.push(Route::Search { q: q_for_click.clone() });
                                        query.set(String::new());
                                        show_dropdown.set(false);
                                    },
                                    span { class: "text-sm text-blue-600 dark:text-blue-400",
                                        "🔗 Go to {q}"
                                    }
                                }
                            }
                        }
                    }
                    if is_searching {
                        div { class: "px-4 py-2 text-sm text-gray-500 dark:text-gray-400", "Searching..." }
                    }
                    for (i, profile) in results.iter().enumerate() {
                        {
                            let extra_count = count_extra_items(&q, false, false);
                            let item_idx = i + extra_count;
                            let profile_clone = profile.clone();
                            let is_selected = item_idx == selected_index;
                            rsx! {
                                button {
                                    key: "{profile.pubkey.to_hex()}",
                                    class: if is_selected { "w-full px-4 py-2 flex items-center gap-3 hover:bg-blue-50 dark:hover:bg-blue-900 bg-blue-50 dark:bg-blue-900 cursor-pointer transition" } else { "w-full px-4 py-2 flex items-center gap-3 hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer transition" },
                                    onmousedown: move |evt| {
                                        evt.prevent_default();
                                        let pubkey_hex = profile_clone.pubkey.to_hex();
                                        search_history::add_profile(
                                            pubkey_hex.clone(),
                                            profile_clone.get_display_name(),
                                        );
                                        navigator
                                            .push(Route::AddressViewer {
                                                address: crate::utils::nip19_urls::profile_route_id(&pubkey_hex),
                                            });
                                        query.set(String::new());
                                        show_dropdown.set(false);
                                    },
                                    div { class: "shrink-0",
                                        if let Some(picture) = &profile.picture {
                                            img {
                                                src: "{picture}",
                                                class: "w-8 h-8 rounded-full",
                                                alt: "{profile.get_display_name()}",
                                                loading: "lazy",
                                            }
                                        } else {
                                            div { class: "w-8 h-8 rounded-full bg-gray-300 dark:bg-gray-600 flex items-center justify-center text-xs font-bold",
                                                {profile.get_display_name().chars().next().unwrap_or('?').to_string()}
                                            }
                                        }
                                    }
                                    div { class: "flex-1 text-left min-w-0",
                                        div { class: "font-semibold text-sm text-gray-900 dark:text-gray-100 truncate",
                                            {profile.get_display_name()}
                                        }
                                        if let Some(username) = profile.get_username() {
                                            div { class: "text-xs text-gray-500 dark:text-gray-400 truncate", "@{username}" }
                                        }
                                    }
                                    if profile.is_contact {
                                        div { class: "shrink-0 text-xs px-2 py-1 bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 rounded-full",
                                            "Following"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !is_searching && results.is_empty() && !q.is_empty() {
                        div { class: "px-4 py-3 text-sm text-gray-500 dark:text-gray-400", "No profiles found" }
                    }
                }
            }
        }
    }
}
