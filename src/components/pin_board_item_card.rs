//! Pin Card Component
//! Displays individual pins (Kind 39067) with content type-specific rendering

use dioxus::prelude::*;

use crate::routes::Route;
use crate::stores::pin_boards_store::{Pin, PinContentType, PinReference};

// ============================================================================
// Content Type Icon Component
// ============================================================================

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
    };

    rsx! {
        span {
            class: "text-xs",
            title: "{label}",
            "{icon}"
        }
    }
}

// ============================================================================
// Main PinCard Component
// ============================================================================

/// Card display for a single pin (Kind 39067)
#[component]
pub fn PinCard(
    pin: Pin,
    #[props(default = false)]
    show_remove: bool,
    #[props(default)]
    on_remove: Option<EventHandler<Pin>>,
) -> Element {
    let content_type = pin.content_type();
    let title = pin.display_title();
    let content = pin.content.clone();
    let pin_for_remove = pin.clone();

    // Get display reference and determine link target
    let (display_ref, item_route) = match &pin.reference {
        PinReference::Event { id, .. } => {
            // Note reference - link to note page
            (id.clone(), Some(Route::Nip19Handler { identifier: id.clone() }))
        }
        PinReference::Coordinate { address, .. } => {
            // Addressable event - determine route from kind
            let route = match content_type {
                PinContentType::Recipe => Some(Route::RecipeDetail { naddr: address.clone() }),
                PinContentType::Community => Some(Route::CommunityPage { a_tag: address.clone() }),
                PinContentType::CodeRepo => Some(Route::CodeRepo { naddr: address.clone() }),
                PinContentType::CalendarEvent => Some(Route::CalendarEventDetail { naddr: address.clone(), from: None }),
                PinContentType::Article => Some(Route::ArticleDetail { naddr: address.clone() }),
                PinContentType::LiveStream => Some(Route::LiveStreamDetail { note_id: address.clone() }),
                PinContentType::Badge => Some(Route::BadgeDetail { naddr: address.clone() }),
                PinContentType::Pinboard => Some(Route::PinBoardDetail { naddr: address.clone() }),
                PinContentType::Profile => Some(Route::Profile { pubkey: address.clone() }),
                _ => None,
            };
            (address.clone(), route)
        }
        PinReference::External { url } => {
            // External URL - no internal route
            (url.clone(), None)
        }
    };

    let should_show_remove = show_remove && on_remove.is_some();

    rsx! {
        div {
            class: "group relative bg-card rounded-lg border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-md",

            // Main content (linked or external)
            if let Some(route) = item_route {
                Link {
                    to: route,
                    class: "block",
                    PinContent {
                        content_type: content_type.clone(),
                        title: title.clone(),
                        description: if content.is_empty() { None } else { Some(content.clone()) },
                        reference: display_ref.clone(),
                    }
                }
            } else {
                // External link
                a {
                    href: "{display_ref}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    class: "block",
                    PinContent {
                        content_type: content_type.clone(),
                        title: title.clone(),
                        description: if content.is_empty() { None } else { Some(content.clone()) },
                        reference: display_ref.clone(),
                    }
                }
            }

            // Remove button overlay (visible on hover for owners)
            if should_show_remove {
                RemoveButton {
                    pin: pin_for_remove,
                    on_remove: on_remove.unwrap(),
                }
            }

            // Content type badge
            div {
                class: "absolute top-2 left-2 px-2 py-0.5 rounded-full bg-background/80 backdrop-blur-sm",
                ContentTypeIcon { content_type: content_type.clone() }
            }
        }
    }
}

/// Internal remove button component
#[component]
fn RemoveButton(
    pin: Pin,
    on_remove: EventHandler<Pin>,
) -> Element {
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
                line { x1: "18", y1: "6", x2: "6", y2: "18" }
                line { x1: "6", y1: "6", x2: "18", y2: "18" }
            }
        }
    }
}

// ============================================================================
// Pin Content Component
// ============================================================================

/// Inner content rendering for pins
#[component]
fn PinContent(
    content_type: PinContentType,
    title: String,
    description: Option<String>,
    reference: String,
) -> Element {
    let is_image = matches!(content_type, PinContentType::Image);
    let is_external = matches!(
        content_type,
        PinContentType::Link | PinContentType::Video | PinContentType::Podcast | PinContentType::Music
    );

    rsx! {
        if is_image {
            // Image display
            div {
                class: "w-full",
                img {
                    src: "{reference}",
                    alt: "{title}",
                    class: "w-full h-auto max-h-80 object-contain bg-muted",
                    loading: "lazy",
                }
                div {
                    class: "p-2",
                    p {
                        class: "text-xs text-muted-foreground line-clamp-2",
                        "{title}"
                    }
                }
            }
        } else {
            // Standard content display
            div {
                class: "w-full aspect-video bg-muted overflow-hidden flex items-center justify-center",
                PinPlaceholder { content_type: content_type.clone() }
            }

            div {
                class: "p-3",

                h4 {
                    class: "font-semibold text-sm line-clamp-2 group-hover:text-primary transition-colors",
                    "{title}"
                }

                if let Some(ref desc) = description {
                    p {
                        class: "text-xs text-muted-foreground line-clamp-2 mt-1",
                        "{desc}"
                    }
                }

                // Domain hint for external links
                if is_external {
                    p {
                        class: "text-xs text-muted-foreground mt-2 flex items-center gap-1",
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
                        span {
                            class: "truncate",
                            {extract_domain(&reference)}
                        }
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
    };

    rsx! {
        div {
            class: "w-full h-full bg-gradient-to-br {gradient} flex items-center justify-center",
            span {
                class: "text-4xl",
                "{icon}"
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

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

// ============================================================================
// Skeleton Loader
// ============================================================================

/// Loading skeleton for PinCard
#[component]
pub fn PinCardSkeleton() -> Element {
    rsx! {
        div {
            class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse",

            // Image skeleton
            div {
                class: "w-full aspect-video bg-muted",
            }

            // Content skeleton
            div {
                class: "p-3 space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-full mt-2" }
            }
        }
    }
}

// ============================================================================
// Pin Grid
// ============================================================================

/// Grid layout for pins
#[component]
pub fn PinGrid(
    pins: Vec<Pin>,
    #[props(default = false)]
    show_remove: bool,
    #[props(default)]
    on_remove: Option<EventHandler<Pin>>,
    #[props(default = false)]
    loading: bool,
    #[props(default = 8)]
    skeleton_count: usize,
) -> Element {
    rsx! {
        div {
            class: "grid gap-4 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 auto-rows-auto",

            // Show pins
            for pin in pins.iter() {
                PinCard {
                    key: "{pin.event_id}",
                    pin: pin.clone(),
                    show_remove: show_remove,
                    on_remove: on_remove.clone(),
                }
            }

            // Show skeletons while loading
            if loading {
                for i in 0..skeleton_count {
                    PinCardSkeleton { key: "skeleton-{i}" }
                }
            }
        }
    }
}
