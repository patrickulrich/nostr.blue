//! Highlights Feed Page (NIP-84 Kind 9802)
//!
//! Displays a feed of highlights with Following/Global toggle.
use crate::components::{ClientInitializing, HighlightCard, HighlightCardSkeleton};
use crate::hooks::use_infinite_scroll_with_generation;
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip84::{self, Highlight};
use dioxus::prelude::*;
use nostr_sdk::PublicKey;
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
pub fn Highlights() -> Element {
    let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
    let mut highlights = use_signal(Vec::<Highlight>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut request_id = use_signal(|| 0u32);
    let mut fallback_in_progress = use_signal(|| false);
    let mut feed_reset_generation = use_signal(|| 0u64);
    use_effect(move || {
        let _ = refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let is_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !is_initialized {
            return;
        }
        if *fallback_in_progress.peek() {
            fallback_in_progress.set(false);
            return;
        }
        loading.set(true);
        error.set(None);
        // The error/empty branches replace the list (unmounting the sentinel)
        // while `has_more` stays true. Bump the generation so a refresh or
        // feed-type switch re-attaches the observer to the re-mounted sentinel.
        feed_reset_generation += 1;
        oldest_timestamp.set(None);
        has_more.set(true);
        let current_id = *request_id.peek() + 1;
        request_id.set(current_id);
        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_highlights(None).await,
                FeedType::Global => load_global_highlights(None).await.map(|h| (h, false)),
            };
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale highlights result");
                return;
            }
            match result {
                Ok((new_highlights, did_fallback)) => {
                    if did_fallback {
                        fallback_in_progress.set(true);
                        feed_type.set(FeedType::Global);
                    }
                    if let Some(last) = new_highlights.last() {
                        oldest_timestamp.set(Some(last.created_at.saturating_sub(1)));
                    }
                    has_more.set(new_highlights.len() >= 30);
                    highlights.set(new_highlights);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }
        let until = *oldest_timestamp.read();
        let current_feed_type = *feed_type.read();
        loading.set(true);
        let current_id = *request_id.peek() + 1;
        request_id.set(current_id);
        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => match load_following_highlights(until).await {
                    Ok((highlights, did_fallback)) => {
                        if did_fallback {
                            log::info!(
                                    "Pagination fallback detected, returning empty to preserve feed type"
                                );
                            Ok(Vec::new())
                        } else {
                            Ok(highlights)
                        }
                    }
                    Err(e) => Err(e),
                },
                FeedType::Global => load_global_highlights(until).await,
            };
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale load_more highlights result");
                return;
            }
            match result {
                Ok(new_highlights) => {
                    let existing_ids: std::collections::HashSet<_> =
                        highlights.read().iter().map(|h| h.event.id).collect();
                    let unique: Vec<_> = new_highlights
                        .iter()
                        .filter(|h| !existing_ids.contains(&h.event.id))
                        .cloned()
                        .collect();
                    if let Some(last) = new_highlights.last() {
                        oldest_timestamp.set(Some(last.created_at.saturating_sub(1)));
                    }
                    has_more.set(new_highlights.len() >= 30 && !unique.is_empty());
                    highlights.write().extend(unique);
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more highlights: {}", e);
                    loading.set(false);
                    has_more.set(false);
                }
            }
        });
    };
    let sentinel_id =
        use_infinite_scroll_with_generation(load_more, has_more, loading, feed_reset_generation);
    if !client_initialized {
        return rsx! {
            ClientInitializing {}
        };
    }
    rsx! {
        div { class: "flex flex-col min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/95 backdrop-blur-sm border-b border-border",
                div { class: "max-w-[600px] mx-auto px-4 py-3 flex items-center justify-between",
                    h1 { class: "text-xl font-bold", "Highlights" }
                    div { class: "relative",
                        button {
                            class: "px-3 py-1.5 rounded-lg bg-accent hover:bg-accent/80 flex items-center gap-2 text-sm font-medium transition-colors",
                            onclick: move |_| show_dropdown.toggle(),
                            "{feed_type.read().label()}"
                            svg {
                                xmlns: "http://www.w3.org/2000/svg",
                                class: "w-4 h-4",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M19 9l-7 7-7-7",
                                }
                            }
                        }
                        if *show_dropdown.read() {
                            div {
                                class: "absolute right-0 mt-2 w-40 bg-card border border-border rounded-lg shadow-lg z-50",
                                onclick: move |_| show_dropdown.set(false),
                                button {
                                    class: if *feed_type.read() == FeedType::Following { "w-full px-4 py-2 text-left hover:bg-accent rounded-t-lg transition-colors bg-accent/50" } else { "w-full px-4 py-2 text-left hover:bg-accent rounded-t-lg transition-colors" },
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Following);
                                        refresh_trigger.set(refresh_trigger() + 1);
                                    },
                                    "Following"
                                }
                                button {
                                    class: if *feed_type.read() == FeedType::Global { "w-full px-4 py-2 text-left hover:bg-accent rounded-b-lg transition-colors bg-accent/50" } else { "w-full px-4 py-2 text-left hover:bg-accent rounded-b-lg transition-colors" },
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Global);
                                        refresh_trigger.set(refresh_trigger() + 1);
                                    },
                                    "Global"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "flex-1 p-4 max-w-[600px] mx-auto w-full",
                if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-8 text-destructive", "Error loading highlights: {err}" }
                } else if *loading.read() && highlights.read().is_empty() {
                    div { class: "space-y-4",
                        for _ in 0..5 {
                            HighlightCardSkeleton {}
                        }
                    }
                } else if highlights.read().is_empty() {
                    div { class: "text-center py-12 text-muted-foreground",
                        div { class: "text-4xl mb-4", "📝" }
                        h2 { class: "text-lg font-semibold mb-2", "No highlights found" }
                        p { class: "text-sm",
                            if *feed_type.read() == FeedType::Following {
                                "People you follow haven't shared any highlights yet."
                            } else {
                                "No highlights available. Be the first to highlight something!"
                            }
                        }
                    }
                } else {
                    div { class: "space-y-4",
                        for highlight in highlights.read().iter() {
                            HighlightCard {
                                key: "{highlight.event.id}",
                                event: highlight.event.clone(),
                            }
                        }
                        div { id: "{sentinel_id}", class: "h-4" }
                        if *loading.read() && !highlights.read().is_empty() {
                            div { class: "py-4", HighlightCardSkeleton {} }
                        }
                        if !*has_more.read() && !highlights.read().is_empty() {
                            div { class: "text-center py-8 text-muted-foreground text-sm",
                                "You've reached the end"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Load highlights from people the user follows
/// Returns (highlights, did_fallback) where did_fallback is true if we fell back to global
async fn load_following_highlights(until: Option<u64>) -> Result<(Vec<Highlight>, bool), String> {
    let pubkey_hex = match auth_store::get_pubkey() {
        Some(pk) => pk,
        None => {
            log::info!("User not authenticated, falling back to global highlights");
            let highlights = load_global_highlights(until).await?;
            return Ok((highlights, true));
        }
    };
    let contacts = match nostr_client::fetch_contacts(pubkey_hex.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!(
                "Failed to fetch contacts: {}, falling back to global feed",
                e
            );
            let highlights = load_global_highlights(until).await?;
            return Ok((highlights, true));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global highlights");
        let highlights = load_global_highlights(until).await?;
        return Ok((highlights, true));
    }
    log::info!("User follows {} accounts", contacts.len());
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }
    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let highlights = load_global_highlights(until).await?;
        return Ok((highlights, true));
    }
    let highlights = nip84::fetch_highlights_by_authors(authors, 30, until).await?;
    Ok((highlights, false))
}
/// Load global highlights (discovery feed)
async fn load_global_highlights(until: Option<u64>) -> Result<Vec<Highlight>, String> {
    nip84::fetch_global_highlights(30, until).await
}
