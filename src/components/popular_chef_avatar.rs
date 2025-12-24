//! Popular Chef Avatar Component
//! Displays a chef's avatar with their name below
//! Matches ~/frontend ProfileAvatar.svelte (w-20, avatar 64px)

use dioxus::prelude::*;
use nostr_sdk::PublicKey;
use nostr_sdk::prelude::ToBech32;
use crate::hooks::use_author_metadata;
use crate::routes::Route;
use crate::utils::truncate_pubkey;

/// Popular chef avatar for the explore page
#[component]
pub fn PopularChefAvatar(pubkey: String) -> Element {
    let pubkey_for_nav = pubkey.clone();

    // Profile metadata - uses shared hook for database-first, network-fallback pattern
    let profile_metadata = use_author_metadata(pubkey.clone());

    // Get display name from metadata or fallback
    let display_name = profile_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&pubkey));

    let profile_picture = profile_metadata.read().as_ref()
        .and_then(|m| m.picture.clone());

    // Avatar fallback letter
    let avatar_letter = display_name.chars().next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    // Create npub for navigation
    let npub = PublicKey::from_hex(&pubkey_for_nav)
        .ok()
        .and_then(|pk| pk.to_bech32().ok())
        .unwrap_or_else(|| pubkey_for_nav.clone());

    rsx! {
        Link {
            to: Route::Profile { pubkey: npub },
            class: "flex flex-col items-center gap-2 flex-shrink-0 w-20 cursor-pointer group",

            // Avatar
            div {
                class: "relative",

                div {
                    class: "w-16 h-16 rounded-full overflow-hidden bg-muted flex items-center justify-center group-hover:scale-105 transition-transform",

                    if let Some(ref pic_url) = profile_picture {
                        img {
                            src: "{pic_url}",
                            alt: "{display_name}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        span {
                            class: "text-xl font-semibold text-muted-foreground",
                            "{avatar_letter}"
                        }
                    }
                }
            }

            // Name
            div {
                class: "text-center w-full",

                span {
                    class: "text-xs font-medium truncate block max-w-[80px] mx-auto",
                    "{display_name}"
                }
            }
        }
    }
}

/// Skeleton loader for popular chef avatars
#[component]
pub fn PopularChefAvatarSkeleton() -> Element {
    rsx! {
        div {
            class: "flex-shrink-0 w-20 flex flex-col items-center gap-2",

            div {
                class: "w-16 h-16 bg-muted rounded-full animate-pulse"
            }
            div {
                class: "h-4 w-16 bg-muted rounded animate-pulse"
            }
        }
    }
}
