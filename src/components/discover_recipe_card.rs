//! Discover Recipe Card Component
//! Recipe card for the Discover New section with author avatar in top-left
//! Matches ~/frontend TrendingRecipeCard.svelte (w-56 h-72)
use crate::hooks::use_author_metadata;
use crate::routes::Route;
use crate::stores::recipe_store::CachedRecipe;
use crate::utils::truncate_pubkey;
use crate::utils::validation::{css_safe_url, is_valid_http_url};
use dioxus::prelude::*;
/// Fallback image for recipes without a valid image URL
#[component]
fn RecipeFallbackImage() -> Element {
    rsx! {
        div { class: "absolute inset-0 bg-gradient-to-br from-gray-200 to-gray-300 flex items-center justify-center",
            span { class: "text-5xl opacity-50", "🍳" }
        }
    }
}
/// Discover recipe card for the explore page
/// Similar to RecipeCardTrending but with author avatar in TOP-LEFT
#[component]
pub fn DiscoverRecipeCard(recipe: CachedRecipe) -> Element {
    let title = recipe.metadata.title.clone();
    let image_url = recipe.metadata.primary_image().cloned();
    let naddr = recipe.naddr.clone();
    let author_pubkey = recipe.event.pubkey.to_hex();
    let author_metadata = use_author_metadata(author_pubkey.clone());
    let display_name = author_metadata
        .read()
        .as_ref()
        .and_then(|m| {
            m.display_name
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .or(m.name.as_ref().filter(|s| !s.trim().is_empty()))
                .cloned()
        })
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey));
    let profile_picture = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.picture.clone());
    let avatar_letter = display_name.chars().next().unwrap_or('?').to_ascii_uppercase();
    rsx! {
        div { class: "group relative rounded-xl overflow-hidden hover:scale-[1.02] transition-transform duration-200 w-56 h-72 shrink-0 cursor-pointer",
            Link {
                to: Route::RecipeDetail {
                    naddr: naddr.clone(),
                },
                class: "block w-full h-full",
                if let Some(ref img_url) = image_url {
                    if let Some(safe_url) = css_safe_url(img_url) {
                        div {
                            class: "absolute inset-0",
                            style: "background-image: url('{safe_url}'); background-size: cover; background-position: center;",
                            div { class: "absolute inset-0 bg-black/20 group-hover:bg-black/10 transition-colors" }
                        }
                    } else {
                        RecipeFallbackImage {}
                    }
                } else {
                    RecipeFallbackImage {}
                }
                div { class: "absolute top-3 left-3",
                    div { class: "w-8 h-8 rounded-full overflow-hidden ring-2 ring-white bg-muted flex items-center justify-center",
                        if let Some(ref pic_url) = profile_picture {
                            if is_valid_http_url(pic_url) {
                                img {
                                    src: "{pic_url}",
                                    alt: "{display_name}",
                                    class: "w-full h-full object-cover",
                                    loading: "lazy",
                                }
                            } else {
                                span { class: "text-xs font-semibold text-muted-foreground",
                                    "{avatar_letter}"
                                }
                            }
                        } else {
                            span { class: "text-xs font-semibold text-muted-foreground",
                                "{avatar_letter}"
                            }
                        }
                    }
                }
                div { class: "absolute inset-x-0 bottom-0 p-4 bg-gradient-to-t from-black/80 via-black/60 to-transparent",
                    h3 { class: "text-white font-semibold text-sm line-clamp-2", "{title}" }
                }
            }
        }
    }
}
/// Skeleton loader for discover recipe cards
#[component]
pub fn DiscoverRecipeCardSkeleton() -> Element {
    rsx! {
        div { class: "shrink-0 w-56 h-72 bg-muted rounded-xl animate-pulse" }
    }
}
