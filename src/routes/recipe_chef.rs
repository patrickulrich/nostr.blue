//! Recipe Chef Profile Page
//! Displays a chef's profile and their recipes

use crate::components::{RecipeCard, RecipeCardSkeleton};
use crate::hooks::use_author_metadata;
use crate::routes::Route;
use crate::stores::nostr_client;
use crate::stores::recipe_store::{self, CachedRecipe};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

#[component]
pub fn RecipeChef(npub: String) -> Element {
    let mut recipes = use_signal(Vec::<CachedRecipe>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Pagination
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut pagination_loading = use_signal(|| false);

    // Parse pubkey
    let pubkey_hex = use_memo(move || {
        // Handle both npub and hex formats
        if npub.starts_with("npub") {
            if let Ok(key) = PublicKey::from_bech32(&npub) {
                return Some(key.to_hex());
            }
        }
        // Try as hex
        if PublicKey::from_hex(&npub).is_ok() {
            return Some(npub.clone());
        }
        None
    });

    // Fetch chef metadata using the reusable hook
    let chef_metadata = use_author_metadata(pubkey_hex.read().clone().unwrap_or_default());

    // Fetch recipes on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        let Some(pubkey_str) = pubkey_hex.read().clone() else {
            error.set(Some("Invalid public key".to_string()));
            loading.set(false);
            return;
        };

        loading.set(true);
        error.set(None);

        spawn(async move {
            match recipe_store::fetch_recipes_by_author(&pubkey_str, 50, None).await {
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
    let load_more = move |_| {
        if *pagination_loading.peek() || !*has_more.peek() {
            return;
        }

        let Some(pubkey_str) = pubkey_hex.peek().clone() else {
            return;
        };

        pagination_loading.set(true);
        let until = *oldest_timestamp.peek();

        spawn(async move {
            match recipe_store::fetch_recipes_by_author(&pubkey_str, 50, until).await {
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
    };

    // Display values
    let display_name = chef_metadata
        .read()
        .as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            pubkey_hex
                .read()
                .clone()
                .map(|pk| truncate_pubkey(&pk))
                .unwrap_or_else(|| "Unknown Chef".to_string())
        });

    let profile_picture = chef_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone());

    let bio = chef_metadata.read().as_ref().and_then(|m| m.about.clone());

    let avatar_letter = display_name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    // Compute recipe count text for pluralization
    let chef_recipe_count = recipes.read().len();
    let chef_recipe_text = if chef_recipe_count == 1 {
        "recipe"
    } else {
        "recipes"
    };

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
                }
            }

            // Content
            if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && recipes.read().is_empty()) {
                // Skeleton loading state
                div {
                    class: "p-4",

                    // Chef profile skeleton
                    div {
                        class: "flex items-start gap-4 mb-8 p-4 bg-card border border-border rounded-xl animate-pulse",
                        div { class: "w-20 h-20 rounded-full bg-muted shrink-0" }
                        div {
                            class: "flex-1 space-y-3",
                            div { class: "h-6 w-48 bg-muted rounded" }
                            div { class: "h-4 w-full bg-muted rounded" }
                            div { class: "h-4 w-24 bg-muted rounded" }
                        }
                    }

                    // Recipe grid skeleton
                    div { class: "h-6 w-48 bg-muted rounded animate-pulse mb-4" }
                    div {
                        class: "grid gap-4 grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6",
                        for _ in 0..12 {
                            RecipeCardSkeleton {}
                        }
                    }
                }
            } else if pubkey_hex.read().is_none() {
                div {
                    class: "p-4",
                    div {
                        class: "flex flex-col items-center justify-center py-12 text-center",
                        span { class: "text-5xl mb-4", "🍽️" }
                        h2 {
                            class: "text-xl font-semibold mb-2",
                            "Chef Not Found"
                        }
                        p {
                            class: "text-muted-foreground mb-4",
                            "Invalid public key format"
                        }
                        Link {
                            to: Route::RecipesHome {},
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition",
                            "Browse Recipes"
                        }
                    }
                }
            } else {
                div {
                    class: "p-4",

                    // Chef profile card
                    div {
                        class: "flex items-start gap-4 mb-8 p-4 bg-card border border-border rounded-xl",

                        // Avatar
                        div {
                            class: "w-20 h-20 rounded-full overflow-hidden bg-muted flex items-center justify-center shrink-0",
                            if let Some(ref pic_url) = profile_picture {
                                img {
                                    src: "{pic_url}",
                                    alt: "{display_name}",
                                    class: "w-full h-full object-cover",
                                }
                            } else {
                                span {
                                    class: "text-3xl font-semibold text-muted-foreground",
                                    "{avatar_letter}"
                                }
                            }
                        }

                        div {
                            class: "flex-1 min-w-0",

                            h1 {
                                class: "text-xl font-bold flex items-center gap-2",
                                span { class: "text-2xl", "👨‍🍳" }
                                "{display_name}"
                            }

                            if let Some(ref about) = bio {
                                p {
                                    class: "text-muted-foreground mt-1 line-clamp-2",
                                    "{about}"
                                }
                            }

                            p {
                                class: "text-sm text-muted-foreground mt-2",
                                "{chef_recipe_count} {chef_recipe_text}"
                            }

                            // View full profile link
                            if let Some(ref pk) = *pubkey_hex.read() {
                                Link {
                                    to: Route::Profile { pubkey: pk.clone() },
                                    class: "text-sm text-primary hover:underline mt-2 inline-block",
                                    "View full profile →"
                                }
                            }
                        }
                    }

                    // Recipes section
                    h2 {
                        class: "text-lg font-semibold mb-4",
                        "Recipes by {display_name}"
                    }

                    if let Some(err) = error.read().as_ref() {
                        div {
                            class: "p-4 bg-destructive/10 text-destructive rounded-lg",
                            "{err}"
                        }
                    } else if recipes.read().is_empty() {
                        div {
                            class: "flex flex-col items-center justify-center py-12 text-center",
                            span { class: "text-5xl mb-4", "🍳" }
                            h3 {
                                class: "text-lg font-medium mb-1",
                                "No recipes yet"
                            }
                            p {
                                class: "text-muted-foreground text-sm",
                                "This chef hasn't published any recipes"
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
