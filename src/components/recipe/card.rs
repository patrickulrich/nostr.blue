//! Recipe Card Component
//! Displays a recipe in a grid card format with image, title, and author
//! Standard recipe card (160x237px)
use super::tag_chip::RecipeTagChipSmall;
use crate::components::content_menu::{ContentMenu, ContentMenuType};
use crate::hooks::use_author_metadata;
use crate::routes::Route;
use crate::stores::recipe_store::CachedRecipe;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
/// Recipe card for grid display
#[component]
pub fn RecipeCard(recipe: CachedRecipe) -> Element {
    let title = recipe.metadata.title.clone();
    let image_url = recipe.metadata.primary_image().cloned();
    let summary = recipe.metadata.summary.clone();
    let tags = recipe.metadata.tags.clone();
    let naddr = recipe.naddr.clone();
    let author_pubkey = recipe.event.pubkey.to_hex();
    let author_metadata = use_author_metadata(author_pubkey.clone());
    let (prep_time, cook_time, servings) = if let Some(ref parsed) = recipe.parsed {
        (
            parsed.details.prep_time.clone(),
            parsed.details.cook_time.clone(),
            parsed.details.servings.clone(),
        )
    } else {
        (None, None, None)
    };
    let display_name = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));
    let profile_picture = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone());
    let avatar_letter = display_name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let displayed_tags: Vec<String> = tags.iter().take(2).cloned().collect();
    let preview = summary
        .or_else(|| recipe.parsed.as_ref().and_then(|p| p.chef_notes.clone()))
        .map(|s| {
            if s.chars().count() > 80 {
                let truncated: String = s.chars().take(80).collect();
                format!("{}...", truncated.trim())
            } else {
                s
            }
        });
    let author_pubkey_menu = author_pubkey.clone();
    let naddr_menu = naddr.clone();
    let event_id = recipe.event.id.to_hex();
    rsx! {
        div { class: "group relative bg-card rounded-lg border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-lg w-[160px] min-h-[237px] flex flex-col",
            div { class: "absolute top-1 right-1 z-20",
                ContentMenu {
                    content_type: ContentMenuType::Recipe,
                    author_pubkey: author_pubkey_menu,
                    naddr: naddr_menu,
                    event_id: Some(event_id),
                }
            }
            Link {
                to: Route::RecipeDetail {
                    naddr: naddr.clone(),
                },
                class: "block flex-1 flex flex-col",
                div { class: "relative w-full h-24 bg-muted overflow-hidden",
                    if let Some(ref img_url) = image_url {
                        img {
                            src: "{img_url}",
                            alt: "{title}",
                            class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-primary/20 to-primary/5",
                            span { class: "text-3xl", "🍳" }
                        }
                    }
                    if let Some(ref time) = cook_time {
                        div { class: "absolute top-1 left-1 px-1.5 py-0.5 bg-black/70 text-white text-xs rounded",
                            "⏱️ {time}"
                        }
                    }
                }
                div { class: "p-2 flex-1 flex flex-col gap-1",
                    h3 { class: "text-sm font-semibold line-clamp-2 group-hover:text-primary transition-colors",
                        "{title}"
                    }
                    {
                        let mut meta_parts: Vec<String> = Vec::new();
                        if let Some(ref pt) = prep_time {
                            meta_parts.push(format!("⏲️ {}", pt));
                        }
                        if let Some(ref ct) = cook_time {
                            if prep_time.is_none() {
                                meta_parts.push(format!("⏱️ {}", ct));
                            }
                        }
                        if let Some(ref srv) = servings {
                            meta_parts.push(format!("🍽️ {}", srv));
                        }
                        if !meta_parts.is_empty() {
                            rsx! {
                                p { class: "text-[10px] text-muted-foreground", "{meta_parts.join(\" · \")}" }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                    if let Some(ref text) = preview {
                        p { class: "text-xs text-muted-foreground line-clamp-2", "{text}" }
                    }
                    div { class: "flex-1" }
                    if !displayed_tags.is_empty() {
                        div { class: "flex flex-wrap gap-1",
                            for tag in displayed_tags {
                                RecipeTagChipSmall { tag, clickable: false }
                            }
                        }
                    }
                    div { class: "flex items-center gap-1 mt-1",
                        div { class: "w-5 h-5 rounded-full overflow-hidden bg-muted flex items-center justify-center shrink-0",
                            if let Some(ref pic_url) = profile_picture {
                                img {
                                    src: "{pic_url}",
                                    alt: "{display_name}",
                                    class: "w-full h-full object-cover",
                                    loading: "lazy",
                                }
                            } else {
                                span { class: "text-[8px] font-semibold text-muted-foreground",
                                    "{avatar_letter}"
                                }
                            }
                        }
                        span { class: "text-xs text-muted-foreground truncate", "{display_name}" }
                    }
                }
            }
        }
    }
}
/// Skeleton loader for recipe cards
#[component]
pub fn RecipeCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse w-[160px] min-h-[237px]",
            div { class: "w-full h-24 bg-muted" }
            div { class: "p-2 space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-full" }
                div { class: "flex gap-1 pt-1",
                    div { class: "h-4 w-12 bg-muted rounded-full" }
                    div { class: "h-4 w-10 bg-muted rounded-full" }
                }
                div { class: "flex items-center gap-1 pt-1",
                    div { class: "w-5 h-5 rounded-full bg-muted" }
                    div { class: "h-3 w-16 bg-muted rounded" }
                }
            }
        }
    }
}
