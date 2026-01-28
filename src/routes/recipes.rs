//! Recipes Explore Page
//! Browse and discover recipes

use dioxus::prelude::*;
use crate::stores::recipe_store::{self, CachedRecipe, TagWithCount};
use crate::stores::pin_boards_store::{self, Pinboard};
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::components::{
    CookbookCard, CookbookCardSkeleton, CreateCookbookModal,
    DiscoverRecipeCard, DiscoverRecipeCardSkeleton,
    RecipeTagChipExplore,
};
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;

const PAGE_SIZE: usize = 24;

#[component]
pub fn RecipesHome() -> Element {
    // Cookbooks state (pinboards tagged with "cookbook")
    let mut cookbooks = use_signal(Vec::<Pinboard>::new);
    let mut cookbooks_loading = use_signal(|| true);

    // Hot tags state
    let mut popular_tags = use_signal(Vec::<TagWithCount>::new);
    let mut tags_loading = use_signal(|| true);

    // Discover recipes state with pagination
    let mut discover_recipes = use_signal(Vec::<CachedRecipe>::new);
    let mut discover_loading = use_signal(|| true);
    let mut pagination_loading = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Create cookbook modal state
    let mut show_create_cookbook_modal = use_signal(|| false);

    // Search state
    let mut search_query = use_signal(String::new);
    let mut search_results = use_signal(|| None::<Vec<CachedRecipe>>);
    let mut search_loading = use_signal(|| false);
    let mut search_version = use_signal(|| 0u64);

    // Fetch all data on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        // Fetch cookbooks (pinboards tagged with "cookbook")
        spawn(async move {
            match pin_boards_store::fetch_cookbooks(10).await {
                Ok(books) => cookbooks.set(books),
                Err(e) => log::error!("Failed to fetch cookbooks: {}", e),
            }
            cookbooks_loading.set(false);
        });

        // Fetch popular tags
        spawn(async move {
            match recipe_store::compute_popular_tags(12).await {
                Ok(tags) => popular_tags.set(tags),
                Err(e) => log::error!("Failed to compute popular tags: {}", e),
            }
            tags_loading.set(false);
        });

        // Fetch discover recipes (initial load)
        spawn(async move {
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
                    discover_recipes.set(valid);
                }
                Err(e) => log::error!("Failed to fetch discover recipes: {}", e),
            }
            discover_loading.set(false);
        });
    });

    // Load more function for infinite scroll
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

                        // Deduplicate and append
                        let mut current = discover_recipes.peek().clone();
                        let existing_ids: std::collections::HashSet<_> = current
                            .iter()
                            .map(|r| r.event.id.to_hex())
                            .collect();

                        for recipe in valid {
                            if !existing_ids.contains(&recipe.event.id.to_hex()) {
                                current.push(recipe);
                            }
                        }

                        discover_recipes.set(current);

                        if fetched_count < PAGE_SIZE / 2 {
                            has_more.set(false);
                        }
                    }
                }
                Err(e) => log::error!("Failed to load more recipes: {}", e),
            }

            pagination_loading.set(false);
        });
    };

    // Setup infinite scroll
    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);

    // Debounced search effect
    use_effect(move || {
        let query = search_query.read().clone();

        if query.len() < 2 {
            search_results.set(None);
            search_loading.set(false);
            return;
        }

        let version = search_version.with_mut(|v| { *v += 1; *v });
        search_loading.set(true);

        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(300).await;

            if *search_version.peek() != version {
                return;
            }

            match recipe_store::search_recipes(&query, 50).await {
                Ok(results) => {
                    if *search_version.peek() == version {
                        search_results.set(Some(results));
                        search_loading.set(false);
                    }
                }
                Err(e) => {
                    log::error!("Search failed: {}", e);
                    if *search_version.peek() == version {
                        search_loading.set(false);
                    }
                }
            }
        });
    });

    let is_searching = search_query.read().len() >= 2;

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3",
                    div {
                        class: "flex items-center justify-between mb-3",
                        h1 {
                            class: "text-xl font-bold flex items-center gap-2",
                            span { class: "text-2xl", "🍳" }
                            "Recipes"
                        }
                        // Create Recipe button (only if signed in)
                        if *HAS_SIGNER.read() {
                            Link {
                                to: Route::RecipeNew {},
                                class: "px-4 py-2 bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg font-medium transition",
                                "+ New Recipe"
                            }
                        }
                    }
                    p {
                        class: "text-sm text-muted-foreground mb-3",
                        "Discover and share recipes on Nostr"
                    }

                    // Search
                    div {
                        class: "relative",
                        svg {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 w-5 h-5 text-muted-foreground",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            circle { cx: "11", cy: "11", r: "8" }
                            line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                        }
                        input {
                            class: "w-full pl-10 pr-4 py-2 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Search recipes...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value())
                        }
                        if *search_loading.read() {
                            div {
                                class: "absolute right-3 top-1/2 -translate-y-1/2",
                                span {
                                    class: "inline-block w-4 h-4 border-2 border-primary border-t-transparent rounded-full animate-spin"
                                }
                            }
                        }
                    }
                }
            }

            // Content
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                // Full skeleton loading
                div {
                    class: "p-4 space-y-8",

                    // Cookbooks skeleton
                    div {
                        div { class: "h-6 w-40 bg-muted rounded animate-pulse mb-4" }
                        div {
                            class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4",
                            for _ in 0..5 {
                                CookbookCardSkeleton {}
                            }
                        }
                    }

                    // Tags skeleton
                    div {
                        div { class: "h-6 w-32 bg-muted rounded animate-pulse mb-4" }
                        div {
                            class: "flex flex-wrap gap-2",
                            for _ in 0..10 {
                                div { class: "h-9 w-24 bg-muted rounded-full animate-pulse" }
                            }
                        }
                    }

                    // Discover skeleton
                    div {
                        div { class: "h-6 w-36 bg-muted rounded animate-pulse mb-4" }
                        div {
                            class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                            for _ in 0..12 {
                                DiscoverRecipeCardSkeleton {}
                            }
                        }
                    }
                }
            } else if is_searching {
                // Search results
                div {
                    class: "p-4",

                    if let Some(results) = search_results.read().as_ref() {
                        if results.is_empty() {
                            div {
                                class: "flex flex-col items-center justify-center py-12 text-center",
                                span { class: "text-5xl mb-4", "🔍" }
                                h3 { class: "text-lg font-medium mb-1", "No recipes found" }
                                p { class: "text-muted-foreground text-sm", "Try a different search term" }
                            }
                        } else {
                            h2 {
                                class: "text-lg font-semibold mb-4",
                                "Search Results ({results.len()})"
                            }
                            div {
                                class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                                for recipe in results.iter() {
                                    DiscoverRecipeCard {
                                        key: "{recipe.naddr}",
                                        recipe: recipe.clone()
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // Main explore content
                div {
                    class: "flex flex-col gap-8 p-4",

                    // Cookbooks
                    section {
                        class: "flex flex-col gap-4",

                        div {
                            class: "flex justify-between items-center",
                            h2 {
                                class: "text-2xl font-bold flex items-center gap-2",
                                span { "📚" }
                                span { "Cookbooks" }
                            }
                            // Create Cookbook button (only if signed in)
                            if *HAS_SIGNER.read() {
                                button {
                                    r#type: "button",
                                    class: "text-sm text-primary hover:underline flex items-center gap-1",
                                    onclick: move |_| show_create_cookbook_modal.set(true),
                                    span { "+" }
                                    "Create Cookbook"
                                }
                            }
                        }

                        if *cookbooks_loading.read() {
                            div {
                                class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4",
                                for _ in 0..5 {
                                    CookbookCardSkeleton {}
                                }
                            }
                        } else if cookbooks.read().is_empty() {
                            div {
                                class: "flex flex-col items-center justify-center py-8 text-center",
                                span { class: "text-4xl mb-2", "📚" }
                                p { class: "text-muted-foreground text-sm", "No cookbooks yet. Be the first to create one!" }
                            }
                        } else {
                            div {
                                class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4 scrollbar-hide",
                                for cookbook in cookbooks.read().iter() {
                                    CookbookCard {
                                        key: "{cookbook.naddr}",
                                        cookbook: cookbook.clone()
                                    }
                                }
                            }
                        }
                    }

                    // Hot Tags
                    section {
                        class: "flex flex-col gap-4",

                        div {
                            class: "flex justify-between items-center",
                            h2 {
                                class: "text-2xl font-bold flex items-center gap-2",
                                span { "⭐" }
                                span { "Hot Tags" }
                            }
                            Link {
                                to: Route::RecipesByTag { tag: "all".to_string() },
                                class: "text-sm text-primary hover:underline flex items-center gap-1",
                                span { "View all tags" }
                                span { "→" }
                            }
                        }

                        if *tags_loading.read() {
                            div {
                                class: "flex flex-wrap gap-2",
                                for _ in 0..10 {
                                    div { class: "h-9 w-24 bg-muted rounded-full animate-pulse" }
                                }
                            }
                        } else if !popular_tags.read().is_empty() {
                            div {
                                class: "flex flex-wrap gap-2",
                                for tag in popular_tags.read().iter().take(12) {
                                    {
                                        let unique_key = format!("hot-{}", tag.tag);
                                        rsx! {
                                            RecipeTagChipExplore {
                                                key: "{unique_key}",
                                                tag: tag.tag.clone(),
                                                count: Some(tag.count),
                                                clickable: true
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Discover New (with infinite scroll grid)
                    section {
                        class: "flex flex-col gap-4",

                        div {
                            class: "flex justify-between items-center",
                            h2 {
                                class: "text-2xl font-bold flex items-center gap-2",
                                span { "✨" }
                                span { "Discover New" }
                            }
                            Link {
                                to: Route::RecipesAll {},
                                class: "text-sm text-primary hover:underline flex items-center gap-1",
                                span { "View all recipes" }
                                span { "→" }
                            }
                        }

                        if *discover_loading.read() {
                            div {
                                class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-4 xl:grid-cols-5 gap-4",
                                for _ in 0..12 {
                                    DiscoverRecipeCardSkeleton {}
                                }
                            }
                        } else if discover_recipes.read().is_empty() {
                            div {
                                class: "flex flex-col items-center justify-center py-8 text-center",
                                span { class: "text-4xl mb-2", "🍳" }
                                p { class: "text-muted-foreground text-sm", "No recipes yet. Be the first to share one!" }
                            }
                        } else {
                            div {
                                class: "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-4 xl:grid-cols-5 gap-4",

                                for recipe in discover_recipes.read().iter() {
                                    DiscoverRecipeCard {
                                        key: "{recipe.event.id.to_hex()}",
                                        recipe: recipe.clone()
                                    }
                                }

                                // Loading more skeletons
                                if *pagination_loading.read() {
                                    for i in 0..6 {
                                        DiscoverRecipeCardSkeleton { key: "loading-{i}" }
                                    }
                                }
                            }

                            // Infinite scroll sentinel
                            if *has_more.read() {
                                div {
                                    id: "{sentinel_id}",
                                    class: "h-4"
                                }
                            }

                            // End of list indicator
                            if !*has_more.read() && !discover_recipes.read().is_empty() {
                                div {
                                    class: "text-center py-8 text-muted-foreground",
                                    "You've reached the end!"
                                }
                            }
                        }
                    }
                }
            }

            // Create Cookbook Modal
            if *show_create_cookbook_modal.read() {
                CreateCookbookModal {
                    on_close: move |_| show_create_cookbook_modal.set(false)
                }
            }
        }
    }
}
