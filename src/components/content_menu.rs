//! Content Menu Component
//! A reusable dropdown menu for Wiki, Recipe, Publication, and other content types
//! Similar to NoteMenu but designed for addressable events with naddr

use dioxus::prelude::*;
use crate::components::icons::MoreHorizontalIcon;
use crate::components::{ReportModal, AddToListModal};
use crate::components::pin_board_item_selector::PinToBoardModal;
use crate::stores::pin_boards_store::{PinContentType, PinReference};
use crate::stores::nostr_client::{self, HAS_SIGNER};
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::time::Duration;

/// Content type for the menu - determines labels and behavior
#[derive(Clone, Copy, PartialEq, Debug)]
#[allow(dead_code)] // Variants for future content type integrations
pub enum ContentMenuType {
    Wiki,
    Recipe,
    Publication,
    Podcast,
    CalendarEvent,
    Badge,
    CodeRepo,
    Citation,
}

impl ContentMenuType {
    /// Get display name for this content type
    pub fn display_name(&self) -> &'static str {
        match self {
            ContentMenuType::Wiki => "wiki page",
            ContentMenuType::Recipe => "recipe",
            ContentMenuType::Publication => "publication",
            ContentMenuType::Podcast => "podcast",
            ContentMenuType::CalendarEvent => "event",
            ContentMenuType::Badge => "badge",
            ContentMenuType::CodeRepo => "repository",
            ContentMenuType::Citation => "citation",
        }
    }

    /// Convert to PinContentType for pin board integration
    pub fn to_pin_content_type(&self) -> PinContentType {
        match self {
            ContentMenuType::Wiki => PinContentType::Article,
            ContentMenuType::Recipe => PinContentType::Recipe,
            ContentMenuType::Publication => PinContentType::Article,
            ContentMenuType::Podcast => PinContentType::Podcast,
            ContentMenuType::CalendarEvent => PinContentType::CalendarEvent,
            ContentMenuType::Badge => PinContentType::Badge,
            ContentMenuType::CodeRepo => PinContentType::CodeRepo,
            ContentMenuType::Citation => PinContentType::Article, // Citations are metadata, map to Article
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ContentMenuProps {
    /// Type of content (Wiki, Recipe, Publication, etc.)
    pub content_type: ContentMenuType,
    /// Public key of the content author (hex)
    pub author_pubkey: String,
    /// NIP-19 naddr for addressable events
    pub naddr: String,
    /// Optional event ID (hex) for non-addressable or as fallback
    #[props(default)]
    pub event_id: Option<String>,
}

#[component]
pub fn ContentMenu(props: ContentMenuProps) -> Element {
    let mut is_open = use_signal(|| false);
    let mut is_following = use_signal(|| false);
    let mut is_loading_follow_state = use_signal(|| true);
    let mut is_updating_follow = use_signal(|| false);
    let mut show_report_modal = use_signal(|| false);
    let mut show_add_to_list_modal = use_signal(|| false);
    let mut show_pin_to_board_modal = use_signal(|| false);

    // Get toast API at component level
    let toast = consume_toast();

    // Clone props for use in closures
    let content_type = props.content_type;
    let author_pubkey = props.author_pubkey.clone();
    let author_pubkey_follow_check = author_pubkey.clone();
    let author_pubkey_follow_action = author_pubkey.clone();
    let author_pubkey_block = author_pubkey.clone();
    let author_pubkey_modal = author_pubkey.clone();
    let naddr = props.naddr.clone();
    let naddr_copy = naddr.clone();
    let naddr_list = naddr.clone();
    let naddr_report = naddr.clone();
    let naddr_pin_board = naddr.clone();
    let event_id_for_list = props.event_id.clone().unwrap_or_default();

    // Check follow status on mount
    use_effect(use_reactive(&author_pubkey_follow_check, move |pubkey| {
        spawn(async move {
            match nostr_client::is_following(pubkey).await {
                Ok(following) => {
                    is_following.set(following);
                    is_loading_follow_state.set(false);
                }
                Err(e) => {
                    log::warn!("Failed to check follow status: {}", e);
                    is_loading_follow_state.set(false);
                }
            }
        });
    }));

    let content_name = content_type.display_name();

    rsx! {
        div {
            class: "relative",

            // Menu button
            button {
                class: "p-2 rounded-full hover:bg-accent transition-colors text-muted-foreground hover:text-foreground",
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    is_open.set(!is_open());
                },
                MoreHorizontalIcon {
                    class: "h-5 w-5".to_string(),
                    filled: false
                }
            }

            // Dropdown menu
            if *is_open.read() {
                // Backdrop to close menu when clicking outside
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        is_open.set(false);
                    }
                }

                // Menu content
                div {
                    class: "absolute right-0 mt-2 w-48 bg-background border border-border rounded-lg shadow-lg z-50 py-1",

                    // Follow/Unfollow user
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        disabled: *is_loading_follow_state.read() || *is_updating_follow.read() || !*HAS_SIGNER.read(),
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();

                            if !*HAS_SIGNER.read() {
                                log::warn!("Cannot follow/unfollow user: No signer connected");
                                return;
                            }

                            let pubkey = author_pubkey_follow_action.clone();
                            let currently_following = *is_following.read();

                            is_updating_follow.set(true);
                            is_open.set(false);

                            spawn(async move {
                                let result = if currently_following {
                                    nostr_client::unfollow_user(pubkey.clone()).await
                                } else {
                                    nostr_client::follow_user(pubkey.clone()).await
                                };

                                match result {
                                    Ok(_) => {
                                        is_following.set(!currently_following);
                                        log::info!("{} user: {}",
                                            if currently_following { "Unfollowed" } else { "Followed" },
                                            pubkey
                                        );
                                    }
                                    Err(e) => {
                                        log::error!("Failed to {} user: {}",
                                            if currently_following { "unfollow" } else { "follow" },
                                            e
                                        );
                                    }
                                }
                                is_updating_follow.set(false);
                            });
                        },
                        span {
                            class: "text-sm",
                            {
                                if *is_loading_follow_state.read() {
                                    "Loading...".to_string()
                                } else if *is_updating_follow.read() {
                                    if *is_following.read() {
                                        "Unfollowing...".to_string()
                                    } else {
                                        "Following...".to_string()
                                    }
                                } else if *is_following.read() {
                                    "Unfollow author".to_string()
                                } else {
                                    "Follow author".to_string()
                                }
                            }
                        }
                    }

                    // Add to list (if we have an event_id)
                    if !event_id_for_list.is_empty() {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_add_to_list_modal.set(true);
                                is_open.set(false);
                            },
                            span {
                                class: "text-sm",
                                "Add to list"
                            }
                        }
                    }

                    // Pin to Board
                    if *HAS_SIGNER.read() {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_pin_to_board_modal.set(true);
                                is_open.set(false);
                            },
                            span {
                                class: "text-sm",
                                "Pin to Board"
                            }
                        }
                    }

                    // Copy Link (naddr)
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            is_open.set(false);

                            let naddr_to_copy = naddr_copy.clone();
                            let toast_api = toast.clone();

                            // Create nostr: URI
                            let nostr_uri = format!("nostr:{}", naddr_to_copy);

                            // Copy to clipboard (await the promise for proper error handling)
                            spawn(async move {
                                if let Some(window) = web_sys::window() {
                                    let clipboard = window.navigator().clipboard();
                                    let promise = clipboard.write_text(&nostr_uri);
                                    match wasm_bindgen_futures::JsFuture::from(promise).await {
                                        Ok(_) => {
                                            toast_api.success(
                                                "Copied!".to_string(),
                                                ToastOptions::new()
                                                    .description(format!("Link to {} copied to clipboard", content_name))
                                                    .duration(Duration::from_secs(2))
                                                    .permanent(false),
                                            );
                                        }
                                        Err(_) => {
                                            toast_api.error(
                                                "Failed to copy".to_string(),
                                                ToastOptions::new()
                                                    .description("Could not access clipboard".to_string())
                                                    .duration(Duration::from_secs(2))
                                                    .permanent(false),
                                            );
                                        }
                                    }
                                }
                            });
                        },
                        span {
                            class: "text-sm",
                            "Copy link"
                        }
                    }

                    // Divider
                    div {
                        class: "h-px bg-border my-1"
                    }

                    // Block user
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-muted-foreground",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            is_open.set(false);

                            let pubkey = author_pubkey_block.clone();
                            spawn(async move {
                                match nostr_client::block_user(pubkey).await {
                                    Ok(_) => log::info!("User blocked successfully"),
                                    Err(e) => log::error!("Failed to block user: {}", e),
                                }
                            });
                        },
                        span {
                            class: "text-sm",
                            "Block author"
                        }
                    }

                    // Report content
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-red-500 hover:text-red-600",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            show_report_modal.set(true);
                            is_open.set(false);
                        },
                        span {
                            class: "text-sm",
                            "Report {content_name}"
                        }
                    }
                }
            }
        }

        // Report Modal (uses event_id or naddr as identifier)
        if *show_report_modal.read() {
            ReportModal {
                event_id: naddr_report.clone(),
                author_pubkey: author_pubkey_modal.clone(),
                on_close: move |_| {
                    show_report_modal.set(false);
                }
            }
        }

        // Add to List Modal
        if *show_add_to_list_modal.read() {
            AddToListModal {
                event_id: naddr_list.clone(),
                on_close: move |_| show_add_to_list_modal.set(false)
            }
        }

        // Pin to Board Modal
        if *show_pin_to_board_modal.read() {
            PinToBoardModal {
                reference: PinReference::Coordinate { address: naddr_pin_board.clone(), relay_hint: None },
                content_type: content_type.to_pin_content_type(),
                on_close: move |_| show_pin_to_board_modal.set(false),
            }
        }
    }
}
