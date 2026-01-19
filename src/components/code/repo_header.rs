//! Repository Header Component
//!
//! Displays the repository header with icon, owner/repo breadcrumb, and visibility badge.
//! Styled to match gittr's layout-client.tsx header pattern.

use dioxus::prelude::*;
use crate::routes::Route;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::nip34::Repository;

/// Repository header with icon, breadcrumb, and badge
#[component]
pub fn RepoHeader(
    repo: Repository,
    #[props(default = None)] owner_picture: Option<String>,
) -> Element {
    // Get owner profile for display name and picture
    let owner_profile = PROFILE_CACHE.read().peek(&repo.pubkey).cloned();
    let owner_name = owner_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| repo.pubkey_display());

    // Use provided picture, profile picture, or fallback
    let picture_url = owner_picture
        .or_else(|| owner_profile.as_ref().and_then(|p| p.picture.clone()));

    let display_name = repo.name.clone().unwrap_or_else(|| repo.id.clone());

    rsx! {
        div {
            class: "flex items-center gap-2 text-lg",

            // Book/repo icon (like gittr's)
            svg {
                class: "w-4 h-4 text-muted-foreground shrink-0",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" }
                path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" }
            }

            // Owner avatar (circular with ring)
            div {
                class: "w-5 h-5 rounded-full overflow-hidden ring-2 ring-primary shrink-0",
                if let Some(url) = &picture_url {
                    img {
                        class: "w-5 h-5 object-cover",
                        src: "{url}",
                        alt: "{owner_name}",
                    }
                } else {
                    div {
                        class: "w-5 h-5 bg-muted flex items-center justify-center text-xs font-bold",
                        "{owner_name.chars().next().unwrap_or('?').to_ascii_uppercase()}"
                    }
                }
            }

            // Owner name (clickable)
            Link {
                to: Route::Profile { pubkey: repo.pubkey.clone() },
                class: "text-primary hover:underline cursor-pointer",
                "{owner_name}"
            }

            // Separator
            span {
                class: "text-muted-foreground",
                "/"
            }

            // Repo name (clickable)
            Link {
                to: Route::CodeRepo { naddr: repo.naddr.clone() },
                class: "text-primary hover:underline cursor-pointer font-semibold",
                "{display_name}"
            }

            // Visibility badge (Public for now, could extend for private repos)
            span {
                class: "ml-1.5 px-1.5 py-0.5 text-xs rounded-full border border-border text-muted-foreground",
                "Public"
            }
        }
    }
}

/// Compact header for nested pages (tree, blob, commits, etc.)
#[component]
pub fn RepoHeaderCompact(
    repo: Repository,
    #[props(default = None)] current_path: Option<String>,
) -> Element {
    let owner_profile = PROFILE_CACHE.read().peek(&repo.pubkey).cloned();
    let owner_name = owner_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| repo.pubkey_display());

    let display_name = repo.name.clone().unwrap_or_else(|| repo.id.clone());

    rsx! {
        div {
            class: "flex items-center gap-1.5 text-sm flex-wrap",

            // Owner
            Link {
                to: Route::Profile { pubkey: repo.pubkey.clone() },
                class: "text-primary hover:underline",
                "{owner_name}"
            }

            span { class: "text-muted-foreground", "/" }

            // Repo
            Link {
                to: Route::CodeRepo { naddr: repo.naddr.clone() },
                class: "text-primary hover:underline font-medium",
                "{display_name}"
            }

            // Current path if provided
            if let Some(path) = current_path {
                if !path.is_empty() {
                    span { class: "text-muted-foreground", "/" }
                    span { class: "text-muted-foreground truncate max-w-[200px]", "{path}" }
                }
            }
        }
    }
}
