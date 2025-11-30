use dioxus::prelude::*;
use dioxus::events::MouseData;
use nostr_sdk::{Event as NostrEvent, Timestamp};
use crate::routes::Route;
use crate::stores::nostr_client::CLIENT_INITIALIZED;
use crate::stores::profiles;
use crate::components::{StreamStatus, parse_nip53_live_event, extract_live_event_host};

#[derive(Clone, Debug, PartialEq)]
pub struct LiveStreamMeta {
    pub d_tag: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub image: Option<String>,
    pub streaming_url: Option<String>,
    pub status: StreamStatus,
    pub current_participants: Option<u64>,
    pub starts: Option<Timestamp>,
    pub tags: Vec<String>,
    /// The host pubkey from p tag with Host marker (case-insensitive)
    pub host_pubkey: Option<String>,
    /// Whether the host has a valid proof signature per NIP-53
    pub host_verified: bool,
}

/// Parse NIP-53 Kind 30311 live streaming event into LiveStreamCard's meta format
pub fn parse_live_stream_event(event: &NostrEvent) -> Option<LiveStreamMeta> {
    let live_event = parse_nip53_live_event(event)?;

    log::debug!("Parsed LiveEvent: d_tag={}, title={:?}, streaming={:?}, status={:?}",
        live_event.id,
        live_event.title,
        live_event.streaming,
        live_event.status
    );

    // Extract host with case-insensitive fallback and proof verification
    let host = extract_live_event_host(event, &live_event);
    let host_pubkey = host.as_ref().map(|h| h.public_key.clone());
    let host_verified = host.as_ref().map(|h| h.is_verified).unwrap_or(false);

    // Get raw status and apply stale check
    let raw_status = live_event.status.as_ref()
        .map(StreamStatus::from)
        .unwrap_or(StreamStatus::Planned);
    let effective_status = StreamStatus::effective_status(raw_status, event.created_at);

    // Get hashtags from SDK, or fallback to manual "t" tag extraction
    let tags = if !live_event.hashtags.is_empty() {
        live_event.hashtags
    } else {
        // Fallback: manually extract "t" tags
        event.tags.iter()
            .filter(|tag| tag.as_slice().first().map(|s| s.as_str()) == Some("t"))
            .filter_map(|tag| tag.as_slice().get(1).map(|s| s.to_string()))
            .collect()
    };

    Some(LiveStreamMeta {
        d_tag: live_event.id,
        title: live_event.title,
        summary: live_event.summary,
        image: live_event.image.map(|(url, _dims)| url.to_string()),
        streaming_url: live_event.streaming.map(|url| url.to_string()),
        status: effective_status,
        current_participants: live_event.current_participants,
        starts: live_event.starts,
        tags,
        host_pubkey,
        host_verified,
    })
}

