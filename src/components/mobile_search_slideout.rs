use dioxus::prelude::Event as DioxusEvent;
use dioxus::prelude::*;
use dioxus_core::Task;
use nostr_sdk::prelude::*;

use crate::components::icons::{SearchIcon, XIcon};
use crate::routes::Route;
use crate::services::profile_search::{
    get_contact_pubkeys, search_cached_profiles, search_profiles, ProfileSearchResult,
};
use crate::services::search::query_parser;
use crate::stores::ui::search_history;

#[component]
pub fn MobileSearchSlideout(show: bool, on_close: EventHandler<()>) -> Element {
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

    use_effect(use_reactive(&show, move |is_open| {
        if !is_open {
            query.set(String::new());
            show_dropdown.set(false);
            search_results.set(Vec::new());
            selected_index.set(0);
            is_searching.set(false);
            if let Some(task) = relay_search_task.take() {
                task.cancel();
            }
        }
    }));

    let handle_input = move |evt: DioxusEvent<FormData>| {
        let new_value = evt.value().clone();
        query.set(new_value.clone());

        if new_value.is_empty() {
            show_dropdown.set(true);
            search_results.set(Vec::new());
            selected_index.set(0);
            if let Some(task) = relay_search_task.take() {
                task.cancel();
            }
            is_searching.set(false);
            return;
        }

        show_dropdown.set(true);
        selected_index.set(0);

        let contacts = contact_pubkeys.read().clone();
        let cached_results = search_cached_profiles(&new_value, 10, &contacts, &[]);
        search_results.set(cached_results.clone());

        if new_value.len() >= 2 && cached_results.len() < 5 {
            is_searching.set(true);
            if let Some(task) = relay_search_task.take() {
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
            if let Some(task) = relay_search_task.take() {
                task.cancel();
            }
            is_searching.set(false);
        }
    };

    let handle_keydown = {
        move |evt: DioxusEvent<KeyboardData>| {
            let key = evt.key();

            if key == Key::Escape {
                on_close.call(());
                return;
            }

            let q = query.read().clone();
            let has_profiles = !search_results.read().is_empty();
            let is_empty_query = q.is_empty();
            let history_items = search_history::get_items();
            let has_history = !history_items.is_empty();
            let extra_items = if is_empty_query {
                if has_history { 1 } else { 0 }
            } else {
                1 + if matches!(
                    query_parser::detect_search_type(&q),
                    query_parser::SearchType::ProfileLookup { .. }
                        | query_parser::SearchType::NoteLookup { .. }
                        | query_parser::SearchType::AddressLookup { .. }
                ) {
                    1
                } else {
                    0
                }
            };
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
                            if !is_empty_query {
                                search_history::add_query(q.clone());
                                navigator.push(Route::Search { q });
                                on_close.call(());
                            }
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
                                on_close.call(());
                            }
                        } else if !is_empty_query {
                            search_history::add_query(q.clone());
                            navigator.push(Route::Search { q });
                            on_close.call(());
                        }
                    }
                    _ => {}
                }
            } else if key == Key::Enter {
                evt.prevent_default();
                if !q.is_empty() {
                    search_history::add_query(q.clone());
                    navigator.push(Route::Search { q });
                    on_close.call(());
                }
            }
        }
    };

    if !show {
        return rsx! {};
    }

    rsx! {
        div {
            class: "fixed inset-0 z-40 bg-black/50 backdrop-blur-sm lg:hidden",
            onclick: move |_| on_close.call(()),
        }

        div {
            class: "fixed top-0 left-0 right-0 z-50 bg-background shadow-2xl lg:hidden pt-safe-top
                    transform transition-transform duration-300 ease-out",
            role: "dialog",
            "aria-modal": "true",
            "aria-label": "Search",
            onclick: move |e| e.stop_propagation(),
            onkeydown: handle_keydown,

            div { class: "flex items-center gap-3 p-4 border-b border-border",
                div { class: "flex-1 relative",
                    input {
                        r#type: "text",
                        placeholder: "Search Nostr...",
                        value: "{query}",
                        class: "w-full px-4 py-3 pl-10 bg-muted border border-border rounded-full text-base focus:outline-hidden focus:ring-2 focus:ring-ring",
                        autofocus: true,
                        oninput: handle_input,
                        onkeydown: handle_keydown,
                    }
                    div { class: "absolute left-3 top-1/2 -translate-y-1/2",
                        SearchIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                    }
                }

                button {
                    class: "p-2 hover:bg-muted rounded-full transition shrink-0",
                    onclick: move |_| on_close.call(()),
                    "aria-label": "Close search",
                    XIcon { class: "w-6 h-6".to_string() }
                }
            }

            if *show_dropdown.read() {
                div { class: "max-h-[60vh] overflow-y-auto",
                    {render_mobile_results(
                        &search_results.read(),
                        *selected_index.read(),
                        *is_searching.read(),
                        query,
                        on_close,
                    )}
                }
            }
        }
    }
}

