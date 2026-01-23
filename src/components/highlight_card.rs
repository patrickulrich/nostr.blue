//! Highlight Card Component (NIP-84)
//!
//! Display component for a single Kind 9802 highlight event.

use dioxus::prelude::*;
use nostr::Event as NostrEvent;
use crate::hooks::use_author_metadata;
use crate::routes::Route;
use crate::utils::nip84::{self, HighlightSource};
use crate::utils::{format_relative_time_or, truncate_pubkey, is_valid_http_url};

/// Props for the HighlightCard component
#[derive(Props, Clone, PartialEq)]
pub struct HighlightCardProps {
    /// The Kind 9802 highlight event
    pub event: NostrEvent,
}

/// Display card for a highlight event
#[component]
pub fn HighlightCard(props: HighlightCardProps) -> Element {
    // Parse the event into a Highlight struct
    let highlight = match nip84::parse_highlight(&props.event) {
        Some(h) => h,
        None => return rsx! { }, // Invalid event, render nothing
    };

    let author_pubkey = highlight.pubkey.clone();
    let created_at = highlight.created_at;

    // Author profile metadata
    let author_metadata = use_author_metadata(author_pubkey.clone());

    // Format timestamp
    let timestamp = format_relative_time_or(created_at, "Unknown");

    // Get display name from metadata or fallback
    let display_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));

    let profile_picture = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone())
        .filter(|url| is_valid_http_url(url));

    // Generate avatar fallback letter
    let avatar_letter = display_name.chars().next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    // Get source display info
    let source_display = get_source_display(&highlight.source);

    rsx! {
        div {
            class: "bg-card rounded-lg border border-border shadow-sm hover:border-primary/50 transition-all p-4",

            // Quote block with left border accent
            div {
                class: "border-l-4 border-primary pl-4 mb-3",

                // Highlighted text - serif font for readability
                p {
                    class: "font-serif text-foreground leading-8 text-lg",
                    "{highlight.content}"
                }

                // User comment if present
                if let Some(comment) = &highlight.comment {
                    p {
                        class: "text-muted-foreground italic mt-3 text-sm",
                        "— {comment}"
                    }
                }

                // Context/reference if present
                if let Some(context) = &highlight.context {
                    p {
                        class: "text-xs text-muted-foreground mt-2",
                        "{context}"
                    }
                }
            }

            // Source section (if available)
            if let Some((source_icon, source_text, source_url)) = source_display {
                div {
                    class: "bg-secondary/30 border-t border-border -mx-4 -mb-4 px-4 py-3 mt-3 rounded-b-lg",

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
                        div {
                            class: "flex items-center gap-2 text-sm text-muted-foreground",
                            span { class: "text-base", "{source_icon}" }
                            span { class: "truncate", "{source_text}" }
                        }
                    }
                }
            }

            // Author row
            div {
                class: "flex items-center gap-2 mt-3 pt-3 border-t border-border/50",

                // Avatar link
                Link {
                    to: Route::Profile { pubkey: author_pubkey.clone() },
                    class: "shrink-0",

                    if let Some(pic_url) = profile_picture.clone() {
                        img {
                            src: "{pic_url}",
                            alt: "{display_name}",
                            class: "w-6 h-6 rounded-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div {
                            class: "w-6 h-6 rounded-full bg-primary/20 flex items-center justify-center text-xs font-medium",
                            "{avatar_letter}"
                        }
                    }
                }

                // Author name and timestamp
                div {
                    class: "flex items-center gap-1 text-xs text-muted-foreground min-w-0",

                    Link {
                        to: Route::Profile { pubkey: author_pubkey.clone() },
                        class: "font-medium hover:text-foreground transition-colors truncate",
                        "{display_name}"
                    }

                    span { "·" }

                    span {
                        class: "shrink-0",
                        "{timestamp}"
                    }
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
            // Extract domain for display
            let display = url
                .replace("https://", "")
                .replace("http://", "")
                .split('/')
                .next()
                .unwrap_or(url)
                .to_string();
            // Only return href if URL has valid http/https scheme (prevents javascript: etc.)
            let href = if is_valid_http_url(url) {
                Some(url.clone())
            } else {
                None
            };
            Some(("🔗", display, href))
        }
        HighlightSource::Article { .. } => {
            // Could link to naddr in the future
            Some(("📄", "Nostr article".to_string(), None))
        }
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
        div {
            class: "bg-card rounded-lg border border-border shadow-sm p-4 animate-pulse",

            // Quote block skeleton
            div {
                class: "border-l-4 border-muted pl-4 mb-3 space-y-2",

                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
                div { class: "h-4 bg-muted rounded w-4/6" }
            }

            // Source skeleton
            div {
                class: "bg-secondary/30 border-t border-border -mx-4 -mb-4 px-4 py-3 mt-3 rounded-b-lg",
                div { class: "h-4 bg-muted rounded w-1/3" }
            }

            // Author row skeleton
            div {
                class: "flex items-center gap-2 mt-3 pt-3 border-t border-border/50",
                div { class: "w-6 h-6 rounded-full bg-muted" }
                div { class: "h-3 bg-muted rounded w-24" }
            }
        }
    }
}
