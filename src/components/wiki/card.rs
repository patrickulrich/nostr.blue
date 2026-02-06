//! Wiki Card Component
//! Display card for wiki pages (NIP-54 Kind 30818)
use crate::components::publication::asciidoc_content::AsciiDocPreview;
use crate::components::content_menu::{ContentMenu, ContentMenuType};
use crate::components::icons::{FileVideoIcon, Link2Icon};
use crate::routes::Route;
use crate::stores::profiles;
use crate::stores::wiki_store::{CachedWikiPage, WikiMetadata};
use crate::utils::asciidoc::content_to_plain_text;
use crate::utils::time::format_relative_time_ex;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::Timestamp;
/// Wiki page card for feed display
#[component]
pub fn WikiCard(page: CachedWikiPage) -> Element {
    let nav = use_navigator();
    let author_hex = page.event.pubkey.to_hex();
    let author_profile = profiles::get_profile(&author_hex);
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_hex));
    let author_picture = author_profile.as_ref().and_then(|p| p.picture.clone());
    let identifier = page.article.identifier.clone();
    let card_click = move |_| {
        nav.push(Route::WikiDetail {
            identifier: identifier.clone(),
        });
    };
    let time_ago = format_relative_time_ex(page.event.created_at, true, true);
    let author_hex_menu = author_hex.clone();
    let naddr = page.article.naddr.clone();
    let event_id = page.event.id.to_hex();
    rsx! {
        div {
            class: "relative bg-card border border-border rounded-lg p-4 hover:border-primary/50 transition-colors cursor-pointer",
            onclick: card_click,
            div { class: "absolute top-2 right-2 z-10",
                ContentMenu {
                    content_type: ContentMenuType::Wiki,
                    author_pubkey: author_hex_menu,
                    naddr,
                    event_id: Some(event_id),
                }
            }
            div { class: "space-y-3",
                h3 { class: "font-semibold text-lg text-foreground line-clamp-2 pr-8",
                    "{page.article.title}"
                }
                if let Some(ref summary) = page.article.summary {
                    p { class: "text-sm text-muted-foreground line-clamp-3", "{summary}" }
                } else {
                    AsciiDocPreview { content: page.event.content.clone(), max_chars: 150 }
                }
                if !page.forward_links.is_empty() {
                    div { class: "flex flex-wrap gap-1",
                        for link in page.forward_links.iter().take(5) {
                            span { class: "inline-flex items-center px-2 py-0.5 text-xs bg-accent rounded-md",
                                Link2Icon { class: "w-3 h-3 mr-1" }
                                "{link}"
                            }
                        }
                        if page.forward_links.len() > 5 {
                            span { class: "text-xs text-muted-foreground",
                                "+{page.forward_links.len() - 5} more"
                            }
                        }
                    }
                }
                div { class: "flex items-center justify-between pt-2 border-t border-border",
                    div { class: "flex items-center gap-2",
                        if let Some(ref picture) = author_picture {
                            img {
                                class: "w-5 h-5 rounded-full object-cover",
                                src: "{picture}",
                                alt: "{author_name}",
                            }
                        } else {
                            div { class: "w-5 h-5 rounded-full bg-gradient-to-br from-primary/50 to-accent/50 flex items-center justify-center text-xs font-medium text-primary-foreground",
                                "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                            }
                        }
                        span { class: "text-sm text-muted-foreground", "{author_name}" }
                    }
                    span { class: "text-xs text-muted-foreground", "{time_ago}" }
                }
            }
        }
    }
}
/// Compact wiki card for lists
#[component]
pub fn WikiCardCompact(page: CachedWikiPage) -> Element {
    let nav = use_navigator();
    let identifier = page.article.identifier.clone();
    let card_click = move |_| {
        nav.push(Route::WikiDetail {
            identifier: identifier.clone(),
        });
    };
    rsx! {
        div {
            class: "flex items-start gap-3 p-3 bg-card border border-border rounded-lg hover:border-primary/50 transition-colors cursor-pointer",
            onclick: card_click,
            div { class: "shrink-0 w-8 h-8 rounded bg-primary/10 flex items-center justify-center",
                FileVideoIcon { class: "w-4 h-4 text-primary" }
            }
            div { class: "flex-1 min-w-0",
                h4 { class: "font-medium text-foreground line-clamp-1", "{page.article.title}" }
                p { class: "text-xs text-muted-foreground mt-0.5", "{page.article.identifier}" }
            }
            if !page.forward_links.is_empty() {
                span { class: "shrink-0 text-xs text-muted-foreground",
                    "{page.forward_links.len()} links"
                }
            }
        }
    }
}
/// Wiki card for search results
#[component]
pub fn WikiCardSearchResult(
    page: CachedWikiPage,
    #[props(default = None)]
    highlight: Option<String>,
) -> Element {
    let nav = use_navigator();
    let identifier = page.article.identifier.clone();
    let card_click = move |_| {
        nav.push(Route::WikiDetail {
            identifier: identifier.clone(),
        });
    };
    rsx! {
        div {
            class: "p-4 border-b border-border hover:bg-accent/50 transition-colors cursor-pointer",
            onclick: card_click,
            div { class: "flex items-baseline gap-2 mb-1",
                h3 { class: "font-semibold text-foreground", "{page.article.title}" }
                span { class: "text-xs text-muted-foreground", "({page.article.identifier})" }
            }
            p { class: "text-sm text-muted-foreground line-clamp-2",
                if let Some(ref summary) = page.article.summary {
                    "{summary}"
                } else {
                    {get_content_preview(&page.event.content, 200)}
                }
            }
            div { class: "flex items-center gap-4 mt-2 text-xs text-muted-foreground",
                span { "{page.forward_links.len()} outgoing links" }
                span { "{page.backward_links.len()} backlinks" }
            }
        }
    }
}
/// Skeleton loader for wiki card
#[component]
pub fn WikiCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 animate-pulse",
            div { class: "space-y-3",
                div { class: "h-6 bg-muted rounded w-3/4" }
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
                div { class: "flex items-center justify-between pt-2",
                    div { class: "flex items-center gap-2",
                        div { class: "w-5 h-5 bg-muted rounded-full" }
                        div { class: "h-3 bg-muted rounded w-20" }
                    }
                    div { class: "h-3 bg-muted rounded w-16" }
                }
            }
        }
    }
}
/// Wiki page grid
#[component]
pub fn WikiGrid(
    pages: Vec<CachedWikiPage>,
    #[props(default = false)]
    loading: bool,
) -> Element {
    rsx! {
        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
            for page in pages.iter() {
                WikiCard { key: "{page.event.id.to_hex()}", page: page.clone() }
            }
            if loading {
                for _ in 0..4 {
                    WikiCardSkeleton {}
                }
            }
        }
    }
}
/// Wiki metadata card (for sidebar)
#[component]
pub fn WikiMetadataCard(metadata: WikiMetadata) -> Element {
    let time_ago = format_relative_time_ex(
        Timestamp::from(metadata.created_at),
        true,
        true,
    );
    let author_profile = profiles::get_profile(&metadata.author_pubkey);
    let author_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&metadata.author_pubkey));
    let author_picture = author_profile.as_ref().and_then(|p| p.picture.clone());
    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4 space-y-3",
            h4 { class: "text-sm font-medium text-foreground", "Page Info" }
            dl { class: "space-y-2 text-sm",
                div {
                    dt { class: "text-muted-foreground", "Identifier" }
                    dd { class: "font-mono text-foreground", "{metadata.identifier}" }
                }
                div {
                    dt { class: "text-muted-foreground", "Author" }
                    dd { class: "flex items-center gap-2",
                        if let Some(ref picture) = author_picture {
                            img {
                                class: "w-4 h-4 rounded-full object-cover",
                                src: "{picture}",
                                alt: "{author_name}",
                            }
                        } else {
                            div { class: "w-4 h-4 rounded-full bg-gradient-to-br from-primary/50 to-accent/50 flex items-center justify-center text-xs font-medium text-primary-foreground",
                                "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                            }
                        }
                        span { "{author_name}" }
                    }
                }
                div {
                    dt { class: "text-muted-foreground", "Last Updated" }
                    dd { class: "text-foreground", "{time_ago}" }
                }
            }
        }
    }
}
/// Get content preview (strip markup and truncate)
/// Uses format-aware stripping for both Markdown and AsciiDoc
fn get_content_preview(content: &str, max_chars: usize) -> String {
    let plain = content_to_plain_text(content);
    if plain.chars().count() > max_chars {
        let truncated: String = plain.chars().take(max_chars).collect();
        format!("{}...", truncated.trim_end())
    } else {
        plain
    }
}
