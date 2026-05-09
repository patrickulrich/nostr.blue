use dioxus::prelude::*;
use nostr_sdk::Event;

use crate::components::blobbi::core::parsers::parse_blobbi_from_event;
use crate::components::blobbi::visual::blobbi_visual::BlobbiVisual;
use crate::hooks::use_author_metadata;
use crate::utils::{format_relative_time_or, is_valid_http_url, truncate_pubkey};

#[component]
pub fn BlobbiCard(event: Event) -> Element {
    let blobbi = parse_blobbi_from_event(&event);
    let author_pubkey = event.pubkey.to_string();
    let author_metadata = use_author_metadata(author_pubkey.clone());
    let display_name = author_metadata
        .read()
        .as_ref()
        .and_then(|m| {
            m.display_name
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .or(m.name.as_ref().filter(|s| !s.trim().is_empty()))
                .cloned()
        })
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));
    let profile_picture = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone())
        .filter(|url| !url.trim().is_empty() && is_valid_http_url(url));
    let avatar_letter = display_name
        .trim_start()
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let timestamp = format_relative_time_or(event.created_at.as_secs(), "Unknown date");

    let stage_icon = match blobbi.stage {
        crate::utils::nip_bb::BlobbiStage::Egg => "🥚",
        crate::utils::nip_bb::BlobbiStage::Baby => "🐣",
        crate::utils::nip_bb::BlobbiStage::Adult => "🐾",
    };

    rsx! {
        div { class: "group bg-card rounded-lg border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-lg",
            div { class: "p-4",
                div { class: "flex items-center gap-3",
                    div { class: "shrink-0",
                        BlobbiVisual { blobbi: blobbi.clone(), size: Some("80".to_string()), feed_mode: Some(true) }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "flex items-center gap-2",
                            span { class: "text-sm", "{stage_icon}" }
                            h3 { class: "text-sm font-bold truncate",
                                "{blobbi.display_name()}"
                            }
                            span { class: "text-xs text-muted-foreground capitalize",
                                "{blobbi.stage.label()}"
                            }
                        }
                        if !blobbi.personality.mood.is_empty() {
                            p { class: "text-xs text-muted-foreground mt-0.5",
                                "Mood: {blobbi.personality.mood}"
                            }
                        }
                        p { class: "text-xs text-muted-foreground mt-0.5",
                            "XP: {blobbi.experience} · Gen: {blobbi.generation} · Streak: {blobbi.care_streak}d"
                        }
                        if !blobbi.content.is_empty() {
                            p { class: "text-xs text-muted-foreground mt-1 line-clamp-2",
                                "{blobbi.content}"
                            }
                        }
                    }
                }

                div { class: "flex items-center justify-between mt-3 pt-3 border-t border-border",
                    div { class: "flex items-center gap-2",
                        if let Some(pic_url) = profile_picture {
                            img {
                                src: "{pic_url}",
                                alt: "{display_name}",
                                class: "w-6 h-6 rounded-full object-cover",
                                loading: "lazy",
                            }
                        } else {
                            div { class: "w-6 h-6 rounded-full bg-muted flex items-center justify-center",
                                span { class: "text-[10px] font-semibold text-muted-foreground",
                                    "{avatar_letter}"
                                }
                            }
                        }
                        span { class: "text-xs font-medium truncate", "{display_name}" }
                        span { class: "text-xs text-muted-foreground", "·" }
                        span { class: "text-xs text-muted-foreground", "{timestamp}" }
                    }
                    span { class: "text-[10px] text-muted-foreground shrink-0",
                        "Blobbi Pet"
                    }
                }
            }
        }
    }
}
