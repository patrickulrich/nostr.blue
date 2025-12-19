//! Recipes By Tag Page
//! Browse recipes filtered by category tag

use dioxus::prelude::*;
use crate::stores::recipe_store::{self, CachedRecipe};
use crate::stores::nostr_client;
use crate::components::{RecipeCard, RecipeCardSkeleton};
use crate::utils::recipe_tags::{find_tag_by_slug, CURATED_TAG_SECTIONS, RECIPE_TAGS};
use crate::routes::Route;

#[component]
pub fn RecipesByTag(tag: String) -> Element {
    // Clone tag for use in closures
    let tag_for_effect = tag.clone();
    let tag_for_load_more = tag.clone();
    let tag_for_display = tag.clone();

    let mut recipes = use_signal(|| Vec::<CachedRecipe>::new());
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Pagination
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut pagination_loading = use_signal(|| false);

    // Special case: "all" shows all tag sections
    let is_all_tags = tag == "all";

    // Get tag info
    let tag_info = find_tag_by_slug(&tag_for_display);
    let display_name = tag_info.map(|t| t.name).unwrap_or(&tag_for_display);
    let emoji = tag_info.and_then(|t| t.emoji);

    // Fetch recipes by tag on mount
    use_effect(move || {
        if is_all_tags {
            loading.set(false);
            return;
        }

        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        let tag_clone = tag_for_effect.clone();
        loading.set(true);
        error.set(None);

        spawn(async move {
            match recipe_store::fetch_recipes_by_tag(&tag_clone, 50, None).await {
                Ok(recipe_list) => {
                    if let Some(last) = recipe_list.last() {
                        oldest_timestamp.set(Some(last.event.created_at.as_secs()));
                    }
                    recipes.set(recipe_list);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Load more function
    let load_more = {
        let tag_clone = tag_for_load_more.clone();
        move |_| {
            if *pagination_loading.peek() || !*has_more.peek() || is_all_tags {
                return;
            }

            pagination_loading.set(true);
            let until = *oldest_timestamp.peek();
            let tag_for_fetch = tag_clone.clone();

            spawn(async move {
                match recipe_store::fetch_recipes_by_tag(&tag_for_fetch, 50, until).await {
                    Ok(new_recipes) => {
                        if new_recipes.is_empty() {
                            has_more.set(false);
                        } else {
                            if let Some(last) = new_recipes.last() {
                                oldest_timestamp.set(Some(last.event.created_at.as_secs()));
                            }
                            recipes.write().extend(new_recipes);
                        }
                        pagination_loading.set(false);
                    }
                    Err(e) => {
                        log::error!("Failed to load more recipes: {}", e);
                        pagination_loading.set(false);
                    }
                }
            });
        }
    };

    // Compute recipe count text for pluralization
    let recipe_count = recipes.read().len();
    let recipe_text = if recipe_count == 1 { "recipe" } else { "recipes" };

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3",

                    // Back link
                    Link {
                        to: Route::RecipesHome {},
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition mb-3",
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7"
                            }
                        }
                        "All Recipes"
                    }

                    h1 {
                        class: "text-xl font-bold flex items-center gap-2",
                        if is_all_tags {
                            span { class: "text-2xl", "🏷️" }
                            "Browse All Tags"
                        } else {
                            if let Some(e) = emoji {
                                span { class: "text-2xl", "{e}" }
                            }
                            "{display_name}"
                        }
                    }

                    if !is_all_tags {
                        p {
                            class: "text-sm text-muted-foreground mt-1",
                            "{recipe_count} {recipe_text}"
                        }
                    }
                }
            }

            // Content
            if is_all_tags {
                // All tags browser - show immediately (no loading needed)
                div {
                    class: "p-4 space-y-8",

                    for section in CURATED_TAG_SECTIONS.iter() {
                        div {
                            key: "{section.title}",

                            h2 {
                                class: "text-lg font-semibold mb-3 flex items-center gap-2",
                                span { class: "text-xl", "{section.emoji}" }
                                "{section.title}"
                            }

                            div {
                                class: "flex flex-wrap gap-2",

                                for tag_slug in section.tags.iter() {
                                    {
                                        let tag_info = find_tag_by_slug(tag_slug);
                                        let tag_name = tag_info.map(|t| t.name).unwrap_or(*tag_slug);

                                        rsx! {
                                            Link {
                                                key: "{tag_slug}",
                                                to: Route::RecipesByTag { tag: tag_slug.to_string() },
                                                class: "px-3 py-1.5 rounded-full bg-muted hover:bg-primary/20 text-sm transition",
                                                "{tag_name}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // All tags alphabetically
                    div {
                        h2 {
                            class: "text-lg font-semibold mb-3 flex items-center gap-2",
                            span { class: "text-xl", "📚" }
                            "All Tags (A-Z)"
                        }

                        div {
                            class: "flex flex-wrap gap-2",

                            for tag in RECIPE_TAGS.iter() {
                                Link {
                                    key: "{tag.slug}",
                                    to: Route::RecipesByTag { tag: tag.slug.to_string() },
                                    class: "px-3 py-1.5 rounded-full bg-muted hover:bg-primary/20 text-sm transition",

                                    if let Some(e) = tag.emoji {
                                        "{e} {tag.name}"
                                    } else {
                                        "{tag.name}"
                                    }
                                }
                            }
                        }
                    }
                }
            } else if !*nostr_client::CLIENT_INITIALIZED.read() || *loading.read() {
                // Skeleton grid for loading state
                div {
                    class: "p-4",
                    div {
                        class: "grid gap-4 grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6",
                        for _ in 0..12 {
                            RecipeCardSkeleton {}
                        }
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-4",
                    div {
                        class: "p-4 bg-destructive/10 text-destructive rounded-lg",
                        "{err}"
                    }
                }
            } else {
                div {
                    class: "p-4",

                    // Related tags
                    if let Some(section) = CURATED_TAG_SECTIONS.iter().find(|s| s.tags.contains(&tag.as_str())) {
                        div {
                            class: "mb-6",
                            h3 {
                                class: "text-sm font-medium text-muted-foreground mb-2",
                                "Related Tags"
                            }
                            div {
                                class: "flex flex-wrap gap-2",
                                for related_tag in section.tags.iter().filter(|t| **t != tag.as_str()).take(6) {
                                    Link {
                                        to: Route::RecipesByTag { tag: related_tag.to_string() },
                                        class: "px-2 py-1 rounded-full bg-muted hover:bg-primary/20 text-xs transition",
                                        "{related_tag}"
                                    }
                                }
                            }
                        }
                    }

                    if recipes.read().is_empty() {
                        div {
                            class: "flex flex-col items-center justify-center py-12 text-center",
                            span { class: "text-5xl mb-4", "🍽️" }
                            h3 {
                                class: "text-lg font-medium mb-1",
                                "No recipes found"
                            }
                            p {
                                class: "text-muted-foreground text-sm mb-4",
                                "No recipes with this tag yet"
                            }
                            Link {
                                to: Route::RecipesHome {},
                                class: "text-primary hover:underline",
                                "Browse all recipes"
                            }
                        }
                    } else {
                        // Recipe grid
                        div {
                            class: "grid gap-4 grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6",

                            for recipe in recipes.read().iter() {
                                RecipeCard {
                                    key: "{recipe.naddr}",
                                    recipe: recipe.clone()
                                }
                            }
                        }

                        // Load more
                        if *has_more.read() {
                            div {
                                class: "flex justify-center pt-6",
                                button {
                                    class: "px-6 py-3 bg-muted hover:bg-muted/80 rounded-lg font-medium transition disabled:opacity-50",
                                    disabled: *pagination_loading.read(),
                                    onclick: load_more,

                                    if *pagination_loading.read() {
                                        span {
                                            class: "inline-block w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin mr-2"
                                        }
                                        "Loading..."
                                    } else {
                                        "Load More"
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