#[component]
pub fn LiveStreamCard(event: NostrEvent) -> Element {
    let stream_meta = match parse_live_stream_event(&event) {
        Some(meta) => meta,
        None => return rsx! { div { class: "hidden" } }
    };

    // Clone values for closures
    // Use host pubkey from p tag if available, otherwise fall back to event publisher
    let author_pubkey = stream_meta.host_pubkey.clone()
        .unwrap_or_else(|| event.pubkey.to_string());
    let author_pubkey_for_fetch = author_pubkey.clone();
    let author_pubkey_display = author_pubkey.clone();
    let host_verified = stream_meta.host_verified;
    let created_at = event.created_at;

    // Create naddr for the livestream (still uses event publisher for fetching)
    let naddr = format!("30311:{}:{}", event.pubkey, stream_meta.d_tag);

    // Get author metadata from profile store (uses LRU cache + database, much faster)
    let author_metadata = use_memo(move || {
        profiles::get_profile(&author_pubkey_for_fetch)
    });

    // Fetch author profile in background if not cached
    use_effect(use_reactive((&author_pubkey_display, &*CLIENT_INITIALIZED.read()), move |(pk, client_initialized)| {
        if !client_initialized {
            return;
        }
        spawn(async move {
            let _ = profiles::fetch_profile(pk).await;
        });
    }));

    // Get author display info
    let author_name = if let Some(ref metadata) = *author_metadata.read() {
        metadata.display_name.clone()
            .or_else(|| metadata.name.clone())
            .unwrap_or_else(|| format!("{}...{}", &author_pubkey_display[..8], &author_pubkey_display[author_pubkey_display.len()-4..]))
    } else {
        format!("{}...{}", &author_pubkey_display[..8], &author_pubkey_display[author_pubkey_display.len()-4..])
    };

    let author_picture = author_metadata.read().as_ref()
        .and_then(|m| m.picture.as_ref().map(|u| u.to_string()));

    rsx! {
        div {
            class: "border-b border-border hover:bg-accent/5 transition",

            // Author header
            div {
                class: "p-4 flex items-center gap-3",
                Link {
                    to: Route::Profile { pubkey: author_pubkey.clone() },
                    class: "flex items-center gap-3 flex-1",
                    if let Some(pic_url) = author_picture {
                        img {
                            src: "{pic_url}",
                            class: "w-12 h-12 rounded-full object-cover",
                            alt: "Avatar",
                            loading: "lazy"
                        }
                    } else {
                        div {
                            class: "w-12 h-12 rounded-full bg-blue-600 flex items-center justify-center text-white font-bold",
                            "{author_name.chars().next().unwrap_or('?').to_ascii_uppercase()}"
                        }
                    }
                    div {
                        class: "flex-1",
                        div {
                            class: "font-semibold flex items-center gap-1",
                            "{author_name}"
                            // Show verified badge if host proof is valid
                            if host_verified {
                                span {
                                    class: "text-green-500",
                                    title: "Verified host",
                                    svg {
                                        class: "w-4 h-4",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        fill: "currentColor",
                                        view_box: "0 0 20 20",
                                        path {
                                            fill_rule: "evenodd",
                                            d: "M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z",
                                            clip_rule: "evenodd"
                                        }
                                    }
                                }
                            }
                        }
                        div {
                            class: "text-sm text-muted-foreground",
                            "{created_at.to_human_datetime()}"
                        }
                    }
                }
            }

            // Stream thumbnail with LIVE badge
            Link {
                to: Route::LiveStreamDetail { note_id: naddr.clone() },
                div {
                    class: "relative bg-black cursor-pointer group",
                    if let Some(img_url) = &stream_meta.image {
                        img {
                            src: "{img_url}",
                            class: "w-full aspect-video object-cover group-hover:opacity-90 transition",
                            alt: "Stream thumbnail",
                            loading: "lazy"
                        }
                    } else {
                        div {
                            class: "w-full aspect-video bg-gray-800 flex items-center justify-center",
                            svg {
                                class: "w-24 h-24 text-gray-600",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
                                }
                            }
                        }
                    }

                    // LIVE badge (top left)
                    if stream_meta.status == StreamStatus::Live {
                        div {
                            class: "absolute top-2 left-2 bg-red-600 text-white text-xs font-bold px-3 py-1 rounded flex items-center gap-1",
                            div {
                                class: "w-2 h-2 bg-white rounded-full animate-pulse"
                            }
                            "LIVE"
                        }
                    } else if stream_meta.status == StreamStatus::Planned {
                        div {
                            class: "absolute top-2 left-2 bg-blue-600 text-white text-xs font-bold px-3 py-1 rounded",
                            "UPCOMING"
                        }
                    }

                    // Viewer count (bottom right)
                    if stream_meta.status == StreamStatus::Live {
                        if let Some(viewers) = stream_meta.current_participants {
                            div {
                                class: "absolute bottom-2 right-2 bg-black/75 text-white text-xs px-2 py-1 rounded flex items-center gap-1",
                                svg {
                                    class: "w-3 h-3",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M15 12a3 3 0 11-6 0 3 3 0 016 0z"
                                    }
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
                                    }
                                }
                                "{viewers}"
                            }
                        }
                    }
                }
            }

            // Title and description
            div {
                class: "p-4",
                if let Some(title_text) = &stream_meta.title {
                    h3 {
                        class: "font-bold text-lg mb-2",
                        "{title_text}"
                    }
                }
                if let Some(summary_text) = &stream_meta.summary {
                    if !summary_text.is_empty() {
                        p {
                            class: "text-sm text-muted-foreground whitespace-pre-wrap line-clamp-2",
                            "{summary_text}"
                        }
                    }
                }

                // Tags
                if !stream_meta.tags.is_empty() {
                    div {
                        class: "flex flex-wrap gap-2 mt-3",
                        for tag in &stream_meta.tags {
                            Link {
                                to: Route::VideosLiveTag { tag: tag.clone() },
                                class: "text-xs bg-accent hover:bg-accent/80 text-primary px-2 py-1 rounded transition",
                                onclick: move |e: Event<MouseData>| e.stop_propagation(),
                                "#{tag}"
                            }
                        }
                    }
                }
            }
        }
    }
}
