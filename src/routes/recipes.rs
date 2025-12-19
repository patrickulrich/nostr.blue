//! Recipes Explore Page
//! Browse and discover recipes

use dioxus::prelude::*;
use crate::stores::recipe_store::{
    self, CachedRecipe, PopularChef, TagWithCount, Collection
};
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::components::{
    CollectionCard, CollectionCardSkeleton,
    PopularChefAvatar, PopularChefAvatarSkeleton,
    DiscoverRecipeCard, DiscoverRecipeCardSkeleton,
    RecipeTagChipExplore, TagSectionCard,
};
use crate::utils::recipe_tags::CURATED_TAG_SECTIONS;
use crate::routes::Route;

#[component]
pub fn RecipesHome() -> Element {
    // Collections state
    let mut collections = use_signal(Vec::<Collection>::new);
    let mut collections_loading = use_signal(|| true);

    // Popular chefs state
    let mut popular_chefs = use_signal(Vec::<PopularChef>::new);
    let mut chefs_loading = use_signal(|| true);

    // Discover recipes state
    let mut discover_recipes = use_signal(Vec::<CachedRecipe>::new);
    let mut discover_loading = use_signal(|| true);

    // Hot tags state
    let mut popular_tags = use_signal(Vec::<TagWithCount>::new);
    let mut tags_loading = use_signal(|| true);

    // Culture section expand state
    let mut culture_expanded = use_signal(|| false);

    // Search state (keeping from original)
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

        // Fetch collections
        spawn(async move {
            let result = recipe_store::fetch_collections_with_images().await;
            collections.set(result);
            collections_loading.set(false);
        });

        // Fetch popular chefs
        spawn(async move {
            match recipe_store::fetch_popular_chefs(12).await {
                Ok(chefs) => {
                    popular_chefs.set(chefs);
                }
                Err(e) => {
                    log::error!("Failed to fetch popular chefs: {}", e);
                }
            }
            chefs_loading.set(false);
        });

        // Fetch discover recipes
        spawn(async move {
            match recipe_store::fetch_discover_recipes(12).await {
                Ok(recipes) => {
                    discover_recipes.set(recipes);
                }
                Err(e) => {
                    log::error!("Failed to fetch discover recipes: {}", e);
                }
            }
            discover_loading.set(false);
        });

        // Fetch popular tags
        spawn(async move {
            match recipe_store::compute_popular_tags(12).await {
                Ok(tags) => {
                    popular_tags.set(tags);
                }
                Err(e) => {
                    log::error!("Failed to compute popular tags: {}", e);
                }
            }
            tags_loading.set(false);
        });
    });

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

    // Get curated sections
    let intent_section = CURATED_TAG_SECTIONS.iter()
        .find(|s| s.title == "Why are you cooking?");
    let culture_section = CURATED_TAG_SECTIONS.iter()
        .find(|s| s.title == "Explore by culture");
    let collapsible_sections: Vec<_> = CURATED_TAG_SECTIONS.iter()
        .filter(|s| s.title != "Why are you cooking?" && s.title != "Explore by culture")
        .collect();

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
                            class: "w-full pl-10 pr-4 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-primary",
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

                    // Collections skeleton
                    div {
                        div { class: "h-6 w-40 bg-muted rounded animate-pulse mb-4" }
                        div {
                            class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4",
                            for _ in 0..5 {
                                CollectionCardSkeleton {}
                            }
                        }
                    }

                    // Chefs skeleton
                    div {
                        div { class: "h-6 w-36 bg-muted rounded animate-pulse mb-4" }
                        div {
                            class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4",
                            for _ in 0..6 {
                                PopularChefAvatarSkeleton {}
                            }
                        }
                    }

                    // Discover skeleton
                    div {
                        div { class: "h-6 w-32 bg-muted rounded animate-pulse mb-4" }
                        div {
                            class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4",
                            for _ in 0..6 {
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
                                class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4 scrollbar-hide",
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

                    // Top Collections
                    section {
                        class: "flex flex-col gap-4",

                        h2 {
                            class: "text-2xl font-bold flex items-center gap-2",
                            span { "📚" }
                            span { "Top Collections" }
                        }

                        if *collections_loading.read() {
                            div {
                                class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4",
                                for _ in 0..5 {
                                    CollectionCardSkeleton {}
                                }
                            }
                        } else if !collections.read().is_empty() {
                            div {
                                class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4 scrollbar-hide",
                                for collection in collections.read().iter() {
                                    CollectionCard {
                                        key: "{collection.id}",
                                        title: collection.title.clone(),
                                        subtitle: collection.subtitle.clone(),
                                        image_url: collection.image_url.clone(),
                                        on_click: {
                                            let tag = collection.tag.clone();
                                            move |_| {
                                                let nav = navigator();
                                                nav.push(Route::RecipesByTag { tag: tag.clone() });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Popular Chefs
                    section {
                        class: "flex flex-col gap-4",

                        h2 {
                            class: "text-2xl font-bold flex items-center gap-2",
                            span { "👨‍🍳" }
                            span { "Popular Chefs" }
                        }

                        if *chefs_loading.read() {
                            div {
                                class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4",
                                for _ in 0..6 {
                                    PopularChefAvatarSkeleton {}
                                }
                            }
                        } else if !popular_chefs.read().is_empty() {
                            div {
                                class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4 scrollbar-hide",
                                for chef in popular_chefs.read().iter() {
                                    PopularChefAvatar {
                                        key: "{chef.pubkey}",
                                        pubkey: chef.pubkey.clone()
                                    }
                                }
                            }
                        }
                    }

                    // Discover New
                    section {
                        class: "flex flex-col gap-4",

                        h2 {
                            class: "text-2xl font-bold flex items-center gap-2",
                            span { "✨" }
                            span { "Discover New" }
                        }

                        if *discover_loading.read() {
                            div {
                                class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4",
                                for _ in 0..6 {
                                    DiscoverRecipeCardSkeleton {}
                                }
                            }
                        } else if !discover_recipes.read().is_empty() {
                            div {
                                class: "flex gap-4 overflow-x-auto pb-2 -mx-4 px-4 scrollbar-hide",
                                for recipe in discover_recipes.read().iter() {
                                    DiscoverRecipeCard {
                                        key: "{recipe.naddr}",
                                        recipe: recipe.clone()
                                    }
                                }
                            }
                        }
                    }

                    // Hot Tags
                    section {
                        class: "flex flex-col gap-4",

                        h2 {
                            class: "text-2xl font-bold flex items-center gap-2",
                            span { "⭐" }
                            span { "Hot Tags" }
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

                    // Explore More
                    div {
                        class: "flex flex-col gap-6 pt-4 border-t",

                        h2 {
                            class: "text-2xl font-bold flex items-center gap-2",
                            span { "🔍" }
                            span { "Explore More" }
                        }

                        // Intent Section (always expanded)
                        if let Some(section) = intent_section {
                            TagSectionCard {
                                emoji: section.emoji.to_string(),
                                title: section.title.to_string(),
                                helper_text: Some("Browse by intention, not ingredients.".to_string()),
                                tags: section.tags.iter().take(8).map(|s| s.to_string()).collect(),
                                always_expanded: true,
                                preview_count: 8
                            }
                        }

                        // Culture Section (custom with expand toggle)
                        if let Some(section) = culture_section {
                            div {
                                class: "rounded-xl border border-border bg-card shadow-sm p-5 md:p-6 transition-all duration-300",

                                div {
                                    class: "flex items-start justify-between gap-4 mb-4",

                                    div {
                                        class: "flex-1",
                                        h2 {
                                            class: "text-2xl font-bold flex items-center gap-2 mb-1.5 text-foreground",
                                            span { "{section.emoji}" }
                                            span { "{section.title}" }
                                        }
                                    }

                                    if section.tags.len() > 10 {
                                        button {
                                            r#type: "button",
                                            class: "flex-shrink-0 text-sm text-primary hover:text-primary/80 transition-colors font-medium",
                                            onclick: move |_| {
                                                let current = *culture_expanded.peek();
                                                culture_expanded.set(!current);
                                            },

                                            if *culture_expanded.read() {
                                                "Show less"
                                            } else {
                                                "Show all cultures"
                                            }
                                        }
                                    }
                                }

                                div {
                                    class: "flex flex-wrap gap-2 transition-all duration-300",

                                    {
                                        let tags_to_show = if *culture_expanded.read() {
                                            section.tags.len()
                                        } else {
                                            10
                                        };

                                        section.tags.iter().take(tags_to_show).map(|tag| {
                                            let unique_key = format!("culture-{}", tag);
                                            rsx! {
                                                RecipeTagChipExplore {
                                                    key: "{unique_key}",
                                                    tag: tag.to_string(),
                                                    clickable: true
                                                }
                                            }
                                        })
                                    }
                                }
                            }
                        }

                        // Collapsible sections
                        for section in collapsible_sections.iter() {
                            TagSectionCard {
                                key: "{section.title}",
                                emoji: section.emoji.to_string(),
                                title: section.title.to_string(),
                                tags: section.tags.iter().map(|s| s.to_string()).collect(),
                                always_expanded: false,
                                preview_count: 8
                            }
                        }

                        // View all links
                        div {
                            class: "pt-4 border-t flex flex-wrap gap-4",

                            Link {
                                to: Route::RecipesAll {},
                                class: "inline-flex items-center gap-2 text-primary hover:underline font-medium",

                                span { "View all recipes" }
                                span { "→" }
                            }

                            Link {
                                to: Route::RecipesByTag { tag: "all".to_string() },
                                class: "inline-flex items-center gap-2 text-primary hover:underline font-medium",

                                span { "View all tags" }
                                span { "→" }
                            }
                        }
                    }
                }
            }
        }
    }
}
