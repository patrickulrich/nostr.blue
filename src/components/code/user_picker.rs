//! Nostr User Picker Component
//!
//! Reusable autocomplete for selecting Nostr users by name, npub, or hex pubkey.
//! Used by repo settings, zap distribution, and issue assignees.
use crate::services::search::profile_search::{
    search_cached_profiles, search_profiles, ProfileSearchResult,
};
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus_core::Task;
use nostr_sdk::prelude::PublicKey;
use std::collections::HashSet;

/// Reusable Nostr user picker with autocomplete
#[component]
pub fn NostrUserPicker(
    /// Currently selected pubkeys (hex strings)
    selected: Signal<Vec<String>>,
    /// Input placeholder text
    #[props(default = "Search users or paste npub...".to_string())]
    placeholder: String,
    /// Maximum number of selections (0 = unlimited)
    #[props(default = 0)]
    max_selections: usize,
    /// Priority pubkeys shown first (e.g., repo contributors)
    #[props(default = Vec::new())]
    priority_pubkeys: Vec<String>,
    /// Disable all interactions
    #[props(default = false)]
    disabled: bool,
    /// Callback when selection changes
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut results = use_signal(Vec::<ProfileSearchResult>::new);
    let mut selected_index = use_signal(|| 0usize);
    let mut show_dropdown = use_signal(|| false);
    let mut is_searching = use_signal(|| false);
    let mut search_task = use_signal(|| None::<Task>);
    let mut blur_hide_task = use_signal(|| None::<Task>);

    use_effect(use_reactive(&disabled, move |is_disabled| {
        if !is_disabled {
            return;
        }
        if let Some(task) = search_task.take() {
            task.cancel();
        }
        if let Some(task) = blur_hide_task.take() {
            task.cancel();
        }
        results.set(Vec::new());
        is_searching.set(false);
        show_dropdown.set(false);
    }));

    let at_max = max_selections > 0 && selected.read().len() >= max_selections;

    let mut do_select = move |pubkey: String| {
        if disabled {
            return;
        }
        if max_selections > 0 && selected.read().len() >= max_selections {
            return;
        }
        let mut current = selected.read().clone();
        if !current.contains(&pubkey) {
            current.push(pubkey);
            selected.set(current.clone());
            on_change.call(current);
        }
    };

    // Handle search input
    let handle_input = move |evt: Event<FormData>| {
        if disabled {
            return;
        }
        if let Some(task) = blur_hide_task.take() {
            task.cancel();
        }
        let val = evt.value();
        query.set(val.clone());
        selected_index.set(0);

        if val.trim().is_empty() {
            if let Some(task) = search_task.take() {
                task.cancel();
            }
            is_searching.set(false);
            show_dropdown.set(false);
            results.set(Vec::new());
            return;
        }

        // Try direct npub/hex parse
        if let Ok(pk) = PublicKey::parse(val.trim()) {
            let hex = pk.to_hex();
            if !selected.read().contains(&hex) {
                // Cancel any in-flight search task
                if let Some(task) = search_task.take() {
                    task.cancel();
                }
                is_searching.set(false);
                let profile = PROFILE_CACHE.read().peek(&hex).cloned();
                let result = ProfileSearchResult {
                    pubkey: pk,
                    name: profile.as_ref().and_then(|p| p.name.clone()),
                    display_name: profile.as_ref().and_then(|p| p.display_name.clone()),
                    picture: profile.as_ref().and_then(|p| p.picture.clone()),
                    nip05: None,
                    is_contact: false,
                    is_thread_participant: false,
                    relevance: 100,
                };
                results.set(vec![result]);
                show_dropdown.set(true);
            } else {
                // Already selected — clear stale search state
                if let Some(task) = search_task.take() {
                    task.cancel();
                }
                is_searching.set(false);
                show_dropdown.set(false);
                results.set(Vec::new());
            }
            return;
        }

        // Search cached profiles
        let cached = search_cached_profiles(&val, 8, &[], &[]);
        let selected_snapshot = selected.read().clone();
        let mut filtered: Vec<ProfileSearchResult> = cached
            .into_iter()
            .filter(|r| !selected_snapshot.contains(&r.pubkey.to_hex()))
            .collect();
        // Sort priority pubkeys first
        let priority_set: HashSet<&str> = priority_pubkeys.iter().map(|s| s.as_str()).collect();
        filtered.sort_by(|a, b| {
            let a_pri = priority_set.contains(a.pubkey.to_hex().as_str());
            let b_pri = priority_set.contains(b.pubkey.to_hex().as_str());
            b_pri
                .cmp(&a_pri)
                .then_with(|| b.relevance.cmp(&a.relevance))
        });
        results.set(filtered);
        show_dropdown.set(true);

        // Use char count for threshold checks (handles multi-byte input)
        let char_count = val.chars().count();

        // Cancel search and clear state when query drops below threshold
        if char_count < 3 {
            if let Some(task) = search_task.read().as_ref() {
                task.cancel();
            }
            is_searching.set(false);
        }

        // Debounced relay search for longer queries
        if char_count >= 3 {
            if let Some(task) = search_task.read().as_ref() {
                task.cancel();
            }
            let query_snapshot = val.clone();
            let priority_snapshot = priority_pubkeys.clone();
            is_searching.set(true);
            let new_task = spawn(async move {
                crate::platform::timer::sleep_ms(300).await;
                match search_profiles(&query_snapshot, 8, true).await {
                    Ok(relay_results) => {
                        if *query.peek() == query_snapshot {
                            let selected_snapshot = selected.read().clone();
                            let mut filtered: Vec<ProfileSearchResult> = relay_results
                                .into_iter()
                                .filter(|r| !selected_snapshot.contains(&r.pubkey.to_hex()))
                                .collect();
                            let priority_set: HashSet<&str> =
                                priority_snapshot.iter().map(|s| s.as_str()).collect();
                            filtered.sort_by(|a, b| {
                                let a_pri = priority_set.contains(a.pubkey.to_hex().as_str());
                                let b_pri = priority_set.contains(b.pubkey.to_hex().as_str());
                                b_pri
                                    .cmp(&a_pri)
                                    .then_with(|| b.relevance.cmp(&a.relevance))
                            });
                            results.set(filtered);
                            selected_index.set(0);
                            is_searching.set(false);
                        }
                    }
                    Err(_) => {
                        if *query.peek() == query_snapshot {
                            is_searching.set(false);
                        }
                    }
                }
            });
            search_task.set(Some(new_task));
        }
    };

    // Handle keyboard navigation
    let handle_keydown = move |evt: Event<KeyboardData>| {
        if disabled {
            return;
        }
        if !*show_dropdown.read() {
            return;
        }
        match evt.key() {
            Key::ArrowDown => {
                evt.prevent_default();
                let current = *selected_index.read();
                let max = results.read().len().saturating_sub(1);
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
                let results_snapshot = results.read().clone();
                if let Some(profile) = results_snapshot.get(idx) {
                    do_select(profile.pubkey.to_hex());
                    query.set(String::new());
                    show_dropdown.set(false);
                    results.set(Vec::new());
                }
            }
            Key::Escape => {
                show_dropdown.set(false);
            }
            _ => {}
        }
    };

    rsx! {
        div { class: "space-y-2",
            // Selected chips
            if !selected.read().is_empty() {
                div { class: "flex flex-wrap gap-2",
                    for pubkey in selected.read().iter() {
                        SelectedUserChip {
                            key: "{pubkey}",
                            pubkey: pubkey.clone(),
                            disabled: disabled,
                            on_remove: {
                                let pubkey = pubkey.clone();
                                move |_| {
                                    if disabled {
                                        return;
                                    }
                                    let mut current = selected.read().clone();
                                    current.retain(|p| p != &pubkey);
                                    selected.set(current.clone());
                                    on_change.call(current);
                                }
                            },
                        }
                    }
                }
            }
            // Input
            if !at_max {
                div { class: "relative",
                    input {
                        class: if disabled {
                            "w-full px-3 py-2 bg-muted rounded-lg text-sm opacity-50 cursor-not-allowed"
                        } else {
                            "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary"
                        },
                        r#type: "text",
                        placeholder: "{placeholder}",
                        value: "{query}",
                        disabled: disabled,
                        oninput: handle_input,
                        onkeydown: handle_keydown,
                        onfocus: move |_| {
                            if disabled {
                                return;
                            }
                            if let Some(task) = blur_hide_task.take() {
                                task.cancel();
                            }
                            if !query.read().trim().is_empty() && (!results.read().is_empty() || *is_searching.read()) {
                                show_dropdown.set(true);
                            }
                        },
                        onfocusout: move |_| {
                            if disabled {
                                return;
                            }
                            if let Some(task) = blur_hide_task.take() {
                                task.cancel();
                            }
                            let task = spawn(async move {
                                crate::platform::timer::sleep_ms(200).await;
                                show_dropdown.set(false);
                                blur_hide_task.set(None);
                            });
                            blur_hide_task.set(Some(task));
                        },
                    }
                    // Dropdown
                    if *show_dropdown.read() && (!results.read().is_empty() || *is_searching.read()) {
                        div { class: "absolute z-50 w-full mt-1 bg-background border border-border rounded-lg shadow-lg max-h-60 overflow-y-auto",
                            if *is_searching.read() && results.read().is_empty() {
                                div { class: "px-3 py-2 text-sm text-muted-foreground", "Searching..." }
                            }
                            for (index, profile) in results.read().iter().enumerate() {
                                {
                                    let is_sel = index == *selected_index.read();
                                    let hex = profile.pubkey.to_hex();
                                    let display = profile.get_display_name();
                                    let username = profile.get_username();
                                    let picture = profile.picture.clone();
                                    rsx! {
                                        button {
                                            key: "{hex}",
                                            r#type: "button",
                                            class: if is_sel {
                                                "w-full flex items-center gap-3 px-3 py-2 text-left hover:bg-accent bg-accent transition"
                                            } else {
                                                "w-full flex items-center gap-3 px-3 py-2 text-left hover:bg-accent transition"
                                            },
                                            onmousedown: {
                                                let hex = hex.clone();
                                                move |evt: MouseEvent| {
                                                    if disabled {
                                                        return;
                                                    }
                                                    evt.prevent_default();
                                                    do_select(hex.clone());
                                                    query.set(String::new());
                                                    show_dropdown.set(false);
                                                    results.set(Vec::new());
                                                }
                                            },
                                            if let Some(ref pic) = picture {
                                                img {
                                                    src: "{pic}",
                                                    class: "w-8 h-8 rounded-full shrink-0",
                                                    alt: "{display}",
                                                    loading: "lazy",
                                                }
                                            } else {
                                                div { class: "w-8 h-8 rounded-full bg-muted flex items-center justify-center text-xs font-bold shrink-0",
                                                    {display.chars().next().unwrap_or('?').to_string()}
                                                }
                                            }
                                            div { class: "flex-1 min-w-0",
                                                div { class: "text-sm font-medium truncate", "{display}" }
                                                if let Some(ref uname) = username {
                                                    div { class: "text-xs text-muted-foreground truncate", "@{uname}" }
                                                }
                                            }
                                            div { class: "text-xs text-muted-foreground font-mono",
                                                "{truncate_pubkey(&hex)}"
                                            }
                                        }
                                    }
                                }
                            }
                            if !results.read().is_empty() {
                                div { class: "px-3 py-1.5 text-xs text-muted-foreground border-t border-border",
                                    "↑↓ navigate · Enter select · Esc close"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Chip displaying a selected user with remove button
#[component]
fn SelectedUserChip(pubkey: String, disabled: bool, on_remove: EventHandler<MouseEvent>) -> Element {
    let profile = PROFILE_CACHE.read().peek(&pubkey).cloned();
    let display = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&pubkey));
    let picture = profile.as_ref().and_then(|p| p.picture.clone());

    rsx! {
        div { class: "flex items-center gap-1.5 px-2 py-1 bg-muted rounded-full text-sm",
            if let Some(ref pic) = picture {
                img {
                    src: "{pic}",
                    class: "w-5 h-5 rounded-full",
                    alt: "{display}",
                    loading: "lazy",
                }
            }
            span { "{display}" }
            button {
                r#type: "button",
                class: if disabled {
                    "ml-0.5 p-0.5 rounded-full transition text-muted-foreground opacity-50 cursor-not-allowed"
                } else {
                    "ml-0.5 p-0.5 hover:bg-accent rounded-full transition text-muted-foreground hover:text-foreground"
                },
                aria_label: "Remove {display}",
                disabled: disabled,
                onclick: move |evt| on_remove.call(evt),
                svg {
                    class: "w-3 h-3",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    stroke_linecap: "round",
                    stroke_linejoin: "round",
                    line { x1: "18", y1: "6", x2: "6", y2: "18" }
                    line { x1: "6", y1: "6", x2: "18", y2: "18" }
                }
            }
        }
    }
}
