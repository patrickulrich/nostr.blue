use crate::hooks::use_author_metadata;
use crate::routes::Route;
use crate::utils::article_meta::{
    calculate_read_time, get_hashtags, get_identifier, get_image, get_published_at,
    get_summary, get_title,
};
use crate::utils::{format_relative_time_or, is_valid_http_url, truncate_pubkey};
use dioxus::prelude::*;
use nostr::prelude::{Coordinate, Nip19Coordinate, ToBech32};
use nostr_sdk::Event as NostrEvent;
#[component]
pub fn ArticleCard(event: NostrEvent) -> Element {
    let title = get_title(&event);
    let summary = get_summary(&event);
    let image_url = get_image(&event);
    let published_at = get_published_at(&event);
    let hashtags = get_hashtags(&event);
    let identifier = get_identifier(&event);
    let read_time = calculate_read_time(&event.content);
    let author_pubkey = event.pubkey.to_string();
    let author_metadata = use_author_metadata(author_pubkey.clone());
    let timestamp = format_relative_time_or(published_at, "Unknown date");
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
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let naddr_opt = identifier
        .clone()
        .filter(|id| !id.trim().is_empty())
        .and_then(|id| {
            let coord = Coordinate::new(event.kind, event.pubkey).identifier(id);
            let relays = vec![];
            let nip19_coord = Nip19Coordinate::new(coord, relays);
            nip19_coord.to_bech32().ok()
        });
    let displayed_tags: Vec<String> = hashtags.iter().take(3).cloned().collect();
    let preview_text = if let Some(sum) = summary {
        sum
    } else {
        let content = event.content.clone();
        let char_count = content.chars().count();
        if char_count > 150 {
            let truncated: String = content.chars().take(150).collect();
            if let Some(last_space) = truncated.rfind(' ') {
                format!("{}...", &truncated[..last_space])
            } else {
                format!("{}...", truncated)
            }
        } else {
            content
        }
    };
    rsx! {
        div { class: "group bg-card rounded-lg border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-lg",
            if let Some(naddr) = naddr_opt {
                Link {
                    to: Route::ArticleDetail {
                        naddr: naddr.clone(),
                    },
                    class: "block",
                    if let Some(img_url) = image_url {
                        div { class: "aspect-video w-full bg-muted overflow-hidden",
                            img {
                                src: "{img_url}",
                                alt: "{title}",
                                class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200",
                                loading: "lazy",
                            }
                        }
                    }
                    div { class: "p-4 space-y-3",
                        if !displayed_tags.is_empty() {
                            div { class: "flex flex-wrap gap-2",
                                for tag in displayed_tags {
                                    span {
                                        key: "{tag}",
                                        class: "px-2 py-1 text-xs rounded-full bg-primary/10 text-primary font-medium",
                                        "#{tag}"
                                    }
                                }
                            }
                        }
                        h3 { class: "text-xl font-bold line-clamp-2 group-hover:text-primary transition-colors",
                            "{title}"
                        }
                        p { class: "text-sm text-muted-foreground line-clamp-3", "{preview_text}" }
                        div { class: "flex items-center justify-between pt-2",
                            div { class: "flex items-center gap-2",
                                Link {
                                    to: Route::Profile {
                                        pubkey: author_pubkey.clone(),
                                    },
                                    onclick: move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                    },
                                    class: "shrink-0",
                                    div { class: "w-8 h-8 rounded-full overflow-hidden bg-muted flex items-center justify-center",
                                        if let Some(pic_url) = profile_picture {
                                            img {
                                                src: "{pic_url}",
                                                alt: "{display_name}",
                                                class: "w-full h-full object-cover",
                                                loading: "lazy",
                                            }
                                        } else {
                                            span { class: "text-xs font-semibold text-muted-foreground",
                                                "{avatar_letter}"
                                            }
                                        }
                                    }
                                }
                                div { class: "flex flex-col min-w-0",
                                    Link {
                                        to: Route::Profile {
                                            pubkey: author_pubkey,
                                        },
                                        onclick: move |e: Event<MouseData>| {
                                            e.stop_propagation();
                                        },
                                        class: "text-sm font-medium hover:underline truncate",
                                        "{display_name}"
                                    }
                                    span { class: "text-xs text-muted-foreground", "{timestamp}" }
                                }
                            }
                            div { class: "flex items-center gap-1 text-xs text-muted-foreground shrink-0",
                                span { "📄" }
                                span { "{read_time} min read" }
                            }
                        }
                    }
                }
            } else {
                div { class: "p-4 bg-yellow-500/10 border-l-4 border-yellow-500",
                    p { class: "text-sm text-yellow-700 dark:text-yellow-300",
                        "⚠️ This article is missing a required identifier (d tag) and cannot be displayed properly."
                    }
                }
            }
        }
    }
}
/// Skeleton loader for article cards
#[component]
pub fn ArticleCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse",
            div { class: "aspect-video w-full bg-muted" }
            div { class: "p-4 space-y-3",
                div { class: "flex gap-2",
                    div { class: "h-6 w-16 bg-muted rounded-full" }
                    div { class: "h-6 w-20 bg-muted rounded-full" }
                }
                div { class: "h-6 bg-muted rounded w-3/4" }
                div { class: "h-6 bg-muted rounded w-1/2" }
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-2/3" }
                div { class: "flex items-center justify-between pt-2",
                    div { class: "flex items-center gap-2",
                        div { class: "w-8 h-8 rounded-full bg-muted" }
                        div {
                            div { class: "h-4 w-24 bg-muted rounded mb-1" }
                            div { class: "h-3 w-16 bg-muted rounded" }
                        }
                    }
                    div { class: "h-4 w-20 bg-muted rounded" }
                }
            }
        }
    }
}
