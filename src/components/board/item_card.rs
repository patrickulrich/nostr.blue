//! Pin Card Component
//! Displays individual pins (Kind 39067) with content type-specific rendering
use super::pin_menu::PinToBoardRequest;
use crate::components::PinMenu;
use crate::routes::Route;
use crate::stores::pin_boards_store::{Pin, PinContentType, PinMetadata, PinReference};
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::{FromBech32, ToBech32};
/// Convert an address to naddr format.
/// Supports both coordinate format ("kind:pubkey:d-tag") and naddr format.
/// Returns the original address if conversion fails.
fn to_naddr(address: &str) -> String {
    if address.starts_with("naddr1") {
        return address.to_string();
    }
    if let Ok(coord) = Coordinate::parse(address) {
        if let Ok(naddr) = coord.to_bech32() {
            return naddr;
        }
    }
    address.to_string()
}
/// Parse a coordinate from address supporting both formats.
fn parse_coordinate(address: &str) -> Option<Coordinate> {
    if address.starts_with("naddr1") {
        Coordinate::from_bech32(address).ok()
    } else {
        Coordinate::parse(address).ok()
    }
}
/// Icon indicator for content type
#[component]
fn ContentTypeIcon(content_type: PinContentType) -> Element {
    let (icon, label) = match content_type {
        PinContentType::Text => ("📝", "Text"),
        PinContentType::Link => ("🔗", "Link"),
        PinContentType::Image => ("🖼️", "Image"),
        PinContentType::Video => ("🎬", "Video"),
        PinContentType::Profile => ("👤", "Profile"),
        PinContentType::Note => ("📄", "Note"),
        PinContentType::Recipe => ("🍳", "Recipe"),
        PinContentType::Community => ("👥", "Community"),
        PinContentType::CodeRepo => ("💻", "Repository"),
        PinContentType::Podcast => ("🎙️", "Podcast"),
        PinContentType::Music => ("🎵", "Music"),
        PinContentType::CalendarEvent => ("📅", "Event"),
        PinContentType::Article => ("📰", "Article"),
        PinContentType::LiveStream => ("📺", "Live Stream"),
        PinContentType::Badge => ("🏅", "Badge"),
        PinContentType::Pinboard => ("📌", "Pinboard"),
        PinContentType::Book => ("📚", "Book"),
        PinContentType::Location => ("📍", "Location"),
    };
    rsx! {
        span { class: "text-xs", title: "{label}", "{icon}" }
    }
}
/// Card display for a single pin (Kind 39067)
#[component]
pub fn PinCard(
    pin: Pin,
    /// Whether the current user owns this pin (can delete)
    #[props(default = false)]
    is_owner: bool,
    #[props(default)] on_delete: Option<EventHandler<String>>,
    /// Optional content type override (from fetching the referenced event)
    /// Used when Kind 30023 needs to be distinguished between article/recipe
    #[props(default)]
    content_type_override: Option<PinContentType>,
    /// Optional metadata from the referenced event (title, image, summary)
    #[props(default)]
    metadata: Option<PinMetadata>,
    /// Optional callback when "Pin to Board" is requested (lifts modal to parent)
    #[props(default)]
    on_pin_to_board: Option<EventHandler<PinToBoardRequest>>,
) -> Element {
    let content_type = metadata
        .as_ref()
        .and_then(|m| m.content_type.clone())
        .or(content_type_override)
        .unwrap_or_else(|| pin.content_type());
    let title = metadata
        .as_ref()
        .and_then(|m| m.title.clone())
        .or(pin.title.clone())
        .unwrap_or_else(|| content_type.display_name().to_string());
    let image_url = metadata.as_ref().and_then(|m| m.image.clone());
    let description = metadata
        .as_ref()
        .and_then(|m| m.summary.clone())
        .or_else(|| {
            if pin.content.is_empty() {
                None
            } else {
                Some(pin.content.clone())
            }
        });
    let pin_for_menu = pin.clone();
    let (display_ref, item_route) = match &pin.reference {
        PinReference::Event { id, .. } => (
            id.clone(),
            Some(Route::Nip19Handler {
                identifier: id.clone(),
            }),
        ),
        PinReference::Coordinate { address, .. } => {
            let naddr = to_naddr(address);
            let route = match content_type {
                PinContentType::Recipe => Some(Route::RecipeDetail {
                    naddr: naddr.clone(),
                }),
                PinContentType::Community => Some(Route::CommunityPage {
                    a_tag: address.clone(),
                }),
                PinContentType::CodeRepo => Some(Route::CodeRepo {
                    naddr: naddr.clone(),
                }),
                PinContentType::CalendarEvent => Some(Route::CalendarEventDetail {
                    naddr: naddr.clone(),
                    from: None,
                }),
                PinContentType::Article => Some(Route::ArticleDetail {
                    naddr: naddr.clone(),
                }),
                PinContentType::LiveStream => Some(Route::LiveStreamDetail {
                    note_id: naddr.clone(),
                }),
                PinContentType::Badge => Some(Route::BadgeDetail {
                    naddr: naddr.clone(),
                }),
                PinContentType::Pinboard => Some(Route::PinBoardDetail {
                    naddr: naddr.clone(),
                }),
                PinContentType::Profile => parse_coordinate(address).map(|coord| Route::Profile {
                    pubkey: coord.public_key.to_hex(),
                }),
                _ => None,
            };
            (address.clone(), route)
        }
        PinReference::External { content, .. } => (content.to_string(), None),
    };
    rsx! {
        div { class: "group relative bg-card rounded-lg border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-md",
            if let Some(route) = item_route {
                Link { to: route, class: "block",
                    PinContent {
                        content_type: content_type.clone(),
                        title: title.clone(),
                        description: description.clone(),
                        reference: display_ref.clone(),
                        image: image_url.clone(),
                    }
                }
            } else if is_valid_http_url(&display_ref) {
                a {
                    href: "{display_ref}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    class: "block",
                    PinContent {
                        content_type: content_type.clone(),
                        title: title.clone(),
                        description: description.clone(),
                        reference: display_ref.clone(),
                        image: image_url.clone(),
                    }
                }
            } else {
                div { class: "block cursor-not-allowed", title: "Invalid URL",
                    PinContent {
                        content_type: content_type.clone(),
                        title: title.clone(),
                        description: description.clone(),
                        reference: display_ref.clone(),
                        image: image_url.clone(),
                    }
                }
            }
            div { class: "absolute top-2 left-2 px-2 py-0.5 rounded-full bg-background/80 backdrop-blur-sm",
                ContentTypeIcon { content_type: content_type.clone() }
            }
            div { class: "absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity bg-background/80 backdrop-blur-sm rounded-full",
                PinMenu {
                    pin: pin_for_menu.clone(),
                    is_owner,
                    on_delete,
                    on_pin_to_board,
                }
            }
        }
    }
}
/// Internal remove button component (kept for PinGrid backward compatibility)
#[component]
fn RemoveButton(pin: Pin, on_remove: EventHandler<Pin>) -> Element {
    rsx! {
        button {
            class: "absolute top-2 right-2 p-1.5 rounded-full bg-red-500 text-white opacity-0 group-hover:opacity-100 transition-opacity hover:bg-red-600",
            title: "Remove pin",
            onclick: move |evt| {
                evt.stop_propagation();
                on_remove.call(pin.clone());
            },
            svg {
                class: "w-3 h-3",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                line {
                    x1: "18",
                    y1: "6",
                    x2: "6",
                    y2: "18",
                }
                line {
                    x1: "6",
                    y1: "6",
                    x2: "18",
                    y2: "18",
                }
            }
        }
    }
}
/// Inner content rendering for pins
#[component]
fn PinContent(
    content_type: PinContentType,
    title: String,
    description: Option<String>,
    reference: String,
    /// Cover image from the referenced event's metadata
    #[props(default)]
    image: Option<String>,
) -> Element {
    let is_image_type = matches!(content_type, PinContentType::Image);
    let is_external = matches!(
        content_type,
        PinContentType::Link
            | PinContentType::Video
            | PinContentType::Podcast
            | PinContentType::Music
    );
    let display_image = if is_image_type {
        if is_valid_http_url(&reference) {
            Some(reference.clone())
        } else {
            None
        }
    } else {
        image.as_ref().filter(|url| is_valid_http_url(url)).cloned()
    };
    rsx! {
        if let Some(img_url) = display_image {
            div { class: "w-full",
                div { class: "w-full aspect-video bg-muted overflow-hidden",
                    img {
                        src: "{img_url}",
                        alt: "{title}",
                        class: "w-full h-full object-cover",
                        loading: "lazy",
                    }
                }
                div { class: "p-3",
                    h4 { class: "font-semibold text-sm line-clamp-2 group-hover:text-primary transition-colors",
                        "{title}"
                    }
                    if let Some(ref desc) = description {
                        p { class: "text-xs text-muted-foreground line-clamp-2 mt-1",
                            "{desc}"
                        }
                    }
                }
            }
        } else {
            div { class: "w-full aspect-video bg-muted overflow-hidden flex items-center justify-center",
                PinPlaceholder { content_type: content_type.clone() }
            }
            div { class: "p-3",
                h4 { class: "font-semibold text-sm line-clamp-2 group-hover:text-primary transition-colors",
                    "{title}"
                }
                if let Some(ref desc) = description {
                    p { class: "text-xs text-muted-foreground line-clamp-2 mt-1",
                        "{desc}"
                    }
                }
                if is_external {
                    p { class: "text-xs text-muted-foreground mt-2 flex items-center gap-1",
                        svg {
                            class: "w-3 h-3",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                            path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                        }
                        span { class: "truncate", {extract_domain(&reference)} }
                    }
                }
            }
        }
    }
}
/// Placeholder content when no image is available
#[component]
fn PinPlaceholder(content_type: PinContentType) -> Element {
    let (icon, gradient) = match content_type {
        PinContentType::Text => ("📝", "from-gray-400/20 to-gray-500/10"),
        PinContentType::Link => ("🔗", "from-blue-400/20 to-blue-500/10"),
        PinContentType::Image => ("🖼️", "from-green-400/20 to-green-500/10"),
        PinContentType::Video => ("🎬", "from-red-400/20 to-red-500/10"),
        PinContentType::Profile => ("👤", "from-purple-400/20 to-purple-500/10"),
        PinContentType::Note => ("📄", "from-amber-400/20 to-amber-500/10"),
        PinContentType::Recipe => ("🍳", "from-orange-400/20 to-orange-500/10"),
        PinContentType::Community => ("👥", "from-indigo-400/20 to-indigo-500/10"),
        PinContentType::CodeRepo => ("💻", "from-emerald-400/20 to-emerald-500/10"),
        PinContentType::Podcast => ("🎙️", "from-pink-400/20 to-pink-500/10"),
        PinContentType::Music => ("🎵", "from-violet-400/20 to-violet-500/10"),
        PinContentType::CalendarEvent => ("📅", "from-cyan-400/20 to-cyan-500/10"),
        PinContentType::Article => ("📰", "from-slate-400/20 to-slate-500/10"),
        PinContentType::LiveStream => ("📺", "from-rose-400/20 to-rose-500/10"),
        PinContentType::Badge => ("🏅", "from-yellow-400/20 to-yellow-500/10"),
        PinContentType::Pinboard => ("📌", "from-primary/20 to-primary/10"),
        PinContentType::Book => ("📚", "from-amber-400/20 to-amber-500/10"),
        PinContentType::Location => ("📍", "from-teal-400/20 to-teal-500/10"),
    };
    rsx! {
        div { class: "w-full h-full bg-gradient-to-br {gradient} flex items-center justify-center",
            span { class: "text-4xl", "{icon}" }
        }
    }
}
/// Extract domain from URL for display
fn extract_domain(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}
/// Loading skeleton for PinCard
#[component]
pub fn PinCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse",
            div { class: "w-full aspect-video bg-muted" }
            div { class: "p-3 space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-full mt-2" }
            }
        }
    }
}
use std::collections::HashMap;
/// Grid layout for pins (standard responsive grid)
#[component]
pub fn PinGrid(
    pins: Vec<Pin>,
    /// Whether the current user owns these pins (can delete)
    #[props(default = false)]
    is_owner: bool,
    #[props(default)] on_delete: Option<EventHandler<String>>,
    #[props(default = false)] loading: bool,
    #[props(default = 8)] skeleton_count: usize,
    /// Content type overrides by pin event_id (for Kind 30023 article/recipe disambiguation)
    /// DEPRECATED: Use metadata_map instead for full metadata including image/title
    #[props(default)]
    content_type_overrides: HashMap<String, PinContentType>,
    /// Full metadata by pin event_id (includes content_type, title, image, summary)
    #[props(default)]
    metadata_map: HashMap<String, PinMetadata>,
    /// Optional callback when "Pin to Board" is requested (lifts modal to parent)
    #[props(default)]
    on_pin_to_board: Option<EventHandler<PinToBoardRequest>>,
) -> Element {
    rsx! {
        div { class: "grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 auto-rows-auto",
            for pin in pins.iter() {
                {
                    let meta = metadata_map.get(&pin.event_id).cloned();
                    let override_type = content_type_overrides.get(&pin.event_id).cloned();
                    rsx! {
                        PinCard {
                            key: "{pin.event_id}",
                            pin: pin.clone(),
                            is_owner,
                            on_delete,
                            content_type_override: override_type,
                            metadata: meta,
                            on_pin_to_board,
                        }
                    }
                }
            }
            if loading {
                for i in 0..skeleton_count {
                    PinCardSkeleton { key: "skeleton-{i}" }
                }
            }
        }
    }
}
/// Masonry-style mosaic card for pins with natural image aspect ratios
/// Designed for CSS columns masonry layout
#[component]
pub fn PinCardMosaic(
    pin: Pin,
    /// Whether the current user owns this pin (can delete)
    #[props(default = false)]
    is_owner: bool,
    #[props(default)] on_delete: Option<EventHandler<String>>,
    #[props(default)] content_type_override: Option<PinContentType>,
    #[props(default)] metadata: Option<PinMetadata>,
    /// Size variant for cards without images (small, medium, large)
    #[props(default)]
    size_variant: Option<String>,
    /// Optional callback when "Pin to Board" is requested (lifts modal to parent)
    #[props(default)]
    on_pin_to_board: Option<EventHandler<PinToBoardRequest>>,
) -> Element {
    let content_type = metadata
        .as_ref()
        .and_then(|m| m.content_type.clone())
        .or(content_type_override)
        .unwrap_or_else(|| pin.content_type());
    let title = metadata
        .as_ref()
        .and_then(|m| m.title.clone())
        .or(pin.title.clone())
        .unwrap_or_else(|| content_type.display_name().to_string());
    let image_url = metadata.as_ref().and_then(|m| m.image.clone());
    let description = metadata
        .as_ref()
        .and_then(|m| m.summary.clone())
        .or_else(|| {
            if pin.content.is_empty() {
                None
            } else {
                Some(pin.content.clone())
            }
        });
    let pin_for_menu = pin.clone();
    let (display_ref, item_route) = match &pin.reference {
        PinReference::Event { id, .. } => (
            id.clone(),
            Some(Route::Nip19Handler {
                identifier: id.clone(),
            }),
        ),
        PinReference::Coordinate { address, .. } => {
            let naddr = to_naddr(address);
            let route = match content_type {
                PinContentType::Recipe => Some(Route::RecipeDetail {
                    naddr: naddr.clone(),
                }),
                PinContentType::Community => Some(Route::CommunityPage {
                    a_tag: address.clone(),
                }),
                PinContentType::CodeRepo => Some(Route::CodeRepo {
                    naddr: naddr.clone(),
                }),
                PinContentType::CalendarEvent => Some(Route::CalendarEventDetail {
                    naddr: naddr.clone(),
                    from: None,
                }),
                PinContentType::Article => Some(Route::ArticleDetail {
                    naddr: naddr.clone(),
                }),
                PinContentType::LiveStream => Some(Route::LiveStreamDetail {
                    note_id: naddr.clone(),
                }),
                PinContentType::Badge => Some(Route::BadgeDetail {
                    naddr: naddr.clone(),
                }),
                PinContentType::Pinboard => Some(Route::PinBoardDetail {
                    naddr: naddr.clone(),
                }),
                PinContentType::Profile => parse_coordinate(address).map(|coord| Route::Profile {
                    pubkey: coord.public_key.to_hex(),
                }),
                _ => None,
            };
            (address.clone(), route)
        }
        PinReference::External { content, .. } => (content.to_string(), None),
    };
    let placeholder_height = match size_variant.as_deref() {
        Some("small") => "h-32",
        Some("large") => "h-64",
        _ => "h-48",
    };
    rsx! {
        div { class: "group relative bg-card rounded-lg border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-lg break-inside-avoid mb-3",
            if let Some(route) = item_route {
                Link { to: route, class: "block",
                    PinMosaicContent {
                        content_type: content_type.clone(),
                        title: title.clone(),
                        description: description.clone(),
                        reference: display_ref.clone(),
                        image: image_url.clone(),
                        placeholder_height: placeholder_height.to_string(),
                    }
                }
            } else if is_valid_http_url(&display_ref) {
                a {
                    href: "{display_ref}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    class: "block",
                    PinMosaicContent {
                        content_type: content_type.clone(),
                        title: title.clone(),
                        description: description.clone(),
                        reference: display_ref.clone(),
                        image: image_url.clone(),
                        placeholder_height: placeholder_height.to_string(),
                    }
                }
            } else {
                div { class: "block cursor-not-allowed", title: "Invalid URL",
                    PinMosaicContent {
                        content_type: content_type.clone(),
                        title: title.clone(),
                        description: description.clone(),
                        reference: display_ref.clone(),
                        image: image_url.clone(),
                        placeholder_height: placeholder_height.to_string(),
                    }
                }
            }
            div { class: "absolute top-2 left-2 px-2 py-0.5 rounded-full bg-background/80 backdrop-blur-sm shadow-xs",
                ContentTypeIcon { content_type: content_type.clone() }
            }
            div { class: "absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity bg-background/80 backdrop-blur-sm rounded-full shadow-xs",
                PinMenu {
                    pin: pin_for_menu.clone(),
                    is_owner,
                    on_delete,
                    on_pin_to_board,
                }
            }
        }
    }
}
/// Inner content for mosaic pin cards with natural image aspect ratios
#[component]
fn PinMosaicContent(
    content_type: PinContentType,
    title: String,
    description: Option<String>,
    reference: String,
    image: Option<String>,
    placeholder_height: String,
) -> Element {
    let is_image_type = matches!(content_type, PinContentType::Image);
    let is_external = matches!(
        content_type,
        PinContentType::Link
            | PinContentType::Video
            | PinContentType::Podcast
            | PinContentType::Music
    );
    let display_image = if is_image_type {
        if is_valid_http_url(&reference) {
            Some(reference.clone())
        } else {
            None
        }
    } else {
        image.as_ref().filter(|url| is_valid_http_url(url)).cloned()
    };
    rsx! {
        if let Some(img_url) = display_image {
            div { class: "w-full",
                div { class: "w-full bg-muted overflow-hidden",
                    img {
                        src: "{img_url}",
                        alt: "{title}",
                        class: "w-full h-auto object-cover",
                        loading: "lazy",
                    }
                }
                div { class: "p-3",
                    h4 { class: "font-semibold text-sm line-clamp-2 group-hover:text-primary transition-colors",
                        "{title}"
                    }
                    if let Some(ref desc) = description {
                        p { class: "text-xs text-muted-foreground line-clamp-3 mt-1",
                            "{desc}"
                        }
                    }
                }
            }
        } else {
            div { class: "w-full {placeholder_height} bg-muted overflow-hidden flex items-center justify-center",
                PinPlaceholder { content_type: content_type.clone() }
            }
            div { class: "p-3",
                h4 { class: "font-semibold text-sm line-clamp-2 group-hover:text-primary transition-colors",
                    "{title}"
                }
                if let Some(ref desc) = description {
                    p { class: "text-xs text-muted-foreground line-clamp-3 mt-1",
                        "{desc}"
                    }
                }
                if is_external {
                    p { class: "text-xs text-muted-foreground mt-2 flex items-center gap-1",
                        svg {
                            class: "w-3 h-3",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }
                            path { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }
                        }
                        span { class: "truncate", {extract_domain(&reference)} }
                    }
                }
            }
        }
    }
}
/// Loading skeleton for PinCardMosaic with varied heights
#[component]
pub fn PinCardMosaicSkeleton(#[props(default)] height_variant: Option<String>) -> Element {
    let height_class = match height_variant.as_deref() {
        Some("small") => "h-32",
        Some("large") => "h-64",
        _ => "h-48",
    };
    rsx! {
        div { class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse break-inside-avoid mb-3",
            div { class: "w-full {height_class} bg-muted" }
            div { class: "p-3 space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-3/4" }
                if matches!(height_variant.as_deref(), Some("large")) {
                    div { class: "h-3 bg-muted rounded w-full mt-1" }
                    div { class: "h-3 bg-muted rounded w-2/3" }
                }
            }
        }
    }
}
/// Size variants for visual variety in masonry layout
const PIN_SIZE_VARIANTS: [&str; 3] = ["small", "medium", "large"];
/// Masonry grid layout for pins using CSS columns
/// Uses CSS columns for true masonry layout with varied card heights
#[component]
pub fn PinMosaicGrid(
    pins: Vec<Pin>,
    /// Whether the current user owns these pins (can delete)
    #[props(default = false)]
    is_owner: bool,
    #[props(default)] on_delete: Option<EventHandler<String>>,
    #[props(default = false)] loading: bool,
    #[props(default = 8)] skeleton_count: usize,
    #[props(default)] content_type_overrides: HashMap<String, PinContentType>,
    #[props(default)] metadata_map: HashMap<String, PinMetadata>,
    /// Callback when more items should be loaded
    #[props(default)]
    on_load_more: Option<EventHandler<()>>,
    /// Whether there are more items to load
    #[props(default = true)]
    has_more: bool,
    /// Whether currently loading more
    #[props(default = false)]
    loading_more: bool,
    /// Optional callback when "Pin to Board" is requested (lifts modal to parent)
    #[props(default)]
    on_pin_to_board: Option<EventHandler<PinToBoardRequest>>,
) -> Element {
    rsx! {
        div { class: "columns-1 sm:columns-2 md:columns-3 lg:columns-4 xl:columns-5 gap-3",
            for (idx , pin) in pins.iter().enumerate() {
                {
                    let meta = metadata_map.get(&pin.event_id).cloned();
                    let override_type = content_type_overrides.get(&pin.event_id).cloned();
                    let has_image = meta.as_ref().and_then(|m| m.image.as_ref()).is_some();
                    rsx! {
                        PinCardMosaic {
                            key: "{pin.event_id}",
                            pin: pin.clone(),
                            is_owner,
                            on_delete,
                            content_type_override: override_type,
                            metadata: meta,
                            size_variant: if has_image { None } else { Some(PIN_SIZE_VARIANTS[idx % 3].to_string()) },
                            on_pin_to_board,
                        }
                    }
                }
            }
            if loading {
                for i in 0..skeleton_count {
                    PinCardMosaicSkeleton {
                        key: "mosaic-skeleton-{i}",
                        height_variant: Some(PIN_SIZE_VARIANTS[i % 3].to_string()),
                    }
                }
            }
        }
        if !loading && has_more && on_load_more.is_some() {
            div { class: "flex justify-center py-8",
                if loading_more {
                    div { class: "flex items-center gap-2 text-muted-foreground",
                        div { class: "w-5 h-5 border-2 border-primary border-t-transparent rounded-full animate-spin" }
                        span { "Loading more..." }
                    }
                } else {
                    button {
                        class: "px-6 py-3 bg-primary/10 hover:bg-primary/20 text-primary rounded-full font-medium transition-colors",
                        onclick: move |_| {
                            if let Some(ref handler) = on_load_more {
                                handler.call(());
                            }
                        },
                        "Load more pins"
                    }
                }
            }
        }
    }
}
