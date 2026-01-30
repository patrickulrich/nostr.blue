//! Recipe Tag Chip Component
//! Displays a recipe category tag with optional emoji

use crate::routes::Route;
use crate::utils::recipe_tags::find_tag;
use dioxus::prelude::*;

/// A clickable tag chip for recipes with emoji support
#[component]
pub fn RecipeTagChip(
    /// The tag name (e.g., "Italian")
    tag: String,
    /// Whether clicking navigates to tag page
    #[props(default = true)]
    clickable: bool,
    /// Optional count to display
    #[props(default)]
    count: Option<usize>,
) -> Element {
    // Look up tag to get emoji
    let tag_info = find_tag(&tag);
    let emoji = tag_info.and_then(|t| t.emoji);
    let display_text = if let Some(e) = emoji {
        format!("{} {}", e, tag)
    } else {
        tag.clone()
    };

    let tag_slug = tag.to_lowercase().replace(' ', "-");

    if clickable {
        rsx! {
            Link {
                to: Route::RecipesByTag { tag: tag_slug },
                class: "inline-flex items-center gap-1 px-3 py-1.5 rounded-full bg-primary/10 text-primary text-sm font-medium hover:bg-primary/20 transition-colors",

                span { "{display_text}" }
                if let Some(c) = count {
                    span {
                        class: "text-xs opacity-70",
                        "({c})"
                    }
                }
            }
        }
    } else {
        rsx! {
            span {
                class: "inline-flex items-center gap-1 px-3 py-1.5 rounded-full bg-primary/10 text-primary text-sm font-medium",

                span { "{display_text}" }
                if let Some(c) = count {
                    span {
                        class: "text-xs opacity-70",
                        "({c})"
                    }
                }
            }
        }
    }
}

/// Small tag chip for use in cards
#[component]
pub fn RecipeTagChipSmall(tag: String, #[props(default = true)] clickable: bool) -> Element {
    let tag_info = find_tag(&tag);
    let emoji = tag_info.and_then(|t| t.emoji);
    let display = if let Some(e) = emoji {
        format!("{} {}", e, tag)
    } else {
        tag.clone()
    };

    let tag_slug = tag.to_lowercase().replace(' ', "-");

    if clickable {
        rsx! {
            Link {
                to: Route::RecipesByTag { tag: tag_slug },
                class: "px-2 py-0.5 text-xs rounded-full bg-primary/10 text-primary font-medium hover:bg-primary/20 transition-colors",
                "{display}"
            }
        }
    } else {
        rsx! {
            span {
                class: "px-2 py-0.5 text-xs rounded-full bg-primary/10 text-primary font-medium",
                "{display}"
            }
        }
    }
}

/// Tag chip with remove button for form editing
#[component]
pub fn RecipeTagChipEditable(tag: String, on_remove: EventHandler<String>) -> Element {
    let tag_info = find_tag(&tag);
    let emoji = tag_info.and_then(|t| t.emoji);
    let display = if let Some(e) = emoji {
        format!("{} {}", e, tag)
    } else {
        tag.clone()
    };

    let tag_for_remove = tag.clone();

    rsx! {
        span {
            class: "inline-flex items-center gap-1 pl-3 pr-1 py-1 rounded-full bg-primary/10 text-primary text-sm font-medium",

            span { "{display}" }

            button {
                r#type: "button",
                class: "p-1 hover:bg-primary/20 rounded-full transition-colors",
                onclick: move |_| on_remove.call(tag_for_remove.clone()),

                // X icon
                svg {
                    class: "w-3 h-3",
                    fill: "none",
                    stroke: "currentColor",
                    view_box: "0 0 24 24",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        stroke_width: "2",
                        d: "M6 18L18 6M6 6l12 12"
                    }
                }
            }
        }
    }
}

/// Tag chip for the explore page (matches frontend TagChip.svelte)
/// Uses bg-input styling with h-9 height
#[component]
pub fn RecipeTagChipExplore(
    tag: String,
    #[props(default)] count: Option<usize>,
    #[props(default = true)] clickable: bool,
) -> Element {
    let tag_info = find_tag(&tag);
    let emoji = tag_info.and_then(|t| t.emoji);

    let tag_slug = tag.to_lowercase().replace(' ', "-");

    if clickable {
        rsx! {
            Link {
                to: Route::RecipesByTag { tag: tag_slug },
                class: "inline-flex items-center gap-1.5 rounded-full px-3 py-2 bg-muted hover:bg-muted/80 transition duration-200 text-sm font-medium cursor-pointer h-9",

                if let Some(e) = emoji {
                    span {
                        class: "text-sm leading-none",
                        "{e}"
                    }
                }
                span { "{tag}" }
                if let Some(c) = count {
                    span {
                        class: "text-xs text-muted-foreground font-normal",
                        "· {c}"
                    }
                }
            }
        }
    } else {
        rsx! {
            span {
                class: "inline-flex items-center gap-1.5 rounded-full px-3 py-2 bg-muted text-sm font-medium h-9",

                if let Some(e) = emoji {
                    span {
                        class: "text-sm leading-none",
                        "{e}"
                    }
                }
                span { "{tag}" }
                if let Some(c) = count {
                    span {
                        class: "text-xs text-muted-foreground font-normal",
                        "· {c}"
                    }
                }
            }
        }
    }
}
