//! Cookbook Card Component
//! Displays a cookbook (pinboard tagged with "cookbook") with background image and title
//! Similar to CollectionCard but uses Pinboard data and links to PinBoardDetail

use crate::routes::Route;
use crate::stores::pin_boards_store::Pinboard;
use crate::utils::css_safe_url;
use dioxus::prelude::*;

/// Cookbook card for the recipes explore page
#[component]
pub fn CookbookCard(cookbook: Pinboard) -> Element {
    // Validate URL before using in CSS to prevent injection
    // css_safe_url rejects dangerous characters rather than trying to escape them
    let bg_style = match &cookbook.image {
        Some(url) => css_safe_url(url)
            .map(|safe_url| format!("background-image: url('{}'); background-size: cover; background-position: center;", safe_url))
            .unwrap_or_default(),
        None => String::new(),
    };

    let naddr = cookbook.naddr.clone();

    rsx! {
        Link {
            to: Route::PinBoardDetail { naddr },
            class: "shrink-0 w-64 h-40 rounded-xl overflow-hidden relative group cursor-pointer transition-transform duration-200 hover:scale-[1.02]",

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
            class: "shrink-0 w-64 h-40 bg-muted rounded-xl animate-pulse"
        }
    }
}
