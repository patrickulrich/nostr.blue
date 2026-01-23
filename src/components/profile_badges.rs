//! Profile Badges Component
//!
//! Displays accepted badges on a user's profile page.
//! Clicking a badge opens the badge detail modal.

use dioxus::prelude::*;

use crate::components::badge_detail_modal::BadgeDetailModal;
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip58::{self, BadgeDefinition};

/// Profile badges section component
/// Shows accepted badges for a user profile
#[component]
pub fn ProfileBadgesSection(pubkey: String) -> Element {
    let mut badges = use_signal(Vec::<BadgeDefinition>::new);
    let mut loading = use_signal(|| true);
    let mut selected_badge = use_signal(|| None::<BadgeDefinition>);
    let mut show_modal = use_signal(|| false);

    // Check if this is the current user's profile
    let is_own_profile = auth_store::get_pubkey()
        .map(|p| p == pubkey)
        .unwrap_or(false);

    // Fetch badges when pubkey changes
    use_effect(use_reactive(&pubkey, move |pk| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }

        spawn(async move {
            loading.set(true);

            // Fetch profile badges (kind 30008)
            match nip58::fetch_profile_badges(&pk).await {
                Ok(entries) if !entries.is_empty() => {
                    // Fetch badge definitions in PARALLEL for better performance
                    let futures: Vec<_> = entries
                        .into_iter()
                        .map(|entry| {
                            let coord = entry.definition_coordinate.clone();
                            async move {
                                match nip58::fetch_badge_by_coordinate(&coord).await {
                                    Ok(badge) => Some(badge),
                                    Err(e) => {
                                        crate::utils::log_fetch_error(&format!("badge {}", coord), e);
                                        None
                                    }
                                }
                            }
                        })
                        .collect();

                    // Wait for all fetches to complete
                    let results = futures::future::join_all(futures).await;
                    let badge_defs: Vec<_> = results.into_iter().flatten().collect();
                    badges.set(badge_defs);
                }
                Ok(_) => {
                    // No badges
                    badges.set(Vec::new());
                }
                Err(e) => {
                    crate::utils::log_fetch_error("profile badges", e);
                    badges.set(Vec::new());
                }
            }

            loading.set(false);
        });
    }));

    // Don't render if no badges and not loading
    let badge_list = badges.read();
    if badge_list.is_empty() && !*loading.read() {
        return rsx! {};
    }

    rsx! {
        // Badge section
        div {
            class: "py-3 px-4",

            // Loading state
            if *loading.read() {
                div {
                    class: "flex gap-2",
                    // Loading skeleton badges
                    for _ in 0..3 {
                        div {
                            class: "w-8 h-8 rounded-full bg-muted animate-pulse"
                        }
                    }
                }
            } else if !badge_list.is_empty() {
                // Badge thumbnails
                div {
                    class: "flex flex-wrap gap-2",
                    for badge in badge_list.iter() {
                        BadgeThumbnail {
                            badge: badge.clone(),
                            on_click: move |b: BadgeDefinition| {
                                selected_badge.set(Some(b));
                                show_modal.set(true);
                            }
                        }
                    }
                }
            }
        }

        // Badge detail modal
        if *show_modal.read() {
            if let Some(badge) = selected_badge.read().clone() {
                BadgeDetailModal {
                    badge: badge.clone(),
                    award: None,
                    is_own_badge: is_own_profile,
                    is_accepted: true, // These are already accepted badges
                    on_close: move |_| {
                        show_modal.set(false);
                        selected_badge.set(None);
                    },
                    on_accept: move |_| {
                        // Already accepted, no action needed
                    },
                    on_reject: {
                        // Clone badge_coord outside the closure for use in the async block
                        let badge_coord = badge.coordinate.clone();
                        move |_| {
                            // Remove from profile badges
                            let badge_coord = badge_coord.clone();
                            spawn(async move {
                                match nip58::reject_badge(&badge_coord).await {
                                    Ok(_) => {
                                        log::info!("Badge rejected successfully");
                                        // Remove from local list
                                        let mut current = badges.read().clone();
                                        current.retain(|b| b.coordinate != badge_coord);
                                        badges.set(current);
                                        show_modal.set(false);
                                        selected_badge.set(None);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to reject badge: {}", e);
                                        // Close modal on failure to reset processing state
                                        // User can reopen to retry
                                        show_modal.set(false);
                                        selected_badge.set(None);
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Individual badge thumbnail component
#[component]
fn BadgeThumbnail(badge: BadgeDefinition, on_click: EventHandler<BadgeDefinition>) -> Element {
    let badge_clone = badge.clone();

    rsx! {
        button {
            class: "w-8 h-8 rounded-full overflow-hidden hover:ring-2 hover:ring-primary transition-all cursor-pointer",
            title: "{badge.get_display_name()}",
            onclick: move |_| on_click.call(badge_clone.clone()),

            if let Some(thumb) = badge.get_thumbnail() {
                img {
                    src: "{thumb}",
                    alt: "{badge.get_display_name()}",
                    class: "w-full h-full object-cover"
                }
            } else {
                // Fallback: initials or placeholder
                div {
                    class: "w-full h-full bg-primary/20 flex items-center justify-center text-xs font-bold text-primary",
                    "{badge.id.chars().next().unwrap_or('?').to_uppercase()}"
                }
            }
        }
    }
}
