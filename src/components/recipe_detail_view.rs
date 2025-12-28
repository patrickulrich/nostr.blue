//! Recipe Detail View Component
//! Displays a full recipe with all sections, engagement, and actions

use dioxus::prelude::*;
use crate::hooks::use_author_metadata;
use crate::routes::Route;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::auth_store;
use crate::stores::recipe_store::CachedRecipe;
use crate::components::recipe_tag_chip::RecipeTagChip;
use crate::components::AddToCookbookModal;
use crate::utils::time::format_relative_time;
use crate::utils::truncate_pubkey;

/// Full recipe detail view
#[component]
pub fn RecipeDetailView(recipe: CachedRecipe) -> Element {
    let title = recipe.metadata.title.clone();
    let image_url = recipe.metadata.primary_image().cloned();
    let summary = recipe.metadata.summary.clone();
    let tags = recipe.metadata.tags.clone();
    let naddr = recipe.naddr.clone();
    let created_at = recipe.event.created_at;
    let created_at_str = format_relative_time(created_at);

    let author_pubkey = recipe.event.pubkey.to_hex();
    let author_pubkey_for_link = author_pubkey.clone();

    // Check if current user owns this recipe
    let is_owner = use_memo({
        let author_pk = author_pubkey.clone();
        move || {
            if let Some(pubkey) = auth_store::get_pubkey() {
                return author_pk == pubkey;
            }
            false
        }
    });

    // Share state (share_copied mutated in wasm32 only)
    #[allow(unused_mut)]
    let mut share_copied = use_signal(|| false);
    #[allow(unused_variables)]
    let naddr_for_share = naddr.clone();
    let naddr_for_fork = naddr.clone();
    let naddr_for_cookbook = naddr.clone();
    let title_for_cookbook = title.clone();

    // Add to cookbook modal state
    let mut show_cookbook_modal = use_signal(|| false);

    // Cooking mode - checkboxes for ingredients
    let mut cooking_mode = use_signal(|| false);
    let mut checked_ingredients = use_signal(Vec::<usize>::new);
    let mut completed_steps = use_signal(Vec::<usize>::new);

    // Author profile metadata - uses shared hook for database-first, network-fallback pattern
    let author_metadata = use_author_metadata(author_pubkey.clone());

    // Extract parsed content
    let parsed = recipe.parsed.clone();

    let chef_notes = parsed.as_ref().and_then(|p| p.chef_notes.clone());
    let prep_time = parsed.as_ref().and_then(|p| p.details.prep_time.clone());
    let cook_time = parsed.as_ref().and_then(|p| p.details.cook_time.clone());
    let servings = parsed.as_ref().and_then(|p| p.details.servings.clone());
    let ingredients = parsed.as_ref().map(|p| p.ingredients.clone()).unwrap_or_default();
    let directions = parsed.as_ref().map(|p| p.directions.clone()).unwrap_or_default();
    let additional_resources = parsed.as_ref().and_then(|p| p.additional_resources.clone());

    // Author display
    let display_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));

    let profile_picture = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone());

    let avatar_letter = display_name.chars().next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    // Has any details to show
    let has_details = prep_time.is_some() || cook_time.is_some() || servings.is_some();

    rsx! {
        div {
            class: "max-w-4xl mx-auto",

            // Hero image
            if let Some(ref img_url) = image_url {
                div {
                    class: "relative w-full aspect-video rounded-xl overflow-hidden mb-6",
                    img {
                        src: "{img_url}",
                        alt: "{title}",
                        class: "w-full h-full object-cover",
                    }
                }
            }

            // Title and meta
            div {
                class: "mb-6",

                h1 {
                    class: "text-3xl md:text-4xl font-bold mb-4",
                    "{title}"
                }

                // Summary
                if let Some(ref sum) = summary {
                    p {
                        class: "text-lg text-muted-foreground mb-4",
                        "{sum}"
                    }
                }

                // Author row
                div {
                    class: "flex items-center gap-3",

                    Link {
                        to: Route::RecipeChef { npub: author_pubkey_for_link.clone() },
                        class: "flex items-center gap-2 hover:opacity-80 transition",

                        // Avatar
                        div {
                            class: "w-10 h-10 rounded-full overflow-hidden bg-muted flex items-center justify-center",
                            if let Some(ref pic_url) = profile_picture {
                                img {
                                    src: "{pic_url}",
                                    alt: "{display_name}",
                                    class: "w-full h-full object-cover",
                                }
                            } else {
                                span {
                                    class: "text-lg font-semibold text-muted-foreground",
                                    "{avatar_letter}"
                                }
                            }
                        }

                        div {
                            span {
                                class: "font-medium",
                                "{display_name}"
                            }
                            span {
                                class: "text-sm text-muted-foreground ml-2",
                                "{created_at_str}"
                            }
                        }
                    }
                }

                // Action buttons
                div {
                    class: "flex items-center gap-2 mt-4",

                    // Share button
                    button {
                        r#type: "button",
                        class: "flex items-center gap-2 px-4 py-2 bg-muted hover:bg-muted/80 rounded-lg transition text-sm font-medium",
                        onclick: move |_| {
                            #[cfg(target_arch = "wasm32")]
                            {
                                let share_url = format!("nostr:{}", naddr_for_share);
                                if let Some(window) = web_sys::window() {
                                    let clipboard = window.navigator().clipboard();
                                    let _ = clipboard.write_text(&share_url);
                                    share_copied.set(true);
                                    // Reset after 2 seconds
                                    spawn(async move {
                                        gloo_timers::future::TimeoutFuture::new(2000).await;
                                        share_copied.set(false);
                                    });
                                }
                            }
                        },

                        if *share_copied.read() {
                            svg {
                                class: "w-4 h-4 text-green-500",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M5 13l4 4L19 7"
                                }
                            }
                            "Copied!"
                        } else {
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.368 2.684 3 3 0 00-5.368-2.684z"
                                }
                            }
                            "Share"
                        }
                    }

                    // Fork/Edit button
                    if *HAS_SIGNER.read() {
                        Link {
                            to: Route::RecipeFork { naddr: naddr_for_fork.clone() },
                            class: "flex items-center gap-2 px-4 py-2 bg-muted hover:bg-muted/80 rounded-lg transition text-sm font-medium",

                            if *is_owner.read() {
                                svg {
                                    class: "w-4 h-4",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        stroke_width: "2",
                                        d: "M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                                    }
                                }
                                "Edit"
                            } else {
                                span { class: "text-base", "🍴" }
                                "Fork"
                            }
                        }

                        // Add to Cookbook button
                        button {
                            r#type: "button",
                            class: "flex items-center gap-2 px-4 py-2 bg-muted hover:bg-muted/80 rounded-lg transition text-sm font-medium",
                            onclick: move |_| show_cookbook_modal.set(true),
                            span { class: "text-base", "📚" }
                            "Save"
                        }
                    }
                }
            }

            // Tags
            if !tags.is_empty() {
                div {
                    class: "flex flex-wrap gap-2 mb-6",
                    for tag in tags.iter() {
                        RecipeTagChip { tag: tag.clone() }
                    }
                }
            }

            // Details card
            if has_details {
                div {
                    class: "grid grid-cols-3 gap-4 p-4 bg-muted/50 rounded-lg mb-6",

                    if let Some(ref prep) = prep_time {
                        div {
                            class: "text-center",
                            div { class: "text-2xl mb-1", "⏱️" }
                            div { class: "text-sm text-muted-foreground", "Prep Time" }
                            div { class: "font-medium", "{prep}" }
                        }
                    }

                    if let Some(ref cook) = cook_time {
                        div {
                            class: "text-center",
                            div { class: "text-2xl mb-1", "🍳" }
                            div { class: "text-sm text-muted-foreground", "Cook Time" }
                            div { class: "font-medium", "{cook}" }
                        }
                    }

                    if let Some(ref srv) = servings {
                        div {
                            class: "text-center",
                            div { class: "text-2xl mb-1", "🍽️" }
                            div { class: "text-sm text-muted-foreground", "Servings" }
                            div { class: "font-medium", "{srv}" }
                        }
                    }
                }
            }

            // Cooking mode toggle
            div {
                class: "flex items-center justify-between mb-6 p-3 bg-card border border-border rounded-lg",

                span {
                    class: "text-sm font-medium",
                    "Cooking Mode"
                }

                button {
                    r#type: "button",
                    class: format!(
                        "relative inline-flex h-6 w-11 items-center rounded-full transition-colors {}",
                        if *cooking_mode.read() { "bg-primary" } else { "bg-muted" }
                    ),
                    onclick: move |_| {
                        let current = *cooking_mode.peek();
                        cooking_mode.set(!current);
                    },

                    span {
                        class: format!(
                            "inline-block h-4 w-4 transform rounded-full bg-white transition {}",
                            if *cooking_mode.read() { "translate-x-6" } else { "translate-x-1" }
                        ),
                    }
                }
            }

            // Chef's notes
            if let Some(ref notes) = chef_notes {
                div {
                    class: "mb-8",
                    h2 {
                        class: "text-xl font-semibold mb-3 flex items-center gap-2",
                        span { "👨‍🍳" }
                        span { "Chef's Notes" }
                    }
                    div {
                        class: "prose prose-neutral dark:prose-invert max-w-none p-4 bg-muted/30 rounded-lg",
                        p { "{notes}" }
                    }
                }
            }

            // Two-column layout for ingredients and directions on larger screens
            div {
                class: "grid md:grid-cols-2 gap-8 mb-8",

                // Ingredients
                div {
                    h2 {
                        class: "text-xl font-semibold mb-4 flex items-center gap-2",
                        span { "🥗" }
                        span { "Ingredients" }
                        span {
                            class: "text-sm font-normal text-muted-foreground",
                            "({ingredients.len()})"
                        }
                    }

                    ul {
                        class: "space-y-2",
                        for (idx, ingredient) in ingredients.iter().enumerate() {
                            li {
                                key: "{idx}",
                                class: "flex items-start gap-3",

                                if *cooking_mode.read() {
                                    button {
                                        r#type: "button",
                                        class: format!(
                                            "mt-0.5 w-5 h-5 rounded border-2 flex items-center justify-center flex-shrink-0 transition {}",
                                            if checked_ingredients.read().contains(&idx) {
                                                "bg-primary border-primary text-white"
                                            } else {
                                                "border-muted-foreground hover:border-primary"
                                            }
                                        ),
                                        onclick: move |_| {
                                            let mut checked = checked_ingredients.write();
                                            if checked.contains(&idx) {
                                                checked.retain(|&i| i != idx);
                                            } else {
                                                checked.push(idx);
                                            }
                                        },

                                        if checked_ingredients.read().contains(&idx) {
                                            svg {
                                                class: "w-3 h-3",
                                                fill: "none",
                                                stroke: "currentColor",
                                                view_box: "0 0 24 24",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    stroke_width: "3",
                                                    d: "M5 13l4 4L19 7"
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    span {
                                        class: "mt-1.5 w-2 h-2 rounded-full bg-primary flex-shrink-0",
                                    }
                                }

                                span {
                                    class: format!(
                                        "{}",
                                        if *cooking_mode.read() && checked_ingredients.read().contains(&idx) {
                                            "line-through text-muted-foreground"
                                        } else {
                                            ""
                                        }
                                    ),
                                    "{ingredient}"
                                }
                            }
                        }
                    }
                }

                // Directions
                div {
                    h2 {
                        class: "text-xl font-semibold mb-4 flex items-center gap-2",
                        span { "📝" }
                        span { "Directions" }
                    }

                    ol {
                        class: "space-y-4",
                        for (idx, direction) in directions.iter().enumerate() {
                            li {
                                key: "{idx}",
                                class: "flex gap-4",

                                // Step number
                                button {
                                    r#type: "button",
                                    class: format!(
                                        "flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center font-semibold transition {}",
                                        if *cooking_mode.read() && completed_steps.read().contains(&idx) {
                                            "bg-green-500 text-white"
                                        } else if *cooking_mode.read() {
                                            "bg-primary/20 text-primary hover:bg-primary hover:text-white cursor-pointer"
                                        } else {
                                            "bg-primary/20 text-primary"
                                        }
                                    ),
                                    onclick: move |_| {
                                        if *cooking_mode.read() {
                                            let mut completed = completed_steps.write();
                                            if completed.contains(&idx) {
                                                completed.retain(|&i| i != idx);
                                            } else {
                                                completed.push(idx);
                                            }
                                        }
                                    },

                                    if *cooking_mode.read() && completed_steps.read().contains(&idx) {
                                        svg {
                                            class: "w-4 h-4",
                                            fill: "none",
                                            stroke: "currentColor",
                                            view_box: "0 0 24 24",
                                            path {
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                stroke_width: "2",
                                                d: "M5 13l4 4L19 7"
                                            }
                                        }
                                    } else {
                                        "{idx + 1}"
                                    }
                                }

                                // Step text
                                p {
                                    class: format!(
                                        "pt-1 {}",
                                        if *cooking_mode.read() && completed_steps.read().contains(&idx) {
                                            "text-muted-foreground"
                                        } else {
                                            ""
                                        }
                                    ),
                                    "{direction}"
                                }
                            }
                        }
                    }
                }
            }

            // Progress bar in cooking mode
            if *cooking_mode.read() && !directions.is_empty() {
                div {
                    class: "mb-8",
                    div {
                        class: "flex items-center justify-between mb-2",
                        span { class: "text-sm font-medium", "Progress" }
                        span {
                            class: "text-sm text-muted-foreground",
                            "{completed_steps.read().len()}/{directions.len()} steps"
                        }
                    }
                    div {
                        class: "w-full h-2 bg-muted rounded-full overflow-hidden",
                        div {
                            class: "h-full bg-primary transition-all duration-300",
                            style: format!(
                                "width: {}%",
                                (completed_steps.read().len() as f32 / directions.len() as f32 * 100.0) as u32
                            ),
                        }
                    }
                }
            }

            // Additional resources
            if let Some(ref resources) = additional_resources {
                div {
                    class: "mb-8",
                    h2 {
                        class: "text-xl font-semibold mb-3 flex items-center gap-2",
                        span { "📚" }
                        span { "Additional Resources" }
                    }
                    div {
                        class: "prose prose-neutral dark:prose-invert max-w-none p-4 bg-muted/30 rounded-lg",
                        p { "{resources}" }
                    }
                }
            }

            // Add to Cookbook Modal
            if *show_cookbook_modal.read() {
                AddToCookbookModal {
                    recipe_naddr: naddr_for_cookbook.clone(),
                    recipe_title: Some(title_for_cookbook.clone()),
                    on_close: move |_| show_cookbook_modal.set(false)
                }
            }
        }
    }
}

