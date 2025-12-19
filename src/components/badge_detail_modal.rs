//! Badge Detail Modal Component
//!
//! Displays full badge information with accept/reject actions.

use dioxus::prelude::*;
use nostr_sdk::prelude::*;

use crate::routes::Route;
use crate::stores::profiles;
use crate::utils::nip58::{BadgeAward, BadgeDefinition};
use crate::utils::time::format_relative_time;
use crate::utils::truncate_pubkey;

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
    let mut processing = use_signal(|| false);

    // Reset processing state when is_accepted changes (operation completed by parent)
    use_effect(use_reactive!(|is_accepted| {
        processing.set(false);
        // Suppress unused variable warning - we react to the value changing
        let _ = is_accepted;
    }));

    // Get issuer profile
    let mut issuer_profile = use_signal(|| None::<nostr_sdk::Metadata>);
    let badge_pubkey = badge.pubkey.clone();

    // Only re-run when badge pubkey changes (not every render)
    use_effect(use_reactive!(|badge_pubkey| {
        if let Some(profile) = profiles::get_profile(&badge_pubkey) {
            issuer_profile.set(Some(profile));
        }
    }));

    // Get issuer display name with UTF-8 safe truncation
    let issuer_name = issuer_profile
        .read()
        .as_ref()
        .and_then(|p| p.display_name.clone().or(p.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&badge.pubkey));

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

                    // Badge image
                    if let Some(image) = badge.get_image() {
                        img {
                            src: "{image}",
                            alt: "{badge.get_display_name()}",
                            class: "w-32 h-32 rounded-lg object-contain"
                        }
                    } else {
                        // Placeholder
                        div {
                            class: "w-32 h-32 rounded-lg bg-primary/30 flex items-center justify-center",
                            span {
                                class: "text-4xl font-bold text-primary",
                                "{badge.id.chars().next().unwrap_or('?').to_uppercase()}"
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

                            if is_accepted {
                                // Already accepted - show remove option
                                button {
                                    class: "flex-1 px-4 py-2 rounded-lg border border-destructive text-destructive hover:bg-destructive/10 transition disabled:opacity-50",
                                    disabled: *processing.read(),
                                    onclick: move |_| {
                                        processing.set(true);
                                        on_reject.call(());
                                    },
                                    if *processing.read() {
                                        "Removing..."
                                    } else {
                                        "Remove from Profile"
                                    }
                                }
                            } else {
                                // Not accepted - show accept/reject options
                                button {
                                    class: "flex-1 px-4 py-2 rounded-lg border border-border hover:bg-accent transition disabled:opacity-50",
                                    disabled: *processing.read(),
                                    onclick: move |_| {
                                        on_reject.call(());
                                    },
                                    "Decline"
                                }

                                button {
                                    class: "flex-1 px-4 py-2 rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50",
                                    disabled: *processing.read(),
                                    onclick: move |_| {
                                        processing.set(true);
                                        on_accept.call(());
                                    },
                                    if *processing.read() {
                                        "Accepting..."
                                    } else {
                                        "Accept Badge"
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
