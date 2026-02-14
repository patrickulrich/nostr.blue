//! Pack Create/Edit Page
//!
//! Form for creating or editing starter packs (kind 39089).
//! Edit mode: reads `pack_edit_naddr` from localStorage.

use dioxus::prelude::*;
use nostr_sdk::prelude::*;

use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client, packs_store, profiles};
use crate::stores::packs_store::PackMember;
use crate::services::search::profile_search;
use crate::utils::truncate_pubkey;
use crate::utils::validation::is_valid_http_url;

/// Pack create/edit page
#[component]
#[allow(unused_mut, clippy::needless_return)]
pub fn PackNew() -> Element {
    let navigator = use_navigator();

    // Form state
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut image_url = use_signal(String::new);
    let mut members = use_signal(Vec::<PackMember>::new);
    let mut edit_d_tag = use_signal(|| None::<String>);
    let mut is_edit = use_signal(|| false);

    // Search state
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(Vec::<profile_search::ProfileSearchResult>::new);
    let mut searching = use_signal(|| false);
    let mut search_request_id = use_signal(|| 0u32);

    // UI state
    let mut publishing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut loading_edit = use_signal(|| false);
    let mut show_remove_all_confirm = use_signal(|| false);

    let is_authenticated = auth_store::is_authenticated();

    // Check for edit mode via localStorage
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(naddr)) = storage.get_item("pack_edit_naddr") {
                        // Clear it immediately
                        let _ = storage.remove_item("pack_edit_naddr");
                        if !naddr.is_empty() {
                            is_edit.set(true);
                            loading_edit.set(true);
                            spawn(async move {
                                match packs_store::fetch_pack_by_naddr(&naddr).await {
                                    Ok(Some(pack)) => {
                                        name.set(pack.name.clone());
                                        description.set(
                                            pack.description.clone().unwrap_or_default(),
                                        );
                                        image_url.set(
                                            pack.image_url.clone().unwrap_or_default(),
                                        );
                                        members.set(pack.members.clone());
                                        edit_d_tag.set(Some(pack.d_tag.clone()));

                                        // Prefetch member profiles
                                        let pubkeys: Vec<String> =
                                            pack.members.iter().map(|m| m.pubkey.clone()).collect();
                                        profiles::prefetch_profiles(pubkeys).await;
                                    }
                                    Ok(None) => {
                                        error.set(Some("Pack not found for editing".to_string()));
                                    }
                                    Err(e) => {
                                        error.set(Some(format!("Failed to load pack: {}", e)));
                                    }
                                }
                                loading_edit.set(false);
                            });
                        }
                    }
                }
            }
        }
    });

    // Auth guard
    if !is_authenticated {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center p-4",
                div { class: "text-center",
                    div { class: "text-6xl mb-4", "🔒" }
                    h2 { class: "text-xl font-bold mb-2", "Authentication Required" }
                    p { class: "text-muted-foreground mb-4",
                        "You need to sign in to create starter packs"
                    }
                    Link {
                        to: Route::PacksHome {},
                        class: "text-primary hover:underline",
                        "← Back to Starter Packs"
                    }
                }
            }
        };
    }

    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! { ClientInitializing {} };
    }

    if *loading_edit.read() {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center",
                div { class: "text-muted-foreground", "Loading pack data..." }
            }
        };
    }

    let can_publish = !name.read().is_empty()
        && !members.read().is_empty()
        && !*publishing.read();

    rsx! {
        div { class: "min-h-screen",
            // Sticky header
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center justify-between px-4 py-3",
                    div { class: "flex items-center gap-4",
                        button {
                            class: "p-2 hover:bg-accent rounded-lg transition",
                            onclick: move |_| navigator.go_back(),
                            svg {
                                class: "w-5 h-5",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M15 19l-7-7 7-7",
                                }
                            }
                        }
                        h1 { class: "text-xl font-bold",
                            if *is_edit.read() { "Edit Starter Pack" } else { "Create Starter Pack" }
                        }
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50 text-sm font-medium",
                        disabled: !can_publish,
                        onclick: move |_| {
                            publishing.set(true);
                            error.set(None);
                            let n = name.read().clone();
                            let d = if description.read().is_empty() {
                                None
                            } else {
                                Some(description.read().clone())
                            };
                            let img = if image_url.read().is_empty() {
                                None
                            } else {
                                Some(image_url.read().clone()).filter(|u| is_valid_http_url(u))
                            };
                            let m = members.read().clone();
                            let d_tag = edit_d_tag.read().clone();
                            spawn(async move {
                                match packs_store::publish_starter_pack(
                                    &n,
                                    d.as_deref(),
                                    img.as_deref(),
                                    &m,
                                    d_tag.as_deref(),
                                ).await {
                                    Ok(naddr) => {
                                        log::info!("Pack published: {}", naddr);
                                        navigator.push(Route::PackDetail { naddr });
                                    }
                                    Err(e) => {
                                        log::error!("Failed to publish pack: {}", e);
                                        error.set(Some(e));
                                    }
                                }
                                publishing.set(false);
                            });
                        },
                        if *publishing.read() {
                            "Publishing..."
                        } else if *is_edit.read() {
                            "Update Starter Pack"
                        } else {
                            "Publish Starter Pack"
                        }
                    }
                }
            }

            div { class: "p-4 max-w-2xl mx-auto space-y-6",
                // Error display
                if let Some(e) = error.read().as_ref() {
                    div { class: "bg-destructive/10 border border-destructive/20 rounded-xl p-4 text-destructive",
                        "{e}"
                    }
                }

                // Name field
                div {
                    label { class: "block text-sm font-medium mb-2",
                        "Name"
                        span { class: "text-destructive ml-1", "*" }
                    }
                    input {
                        class: "w-full px-4 py-2 bg-muted rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "My Starter Pack",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }

                // Description field
                div {
                    label { class: "block text-sm font-medium mb-2", "Description" }
                    textarea {
                        class: "w-full px-4 py-2 bg-muted rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                        rows: "3",
                        placeholder: "Describe what this pack is about...",
                        value: "{description}",
                        oninput: move |e| description.set(e.value()),
                    }
                }

                // Cover image URL
                div {
                    label { class: "block text-sm font-medium mb-2", "Cover Image URL" }
                    input {
                        class: "w-full px-4 py-2 bg-muted rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "url",
                        placeholder: "https://example.com/cover.jpg",
                        value: "{image_url}",
                        oninput: move |e| image_url.set(e.value()),
                    }
                    // Live preview
                    if !image_url.read().is_empty() && is_valid_http_url(&image_url.read()) {
                        div { class: "mt-2 h-32 bg-muted rounded-lg overflow-hidden",
                            img {
                                src: "{image_url}",
                                alt: "Cover preview",
                                class: "w-full h-full object-cover",
                                onerror: move |_| {},
                            }
                        }
                    }
                }

                // Members section
                div { class: "space-y-4",
                    div { class: "flex items-center justify-between",
                        h3 { class: "font-semibold",
                            "Selected Users ({members.read().len()})"
                            span { class: "text-destructive ml-1", "*" }
                        }
                        if !members.read().is_empty() {
                            button {
                                class: "text-sm text-destructive hover:underline",
                                onclick: move |_| {
                                    if *show_remove_all_confirm.read() {
                                        members.set(Vec::new());
                                        show_remove_all_confirm.set(false);
                                    } else {
                                        show_remove_all_confirm.set(true);
                                    }
                                },
                                if *show_remove_all_confirm.read() {
                                    "Confirm Remove All?"
                                } else {
                                    "Remove All"
                                }
                            }
                        }
                    }

                    // Search input
                    div { class: "relative",
                        input {
                            class: "w-full px-4 py-2 bg-muted rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search by name or paste npub/nprofile...",
                            value: "{search_query}",
                            oninput: move |e| {
                                let val = e.value();
                                search_query.set(val.clone());
                                show_remove_all_confirm.set(false);

                                if val.len() >= 3 {
                                    let current_search_id = search_request_id.peek().wrapping_add(1);
                                    search_request_id.set(current_search_id);
                                    searching.set(true);
                                    spawn(async move {
                                        // Check for npub/nprofile paste (synchronous — no staleness check needed)
                                        if val.starts_with("npub") || val.starts_with("nprofile") {
                                            let parsed = if val.starts_with("nprofile") {
                                                // Parse nprofile first to get pubkey + relay hints
                                                Nip19Profile::from_bech32(&val).ok().map(|profile| {
                                                    let hex = profile.public_key.to_hex();
                                                    let relay_hint = profile.relays.first().map(|r| r.to_string());
                                                    (hex, relay_hint)
                                                })
                                            } else {
                                                // npub
                                                PublicKey::parse(&val).ok().map(|pk| (pk.to_hex(), None))
                                            };
                                            if let Some((hex, relay_hint)) = parsed {
                                                let already_added = members.read().iter().any(|m| m.pubkey == hex);
                                                if !already_added {
                                                    members.write().push(PackMember {
                                                        pubkey: hex.clone(),
                                                        relay_hint,
                                                    });
                                                    profiles::prefetch_profiles(vec![hex]).await;
                                                    search_query.set(String::new());
                                                }
                                                searching.set(false);
                                                return;  // Only return on successful parse
                                            }
                                            // On parse failure: fall through to profile search below
                                        }

                                        // Profile search — check for staleness after await
                                        match profile_search::search_profiles(&val, 10, true).await {
                                            Ok(results) => {
                                                if *search_request_id.peek() == current_search_id {
                                                    search_results.set(results);
                                                }
                                            }
                                            Err(e) => log::warn!("Profile search error: {}", e),
                                        }
                                        if *search_request_id.peek() == current_search_id {
                                            searching.set(false);
                                        }
                                    });
                                } else {
                                    let next_id = search_request_id.peek().wrapping_add(1);
                                    search_request_id.set(next_id);
                                    searching.set(false);
                                    search_results.set(Vec::new());
                                }
                            },
                            onkeypress: move |e: KeyboardEvent| {
                                if e.key() == Key::Enter {
                                    e.prevent_default();
                                }
                            },
                        }

                        // Search results dropdown
                        if !search_results.read().is_empty() && !search_query.read().is_empty() {
                            div { class: "absolute top-full left-0 right-0 mt-1 bg-card border border-border rounded-lg shadow-lg z-10 max-h-64 overflow-y-auto",
                                for result in search_results.read().iter() {
                                    {
                                        let hex = result.pubkey.to_hex();
                                        let already_added = members.read().iter().any(|m| m.pubkey == hex);
                                        let display = result.display_name.clone()
                                            .or(result.name.clone())
                                            .unwrap_or_else(|| truncate_pubkey(&hex));
                                        let picture = result.picture.clone();
                                        let pk_hex = hex.clone();

                                        rsx! {
                                            div {
                                                key: "{hex}",
                                                class: "flex items-center gap-3 px-3 py-2 hover:bg-accent transition",
                                                if let Some(ref pic) = picture.as_ref().filter(|u| is_valid_http_url(u)) {
                                                    img {
                                                        src: "{pic}",
                                                        alt: "{display}",
                                                        class: "w-8 h-8 rounded-full shrink-0",
                                                    }
                                                } else {
                                                    div { class: "w-8 h-8 rounded-full bg-primary/20 flex items-center justify-center shrink-0",
                                                        span { class: "text-sm font-bold text-primary",
                                                            "{display.chars().next().unwrap_or('?').to_uppercase()}"
                                                        }
                                                    }
                                                }
                                                div { class: "flex-1 min-w-0",
                                                    span { class: "font-medium text-sm truncate block", "{display}" }
                                                    span { class: "text-xs text-muted-foreground truncate block",
                                                        "{truncate_pubkey(&pk_hex)}"
                                                    }
                                                }
                                                if already_added {
                                                    span { class: "text-xs text-muted-foreground shrink-0", "Added" }
                                                } else {
                                                    button {
                                                        class: "px-2 py-1 text-xs bg-primary text-primary-foreground rounded hover:bg-primary/90 transition shrink-0",
                                                        onclick: {
                                                            let pk = pk_hex.clone();
                                                            move |_| {
                                                                members.write().push(PackMember {
                                                                    pubkey: pk.clone(),
                                                                    relay_hint: None,
                                                                });
                                                                search_query.set(String::new());
                                                                search_results.set(Vec::new());
                                                            }
                                                        },
                                                        "Add"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if *searching.read() {
                            div { class: "absolute top-full left-0 right-0 mt-1 bg-card border border-border rounded-lg p-3 text-sm text-muted-foreground z-10",
                                "Searching..."
                            }
                        }
                    }

                    // Selected members list
                    if members.read().is_empty() {
                        div { class: "text-center py-8 text-muted-foreground",
                            p { "No members added yet. Search for users above." }
                        }
                    } else {
                        div { class: "space-y-1",
                            for (idx , member) in members.read().iter().enumerate() {
                                SelectedMemberRow {
                                    key: "{member.pubkey}",
                                    pubkey: member.pubkey.clone(),
                                    relay_hint: member.relay_hint.clone(),
                                    index: idx,
                                    total: members.read().len(),
                                    on_remove: move |_| {
                                        members.write().remove(idx);
                                    },
                                    on_move_up: move |_| {
                                        if idx > 0 {
                                            members.write().swap(idx, idx - 1);
                                        }
                                    },
                                    on_move_down: move |_| {
                                        let len = members.read().len();
                                        if idx + 1 < len {
                                            members.write().swap(idx, idx + 1);
                                        }
                                    },
                                }
                            }
                        }
                    }

                    if members.read().is_empty() {
                        p { class: "text-xs text-muted-foreground",
                            "At least 1 member is required"
                        }
                    }
                }
            }
        }
    }
}

/// Selected member row with reorder and remove controls
#[component]
fn SelectedMemberRow(
    pubkey: String,
    relay_hint: Option<String>,
    index: usize,
    total: usize,
    on_remove: EventHandler<MouseEvent>,
    on_move_up: EventHandler<MouseEvent>,
    on_move_down: EventHandler<MouseEvent>,
) -> Element {
    let profile = profiles::get_profile(&pubkey);
    let display_name = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&pubkey));
    let picture = profile.as_ref().and_then(|p| p.picture.clone());

    rsx! {
        div { class: "flex items-center gap-3 px-3 py-2 bg-card border border-border rounded-lg",
            // Reorder buttons
            div { class: "flex flex-col shrink-0",
                button {
                    class: "p-2 hover:bg-accent rounded-lg transition text-muted-foreground disabled:opacity-30",
                    disabled: index == 0,
                    onclick: on_move_up,
                    "▲"
                }
                button {
                    class: "p-2 hover:bg-accent rounded-lg transition text-muted-foreground disabled:opacity-30",
                    disabled: index + 1 >= total,
                    onclick: on_move_down,
                    "▼"
                }
            }

            // Avatar
            if let Some(ref pic) = picture.as_ref().filter(|u| is_valid_http_url(u)) {
                img {
                    src: "{pic}",
                    alt: "{display_name}",
                    class: "w-9 h-9 rounded-full shrink-0",
                }
            } else {
                div { class: "w-9 h-9 rounded-full bg-primary/20 flex items-center justify-center shrink-0",
                    span { class: "text-sm font-bold text-primary",
                        "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                    }
                }
            }

            // Info
            div { class: "flex-1 min-w-0",
                span { class: "font-medium text-sm truncate block", "{display_name}" }
                if let Some(ref hint) = relay_hint {
                    span { class: "text-xs text-muted-foreground truncate block", "{hint}" }
                }
            }

            // Remove button
            button {
                class: "p-2 hover:bg-accent rounded-lg transition text-destructive shrink-0",
                onclick: on_remove,
                svg {
                    class: "w-4 h-4",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        stroke_width: "2",
                        d: "M6 18L18 6M6 6l12 12",
                    }
                }
            }
        }
    }
}
