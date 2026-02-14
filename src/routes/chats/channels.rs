use crate::components::icons::HashIcon;
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;
use crate::stores::nostr_client::{CLIENT_INITIALIZED, HAS_SIGNER};
use crate::stores::profiles;
use crate::stores::social::channel_store::{
    fetch_channels_page, get_channel_display_info, search_channels, Channel,
};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use std::collections::HashSet;

const PAGE_SIZE: usize = 50;

#[component]
pub fn Chats() -> Element {
    let mut channels = use_signal(Vec::<Channel>::new);
    let mut loading = use_signal(|| true);
    let client_ready = use_memo(move || *CLIENT_INITIALIZED.read());
    let has_signer = use_memo(move || *HAS_SIGNER.read());

    // Pagination state
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut pagination_loading = use_signal(|| false);

    // Search state
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(|| None::<Vec<Channel>>);
    let mut search_loading = use_signal(|| false);
    let mut search_version = use_signal(|| 0u64);
    let mut load_request_id = use_signal(|| 0u64);

    // Initial load
    use_effect(move || {
        if !*client_ready.read() {
            return;
        }
        let current_id = load_request_id.peek().wrapping_add(1);
        load_request_id.set(current_id);
        spawn(async move {
            loading.set(true);
            match fetch_channels_page(PAGE_SIZE, None).await {
                Ok(parsed) => {
                    if *load_request_id.peek() != current_id { return; }
                    if let Some(last) = parsed.last() {
                        oldest_timestamp.set(Some(last.created_at));
                    }
                    channels.set(parsed);
                }
                Err(e) => log::error!("Failed to fetch channels: {}", e),
            }
            if *load_request_id.peek() == current_id {
                loading.set(false);
            }
        });
    });

    // Search effect with debounce
    use_effect(move || {
        let query = search_query.read().clone();
        if query.len() < 2 {
            search_version.with_mut(|v| *v += 1);
            search_results.set(None);
            search_loading.set(false);
            return;
        }
        let version = search_version.with_mut(|v| {
            *v += 1;
            *v
        });
        search_loading.set(true);
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(300).await;
            if *search_version.peek() != version {
                return;
            }
            match search_channels(&query, 50).await {
                Ok(results) => {
                    if *search_version.peek() == version {
                        search_results.set(Some(results));
                        search_loading.set(false);
                    }
                }
                Err(e) => {
                    log::error!("Channel search failed: {}", e);
                    if *search_version.peek() == version {
                        search_loading.set(false);
                    }
                }
            }
        });
    });

    // Infinite scroll
    let load_more = move || {
        if *pagination_loading.peek() || !*has_more.peek() {
            return;
        }
        if search_results.peek().is_some() {
            return;
        }
        if *search_loading.peek() { return; }
        if !search_query.peek().is_empty() { return; }
        pagination_loading.set(true);
        let until = *oldest_timestamp.peek();
        spawn(async move {
            match fetch_channels_page(PAGE_SIZE, until).await {
                Ok(new_channels) => {
                    if new_channels.is_empty() {
                        has_more.set(false);
                    } else {
                        let existing_ids: HashSet<String> =
                            channels.read().iter().map(|c| c.id.clone()).collect();
                        let filtered: Vec<Channel> = new_channels
                            .into_iter()
                            .filter(|c| !existing_ids.contains(&c.id))
                            .collect();
                        if filtered.is_empty() {
                            has_more.set(false);
                        } else {
                            if let Some(last) = filtered.last() {
                                oldest_timestamp.set(Some(last.created_at));
                            }
                            channels.write().extend(filtered);
                        }
                    }
                    pagination_loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more channels: {}", e);
                    pagination_loading.set(false);
                }
            }
        });
    };
    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);

    let is_searching = search_query.read().len() >= 2;
    let display_channels = use_memo(move || {
        if let Some(results) = search_results.read().as_ref() {
            return results.clone();
        }
        let all = channels.read();
        let mut seen = HashSet::new();
        all.iter()
            .filter(|c| seen.insert(c.id.clone()))
            .cloned()
            .collect()
    });

    rsx! {
        div { class: "min-h-screen",
            // Header
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3",
                    div { class: "flex items-center justify-between mb-3",
                        h1 { class: "text-xl font-bold", "Chats" }
                        if *has_signer.read() {
                            Link {
                                to: Route::ChatNew {},
                                class: "px-3 py-1.5 bg-primary text-primary-foreground hover:bg-primary/90 text-sm font-medium rounded-lg transition",
                                "+ New Channel"
                            }
                        }
                    }
                    // Search input
                    div { class: "relative",
                        svg {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "11", cy: "11", r: "8" }
                            line {
                                x1: "21",
                                y1: "21",
                                x2: "16.65",
                                y2: "16.65",
                            }
                        }
                        input {
                            class: "w-full pl-10 pr-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search channels...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value()),
                        }
                        if *search_loading.read() {
                            div { class: "absolute right-3 top-1/2 -translate-y-1/2",
                                span { class: "inline-block w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                            }
                        }
                    }
                }
            }

            // Content
            div { class: "p-4",
                if *loading.read() && channels.read().is_empty() {
                    div { class: "grid gap-4 sm:grid-cols-2 lg:grid-cols-3",
                        for _ in 0..6 {
                            div { class: "bg-card border border-border rounded-lg p-4 animate-pulse",
                                div { class: "flex items-center gap-3 mb-3",
                                    div { class: "w-12 h-12 rounded-full bg-muted" }
                                    div { class: "flex-1",
                                        div { class: "h-4 bg-muted rounded w-2/3 mb-2" }
                                        div { class: "h-3 bg-muted rounded w-1/3" }
                                    }
                                }
                                div { class: "h-3 bg-muted rounded w-full mb-2" }
                                div { class: "h-3 bg-muted rounded w-4/5" }
                            }
                        }
                    }
                } else if display_channels.read().is_empty() {
                    div { class: "flex flex-col items-center justify-center py-20 text-muted-foreground",
                        HashIcon { class: "w-12 h-12 mb-4".to_string() }
                        p { class: "text-lg font-medium mb-2",
                            if is_searching { "No channels match your search" } else { "No channels found" }
                        }
                        p { class: "text-sm",
                            if is_searching { "Try a different search term." } else { "Public chat channels will appear here." }
                        }
                    }
                } else {
                    div { class: "grid gap-4 sm:grid-cols-2 lg:grid-cols-3",
                        for channel in display_channels.read().iter() {
                            ChannelCard { key: "{channel.id}", channel: channel.clone() }
                        }
                    }
                    if !is_searching && *has_more.read() {
                        div { id: "{sentinel_id}", class: "h-4" }
                    }
                    if *pagination_loading.read() {
                        div { class: "flex justify-center py-4",
                            span { class: "inline-block w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ChannelCard(channel: Channel) -> Element {
    let (name, about, picture) = get_channel_display_info(&channel);
    let display_name = name.unwrap_or_else(|| "Unnamed Channel".to_string());
    let creator_pk = channel.pubkey.clone();
    let creator_pk_for_name = creator_pk.clone();

    let creator_meta = use_memo(move || profiles::get_profile(&creator_pk_for_name));
    let creator_name = use_memo(move || {
        if let Some(ref meta) = *creator_meta.read() {
            meta.display_name
                .clone()
                .or_else(|| meta.name.clone())
                .unwrap_or_else(|| truncate_pubkey(&creator_pk))
        } else {
            truncate_pubkey(&creator_pk)
        }
    });

    rsx! {
        Link {
            to: Route::ChatDetail {
                channel_id: channel.id.clone(),
            },
            class: "bg-card border border-border rounded-lg p-4 hover:bg-accent/50 transition block",
            div { class: "flex items-center gap-3 mb-3",
                if let Some(ref pic) = picture {
                    img {
                        src: "{pic}",
                        class: "w-12 h-12 rounded-full object-cover",
                        alt: "{display_name}",
                        loading: "lazy",
                    }
                } else {
                    div { class: "w-12 h-12 rounded-full bg-accent flex items-center justify-center text-accent-foreground font-bold text-lg",
                        HashIcon { class: "w-6 h-6".to_string() }
                    }
                }
                div { class: "flex-1 min-w-0",
                    h3 { class: "font-semibold truncate", "{display_name}" }
                    p { class: "text-xs text-muted-foreground truncate",
                        "by {creator_name}"
                    }
                }
            }
            if let Some(ref desc) = about {
                p { class: "text-sm text-muted-foreground line-clamp-2", "{desc}" }
            }
        }
    }
}
