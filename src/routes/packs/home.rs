//! Starter Packs Home Page
//!
//! Discovery/browse page for Nostr starter packs (kind 39089).
//! Tabs: All Packs, From Following, Packs I'm In, My Packs.

use dioxus::prelude::*;

use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client, packs_store, profiles};
use crate::stores::packs_store::StarterPack;
use crate::utils::time::format_relative_time;
use crate::utils::truncate_pubkey;
use crate::utils::validation::is_valid_http_url;

/// Tab selection
#[derive(Clone, Copy, PartialEq, Debug)]
enum PacksTab {
    All,
    Following,
    ImIn,
    Mine,
}

impl PacksTab {
    fn label(&self) -> &'static str {
        match self {
            PacksTab::All => "All Packs",
            PacksTab::Following => "From Following",
            PacksTab::ImIn => "Packs I'm In",
            PacksTab::Mine => "My Packs",
        }
    }
}

/// Packs home page
#[component]
pub fn PacksHome() -> Element {
    let mut active_tab = use_signal(|| PacksTab::All);
    let mut search_query = use_signal(String::new);
    let mut packs = use_signal(Vec::<StarterPack>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut request_id = use_signal(|| 0u32);

    let is_authenticated = auth_store::is_authenticated();

    // Load packs when tab, client, or auth state changes
    use_effect(move || {
        let tab = *active_tab.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let pubkey = auth_store::AUTH_STATE.read().pubkey.clone();

        if !client_initialized {
            return;
        }
        let current_id = request_id.peek().wrapping_add(1);
        request_id.set(current_id);
        loading.set(true);
        error.set(None);
        spawn(async move {
            let result = match tab {
                PacksTab::All => packs_store::fetch_starter_packs(50).await,
                PacksTab::Following => {
                    if let Some(pk) = &pubkey {
                        match nostr_client::fetch_contacts(pk.clone()).await {
                            Ok(contacts) => {
                                packs_store::fetch_packs_from_following(&contacts, 50).await
                            }
                            Err(e) => Err(e),
                        }
                    } else {
                        Ok(Vec::new())
                    }
                }
                PacksTab::ImIn => {
                    if let Some(pk) = &pubkey {
                        packs_store::fetch_packs_containing_pubkey(pk, 50).await
                    } else {
                        Ok(Vec::new())
                    }
                }
                PacksTab::Mine => {
                    if let Some(pk) = &pubkey {
                        packs_store::fetch_packs_by_author(pk, 50).await
                    } else {
                        Ok(Vec::new())
                    }
                }
            };

            if *request_id.peek() != current_id {
                log::debug!("Discarding stale packs request");
                return;
            }

            match result {
                Ok(fetched) => {
                    // Prefetch author + first 5 member profiles per pack (deduped)
                    let mut pubkeys_to_prefetch = std::collections::HashSet::new();
                    for pack in &fetched {
                        pubkeys_to_prefetch.insert(pack.author_pubkey.clone());
                        for member in pack.members.iter().take(5) {
                            pubkeys_to_prefetch.insert(member.pubkey.clone());
                        }
                    }
                    let pubkeys_vec: Vec<String> = pubkeys_to_prefetch.into_iter().collect();
                    packs.set(fetched);
                    spawn(async move {
                        profiles::prefetch_profiles(pubkeys_vec).await;
                    });
                }
                Err(e) => {
                    log::warn!("Failed to fetch packs: {}", e);
                    error.set(Some(e));
                    packs.set(Vec::new());
                }
            }
            loading.set(false);
        });
    });

    let filter_packs = |items: &[StarterPack], query: &str| -> Vec<StarterPack> {
        if query.is_empty() {
            return items.to_vec();
        }
        let q = query.to_lowercase();
        items
            .iter()
            .filter(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .cloned()
            .collect()
    };

    if !*nostr_client::CLIENT_INITIALIZED.read() {
        return rsx! { ClientInitializing {} };
    }

    rsx! {
        div { class: "min-h-screen",
            // Sticky header
            div { class: "sticky top-0 bg-background/95 backdrop-blur z-20 border-b border-border",
                div { class: "flex items-center justify-between px-4 py-3",
                    h1 { class: "text-xl font-bold", "Packs" }
                    if is_authenticated {
                        Link {
                            to: Route::PackNew {},
                            class: "flex items-center gap-2 px-3 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm font-medium",
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M12 4v16m8-8H4",
                                }
                            }
                            "Create"
                        }
                    }
                }
                // Search
                div { class: "px-4 pb-3",
                    input {
                        class: "w-full px-4 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "Search packs...",
                        value: "{search_query}",
                        oninput: move |e| search_query.set(e.value()),
                    }
                }
                // Tab pills
                div { class: "flex gap-2 px-4 pb-3 overflow-x-auto",
                    TabButton {
                        label: PacksTab::All.label(),
                        active: *active_tab.read() == PacksTab::All,
                        onclick: move |_| active_tab.set(PacksTab::All),
                    }
                    if is_authenticated {
                        TabButton {
                            label: PacksTab::Following.label(),
                            active: *active_tab.read() == PacksTab::Following,
                            onclick: move |_| active_tab.set(PacksTab::Following),
                        }
                        TabButton {
                            label: PacksTab::ImIn.label(),
                            active: *active_tab.read() == PacksTab::ImIn,
                            onclick: move |_| active_tab.set(PacksTab::ImIn),
                        }
                        TabButton {
                            label: PacksTab::Mine.label(),
                            active: *active_tab.read() == PacksTab::Mine,
                            onclick: move |_| active_tab.set(PacksTab::Mine),
                        }
                    }
                }
            }

            // Content
            div { class: "p-4",
                if *loading.read() {
                    div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                        for _ in 0..8 {
                            PackCardSkeleton {}
                        }
                    }
                } else if let Some(e) = error.read().as_ref() {
                    EmptyState { message: format!("Error loading packs: {}", e) }
                } else {
                    {
                        let filtered = filter_packs(&packs.read(), &search_query.read());
                        if filtered.is_empty() {
                            let msg = match *active_tab.read() {
                                PacksTab::All => {
                                    if search_query.read().is_empty() {
                                        "No starter packs found"
                                    } else {
                                        "No packs match your search"
                                    }
                                }
                                PacksTab::Following => "No packs from people you follow",
                                PacksTab::ImIn => "You're not in any packs yet",
                                PacksTab::Mine => "You haven't created any packs",
                            };
                            rsx! { EmptyState { message: msg.to_string() } }
                        } else {
                            rsx! {
                                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4",
                                    for pack in filtered {
                                        PackCard { key: "{pack.naddr}", pack }
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

/// Tab button component
#[component]
fn TabButton(
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: if active {
                "px-4 py-2 rounded-full bg-primary text-primary-foreground font-medium text-sm whitespace-nowrap"
            } else {
                "px-4 py-2 rounded-full hover:bg-accent font-medium text-sm whitespace-nowrap"
            },
            onclick,
            "{label}"
        }
    }
}

/// Pack card component
#[component]
fn PackCard(pack: StarterPack) -> Element {
    let naddr = pack.naddr.clone();
    let author_profile = profiles::get_profile(&pack.author_pubkey);
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&pack.author_pubkey));
    let author_picture = author_profile.as_ref().and_then(|p| p.picture.clone());
    let member_count = pack.members.len();
    let created_ts = nostr_sdk::Timestamp::from(pack.created_at);

    rsx! {
        Link {
            to: Route::PackDetail { naddr },
            class: "group block bg-card border border-border rounded-lg overflow-hidden hover:border-primary/50 transition",

            // Cover image
            div { class: "h-36 bg-muted overflow-hidden",
                if let Some(ref img) = pack.image_url.as_ref().filter(|u| is_valid_http_url(u)) {
                    img {
                        src: "{img}",
                        alt: "{pack.name}",
                        class: "w-full h-full object-cover group-hover:scale-105 transition",
                    }
                } else {
                    div { class: "w-full h-full flex items-center justify-center",
                        span { class: "text-4xl", "📦" }
                    }
                }
            }

            div { class: "p-4 space-y-3",
                // Pack name
                h3 { class: "font-semibold truncate", "{pack.name}" }

                // Author
                div { class: "flex items-center gap-2",
                    if let Some(ref pic) = author_picture.as_ref().filter(|u| is_valid_http_url(u)) {
                        img {
                            src: "{pic}",
                            alt: "{author_name}",
                            class: "w-5 h-5 rounded-full",
                        }
                    } else {
                        div { class: "w-5 h-5 rounded-full bg-primary/20 flex items-center justify-center",
                            span { class: "text-xs font-bold text-primary",
                                "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                            }
                        }
                    }
                    span { class: "text-sm text-muted-foreground truncate", "{author_name}" }
                }

                // Member avatars preview
                if !pack.members.is_empty() {
                    div { class: "flex items-center",
                        div { class: "flex -space-x-2",
                            for member in pack.members.iter().take(5) {
                                MemberAvatar { pubkey: member.pubkey.clone() }
                            }
                        }
                        if member_count > 5 {
                            span { class: "ml-2 text-xs text-muted-foreground",
                                "+{member_count - 5}"
                            }
                        }
                    }
                }

                // Footer
                div { class: "text-xs text-muted-foreground",
                    "{member_count} people · {format_relative_time(created_ts)}"
                }
            }
        }
    }
}

/// Small member avatar (for card preview)
#[component]
fn MemberAvatar(pubkey: String) -> Element {
    let profile = profiles::get_profile(&pubkey);
    let picture = profile.as_ref().and_then(|p| p.picture.clone()).filter(|u| is_valid_http_url(u));
    let name = profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_default();

    rsx! {
        if let Some(pic) = picture {
            img {
                src: "{pic}",
                alt: "{name}",
                class: "w-7 h-7 rounded-full border-2 border-card",
            }
        } else {
            div { class: "w-7 h-7 rounded-full border-2 border-card bg-muted flex items-center justify-center",
                span { class: "text-xs text-muted-foreground",
                    "{name.chars().next().unwrap_or('?').to_uppercase()}"
                }
            }
        }
    }
}

/// Pack card skeleton
#[component]
fn PackCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card border border-border rounded-lg overflow-hidden animate-pulse",
            div { class: "h-36 bg-muted" }
            div { class: "p-4 space-y-3",
                div { class: "h-5 bg-muted rounded w-3/4" }
                div { class: "flex items-center gap-2",
                    div { class: "w-5 h-5 rounded-full bg-muted" }
                    div { class: "h-4 bg-muted rounded w-1/2" }
                }
                div { class: "flex -space-x-2",
                    for _ in 0..4 {
                        div { class: "w-7 h-7 rounded-full bg-muted border-2 border-card" }
                    }
                }
                div { class: "h-3 bg-muted rounded w-1/3" }
            }
        }
    }
}

/// Empty state
#[component]
fn EmptyState(message: String) -> Element {
    rsx! {
        div { class: "flex flex-col items-center justify-center py-16 text-center",
            div { class: "text-6xl mb-4", "📦" }
            p { class: "text-muted-foreground", "{message}" }
        }
    }
}
