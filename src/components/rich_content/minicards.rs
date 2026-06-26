use crate::components::citation::card::get_citation_style;
use crate::components::icons::{self, HashIcon, NostrBlueMiniLogo};
use crate::routes::Route;
use crate::stores::nostr_music::{NostrPlaylist, NostrTrack};
use crate::stores::pin_boards_store::Pinboard;
use crate::stores::publication_store::PublicationIndex;
use crate::stores::social::channel_store::{get_channel_display_info, parse_channel_creation};
use crate::utils::article_meta::{get_image, get_summary, get_title};
use crate::utils::nip19_urls::note_route_id_with_kind;
use crate::utils::nip34::{Issue, IssueStatus, PullRequest};
use crate::utils::nip58::BadgeDefinition;
use crate::utils::nip99::{Product, ProductCollection, ProductReview};
use crate::utils::nkbip03::Citation;
use crate::utils::recipe::RecipeMetadata;
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use dioxus_primitives::hover_card::{HoverCard, HoverCardContent, HoverCardTrigger};
use dioxus_primitives::ContentSide;
use nostr_sdk::{Event, Metadata, ToBech32};

pub(super) fn render_embedded_article(
    event: &Event,
    metadata: Option<&Metadata>,
    naddr: &str,
) -> Element {
    let title = get_title(event);
    let summary = get_summary(event);
    let image_url = get_image(event).filter(|u| is_valid_http_url(u));
    let pubkey_str = event.pubkey.to_hex();
    let display_name = if let Some(meta) = metadata {
        meta.display_name
            .clone()
            .or_else(|| meta.name.clone())
            .unwrap_or_else(|| {
                format!(
                    "{}...{}",
                    &pubkey_str[..8],
                    &pubkey_str[pubkey_str.len() - 4..]
                )
            })
    } else {
        format!(
            "{}...{}",
            &pubkey_str[..8],
            &pubkey_str[pubkey_str.len() - 4..]
        )
    };
    let display_summary = if let Some(sum) = summary {
        let char_count = sum.chars().count();
        if char_count > 200 {
            let truncated: String = sum.chars().take(200).collect();
            format!("{}...", truncated)
        } else {
            sum
        }
    } else {
        String::new()
    };
    rsx! {
        Link {
            to: Route::AddressViewer {
                address: naddr.to_string(),
            },
            class: "block my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition cursor-pointer",
                if let Some(img_url) = image_url {
                    div { class: "aspect-video w-full bg-muted overflow-hidden",
                        img {
                            src: "{img_url}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    }
                }
                div { class: "p-3",
                    h4 { class: "font-bold text-base mb-1 line-clamp-2", "{title}" }
                    if !display_summary.is_empty() {
                        p { class: "text-sm text-muted-foreground mb-2 line-clamp-2",
                            "{display_summary}"
                        }
                    }
                    div { class: "flex items-center gap-2",
                        if let Some(meta) = metadata {
                            if let Some(picture) = meta.picture.as_ref().filter(|u| is_valid_http_url(u)) {
                                img {
                                    class: "w-6 h-6 rounded-full",
                                    src: "{picture}",
                                    alt: "Avatar",
                                }
                            } else {
                                div { class: "w-6 h-6 rounded-full bg-accent flex items-center justify-center text-accent-foreground text-xs font-bold",
                                    "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                                }
                            }
                        } else {
                            div { class: "w-6 h-6 rounded-full bg-muted flex items-center justify-center text-accent-foreground text-xs",
                                "?"
                            }
                        }
                        span { class: "text-xs text-muted-foreground", "{display_name}" }
                        span { class: "text-xs text-muted-foreground", "• Article" }
                    }
                }
            }
        }
    }
}

/// Render a wiki article minicard with HoverCard preview
pub(super) fn render_wiki_minicard(
    wiki: &crate::utils::nip54::WikiArticle,
    _naddr: &str,
    _event: &Event,
) -> Element {
    let title = wiki.title.clone();
    let identifier = wiki.identifier.clone();
    let author_npub = nostr_sdk::PublicKey::from_hex(&wiki.pubkey)
        .ok()
        .and_then(|pk| pk.to_bech32().ok())
        .unwrap_or_else(|| wiki.pubkey.clone());
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::WikiDetail {
                    npub: author_npub,
                    identifier: identifier.clone(),
                },
                class: "flex items-center gap-2 p-2 bg-card border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-8 h-8 rounded bg-accent/10 flex items-center justify-center shrink-0",
                    icons::BookOpenIcon { class: "w-4 h-4 text-accent".to_string() }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{title}" }
                    p { class: "text-xs text-muted-foreground", "Wiki Article" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-80 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        h4 { class: "font-bold mb-2", "{title}" }
                        if let Some(summary) = &wiki.summary {
                            p { class: "text-sm text-muted-foreground mb-2 line-clamp-3",
                                "{summary}"
                            }
                        }
                        p { class: "text-xs text-muted-foreground", "Click to view full article" }
                    }
                }
            }
        }
    }
}

