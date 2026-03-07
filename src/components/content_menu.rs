//! Content Menu Component
//! A reusable dropdown menu for Wiki, Recipe, Publication, and other content types
//! Similar to NoteMenu but designed for addressable events with naddr
use crate::components::board::item_selector::PinToBoardModal;
use crate::components::icons::MoreHorizontalIcon;
use crate::components::{AddToListModal, ReportModal};
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::pin_boards_store::{PinContentType, PinReference};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::nips::nip19::ToBech32;
use std::time::Duration;
/// Content type for the menu - determines labels and behavior
#[derive(Clone, Copy, PartialEq, Debug)]
#[allow(dead_code)]
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
    pub fn to_pin_content_type(self) -> PinContentType {
        match self {
            ContentMenuType::Wiki => PinContentType::Article,
            ContentMenuType::Recipe => PinContentType::Recipe,
            ContentMenuType::Publication => PinContentType::Article,
            ContentMenuType::Podcast => PinContentType::Podcast,
            ContentMenuType::CalendarEvent => PinContentType::CalendarEvent,
            ContentMenuType::Badge => PinContentType::Badge,
            ContentMenuType::CodeRepo => PinContentType::CodeRepo,
            ContentMenuType::Citation => PinContentType::Article,
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
    /// Optional title of the content (for pin board display)
    #[props(default)]
    pub title: Option<String>,
}
#[component]
pub fn ContentMenu(props: ContentMenuProps) -> Element {
    let mut is_open = use_signal(|| false);
    let mut is_following = use_signal(|| false);
    let mut is_loading_follow_state = use_signal(|| true);
    let mut is_updating_follow = use_signal(|| false);
    let mut follow_check_gen = use_signal(|| 0u32);
    let mut show_report_modal = use_signal(|| false);
    let mut show_add_to_list_modal = use_signal(|| false);
    let mut show_pin_to_board_modal = use_signal(|| false);
    let toast = consume_toast();
    let content_type = props.content_type;
    let author_pubkey = props.author_pubkey.clone();
    let author_pubkey_follow_check = author_pubkey.clone();
    let author_pubkey_follow_action = author_pubkey.clone();
    let author_pubkey_block = author_pubkey.clone();
    let author_pubkey_modal = author_pubkey.clone();
    let author_pubkey_modal_list = author_pubkey.clone();
    let naddr = props.naddr.clone();
    let event_id_hex = props.event_id.clone().unwrap_or_default();
    let clean_naddr: String = if naddr.to_ascii_lowercase().starts_with("nostr:") {
        naddr
            .split_once(':')
            .map(|(_, rest)| rest)
            .unwrap_or(&naddr)
            .to_string()
    } else {
        naddr.clone()
    };
    let clean_naddr_copy = clean_naddr.clone();
    let has_copyable_link = !clean_naddr.is_empty() || !event_id_hex.is_empty();
    let event_id_hex_copy = event_id_hex.clone();
    use_effect(use_reactive(&author_pubkey_follow_check, move |pubkey| {
        is_loading_follow_state.set(true);
        let gen = follow_check_gen.with_mut(|current| {
            *current = current.wrapping_add(1);
            *current
        });
        spawn(async move {
            match nostr_client::is_following(pubkey).await {
                Ok(following) => {
                    if *follow_check_gen.peek() != gen {
                        return;
                    }
                    is_following.set(following);
                    is_loading_follow_state.set(false);
                }
                Err(e) => {
                    if *follow_check_gen.peek() != gen {
                        return;
                    }
                    log::warn!("Failed to check follow status: {}", e);
                    is_following.set(false);
                    is_loading_follow_state.set(false);
                }
            }
        });
    }));
    let content_name = content_type.display_name();
    let pin_title = props
        .title
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| content_name.to_string());
    rsx! {
        div { class: "relative",
            button {
                class: "p-2 rounded-full hover:bg-accent transition-colors text-muted-foreground hover:text-foreground",
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    is_open.set(!is_open());
                },
                MoreHorizontalIcon { class: "h-5 w-5".to_string(), filled: false }
            }
            if *is_open.read() {
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        is_open.set(false);
                    },
                }
                div { class: "absolute right-0 mt-2 w-48 bg-background border border-border rounded-lg shadow-lg z-50 py-1",
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
                            let toast_api = toast;
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
                                        log::info!(
                                            "{} user: {}", if currently_following { "Unfollowed" } else {
                                            "Followed" }, pubkey
                                        );
                                    }
                                    Err(e) => {
                                        log::error!(
                                            "Failed to {} user: {}", if currently_following { "unfollow" }
                                            else { "follow" }, e
                                        );
                                        toast_api
                                            .error(
                                                format!(
                                                    "Failed to {} user: {}",
                                                    if currently_following { "unfollow" } else { "follow" },
                                                    e,
                                                ),
                                                ToastOptions::new(),
                                            );
                                    }
                                }
                                is_updating_follow.set(false);
                            });
                        },
                        span { class: "text-sm",
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
                    if !event_id_hex.is_empty() {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_add_to_list_modal.set(true);
                                is_open.set(false);
                            },
                            span { class: "text-sm", "Add to list" }
                        }
                    }
                    if *HAS_SIGNER.read() {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_pin_to_board_modal.set(true);
                                is_open.set(false);
                            },
                            span { class: "text-sm", "Pin to Board" }
                        }
                    }
                    if has_copyable_link {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                is_open.set(false);
                                let naddr_to_copy = clean_naddr_copy.clone();
                                let event_id_fallback = event_id_hex_copy.clone();
                                let toast_api = toast;
                                let nostr_uri = if !naddr_to_copy.is_empty() {
                                    format!("nostr:{}", naddr_to_copy)
                                } else if !event_id_fallback.is_empty() {
                                    match nostr_sdk::EventId::from_hex(&event_id_fallback) {
                                        Ok(eid) => {
                                            match eid.to_bech32() {
                                                Ok(bech32) => format!("nostr:{}", bech32),
                                                Err(e) => {
                                                    log::error!("Failed to encode event ID to bech32: {}", e);
                                                    toast_api
                                                        .error(
                                                            "Failed to copy link".to_string(),
                                                            ToastOptions::new()
                                                                .description("Could not encode event ID".to_string())
                                                                .duration(Duration::from_secs(2))
                                                                .permanent(false),
                                                        );
                                                    return;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            log::error!("Failed to parse event ID {}: {}", event_id_fallback, e);
                                            toast_api
                                                .error(
                                                    "Failed to copy link".to_string(),
                                                    ToastOptions::new()
                                                        .description("Could not parse event ID".to_string())
                                                        .duration(Duration::from_secs(2))
                                                        .permanent(false),
                                                );
                                            return;
                                        }
                                    }
                                } else {
                                    return;
                                };
                                spawn(async move {
                                    match crate::platform::clipboard::copy_to_clipboard(&nostr_uri).await {
                                        Ok(()) => {
                                            toast_api
                                                .success(
                                                    "Copied!".to_string(),
                                                    ToastOptions::new()
                                                        .description(
                                                            format!("Link to {} copied to clipboard", content_name),
                                                        )
                                                        .duration(Duration::from_secs(2))
                                                        .permanent(false),
                                                );
                                        }
                                        Err(_) => {
                                            toast_api
                                                .error(
                                                    "Failed to copy".to_string(),
                                                    ToastOptions::new()
                                                        .description("Could not access clipboard".to_string())
                                                        .duration(Duration::from_secs(2))
                                                        .permanent(false),
                                                );
                                        }
                                    }
                                });
                            },
                            span { class: "text-sm", "Copy link" }
                        }
                    }
                    div { class: "h-px bg-border my-1" }
                    if *HAS_SIGNER.read() {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-muted-foreground",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                is_open.set(false);
                                let pubkey = author_pubkey_block.clone();
                                let toast = toast;
                                spawn(async move {
                                    match nostr_client::block_user(pubkey).await {
                                        Ok(_) => {
                                            log::info!("User blocked successfully");
                                            toast.success("User blocked".to_string(), ToastOptions::new());
                                        }
                                        Err(e) => {
                                            log::error!("Failed to block user: {}", e);
                                            toast
                                                .error(
                                                    format!("Failed to block user: {}", e),
                                                    ToastOptions::new(),
                                                );
                                        }
                                    }
                                });
                            },
                            span { class: "text-sm", "Block author" }
                        }
                    }
                    if !event_id_hex.is_empty() {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-red-500 hover:text-red-600",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_report_modal.set(true);
                                is_open.set(false);
                            },
                            span { class: "text-sm", "Report {content_name}" }
                        }
                    }
                }
            }
        }
        if *show_report_modal.read() {
            ReportModal {
                event_id: event_id_hex.clone(),
                author_pubkey: author_pubkey_modal.clone(),
                on_close: move |_| {
                    show_report_modal.set(false);
                },
            }
        }
        if *show_add_to_list_modal.read() {
            AddToListModal {
                event_id: event_id_hex.clone(),
                author_pubkey: author_pubkey_modal_list.clone(),
                on_close: move |_| show_add_to_list_modal.set(false),
            }
        }
        if *show_pin_to_board_modal.read() {
            PinToBoardModal {
                reference: PinReference::Coordinate {
                    address: clean_naddr.clone(),
                    relay_hint: None,
                },
                content_type: content_type.to_pin_content_type(),
                title: Some(pin_title.clone()),
                on_close: move |_| show_pin_to_board_modal.set(false),
            }
        }
    }
}
