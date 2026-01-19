//! Recipe Tag Selector Component
//! Autocomplete tag input for selecting recipe categories

use dioxus::prelude::*;
use crate::utils::recipe_tags::{search_tags, CURATED_TAG_SECTIONS, get_section_tags};
use crate::components::recipe_tag_chip::RecipeTagChipEditable;

/// Tag selector with autocomplete and curated sections
#[component]
pub fn RecipeTagSelector(
    /// Currently selected tags
    selected_tags: Signal<Vec<String>>,
    /// Callback when tags change
    on_change: EventHandler<Vec<String>>,
    /// Maximum tags allowed (default 5)
    #[props(default = 5)]
    max_tags: usize,
) -> Element {
    let mut search_query = use_signal(String::new);
    let mut show_suggestions = use_signal(|| false);
    let mut show_browse = use_signal(|| false);

    // Search results
    let search_results = use_memo(move || {
        let query = search_query.read().clone();
        if query.len() >= 2 {
            search_tags(&query).into_iter().take(10).collect()
        } else {
            Vec::new()
        }
    });

    // Add a tag
    let mut add_tag = move |tag: String| {
        let mut current = selected_tags.read().clone();
        if !current.contains(&tag) && current.len() < max_tags {
            current.push(tag);
            on_change.call(current);
        }
        search_query.set(String::new());
        show_suggestions.set(false);
    };

    // Remove a tag
    let remove_tag = move |tag: String| {
        let mut current = selected_tags.read().clone();
        current.retain(|t| t != &tag);
        on_change.call(current);
    };

    let tags = selected_tags.read();
    let can_add = tags.len() < max_tags;
    let results = search_results.read();

    rsx! {
        div {
            class: "space-y-3",

            // Label
            div {
                class: "flex items-center justify-between",
                label {
                    class: "text-sm font-medium",
                    "Tags"
                }
                span {
                    class: "text-xs text-muted-foreground",
                    "{tags.len()}/{max_tags}"
                }
            }

            // Selected tags
            if !tags.is_empty() {
                div {
                    class: "flex flex-wrap gap-2",
                    for tag in tags.iter() {
                        RecipeTagChipEditable {
                            tag: tag.clone(),
                            on_remove: remove_tag,
                        }
                    }
                }
            }

            // Search input
            if can_add {
                div {
                    class: "relative",

                    input {
                        r#type: "text",
                        class: "w-full px-3 py-2 rounded-lg border border-border bg-background focus:outline-hidden focus:ring-2 focus:ring-primary text-sm",
                        placeholder: "Search tags (e.g., Italian, Vegan, Quick)...",
                        value: "{search_query}",
                        oninput: move |evt| {
                            search_query.set(evt.value().clone());
                            show_suggestions.set(true);
                        },
                        onfocus: move |_| show_suggestions.set(true),
                    }

                    // Suggestions dropdown
                    if *show_suggestions.read() && !results.is_empty() {
                        div {
                            class: "absolute z-50 w-full mt-1 py-1 bg-popover border border-border rounded-lg shadow-lg max-h-60 overflow-y-auto",

                            for tag in results.iter() {
                                {
                                    let tag_name = tag.name.to_string();
                                    let tag_name_for_click = tag_name.clone();
                                    let is_selected = tags.contains(&tag_name);
                                    let emoji = tag.emoji;

                                    rsx! {
                                        button {
                                            key: "{tag_name}",
                                            r#type: "button",
                                            class: format!(
                                                "w-full px-3 py-2 text-left hover:bg-accent transition text-sm flex items-center gap-2 {}",
                                                if is_selected { "opacity-50" } else { "" }
                                            ),
                                            disabled: is_selected,
                                            onclick: move |_| {
                                                add_tag(tag_name_for_click.clone());
                                            },

                                            if let Some(e) = emoji {
                                                span { "{e}" }
                                            }
                                            span { "{tag_name}" }
                                            if is_selected {
                                                span {
                                                    class: "ml-auto text-xs text-muted-foreground",
                                                    "Added"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Browse tags button
                button {
                    r#type: "button",
                    class: "text-sm text-primary hover:underline",
                    onclick: move |_| {
                        let current = *show_browse.peek();
                        show_browse.set(!current);
                    },

                    if *show_browse.read() {
                        "Hide tag browser"
                    } else {
                        "Browse all tags"
                    }
                }
            }

            // Tag browser (curated sections)
            if *show_browse.read() && can_add {
                div {
                    class: "border border-border rounded-lg p-4 space-y-4 max-h-80 overflow-y-auto",

                    for section in CURATED_TAG_SECTIONS.iter() {
                        {
                            // Get all RecipeTag objects for this section
                            let section_tags = get_section_tags(section.title).unwrap_or_default();

                            rsx! {
                                div {
                                    key: "{section.title}",

                                    h4 {
                                        class: "text-sm font-medium mb-2 flex items-center gap-2",
                                        span { "{section.emoji}" }
                                        span { "{section.title}" }
                                    }

                                    div {
                                        class: "flex flex-wrap gap-1.5",

                                        for tag in section_tags.iter() {
                                            {
                                                let tag_name = tag.name.to_string();
                                                let tag_name_for_click = tag_name.clone();
                                                let is_selected = tags.contains(&tag_name);
                                                let emoji = tag.emoji;

                                                rsx! {
                                                    button {
                                                        key: "{section.title}-{tag_name}",
                                                        r#type: "button",
                                                        class: format!(
                                                            "px-2 py-1 text-xs rounded-full transition {}",
                                                            if is_selected {
                                                                "bg-primary text-primary-foreground"
                                                            } else {
                                                                "bg-muted hover:bg-primary/20 text-muted-foreground hover:text-primary"
                                                            }
                                                        ),
                                                        disabled: is_selected,
                                                        onclick: move |_| {
                                                            if !is_selected {
                                                                add_tag(tag_name_for_click.clone());
                                                            }
                                                        },

                                                        if let Some(e) = emoji {
                                                            "{e} {tag_name}"
                                                        } else {
                                                            "{tag_name}"
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
                }
            }
        }
    }
}
