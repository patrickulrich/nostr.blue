use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, Kind};
use nostr_sdk::prelude::{Coordinate, ToBech32};
use crate::routes::Route;
use crate::stores::nostr_client::CLIENT_INITIALIZED;
use crate::stores::profiles;
use crate::components::{StreamStatus, parse_nip53_live_event, extract_live_event_host};

#[derive(Clone, Debug)]
pub struct LiveStreamMeta {
    pub d_tag: String,
    pub title: Option<String>,
    pub image: Option<String>,
    pub status: StreamStatus,
    pub current_participants: Option<u64>,
    /// The host pubkey from p tag with Host marker (case-insensitive)
    pub host_pubkey: Option<String>,
    /// Whether the host has a valid proof signature per NIP-53
    pub host_verified: bool,
}

/// Parse NIP-53 Kind 30311 live streaming event into MiniLiveStreamCard's meta format
fn parse_live_stream_event(event: &NostrEvent) -> Option<LiveStreamMeta> {
    let live_event = parse_nip53_live_event(event)?;

    // Extract host with case-insensitive fallback and proof verification
    let host = extract_live_event_host(event, &live_event);
    let host_pubkey = host.as_ref().map(|h| h.public_key.clone());
    let host_verified = host.as_ref().map(|h| h.is_verified).unwrap_or(false);

    // Get raw status and apply stale check
    let raw_status = live_event.status.as_ref()
        .map(StreamStatus::from)
        .unwrap_or(StreamStatus::Planned);
    let effective_status = StreamStatus::effective_status(raw_status, event.created_at);

    Some(LiveStreamMeta {
        d_tag: live_event.id,
        title: live_event.title,
        image: live_event.image.map(|(url, _dims)| url.to_string()),
        status: effective_status,
        current_participants: live_event.current_participants,
        host_pubkey,
        host_verified,
    })
}

#[component]
pub fn MiniLiveStreamCard(event: NostrEvent) -> Element {
    let stream_meta = match parse_live_stream_event(&event) {
        Some(meta) => meta,
        None => return rsx! { div { class: "hidden" } }
    };

    // Use host pubkey from p tag if available, otherwise fall back to event publisher
    let author_pubkey = stream_meta.host_pubkey.clone()
        .unwrap_or_else(|| event.pubkey.to_string());
    let author_pubkey_for_fetch = author_pubkey.clone();
    let host_verified = stream_meta.host_verified;

    // Create bech32 naddr for the livestream
    let coord = Coordinate::new(Kind::from(30311), event.pubkey)
        .identifier(&stream_meta.d_tag);
    let naddr = coord.to_bech32().unwrap_or_else(|_| {
        format!("30311:{}:{}", event.pubkey, stream_meta.d_tag)
    });

    // Get author metadata from profile store (uses LRU cache + database, much faster)
    let author_metadata = use_memo(move || {
        profiles::get_profile(&author_pubkey_for_fetch)
    });

    // Fetch author profile in background if not cached
    use_effect(use_reactive((&author_pubkey, &*CLIENT_INITIALIZED.read()), move |(pk, client_initialized)| {
        if !client_initialized {
            return;
        }
        spawn(async move {
            let _ = profiles::fetch_profile(pk).await;
        });
    }));

    let display_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            // Use author_pubkey (which may be host_pubkey) for consistent fallback display
            let pk = author_pubkey.clone();
            if pk.len() > 16 {
                format!("{}...{}", &pk[..8], &pk[pk.len()-8..])
            } else {
                pk
            }
        });

    // Format timestamp as "X ago"
    let format_time_ago = |timestamp: u64| -> String {
        let now = (js_sys::Date::now() / 1000.0) as u64;
        let diff = now.saturating_sub(timestamp);

        match diff {
            0..=59 => "just now".to_string(),
            60..=3599 => format!("{}m ago", diff / 60),
            3600..=86399 => format!("{}h ago", diff / 3600),
            86400..=604799 => format!("{}d ago", diff / 86400),
            _ => format!("{}w ago", diff / 604800),
        }
    };

    rsx! {
        div {
            class: "group cursor-pointer",

            Link {
                to: Route::LiveStreamDetail { note_id: naddr.clone() },

                div {
                    class: "relative aspect-video bg-muted rounded-lg overflow-hidden mb-3",

                    // Show thumbnail if available
                    if let Some(img_url) = &stream_meta.image {
                        img {
                            src: "{img_url}",
                            alt: "{stream_meta.title.as_deref().unwrap_or(\"Live Stream\")}",
                            class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200"
                        }
                    } else {
                        div {
                            class: "w-full h-full flex items-center justify-center bg-gray-800",
                            svg {
                                class: "w-12 h-12 text-gray-600",
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

                    // LIVE badge (top-left)
                    if stream_meta.status == StreamStatus::Live {
                        div {
                            class: "absolute top-2 left-2 bg-red-600 text-white text-xs font-bold px-2 py-1 rounded flex items-center gap-1",
                            div {
                                class: "w-1.5 h-1.5 bg-white rounded-full animate-pulse"
                            }
                            "LIVE"
                        }
                    } else if stream_meta.status == StreamStatus::Planned {
                        div {
                            class: "absolute top-2 left-2 bg-blue-600 text-white text-xs font-bold px-2 py-1 rounded",
                            "UPCOMING"
                        }
                    }

                    // Viewer count badge (bottom-right)
                    if stream_meta.status == StreamStatus::Live {
                        if let Some(viewers) = stream_meta.current_participants {
                            div {
                                class: "absolute bottom-2 right-2 bg-black/80 text-white text-xs px-2 py-1 rounded flex items-center gap-1",
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

                // Stream info
                div {
                    if let Some(title) = &stream_meta.title {
                        h3 {
                            class: "font-semibold line-clamp-2 mb-1 group-hover:text-primary transition",
                            "{title}"
                        }
                    }

                    p {
                        class: "text-sm text-muted-foreground mb-1 flex items-center gap-1",
                        "{display_name}"
                        // Show verified badge if host proof is valid
                        if host_verified {
                            span {
                                class: "text-green-500",
                                title: "Verified host",
                                svg {
                                    class: "w-3 h-3",
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

                    p {
                        class: "text-xs text-muted-foreground",
                        "{format_time_ago(event.created_at.as_secs())}"
                    }
                }
            }
        }
    }
}