fn render_mobile_results(
    results: &[ProfileSearchResult],
    selected_index: usize,
    is_searching: bool,
    mut query: Signal<String>,
    on_close: EventHandler<()>,
) -> Element {
    let navigator = navigator();
    let q = query.read().clone();
    let is_empty = q.is_empty();
    let history_items = search_history::get_items();
    let has_history = !history_items.is_empty();

    rsx! {
        if is_empty {
            if has_history {
                div { class: "px-4 py-2 flex items-center justify-between",
                    span { class: "text-xs font-semibold text-muted-foreground uppercase", "Recent" }
                    button {
                        class: "text-xs text-muted-foreground hover:text-foreground",
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
                                class: if is_selected { "w-full px-4 py-3 flex items-center gap-3 bg-accent cursor-pointer transition text-left" } else { "w-full px-4 py-3 flex items-center gap-3 hover:bg-muted cursor-pointer transition text-left" },
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
                                    on_close.call(());
                                },
                                {match &item {
                                    search_history::RecentSearchItem::Query(q) => rsx! {
                                        span { class: "text-sm text-foreground truncate", "🔍 {q}" }
                                    },
                                    search_history::RecentSearchItem::Profile { display_name, .. } => rsx! {
                                        span { class: "text-sm text-foreground truncate", "👤 {display_name}" }
                                    },
                                }}
                            }
                        }
                    }
                }
            } else {
                div { class: "px-4 py-4 text-sm text-muted-foreground text-center",
                    "Type to search"
                }
            }
        } else {
            if !q.is_empty() {
                {
                    let is_selected = 0 == selected_index;
                    let q_for_click = q.clone();
                    rsx! {
                        button {
                            class: if is_selected { "w-full px-4 py-3 flex items-center gap-3 bg-accent cursor-pointer transition text-left" } else { "w-full px-4 py-3 flex items-center gap-3 hover:bg-muted cursor-pointer transition text-left" },
                            onclick: move |_| {
                                search_history::add_query(q_for_click.clone());
                                navigator.push(Route::Search { q: q_for_click.clone() });
                                query.set(String::new());
                                on_close.call(());
                            },
                            span { class: "text-sm text-muted-foreground",
                                "🔍 Search posts for \"{q}\""
                            }
                        }
                    }
                }
            }
            if is_searching {
                div { class: "px-4 py-4 text-sm text-muted-foreground text-center",
                    "Searching..."
                }
            }
            for (index, profile) in results.iter().enumerate() {
                {
                    let profile_clone = profile.clone();
                    let is_selected = index + 1 == selected_index;

                    rsx! {
                        button {
                            key: "{profile.pubkey.to_hex()}",
                            class: if is_selected {
                                "w-full px-4 py-3 flex items-center gap-3 bg-accent cursor-pointer transition"
                            } else {
                                "w-full px-4 py-3 flex items-center gap-3 hover:bg-muted cursor-pointer transition"
                            },
                            onmousedown: move |evt| {
                                evt.prevent_default();
                                let pubkey_hex = profile_clone.pubkey.to_hex();
                                search_history::add_profile(
                                    pubkey_hex.clone(),
                                    profile_clone.get_display_name(),
                                );
                                navigator.push(Route::AddressViewer { address: crate::utils::nip19_urls::profile_route_id(&pubkey_hex) });
                                query.set(String::new());
                                on_close.call(());
                            },

                            div { class: "shrink-0",
                                if let Some(picture) = &profile.picture {
                                    img {
                                        src: "{picture}",
                                        class: "w-10 h-10 rounded-full object-cover",
                                        alt: "{profile.get_display_name()}",
                                        loading: "lazy",
                                    }
                                } else {
                                    div { class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center text-sm font-bold",
                                        {profile.get_display_name().chars().next().unwrap_or('?').to_string()}
                                    }
                                }
                            }

                            div { class: "flex-1 text-left min-w-0",
                                div { class: "font-semibold text-sm truncate",
                                    {profile.get_display_name()}
                                }
                                if let Some(username) = profile.get_username() {
                                    div { class: "text-xs text-muted-foreground truncate",
                                        "@{username}"
                                    }
                                }
                            }

                            if profile.is_contact {
                                div { class: "shrink-0 text-xs px-2 py-1 bg-primary/10 text-primary rounded-full",
                                    "Following"
                                }
                            }
                        }
                    }
                }
            }
            if !is_searching && results.is_empty() && !q.is_empty() {
                div { class: "px-4 py-4 text-sm text-muted-foreground text-center",
                    "No profiles found. Press Enter to search all content."
                }
            }
        }
    }
}
