//! Highlight Card Component (NIP-84)
//!
//! Display component for a single Kind 9802 highlight event.
use crate::components::SensitiveContent;
use crate::hooks::use_author_metadata;
use crate::routes::Route;
use crate::utils::nip36;
use crate::utils::nip84::{self, HighlightSource};
use crate::utils::{format_relative_time_or, is_valid_http_url, truncate_pubkey};
use dioxus::prelude::*;
use nostr::Event as NostrEvent;
/// Props for the HighlightCard component
#[derive(Props, Clone, PartialEq)]
pub struct HighlightCardProps {
    /// The Kind 9802 highlight event
    pub event: NostrEvent,
}
/// Display card for a highlight event
#[component]
pub fn HighlightCard(props: HighlightCardProps) -> Element {
    let highlight = match nip84::parse_highlight(&props.event) {
        Some(h) => h,
        None => return rsx! {},
    };
    let author_pubkey = highlight.pubkey.clone();
    let created_at = highlight.created_at;
    let author_metadata = use_author_metadata(author_pubkey.clone());
    let timestamp = format_relative_time_or(created_at, "Unknown");
    let display_name = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));
    let profile_picture = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone())
        .filter(|url| is_valid_http_url(url));
    let avatar_letter = display_name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let content_warning = nip36::get_content_warning(&props.event.tags);
    let source_display = get_source_display(&highlight.source);
    rsx! {
        div { class: "bg-card rounded-lg border border-border shadow-sm hover:border-primary/50 transition-all p-4",
            div { class: "border-l-4 border-primary pl-4 mb-3",
                {
                    let highlight_text = rsx! {
                        p { class: "font-serif text-foreground leading-8 text-lg", "{highlight.content}" }
                    };
                    if let Some(reason) = content_warning {
                        rsx! { SensitiveContent { reason, {highlight_text} } }
                    } else {
                        highlight_text
                    }
                }
                if let Some(comment) = &highlight.comment {
                    p { class: "text-muted-foreground italic mt-3 text-sm", "— {comment}" }
                }
                if let Some(context) = &highlight.context {
                    p { class: "text-xs text-muted-foreground mt-2", "{context}" }
                }
            }
            if let Some((source_icon, source_text, source_url)) = source_display {
                div { class: "bg-secondary/30 border-t border-border -mx-4 -mb-4 px-4 py-3 mt-3 rounded-b-lg",
                    if let Some(url) = source_url {
                        a {
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors",
                            span { class: "text-base", "{source_icon}" }
                            span { class: "truncate", "{source_text}" }
                        }
                    } else {
                        div { class: "flex items-center gap-2 text-sm text-muted-foreground",
                            span { class: "text-base", "{source_icon}" }
                            span { class: "truncate", "{source_text}" }
                        }
                    }
                }
            }
            div { class: "flex items-center gap-2 mt-3 pt-3 border-t border-border/50",
                Link {
                    to: Route::Profile {
                        pubkey: author_pubkey.clone(),
                    },
                    class: "shrink-0",
                    if let Some(pic_url) = profile_picture.clone() {
                        img {
                            src: "{pic_url}",
                            alt: "{display_name}",
                            class: "w-6 h-6 rounded-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-6 h-6 rounded-full bg-primary/20 flex items-center justify-center text-xs font-medium",
                            "{avatar_letter}"
                        }
                    }
                }
                div { class: "flex items-center gap-1 text-xs text-muted-foreground min-w-0",
                    Link {
                        to: Route::Profile {
                            pubkey: author_pubkey.clone(),
                        },
                        class: "font-medium hover:text-foreground transition-colors truncate",
                        "{display_name}"
                    }
                    span { "·" }
                    span { class: "shrink-0", "{timestamp}" }
                }
            }
        }
    }
}
/// Get display info for highlight source
/// Returns (icon, display_text, optional_url)
fn get_source_display(source: &HighlightSource) -> Option<(&'static str, String, Option<String>)> {
    match source {
        HighlightSource::Url(url) => {
            let display = url
                .replace("https://", "")
                .replace("http://", "")
                .split('/')
                .next()
                .unwrap_or(url)
                .to_string();
            let href = if is_valid_http_url(url) {
                Some(url.clone())
            } else {
                None
            };
            Some(("🔗", display, href))
        }
        HighlightSource::Article { .. } => Some(("📄", "Nostr article".to_string(), None)),
        HighlightSource::Event { event_id, .. } => {
            let short_id = if event_id.len() > 12 {
                format!("{}...", &event_id[..12])
            } else {
                event_id.clone()
            };
            Some(("📝", format!("Event {}", short_id), None))
        }
        HighlightSource::Unknown => None,
    }
}
/// Skeleton loader for HighlightCard
#[component]
pub fn HighlightCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card rounded-lg border border-border shadow-sm p-4 animate-pulse",
            div { class: "border-l-4 border-muted pl-4 mb-3 space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
                div { class: "h-4 bg-muted rounded w-4/6" }
            }
            div { class: "bg-secondary/30 border-t border-border -mx-4 -mb-4 px-4 py-3 mt-3 rounded-b-lg",
                div { class: "h-4 bg-muted rounded w-1/3" }
            }
            div { class: "flex items-center gap-2 mt-3 pt-3 border-t border-border/50",
                div { class: "w-6 h-6 rounded-full bg-muted" }
                div { class: "h-3 bg-muted rounded w-24" }
            }
        }
    }
}
