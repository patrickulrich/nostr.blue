//! Cookbook Card Component
//! Displays a cookbook (pinboard tagged with "cookbook") with background image and title
//! Similar to CollectionCard but uses Pinboard data and links to PinBoardDetail

use dioxus::prelude::*;
use crate::stores::pin_boards_store::Pinboard;
use crate::routes::Route;
use crate::utils::is_valid_http_url;

/// Escape a URL for use in CSS url() function
fn escape_css_url(url: &str) -> String {
    url.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace(')', "\\)")
        .replace(['\n', '\r', '"'], "")
}

/// Cookbook card for the recipes explore page
#[component]
pub fn CookbookCard(cookbook: Pinboard) -> Element {
    // Validate URL before using in CSS to prevent injection
    let bg_style = match &cookbook.image {
        Some(url) if is_valid_http_url(url) => {
            let escaped = escape_css_url(url);
            format!("background-image: url('{}'); background-size: cover; background-position: center;", escaped)
        }
        _ => String::new(),
    };

    let naddr = cookbook.naddr.clone();

    rsx! {
        Link {
            to: Route::PinBoardDetail { naddr },
            class: "flex-shrink-0 w-64 h-40 rounded-xl overflow-hidden relative group cursor-pointer transition-transform duration-200 hover:scale-[1.02]",

            // Background Image or gradient fallback
            div {
                class: "absolute inset-0 bg-gradient-to-br from-orange-500/60 to-amber-600/60",
                style: "{bg_style}",

                // Dark overlay for text readability
                div {
                    class: "absolute inset-0 bg-black/30 group-hover:bg-black/20 transition-colors"
                }
            }

            // Content at bottom
            div {
                class: "relative h-full flex flex-col justify-end p-4 text-white",

                h3 {
                    class: "text-lg font-bold mb-1 truncate",
                    "{cookbook.title}"
                }
                if let Some(ref desc) = cookbook.description {
                    p {
                        class: "text-sm text-white/90 truncate",
                        "{desc}"
                    }
                }
            }
        }
    }
}

/// Skeleton loader for cookbook cards
#[component]
pub fn CookbookCardSkeleton() -> Element {
    rsx! {
        div {
            class: "flex-shrink-0 w-64 h-40 bg-muted rounded-xl animate-pulse"
        }
    }
}
