//! Nostr User Picker Component
//!
//! Reusable autocomplete for selecting Nostr users by name, npub, or hex pubkey.
//! Used by repo settings, zap distribution, and issue assignees.
use crate::hooks::use_profile_typeahead::{
    use_profile_typeahead, TypeaheadOptions, UseProfileTypeahead,
};
use crate::services::search::profile_search::ProfileSearchResult;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus_core::Task;
use nostr_sdk::prelude::PublicKey;
use std::collections::HashSet;

/// Filter out already-selected pubkeys and sort priority pubkeys first.
fn filter_picker_results(
    results: &[ProfileSearchResult],
    selected: &[String],
    priority: &[String],
) -> Vec<ProfileSearchResult> {
    let priority_set: HashSet<&str> = priority.iter().map(|s| s.as_str()).collect();
    let mut filtered: Vec<ProfileSearchResult> = results
        .iter()
        .filter(|r| !selected.contains(&r.pubkey.to_hex()))
        .cloned()
        .collect();
    filtered.sort_by(|a, b| {
        let a_pri = priority_set.contains(a.pubkey.to_hex().as_str());
        let b_pri = priority_set.contains(b.pubkey.to_hex().as_str());
        b_pri.cmp(&a_pri).then_with(|| b.relevance.cmp(&a.relevance))
    });
    filtered
}

/// Effective dropdown rows: the instant npub/hex manual parse when present,
/// otherwise the typeahead results post-filtered.
fn picker_effective_results(
    typeahead: &UseProfileTypeahead,
    manual: Option<ProfileSearchResult>,
    selected: &[String],
    priority: &[String],
) -> Vec<ProfileSearchResult> {
    match manual {
        Some(result) => vec![result],
        None => filter_picker_results(&typeahead.results(), selected, priority),
    }
}

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
    let mut manual_result = use_signal(|| None::<ProfileSearchResult>);
    let mut selected_index = use_signal(|| 0usize);
    let mut show_dropdown = use_signal(|| false);
    let mut blur_hide_task = use_signal(|| None::<Task>);
    let enabled = use_signal(|| true);
    let participants = use_signal(Vec::<PublicKey>::new);
    let typeahead = use_profile_typeahead(
        query,
        enabled,
        participants,
        TypeaheadOptions { limit: 8, min_chars_relay: 3, ..Default::default() },
    );
    let is_searching = typeahead.is_searching();
    let results = picker_effective_results(
        &typeahead,
        manual_result.read().clone(),
        &selected.read(),
        &priority_pubkeys,
    );

    use_effect(use_reactive(&disabled, move |is_disabled| {
        if !is_disabled {
            return;
        }
        if let Some(task) = blur_hide_task.take() {
            task.cancel();
        }
        manual_result.set(None);
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
        manual_result.set(None);
        query.set(val.clone());
        selected_index.set(0);

        if val.trim().is_empty() {
            show_dropdown.set(false);
            return;
        }

        // Try direct npub/hex parse (instant, served from cache)
        if let Ok(pk) = PublicKey::parse(val.trim()) {
            let hex = pk.to_hex();
            if !selected.read().contains(&hex) {
                let profile = PROFILE_CACHE.read().peek(&hex).cloned();
                manual_result.set(Some(ProfileSearchResult {
                    pubkey: pk,
                    name: profile.as_ref().and_then(|p| p.name.clone()),
                    display_name: profile.as_ref().and_then(|p| p.display_name.clone()),
                    picture: profile.as_ref().and_then(|p| p.picture.clone()),
                    nip05: None,
                    is_contact: false,
                    is_thread_participant: false,
                    relevance: 100,
                }));
                show_dropdown.set(true);
            } else {
                // Already selected — nothing to suggest
                show_dropdown.set(false);
            }
            return;
        }

        // Name/NIP-05 search runs through the shared typeahead engine
        // (keyed on `query`).
        show_dropdown.set(true);
    };

    // Handle keyboard navigation
    let typeahead_for_keys = typeahead;
    let priority_for_keys = priority_pubkeys.clone();
    let handle_keydown = move |evt: Event<KeyboardData>| {
        if disabled {
            return;
        }
        if !*show_dropdown.read() {
            return;
        }
        let results = picker_effective_results(
            &typeahead_for_keys,
            manual_result.read().clone(),
            &selected.read(),
            &priority_for_keys,
        );
        match evt.key() {
            Key::ArrowDown => {
                evt.prevent_default();
                let current = *selected_index.read();
                let max = results.len().saturating_sub(1);
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
                if let Some(profile) = results.get(idx) {
                    do_select(profile.pubkey.to_hex());
                    query.set(String::new());
                    manual_result.set(None);
                    show_dropdown.set(false);
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
                            if !query.read().trim().is_empty()
                                && (!results.is_empty() || is_searching)
                            {
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
                    if *show_dropdown.read() && (!results.is_empty() || is_searching) {
                        div { class: "absolute z-50 w-full mt-1 bg-background border border-border rounded-lg shadow-lg max-h-60 overflow-y-auto",
                            if is_searching && results.is_empty() {
                                div { class: "px-3 py-2 text-sm text-muted-foreground", "Searching..." }
                            }
                            for (index, profile) in results.iter().enumerate() {
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
                                                    manual_result.set(None);
                                                    show_dropdown.set(false);
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
                            if !results.is_empty() {
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
fn SelectedUserChip(
    pubkey: String,
    disabled: bool,
    on_remove: EventHandler<MouseEvent>,
) -> Element {
    let profile = PROFILE_CACHE.read().peek(&pubkey).cloned();
    let display = profile
        .as_ref()
        .and_then(|p| p.resolved_name())
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
