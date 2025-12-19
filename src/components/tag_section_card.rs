//! Tag Section Card Component
//! Expandable card displaying a category of recipe tags
//! Matches ~/frontend TagSectionCard.svelte

use dioxus::prelude::*;
use crate::components::RecipeTagChipExplore;

/// Expandable tag section card for the explore page
#[component]
pub fn TagSectionCard(
    emoji: String,
    title: String,
    #[props(default)] helper_text: Option<String>,
    tags: Vec<String>,
    #[props(default = false)] always_expanded: bool,
    #[props(default = 8)] preview_count: usize,
) -> Element {
    let mut expanded = use_signal(move || always_expanded);
    let show_toggle = tags.len() > preview_count && !always_expanded;

    // Create unique key prefix from title
    let key_prefix = title.to_lowercase().replace(' ', "-");

    let displayed_tags: Vec<(String, String)> = if *expanded.read() || always_expanded {
        tags.iter().map(|t| (format!("{}-{}", key_prefix, t), t.clone())).collect()
    } else {
        tags.iter().take(preview_count).map(|t| (format!("{}-{}", key_prefix, t), t.clone())).collect()
    };

    rsx! {
        div {
            class: "rounded-xl border border-border bg-card shadow-sm p-5 md:p-6 transition-all duration-300",

            // Header
            div {
                class: "flex items-start justify-between gap-4 mb-4",

                div {
                    class: "flex-1",

                    h2 {
                        class: "text-2xl font-bold flex items-center gap-2 mb-1.5 text-foreground",
                        span { "{emoji}" }
                        span { "{title}" }
                    }

                    if let Some(ref text) = helper_text {
                        p {
                            class: "text-sm text-muted-foreground mt-0.5",
                            "{text}"
                        }
                    }
                }

                // Toggle button
                if show_toggle {
                    button {
                        r#type: "button",
                        class: "flex-shrink-0 text-sm text-primary hover:text-primary/80 transition-colors font-medium",
                        onclick: move |_| {
                            let current = *expanded.peek();
                            expanded.set(!current);
                        },

                        if *expanded.read() {
                            "Show less"
                        } else {
                            "Show more"
                        }
                    }
                }
            }

            // Tags
            div {
                class: "flex flex-wrap gap-2 transition-all duration-300",

                for (key, tag) in displayed_tags {
                    RecipeTagChipExplore {
                        key: "{key}",
                        tag: tag,
                        clickable: true
                    }
                }
            }
        }
    }
}