/// Skeleton loader for recipe detail
#[component]
pub fn RecipeDetailViewSkeleton() -> Element {
    rsx! {
        div {
            class: "max-w-4xl mx-auto animate-pulse",

            // Hero image skeleton
            div {
                class: "w-full aspect-video rounded-xl bg-muted mb-6",
            }

            // Title skeleton
            div {
                class: "mb-6",
                div { class: "h-10 bg-muted rounded w-3/4 mb-4" }
                div { class: "h-6 bg-muted rounded w-1/2 mb-4" }

                // Author skeleton
                div {
                    class: "flex items-center gap-3",
                    div { class: "w-10 h-10 rounded-full bg-muted" }
                    div { class: "h-5 bg-muted rounded w-32" }
                }
            }

            // Tags skeleton
            div {
                class: "flex gap-2 mb-6",
                div { class: "h-8 w-20 bg-muted rounded-full" }
                div { class: "h-8 w-24 bg-muted rounded-full" }
                div { class: "h-8 w-16 bg-muted rounded-full" }
            }

            // Details card skeleton
            div {
                class: "grid grid-cols-3 gap-4 p-4 bg-muted/50 rounded-lg mb-6",
                for _ in 0..3 {
                    div {
                        class: "text-center",
                        div { class: "h-8 w-8 bg-muted rounded mx-auto mb-2" }
                        div { class: "h-4 bg-muted rounded w-16 mx-auto mb-1" }
                        div { class: "h-5 bg-muted rounded w-12 mx-auto" }
                    }
                }
            }

            // Content skeleton
            div {
                class: "grid md:grid-cols-2 gap-8",

                // Ingredients skeleton
                div {
                    div { class: "h-7 bg-muted rounded w-32 mb-4" }
                    div { class: "space-y-3" }
                    for _ in 0..6 {
                        div {
                            class: "flex items-center gap-3",
                            div { class: "w-2 h-2 rounded-full bg-muted" }
                            div { class: "h-5 bg-muted rounded w-full" }
                        }
                    }
                }

                // Directions skeleton
                div {
                    div { class: "h-7 bg-muted rounded w-28 mb-4" }
                    div { class: "space-y-4" }
                    for _ in 0..4 {
                        div {
                            class: "flex gap-4",
                            div { class: "w-8 h-8 rounded-full bg-muted flex-shrink-0" }
                            div { class: "h-16 bg-muted rounded w-full" }
                        }
                    }
                }
            }
        }
    }
}
