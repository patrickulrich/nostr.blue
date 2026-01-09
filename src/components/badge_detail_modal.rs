//! Badge Detail Modal Component
//!
//! Displays full badge information with accept/reject actions.

use dioxus::prelude::*;
use dioxus::dioxus_core::Task;
use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::routes::Route;
use crate::stores::nostr_client::get_client;
use crate::stores::profiles;
use crate::utils::nip58::{BadgeAward, BadgeDefinition};
use crate::utils::time::format_relative_time;
use crate::utils::truncate_pubkey;
use crate::utils::validation::is_valid_http_url;

/// Processing state for accept/decline buttons
#[derive(Clone, Copy, PartialEq, Default)]
enum ProcessingState {
    #[default]
    Idle,
    Accepting,
    Declining,
    Removing,
}

/// Badge detail modal component
#[component]
pub fn BadgeDetailModal(
    badge: BadgeDefinition,
    award: Option<BadgeAward>,
    is_own_badge: bool,
    is_accepted: bool,
    on_close: EventHandler<()>,
    on_accept: EventHandler<()>,
    on_reject: EventHandler<()>,
) -> Element {
    let mut processing_state = use_signal(ProcessingState::default);
    let mut processing_timeout: Signal<Option<dioxus::dioxus_core::Task>> = use_signal(|| None);

    // Reset processing state when is_accepted changes (operation completed by parent)
    // Also reset any pending timeout since the operation completed
    use_effect(use_reactive!(|is_accepted| {
        processing_state.set(ProcessingState::Idle);
        // Cancel any pending timeout since operation completed
        if let Some(task) = processing_timeout.write().take() {
            task.cancel();
        }
        // Suppress unused variable warning - we react to the value changing
        let _ = is_accepted;
    }));

    // Get issuer profile
    let mut issuer_profile = use_signal(|| None::<nostr_sdk::Metadata>);
    let badge_pubkey = badge.pubkey.clone();

    // Store task handle for cancellation to prevent race conditions
    let mut fetch_task: Signal<Option<Task>> = use_signal(|| None);

    // Track the current target pubkey so async code can detect if it changed
    let mut target_pubkey: Signal<String> = use_signal(|| badge_pubkey.clone());

    // Only re-run when badge pubkey changes (not every render)
    use_effect(use_reactive!(|badge_pubkey| {
        // Cancel any existing fetch task before spawning new one
        if let Some(existing_task) = fetch_task.write().take() {
            existing_task.cancel();
        }

        // Update target pubkey for race condition detection
        target_pubkey.set(badge_pubkey.clone());

        if let Some(profile) = profiles::get_profile(&badge_pubkey) {
            issuer_profile.set(Some(profile));
        } else {
            // Profile not in cache - fetch it asynchronously using Nostr client
            let pubkey_str = badge_pubkey.clone();
            let new_task = spawn(async move {
                // Try hex first, then bech32 (npub) format
                let pubkey = match PublicKey::from_hex(&pubkey_str)
                    .or_else(|_| PublicKey::from_bech32(&pubkey_str))
                {
                    Ok(pk) => pk,
                    Err(e) => {
                        log::warn!("Invalid issuer pubkey: {}", e);
                        return;
                    }
                };

                let client = match get_client() {
                    Some(c) => c,
                    None => {
                        log::error!("Client not initialized, cannot fetch issuer metadata");
                        return;
                    }
                };

                // Try database first
                match client.database().metadata(pubkey).await {
                    Ok(Some(metadata)) => {
                        // Check pubkey hasn't changed before updating
                        if *target_pubkey.read() == pubkey_str {
                            issuer_profile.set(Some(metadata));
                        }
                        return;
                    }
                    Ok(None) => {} // No cached data, try network
                    Err(e) => {
                        log::warn!("Database fetch failed for {}: {}", pubkey_str, e);
                    }
                }

                // Fallback to network
                match client.fetch_metadata(pubkey, Duration::from_secs(5)).await {
                    Ok(Some(metadata)) => {
                        // Check pubkey hasn't changed before updating
                        if *target_pubkey.read() == pubkey_str {
                            issuer_profile.set(Some(metadata));
                        }
                    }
                    Ok(None) => {
                        log::debug!("No metadata found for {}", pubkey_str);
                    }
                    Err(e) => {
                        log::warn!("Network fetch failed for {}: {}", pubkey_str, e);
                    }
                }
            });
            fetch_task.set(Some(new_task));
        }
    }));

    // Get issuer display name with UTF-8 safe truncation (memoized)
    let badge_pubkey_for_memo = badge.pubkey.clone();
    let issuer_name = use_memo(move || {
        issuer_profile
            .read()
            .as_ref()
            .and_then(|p| p.display_name.clone().or(p.name.clone()))
            .unwrap_or_else(|| truncate_pubkey(&badge_pubkey_for_memo))
    });

    rsx! {
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-card border border-border rounded-xl max-w-md w-full shadow-xl overflow-hidden",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "badge-modal-title",
                onclick: move |e| e.stop_propagation(),

                // Badge image header
                div {
                    class: "relative bg-gradient-to-br from-primary/20 to-accent/20 p-8 flex items-center justify-center",

                    // Close button
                    button {
                        class: "absolute top-2 right-2 p-2 rounded-full hover:bg-black/20 transition",
                        onclick: move |_| on_close.call(()),
                        svg {
                            class: "w-5 h-5",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M6 18L18 6M6 6l12 12"
                            }
                        }
                    }

                    // Badge image - validate URL before rendering
                    {
                        let valid_image = badge.get_image().filter(|url| is_valid_http_url(url));
                        if let Some(image) = valid_image {
                            rsx! {
                                img {
                                    src: "{image}",
                                    alt: "{badge.get_display_name()}",
                                    class: "w-32 h-32 rounded-lg object-contain"
                                }
                            }
                        } else {
                            // Placeholder
                            rsx! {
                                div {
                                    class: "w-32 h-32 rounded-lg bg-primary/30 flex items-center justify-center",
                                    span {
                                        class: "text-4xl font-bold text-primary",
                                        "{badge.id.chars().next().unwrap_or('?').to_ascii_uppercase()}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Badge info
                div {
                    class: "p-6 space-y-4",

                    // Name
                    h2 {
                        id: "badge-modal-title",
                        class: "text-xl font-bold text-center",
                        "{badge.get_display_name()}"
                    }

                    // Description
                    if let Some(description) = &badge.description {
                        p {
                            class: "text-muted-foreground text-center text-sm",
                            "{description}"
                        }
                    }

                    // Divider
                    div {
                        class: "border-t border-border my-4"
                    }

                    // Issuer info
                    div {
                        class: "flex items-center justify-between text-sm",

                        span {
                            class: "text-muted-foreground",
                            "Issued by"
                        }

                        Link {
                            to: Route::Profile { pubkey: badge.pubkey.clone() },
                            class: "text-primary hover:underline font-medium",
                            onclick: move |_| on_close.call(()),
                            "@{issuer_name}"
                        }
                    }

                    // Award date (if provided)
                    if let Some(award) = &award {
                        div {
                            class: "flex items-center justify-between text-sm",

                            span {
                                class: "text-muted-foreground",
                                "Awarded"
                            }

                            span {
                                class: "text-foreground",
                                "{format_relative_time(Timestamp::from(award.created_at))}"
                            }
                        }
                    }

                    // Badge ID
                    div {
                        class: "flex items-center justify-between text-sm",

                        span {
                            class: "text-muted-foreground",
                            "Badge ID"
                        }

                        span {
                            class: "text-foreground font-mono text-xs",
                            "{badge.id}"
                        }
                    }

                    // Action buttons (only for own badges)
                    if is_own_badge {
                        div {
                            class: "flex gap-3 mt-6",

                            // Helper to start processing with timeout reset
                            {
                                let mut start_processing = move |state: ProcessingState, handler: EventHandler<()>| {
                                    processing_state.set(state);
                                    // Cancel any existing timeout to prevent race conditions
                                    if let Some(existing_task) = processing_timeout.write().take() {
                                        existing_task.cancel();
                                    }
                                    // Set a timeout to reset processing if parent doesn't respond
                                    let timeout_task = spawn(async move {
                                        gloo_timers::future::TimeoutFuture::new(10000).await;
                                        processing_state.set(ProcessingState::Idle);
                                    });
                                    processing_timeout.set(Some(timeout_task));
                                    handler.call(());
                                };

                                let is_processing = *processing_state.read() != ProcessingState::Idle;

                                if is_accepted {
                                    // Already accepted - show remove option
                                    rsx! {
                                        button {
                                            class: "flex-1 px-4 py-2 rounded-lg border border-destructive text-destructive hover:bg-destructive/10 transition disabled:opacity-50",
                                            disabled: is_processing,
                                            onclick: move |_| start_processing(ProcessingState::Removing, on_reject),
                                            if *processing_state.read() == ProcessingState::Removing {
                                                "Removing..."
                                            } else {
                                                "Remove from Profile"
                                            }
                                        }
                                    }
                                } else {
                                    // Not accepted - show accept/reject options
                                    rsx! {
                                        button {
                                            class: "flex-1 px-4 py-2 rounded-lg border border-border hover:bg-accent transition disabled:opacity-50",
                                            disabled: is_processing,
                                            onclick: move |_| start_processing(ProcessingState::Declining, on_reject),
                                            if *processing_state.read() == ProcessingState::Declining {
                                                "Declining..."
                                            } else {
                                                "Decline"
                                            }
                                        }

                                        button {
                                            class: "flex-1 px-4 py-2 rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50",
                                            disabled: is_processing,
                                            onclick: move |_| start_processing(ProcessingState::Accepting, on_accept),
                                            if *processing_state.read() == ProcessingState::Accepting {
                                                "Accepting..."
                                            } else {
                                                "Accept Badge"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // View details link
                    div {
                        class: "mt-4 text-center",

                        Link {
                            to: Route::BadgeDetail { naddr: badge.naddr.clone() },
                            class: "text-sm text-muted-foreground hover:text-primary transition",
                            onclick: move |_| on_close.call(()),
                            "View full details →"
                        }
                    }
                }
            }
        }
    }
}
