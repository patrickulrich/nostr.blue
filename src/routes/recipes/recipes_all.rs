//! All Recipes Page
//! Displays all recipes with infinite scroll pagination
//! Uses DiscoverRecipeCard component for display
use crate::components::{DiscoverRecipeCard, DiscoverRecipeCardSkeleton};
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;
use crate::stores::nostr_client;
use crate::stores::recipe_store::{self, CachedRecipe};
use dioxus::prelude::*;
const PAGE_SIZE: usize = 24;
#[component]
pub fn RecipesAll() -> Element {
    let mut recipes = use_signal(Vec::<CachedRecipe>::new);
    let mut loading = use_signal(|| true);
    let mut pagination_loading = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        if !*nostr_client::CLIENT_INITIALIZED.read() {
            return;
        }
        spawn(async move {
            loading.set(true);
            error.set(None);
            match recipe_store::fetch_recipes(PAGE_SIZE, None).await {
                Ok(fetched_recipes) => {
                    let valid: Vec<CachedRecipe> = fetched_recipes
                        .into_iter()
                        .filter(|r| r.parsed.is_some())
                        .collect();
                    if let Some(oldest) = valid.last() {
                        oldest_timestamp.set(Some(oldest.event.created_at.as_secs()));
                    }
                    has_more.set(valid.len() >= PAGE_SIZE / 2);
                    recipes.set(valid);
                }
                Err(e) => {
                    log::error!("Failed to load recipes: {}", e);
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });
    let load_more = move || {
        if *pagination_loading.peek() || !*has_more.peek() {
            return;
        }
        let until = *oldest_timestamp.peek();
        spawn(async move {
            pagination_loading.set(true);
            match recipe_store::fetch_recipes(PAGE_SIZE, until).await {
                Ok(fetched_recipes) => {
                    let fetched_count = fetched_recipes.len();
                    let valid: Vec<CachedRecipe> = fetched_recipes
                        .into_iter()
                        .filter(|r| r.parsed.is_some())
                        .collect();
                    if valid.is_empty() {
                        has_more.set(false);
                    } else {
                        if let Some(oldest) = valid.last() {
                            oldest_timestamp.set(Some(oldest.event.created_at.as_secs()));
                        }
                        let mut current = recipes.peek().clone();
                        let existing_ids: std::collections::HashSet<_> =
                            current.iter().map(|r| r.event.id.to_hex()).collect();
                        for recipe in valid {
                            if !existing_ids.contains(&recipe.event.id.to_hex()) {
                                current.push(recipe);
                            }
                        }
                        recipes.set(current);
                        if fetched_count < PAGE_SIZE / 2 {
                            has_more.set(false);
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to load more recipes: {}", e);
                }
            }
            pagination_loading.set(false);
        });
    };
    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4",
                    div { class: "flex items-center justify-between",
                        div { class: "flex items-center gap-3",
                            Link {
                                to: Route::RecipesHome {},
                                class: "p-2 hover:bg-muted rounded-full transition-colors",
                                svg {
                                    class: "w-5 h-5",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M15 19l-7-7 7-7",
                                    }
                                }
                            }
                            h1 { class: "text-xl font-bold", "All Recipes" }
                        }
                        Link {
                            to: Route::RecipeNew {},
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-full font-medium hover:bg-primary/90 transition-colors flex items-center gap-2",
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                stroke_width: "2",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M12 4v16m8-8H4",
                                }
                            }
                            span { "New Recipe" }
                        }
                    }
                }
            }
            div { class: "p-4",
                if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-12",
                        p { class: "text-destructive mb-4", "Failed to load recipes: {err}" }
                        button {
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                            onclick: move |_| {
                                loading.set(true);
                                error.set(None);
                            },
                            "Try Again"
                        }
                    }
                } else if *loading.read() {
                    div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                        for i in 0..12 {
                            DiscoverRecipeCardSkeleton { key: "{i}" }
                        }
                    }
                } else if recipes.read().is_empty() {
                    div { class: "text-center py-12",
                        div { class: "text-6xl mb-4", "🍳" }
                        h2 { class: "text-xl font-semibold mb-2", "No recipes found" }
                        p { class: "text-muted-foreground mb-4", "Be the first to share a recipe!" }
                        Link {
                            to: Route::RecipeNew {},
                            class: "inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                            "Create Recipe"
                        }
                    }
                } else {
                    div { class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                        for recipe in recipes.read().iter() {
                            DiscoverRecipeCard {
                                key: "{recipe.event.id.to_hex()}",
                                recipe: recipe.clone(),
                            }
                        }
                        if *pagination_loading.read() {
                            for i in 0..6 {
                                DiscoverRecipeCardSkeleton { key: "loading-{i}" }
                            }
                        }
                    }
                    if *has_more.read() {
                        div { id: "{sentinel_id}", class: "h-4" }
                    }
                    if !*has_more.read() && !recipes.read().is_empty() {
                        div { class: "text-center py-8 text-muted-foreground",
                            "You've reached the end!"
                        }
                    }
                }
            }
        }
    }
}