/// Render a product minicard with HoverCard preview
pub(super) fn render_product_minicard(product: &Product, naddr: &str, _event: &Event) -> Element {
    let title = product.title.clone();
    let price_display = if product.price.is_sats() {
        Some(format!("{}", product.price.amount as u64))
    } else {
        None
    };
    let image_url = product
        .images
        .first()
        .map(|i| i.url.clone())
        .filter(|u| is_valid_http_url(u));
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::ShopProductDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 bg-card border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-10 h-10 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image_url {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center",
                            icons::ShoppingBagIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{title}" }
                    if let Some(ref price) = price_display {
                        p { class: "text-xs text-primary font-semibold", "⚡ {price} sats" }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-80 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = image_url {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-square object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold mb-1", "{title}" }
                        if let Some(ref price) = price_display {
                            p { class: "text-lg text-primary font-semibold", "⚡ {price} sats" }
                        }
                        if let Some(summary) = &product.summary {
                            p { class: "text-sm text-muted-foreground mt-2 line-clamp-2",
                                "{summary}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a badge definition minicard with HoverCard preview
pub(super) fn render_badge_minicard(badge: &BadgeDefinition, naddr: &str) -> Element {
    let name = badge.name.clone().unwrap_or_else(|| "Badge".to_string());
    let image_url = badge.image.clone().filter(|u| is_valid_http_url(u));
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::BadgeDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-8 h-8 rounded-full bg-accent/10 shrink-0 overflow-hidden flex items-center justify-center",
                    if let Some(ref img) = image_url {
                        img {
                            src: "{img}",
                            alt: "{name}",
                            class: "w-full h-full object-cover",
                        }
                    } else {
                        icons::DiscIcon { class: "w-4 h-4 text-accent".to_string() }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{name}" }
                    p { class: "text-xs text-muted-foreground", "Badge" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        div { class: "flex items-center gap-3 mb-2",
                            if let Some(ref img) = image_url {
                                img {
                                    src: "{img}",
                                    alt: "{name}",
                                    class: "w-12 h-12 rounded-full",
                                }
                            }
                            h4 { class: "font-bold", "{name}" }
                        }
                        if let Some(desc) = &badge.description {
                            p { class: "text-sm text-muted-foreground line-clamp-3",
                                "{desc}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a music track minicard with HoverCard preview
pub(super) fn render_track_minicard(track: &NostrTrack, _naddr: &str) -> Element {
    let title = track.title.clone();
    let image = track.image.clone().filter(|u| is_valid_http_url(u));
    let duration = track.duration.map(|d| {
        let mins = d / 60;
        let secs = d % 60;
        format!("{}:{:02}", mins, secs)
    });
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                div { class: "w-10 h-10 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center",
                            icons::MusicIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "🎵 {title}" }
                    if let Some(ref dur) = duration {
                        p { class: "text-xs text-muted-foreground", "{dur}" }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = image {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-square object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold", "{title}" }
                        if let Some(ref dur) = duration {
                            p { class: "text-sm text-muted-foreground", "Duration: {dur}" }
                        }
                        if !track.genres.is_empty() {
                            p { class: "text-xs text-muted-foreground mt-1",
                                "Genres: {track.genres.join(\", \")}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a playlist minicard with HoverCard preview
pub(super) fn render_playlist_minicard(playlist: &NostrPlaylist, naddr: &str) -> Element {
    let title = playlist.title.clone();
    let track_count = playlist.track_refs.len();
    let image = playlist.image.clone().filter(|u| is_valid_http_url(u));
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::MusicPlaylistDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-10 h-10 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-accent/20",
                            icons::MusicIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{title}" }
                    p { class: "text-xs text-muted-foreground", "{track_count} tracks" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = image {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-square object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold", "{title}" }
                        p { class: "text-sm text-muted-foreground", "{track_count} tracks" }
                        if let Some(desc) = &playlist.description {
                            p { class: "text-xs text-muted-foreground mt-1 line-clamp-2",
                                "{desc}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a recipe minicard with HoverCard preview
pub(super) fn render_recipe_minicard(
    meta: &RecipeMetadata,
    naddr: &str,
    _event: &Event,
) -> Element {
    let title = meta.title.clone();
    let image_url = meta
        .primary_image()
        .cloned()
        .filter(|u| is_valid_http_url(u));
    let summary = meta.summary.clone();
    let tags = meta.tags.clone();
    let naddr_owned = naddr.to_string();
    let displayed_tags: Vec<String> = tags.iter().take(2).cloned().collect();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::RecipeDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-12 h-12 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image_url {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-accent/20",
                            span { class: "text-lg", "🍳" }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "🍽️ {title}" }
                    if !displayed_tags.is_empty() {
                        p { class: "text-xs text-muted-foreground truncate",
                            "{displayed_tags.join(\", \")}"
                        }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = image_url {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-video object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold mb-1", "{title}" }
                        if let Some(ref sum) = summary {
                            p { class: "text-sm text-muted-foreground line-clamp-2",
                                "{sum}"
                            }
                        }
                        if !tags.is_empty() {
                            div { class: "flex flex-wrap gap-1 mt-2",
                                for tag in tags.iter().take(4) {
                                    span { class: "px-2 py-0.5 text-xs bg-primary/10 text-primary rounded-full",
                                        "{tag}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn extract_pgn_header(content: &str, key: &str) -> Option<String> {
    let pattern = format!("[{} \"", key);
    let start = content.find(&pattern)?;
    let val_start = start + pattern.len();
    let val_end = content[val_start..].find("\"]")?;
    Some(content[val_start..val_start + val_end].to_string())
}

pub(super) fn render_chess_pgn_minicard(event: &Event) -> Element {
    let content = &event.content;
    let white = extract_pgn_header(content, "White").unwrap_or_else(|| "?".to_string());
    let black = extract_pgn_header(content, "Black").unwrap_or_else(|| "?".to_string());
    let result_str = extract_pgn_header(content, "Result").unwrap_or_else(|| "*".to_string());
    let event_name = extract_pgn_header(content, "Event");

    let event_id_hex = event.id.to_hex();
    let address = note_route_id_with_kind(&event_id_hex, Some(&event.pubkey.to_hex()), Some(event.kind));

    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::AddressViewer { address },
                class: "block",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card hover:bg-accent/5 transition",
                    div { class: "w-10 h-10 rounded bg-muted shrink-0 flex items-center justify-center",
                        span { class: "text-lg", "\u{265F}" }
                    }
                    div { class: "flex-1 min-w-0",
                        p { class: "font-medium text-sm truncate",
                            if let Some(ref name) = event_name {
                                "\u{265F} {name}"
                            } else {
                                "\u{265F} Chess Game"
                            }
                        }
                        p { class: "text-xs text-muted-foreground truncate",
                            "{white} vs {black} — {result_str}"
                        }
                    }
                }
            }
        }
    }
}

/// Render a publication minicard with HoverCard preview
pub(super) fn render_publication_minicard(pub_index: &PublicationIndex, naddr: &str) -> Element {
    let title = pub_index.title.clone();
    let summary = pub_index.summary.clone();
    let cover_image = pub_index
        .cover_image
        .clone()
        .filter(|u| is_valid_http_url(u));
    let section_count = pub_index.section_addresses.len();
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::PublicationDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-12 h-16 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = cover_image {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-accent/20",
                            icons::BookOpenIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "📚 {title}" }
                    p { class: "text-xs text-muted-foreground", "{section_count} sections" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = cover_image {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-[2/1] object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold mb-1", "{title}" }
                        if let Some(ref sum) = summary {
                            p { class: "text-sm text-muted-foreground line-clamp-2",
                                "{sum}"
                            }
                        }
                        p { class: "text-xs text-muted-foreground mt-1", "{section_count} sections" }
                    }
                }
            }
        }
    }
}

/// Render a pinboard minicard with HoverCard preview
pub(super) fn render_pinboard_minicard(board: &Pinboard, naddr: &str) -> Element {
    let title = board.title.clone();
    let description = board.description.clone();
    let image = board.image.clone().filter(|u| is_valid_http_url(u));
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::PinBoardDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-10 h-10 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-accent/20",
                            icons::GridIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "📌 {title}" }
                    if !board.tags.is_empty() {
                        p { class: "text-xs text-muted-foreground truncate",
                            "{board.tags.join(\", \")}"
                        }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = image {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-square object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold", "{title}" }
                        if let Some(ref desc) = description {
                            p { class: "text-sm text-muted-foreground line-clamp-2",
                                "{desc}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a product collection minicard with HoverCard preview
pub(super) fn render_collection_minicard(collection: &ProductCollection, naddr: &str) -> Element {
    let title = collection.title.clone();
    let description = collection.description.clone();
    let product_count = collection.products.len();
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::ShopCollection {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-10 h-10 rounded bg-accent/20 shrink-0 flex items-center justify-center",
                    icons::ShoppingBagIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "🛍️ {title}" }
                    p { class: "text-xs text-muted-foreground", "{product_count} products" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        h4 { class: "font-bold mb-1", "{title}" }
                        p { class: "text-sm text-primary font-medium", "{product_count} products" }
                        if let Some(ref desc) = description {
                            p { class: "text-sm text-muted-foreground mt-1 line-clamp-2",
                                "{desc}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render a product review minicard with HoverCard preview
pub(super) fn render_review_minicard(review: &ProductReview, _naddr: &str) -> Element {
    let content = review.content.clone();
    let rating = review.thumb_rating;
    let rating_display = if rating >= 0.5 { "👍" } else { "👎" };
    let quality_str = review.quality_rating.map(|q| format!("{:.0}%", q * 100.0));
    let value_str = review.value_rating.map(|v| format!("{:.0}%", v * 100.0));
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                div { class: "w-8 h-8 rounded bg-muted shrink-0 flex items-center justify-center",
                    span { class: "text-lg", "{rating_display}" }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm", "Product Review" }
                    p { class: "text-xs text-muted-foreground truncate", "{content}" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        div { class: "flex items-center gap-2 mb-2",
                            span { class: "text-2xl", "{rating_display}" }
                            span { class: "font-bold", "Product Review" }
                        }
                        if !content.is_empty() {
                            p { class: "text-sm text-muted-foreground line-clamp-4",
                                "{content}"
                            }
                        }
                        if let Some(ref q) = quality_str {
                            p { class: "text-xs text-muted-foreground mt-1", "Quality: {q}" }
                        }
                        if let Some(ref v) = value_str {
                            p { class: "text-xs text-muted-foreground", "Value: {v}" }
                        }
                    }
                }
            }
        }
    }
}

/// Render a Git Issue minicard with HoverCard preview
pub(super) fn render_issue_minicard(issue: &Issue) -> Element {
    let title = issue.display_title();
    let status = issue.status;
    let status_class = match status {
        IssueStatus::Open => "bg-green-500/20 text-green-500",
        IssueStatus::Closed => "bg-red-500/20 text-red-500",
        IssueStatus::Applied => "bg-purple-500/20 text-purple-500",
        IssueStatus::Draft => "bg-yellow-500/20 text-yellow-500",
    };
    let status_text = format!("{:?}", status);
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                div { class: "w-8 h-8 rounded bg-muted shrink-0 flex items-center justify-center",
                    icons::CommentIcon { class: "w-4 h-4 text-green-500".to_string() }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "🔧 {title}" }
                    div { class: "flex items-center gap-2",
                        span { class: "px-1.5 py-0.5 text-xs rounded {status_class}",
                            "{status_text}"
                        }
                        if !issue.labels.is_empty() {
                            span { class: "text-xs text-muted-foreground truncate",
                                "{issue.labels.join(\", \")}"
                            }
                        }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        h4 { class: "font-bold mb-1", "{title}" }
                        span { class: "px-2 py-0.5 text-xs rounded {status_class}", "{status_text}" }
                        if !issue.labels.is_empty() {
                            div { class: "flex flex-wrap gap-1 mt-2",
                                for label in issue.labels.iter().take(4) {
                                    span { class: "px-2 py-0.5 text-xs bg-muted text-muted-foreground rounded-full",
                                        "{label}"
                                    }
                                }
                            }
                        }
                        p { class: "text-xs text-muted-foreground mt-2", "Git Issue" }
                    }
                }
            }
        }
    }
}

/// Render a Git PR minicard with HoverCard preview
pub(super) fn render_pr_minicard(pr: &PullRequest) -> Element {
    let title = if pr.is_cover_letter {
        pr.content
            .lines()
            .next()
            .unwrap_or("Pull Request")
            .to_string()
    } else {
        format!(
            "Patch: {}",
            pr.commit
                .as_deref()
                .unwrap_or("")
                .chars()
                .take(8)
                .collect::<String>(),
        )
    };
    let status = pr.status;
    let status_class = match status {
        IssueStatus::Open => "bg-green-500/20 text-green-500",
        IssueStatus::Closed => "bg-red-500/20 text-red-500",
        IssueStatus::Applied => "bg-purple-500/20 text-purple-500",
        IssueStatus::Draft => "bg-yellow-500/20 text-yellow-500",
    };
    let status_text = format!("{:?}", status);
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                div { class: "w-8 h-8 rounded bg-muted shrink-0 flex items-center justify-center",
                    icons::GitMergeIcon { class: "w-4 h-4 text-purple-500".to_string() }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "🔀 {title}" }
                    span { class: "px-1.5 py-0.5 text-xs rounded {status_class}", "{status_text}" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        h4 { class: "font-bold mb-1", "{title}" }
                        span { class: "px-2 py-0.5 text-xs rounded {status_class}", "{status_text}" }
                        if let Some(ref commit) = pr.commit {
                            p { class: "text-xs text-muted-foreground mt-2", "Commit: {commit}" }
                        }
                        p { class: "text-xs text-muted-foreground", "Git Patch/PR" }
                    }
                }
            }
        }
    }
}

/// Render a repost minicard
pub(super) fn render_repost_minicard(event: &Event) -> Element {
    let reposted_id = event
        .tags
        .iter()
        .find_map(|t| {
            if t.kind() == nostr_sdk::TagKind::e() {
                t.content().map(|s| s.to_string())
            } else {
                None
            }
        })
        .or_else(|| {
            if event.content.starts_with("nostr:") {
                Some(
                    event
                        .content
                        .strip_prefix("nostr:")
                        .unwrap_or(&event.content)
                        .to_string(),
                )
            } else {
                None
            }
        });
    let short_id = reposted_id
        .as_ref()
        .map(|id| {
            if id.len() > 16 {
                format!("{}...{}", &id[..8], &id[id.len() - 4..])
            } else {
                id.clone()
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                icons::Repeat2Icon { class: "w-4 h-4 text-green-500".to_string() }
                span { class: "text-sm text-muted-foreground", "Repost of " }
                span { class: "text-sm font-medium text-primary", "{short_id}" }
            }
        }
    }
}

/// Render a comment minicard (NIP-22)
pub(super) fn render_comment_minicard(event: &Event, metadata: Option<&Metadata>) -> Element {
    let content = &event.content;
    let display_content = if content.chars().count() > 100 {
        format!("{}...", content.chars().take(100).collect::<String>())
    } else {
        content.clone()
    };
    let author_name = metadata
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            let pk = event.pubkey.to_hex();
            format!("{}...{}", &pk[..8], &pk[pk.len() - 4..])
        });
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-start gap-2 p-2 border border-border rounded-lg bg-card",
                icons::MessageCircleIcon { class: "w-4 h-4 text-blue-500 shrink-0 mt-0.5".to_string() }
                div { class: "flex-1 min-w-0",
                    p { class: "text-xs text-muted-foreground", "Comment by {author_name}" }
                    p { class: "text-sm line-clamp-2", "{display_content}" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        p { class: "text-xs text-muted-foreground mb-1", "Comment by {author_name}" }
                        p { class: "text-sm", "{content}" }
                    }
                }
            }
        }
    }
}

/// Render a NIP-28 channel minicard with HoverCard preview
pub(super) fn render_channel_minicard(event: &Event, channel_id: &str) -> Element {
    if let Some(channel) = parse_channel_creation(event) {
        let (name, about, picture) = get_channel_display_info(&channel);
        let display_name = name.unwrap_or_else(|| "Unnamed Channel".to_string());
        let description = about.unwrap_or_default();
        let display_desc = if description.chars().count() > 120 {
            format!("{}...", description.chars().take(120).collect::<String>())
        } else {
            description.clone()
        };
        let channel_id_owned = event.id.to_hex();
        rsx! {
            div {
                class: "relative my-2",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                Link {
                    to: Route::ChatDetail {
                        channel_id: channel_id_owned.clone(),
                    },
                    class: "flex items-center gap-2 p-2 bg-card border border-border rounded-lg hover:bg-accent/50 transition",
                    div { class: "w-10 h-10 rounded-full bg-muted shrink-0 overflow-hidden flex items-center justify-center",
                        if let Some(ref pic) = picture.as_ref().filter(|u| is_valid_http_url(u)) {
                            img {
                                src: "{pic}",
                                alt: "{display_name}",
                                class: "w-full h-full object-cover",
                                loading: "lazy",
                            }
                        } else {
                            HashIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        p { class: "font-medium text-sm truncate", "{display_name}" }
                        if !display_desc.is_empty() {
                            p { class: "text-xs text-muted-foreground truncate", "{display_desc}" }
                        }
                    }
                }
                div { class: "absolute bottom-1 right-1",
                    HoverCard { open: Signal::new(None),
                        HoverCardTrigger { NostrBlueMiniLogo {} }
                        HoverCardContent {
                            side: ContentSide::Top,
                            class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                            div { class: "flex items-center gap-2 mb-2",
                                if let Some(ref pic) = picture.as_ref().filter(|u| is_valid_http_url(u)) {
                                    img {
                                        src: "{pic}",
                                        alt: "{display_name}",
                                        class: "w-10 h-10 rounded-full object-cover",
                                    }
                                }
                                h4 { class: "font-bold", "{display_name}" }
                            }
                            if !description.is_empty() {
                                p { class: "text-sm text-muted-foreground line-clamp-3",
                                    "{description}"
                                }
                            }
                            p { class: "text-xs text-muted-foreground mt-1", "Chat Channel" }
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            Link {
                to: Route::ChatDetail {
                    channel_id: channel_id.to_string(),
                },
                class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                HashIcon { class: "w-4 h-4" }
                "View Channel"
            }
        }
    }
}

/// Render a citation minicard with HoverCard preview
pub(super) fn render_citation_minicard(citation: &Citation) -> Element {
    let base = citation.base();
    let title = base.title.clone();
    let author = base.author.clone();
    let citation_type = citation.citation_type();
    let style = get_citation_style(&citation_type);
    let type_icon = style.emoji;
    let type_text = style.label;
    let type_color = style.text_class;
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                div { class: "w-8 h-8 rounded bg-muted shrink-0 flex items-center justify-center",
                    span { class: "text-sm", "{type_icon}" }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{title}" }
                    if !author.is_empty() {
                        p { class: "text-xs text-muted-foreground truncate", "by {author}" }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        div { class: "flex items-center gap-2 mb-2",
                            span { class: "text-lg", "{type_icon}" }
                            span { class: "text-xs font-medium {type_color}", "{type_text}" }
                        }
                        h4 { class: "font-bold mb-1", "{title}" }
                        if !author.is_empty() {
                            p { class: "text-sm text-muted-foreground", "by {author}" }
                        }
                        if let Some(ref summary) = base.summary {
                            p { class: "text-xs text-muted-foreground mt-2 line-clamp-3",
                                "{summary}"
                            }
                        }
                    }
                }
            }
        }
    }
}
