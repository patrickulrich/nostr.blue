use crate::components::live::channel_chat::ChannelChat;
use crate::routes::Route;
use crate::stores::nostr_client::{fetch_events_aggregated, CLIENT_INITIALIZED};
use crate::stores::profiles;
use crate::stores::social::channel_store::{
    cache_channel, cache_channel_metadata, channel_creation_by_id_filter,
    channel_metadata_filter, decode_channel_id, get_cached_channel, get_channel_display_info,
    parse_channel_creation, parse_channel_metadata, Channel,
};
use crate::utils::truncate_pubkey;
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use std::time::Duration;

#[component]
pub fn ChatDetail(channel_id: String) -> Element {
    let mut channel_info: Signal<Option<Channel>> = use_signal(|| None);
    let mut loading = use_signal(|| true);
    let client_ready = use_memo(move || *CLIENT_INITIALIZED.read());
    let mut request_id = use_signal(|| 0u32);

    let channel_id_for_effect = channel_id.clone();
    let channel_id_for_chat = channel_id.clone();

    use_effect(use_reactive((&channel_id_for_effect, &*client_ready.read()), move |(cid, ready)| {
        if !ready {
            return;
        }
        let current_id = request_id.peek().wrapping_add(1);
        request_id.set(current_id);
        spawn(async move {
            let is_stale = || *request_id.peek() != current_id;
            loading.set(true);

            let (event_id, _relay_hints) = match decode_channel_id(&cid) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("Failed to decode channel ID: {}", e);
                    if !is_stale() { loading.set(false); }
                    return;
                }
            };

            let hex_id = event_id.to_hex();

            // Try cache first
            if let Some(cached) = get_cached_channel(&hex_id) {
                if !is_stale() { channel_info.set(Some(cached)); }
            } else {
                // Fetch kind 40
                match fetch_events_aggregated(
                    channel_creation_by_id_filter(event_id),
                    Duration::from_secs(10),
                )
                .await
                {
                    Ok(events) => {
                        if is_stale() { return; }
                        if let Some(event) = events.first() {
                            if let Some(ch) = parse_channel_creation(event) {
                                cache_channel(ch.clone());
                                channel_info.set(Some(ch));
                            }
                        }
                    }
                    Err(e) => log::error!("Failed to fetch channel: {}", e),
                }
            }

            // Fetch latest metadata (kind 41)
            if let Some(ref ch) = *channel_info.peek() {
                match fetch_events_aggregated(
                    channel_metadata_filter(event_id),
                    Duration::from_secs(5),
                )
                .await
                {
                    Ok(events) => {
                        if is_stale() { return; }
                        if let Some(event) = events.first() {
                            if let Some(meta) = parse_channel_metadata(event, &ch.pubkey) {
                                cache_channel_metadata(meta);
                            }
                        }
                    }
                    Err(e) => log::warn!("Failed to fetch channel metadata: {}", e),
                }
            }

            if is_stale() { return; }
            loading.set(false);
        });
    }));

    // Derive display info
    let channel_name = use_memo(move || {
        channel_info
            .read()
            .as_ref()
            .map(|ch| {
                let (name, _, _) = get_channel_display_info(ch);
                name.unwrap_or_else(|| "Unnamed Channel".to_string())
            })
            .unwrap_or_else(|| "Channel".to_string())
    });
    let channel_about = use_memo(move || {
        channel_info.read().as_ref().and_then(|ch| {
            let (_, about, _) = get_channel_display_info(ch);
            about
        })
    });
    let channel_picture = use_memo(move || {
        channel_info.read().as_ref().and_then(|ch| {
            let (_, _, pic) = get_channel_display_info(ch);
            pic
        })
    });
    let creator_pubkey = use_memo(move || {
        channel_info.read().as_ref().map(|ch| ch.pubkey.clone())
    });
    let creator_display_name = use_memo(move || {
        creator_pubkey.read().as_ref().map(|pk| {
            let meta = profiles::get_profile(pk);
            meta.as_ref()
                .and_then(|m| m.display_name.clone().or_else(|| m.name.clone()))
                .unwrap_or_else(|| truncate_pubkey(pk))
        })
    });

    // Resolve hex channel_id for the ChannelChat component
    let hex_channel_id = decode_channel_id(&channel_id_for_chat)
        .map(|(eid, _)| eid.to_hex())
        .unwrap_or_else(|_| channel_id_for_chat.clone());

    rsx! {
        div { class: "flex flex-col h-[calc(100vh-57px)] lg:h-screen",
            // Channel header
            div { class: "shrink-0 border-b border-border px-4 py-3",
                div { class: "flex items-center gap-3",
                    Link {
                        to: Route::Chats {},
                        class: "p-2 hover:bg-accent rounded-lg transition",
                        "← Back"
                    }
                    if let Some(pic) = channel_picture.read().as_ref().filter(|u| is_valid_http_url(u)) {
                        img {
                            src: "{pic}",
                            class: "w-10 h-10 rounded-full object-cover",
                            alt: "Channel",
                            loading: "lazy",
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        h1 { class: "font-bold text-lg truncate", "{channel_name}" }
                        if let Some(ref about) = *channel_about.read() {
                            p { class: "text-xs text-muted-foreground truncate", "{about}" }
                        }
                        if let Some(ref name) = *creator_display_name.read() {
                            p { class: "text-xs text-muted-foreground",
                                "Created by {name}"
                            }
                        }
                    }
                }
            }

            // Chat takes remaining space
            ChannelChat { channel_id: hex_channel_id.clone() }
        }
    }
}
