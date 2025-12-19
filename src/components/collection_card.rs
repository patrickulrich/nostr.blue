//! Collection Card Component
//! Displays a recipe collection with background image, title, and subtitle
//! Matches ~/frontend CollectionCard.svelte (w-64 h-40)

use dioxus::prelude::*;

/// Collection card for the explore page
#[component]
pub fn CollectionCard(
    title: String,
    subtitle: String,
    #[props(default)] image_url: Option<String>,
    on_click: EventHandler<()>,
) -> Element {
    let bg_style = if let Some(ref url) = image_url {
        format!("background-image: url('{}'); background-size: cover; background-position: center;", url)
    } else {
        String::new()
    };

    rsx! {
        button {
            r#type: "button",
            class: "flex-shrink-0 w-64 h-40 rounded-xl overflow-hidden relative group cursor-pointer transition-transform duration-200 hover:scale-[1.02]",
            onclick: move |_| on_click.call(()),

            // Background Image or gradient fallback
            div {
                class: "absolute inset-0 bg-gradient-to-br from-primary/20 to-primary/40",
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
                    class: "text-lg font-bold mb-1",
                    "{title}"
                }
                p {
                    class: "text-sm text-white/90",
                    "{subtitle}"
                }
            }
        }
    }
}

/// Skeleton loader for collection cards
#[component]
pub fn CollectionCardSkeleton() -> Element {
    rsx! {
        div {
            class: "flex-shrink-0 w-64 h-40 bg-muted rounded-xl animate-pulse"
        }
    }
}
