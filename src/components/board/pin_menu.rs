//! Pin Menu Component
//! 3-dot menu for pin cards with actions like delete, copy link, pin to board, etc.
use super::item_selector::PinToBoardModal;
use crate::components::icons::MoreHorizontalIcon;
use crate::components::ReportModal;
use crate::stores::nostr_client::{self, HAS_SIGNER};
use crate::stores::pin_boards_store::{delete_pin, Pin, PinReference};
use crate::utils::clipboard::copy_to_clipboard;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Data passed when requesting to pin content to a board
#[derive(Clone, Debug)]
pub struct PinToBoardRequest {
    pub reference: PinReference,
    pub content_type: crate::stores::pin_boards_store::PinContentType,
    pub title: Option<String>,
}
#[derive(Props, Clone, PartialEq)]
pub struct PinMenuProps {
    /// The pin to show menu for
    pub pin: Pin,
    /// Whether current user owns this pin
    pub is_owner: bool,
    /// Optional callback when pin is deleted
    #[props(default)]
    pub on_delete: Option<EventHandler<String>>,
    /// Optional callback when "Pin to Board" is requested
    /// If provided, the modal won't be rendered internally - parent handles it
    #[props(default)]
    pub on_pin_to_board: Option<EventHandler<PinToBoardRequest>>,
}
#[component]
pub fn PinMenu(props: PinMenuProps) -> Element {
    let mut is_open = use_signal(|| false);
    let mut show_pin_to_board_modal = use_signal(|| false);
    let mut show_report_modal = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);
    let mut is_deleting = use_signal(|| false);
    let toast = consume_toast();
    let pin = props.pin.clone();
    let pin_for_modal = pin.clone();
    let pin_for_copy = pin.clone();
    let pin_for_delete = pin.clone();
    let pin_for_mute = pin.clone();
    let pin_for_report = pin.clone();
    let is_owner = props.is_owner;
    let on_delete = props.on_delete;
    let on_pin_to_board = props.on_pin_to_board;
    let author_pubkey = pin.pubkey.clone();
    rsx! {
        div { class: "relative",
            button {
                class: "p-1.5 rounded-full hover:bg-accent transition-colors text-muted-foreground hover:text-foreground",
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    is_open.set(!is_open());
                },
                MoreHorizontalIcon { class: "h-4 w-4".to_string(), filled: false }
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
                    if *HAS_SIGNER.read() {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                            onclick: {
                                let pin_ref = pin_for_modal.reference.clone();
                                let pin_title = pin_for_modal.title.clone();
                                let content_type = pin_for_modal.content_type();
                                let callback = on_pin_to_board;
                                move |e: MouseEvent| {
                                    e.stop_propagation();
                                    is_open.set(false);
                                    if let Some(handler) = callback {
                                        handler
                                            .call(PinToBoardRequest {
                                                reference: pin_ref.clone(),
                                                content_type: content_type.clone(),
                                                title: pin_title.clone(),
                                            });
                                    } else {
                                        show_pin_to_board_modal.set(true);
                                    }
                                }
                            },
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                stroke_width: "2",
                                path { d: "M12 2L12 12" }
                                circle { cx: "12", cy: "14", r: "2" }
                                path { d: "M12 16L12 22" }
                                path { d: "M6 6L6 8C6 10.2091 7.79086 12 10 12L14 12C16.2091 12 18 10.2091 18 8L18 6" }
                            }
                            span { class: "text-sm", "Pin to Board" }
                        }
                    }
                    if *HAS_SIGNER.read() {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-muted-foreground",
                            onclick: {
                                let pin = pin_for_mute.clone();
                                let toast_api = toast;
                                move |e: MouseEvent| {
                                    e.stop_propagation();
                                    is_open.set(false);
                                    let event_id_to_mute = match &pin.reference {
                                        PinReference::Event { id, .. } => Some(id.clone()),
                                        _ => None,
                                    };
                                    if let Some(eid) = event_id_to_mute {
                                        spawn(async move {
                                            match nostr_client::mute_post(eid).await {
                                                Ok(_) => {
                                                    toast_api
                                                        .success(
                                                            "Muted".to_string(),
                                                            ToastOptions::new()
                                                                .description("Content muted")
                                                                .duration(Duration::from_secs(2))
                                                                .permanent(false),
                                                        );
                                                }
                                                Err(e) => {
                                                    log::error!("Failed to mute content: {}", e);
                                                    toast_api
                                                        .error(
                                                            "Error".to_string(),
                                                            ToastOptions::new()
                                                                .description("Failed to mute content")
                                                                .duration(Duration::from_secs(2))
                                                                .permanent(false),
                                                        );
                                                }
                                            }
                                        });
                                    } else {
                                        toast_api
                                            .error(
                                                "Cannot mute".to_string(),
                                                ToastOptions::new()
                                                    .description("This content type cannot be muted")
                                                    .duration(Duration::from_secs(2))
                                                    .permanent(false),
                                            );
                                    }
                                }
                            },
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M5.586 15H4a1 1 0 01-1-1v-4a1 1 0 011-1h1.586l4.707-4.707C10.923 3.663 12 4.109 12 5v14c0 .891-1.077 1.337-1.707.707L5.586 15z",
                                }
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M17 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2",
                                }
                            }
                            span { class: "text-sm", "Mute content" }
                        }
                    }
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        onclick: {
                            let pin = pin_for_copy.clone();
                            let toast_api = toast;
                            move |e: MouseEvent| {
                                e.stop_propagation();
                                is_open.set(false);
                                if let Ok(event_id) = EventId::from_hex(&pin.event_id) {
                                    let nevent_uri = format!(
                                        "nostr:{}",
                                        event_id.to_bech32().expect("infallible"),
                                    );
                                    spawn(async move {
                                        match copy_to_clipboard(&nevent_uri).await {
                                            Ok(_) => {
                                                toast_api
                                                    .success(
                                                        "Copied!".to_string(),
                                                        ToastOptions::new()
                                                            .description("Pin link copied to clipboard")
                                                            .duration(Duration::from_secs(2))
                                                            .permanent(false),
                                                    );
                                            }
                                            Err(_) => {
                                                toast_api
                                                    .error(
                                                        "Error".to_string(),
                                                        ToastOptions::new()
                                                            .description("Failed to copy to clipboard")
                                                            .duration(Duration::from_secs(2))
                                                            .permanent(false),
                                                    );
                                            }
                                        }
                                    });
                                } else {
                                    toast_api
                                        .error(
                                            "Error".to_string(),
                                            ToastOptions::new()
                                                .description("Invalid pin ID")
                                                .duration(Duration::from_secs(2))
                                                .permanent(false),
                                        );
                                }
                            }
                        },
                        svg {
                            class: "w-4 h-4",
                            fill: "none",
                            stroke: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z",
                            }
                        }
                        span { class: "text-sm", "Copy pin link" }
                    }
                    if *HAS_SIGNER.read() {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-muted-foreground",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_report_modal.set(true);
                                is_open.set(false);
                            },
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z",
                                }
                            }
                            span { class: "text-sm", "Report" }
                        }
                    }
                    if is_owner {
                        div { class: "h-px bg-border my-1" }
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-red-500 hover:text-red-600",
                            disabled: *is_deleting.read(),
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_delete_confirm.set(true);
                                is_open.set(false);
                            },
                            svg {
                                class: "w-4 h-4",
                                fill: "none",
                                stroke: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "2",
                                    d: "M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16",
                                }
                            }
                            span { class: "text-sm",
                                if *is_deleting.read() {
                                    "Deleting..."
                                } else {
                                    "Delete pin"
                                }
                            }
                        }
                    }
                }
            }
            if *show_delete_confirm.read() {
                div {
                    class: "fixed inset-0 z-50 flex items-center justify-center bg-black/50",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        show_delete_confirm.set(false);
                    },
                    div {
                        class: "bg-background border border-border rounded-lg p-6 max-w-sm mx-4 w-full",
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        h3 { class: "text-lg font-bold mb-2", "Delete Pin?" }
                        p { class: "text-muted-foreground text-sm mb-4",
                            "This will remove the pin from this board. This action cannot be undone."
                        }
                        div { class: "flex gap-2 justify-end",
                            button {
                                class: "px-4 py-2 text-sm text-muted-foreground hover:text-foreground",
                                disabled: *is_deleting.read(),
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_delete_confirm.set(false);
                                },
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 text-sm bg-red-500 hover:bg-red-600 text-white rounded-lg disabled:opacity-50",
                                disabled: *is_deleting.read(),
                                onclick: {
                                    let pin = pin_for_delete.clone();
                                    let toast_api = toast;
                                    move |e: MouseEvent| {
                                        e.stop_propagation();
                                        is_deleting.set(true);
                                        let event_id = pin.event_id.clone();
                                        spawn(async move {
                                            match delete_pin(&event_id).await {
                                                Ok(_) => {
                                                    toast_api
                                                        .success(
                                                            "Deleted".to_string(),
                                                            ToastOptions::new()
                                                                .description("Pin removed")
                                                                .duration(Duration::from_secs(2))
                                                                .permanent(false),
                                                        );
                                                    show_delete_confirm.set(false);
                                                    if let Some(handler) = on_delete {
                                                        handler.call(event_id);
                                                    }
                                                }
                                                Err(e) => {
                                                    log::error!("Failed to delete pin: {}", e);
                                                    toast_api
                                                        .error(
                                                            "Error".to_string(),
                                                            ToastOptions::new()
                                                                .description("Failed to delete pin")
                                                                .duration(Duration::from_secs(2))
                                                                .permanent(false),
                                                        );
                                                    is_deleting.set(false);
                                                }
                                            }
                                        });
                                    }
                                },
                                if *is_deleting.read() {
                                    span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin mr-1" }
                                    "Deleting..."
                                } else {
                                    "Delete"
                                }
                            }
                        }
                    }
                }
            }
        }
        if on_pin_to_board.is_none() && *show_pin_to_board_modal.read() {
            PinToBoardModal {
                reference: pin_for_modal.reference.clone(),
                content_type: pin_for_modal.content_type(),
                title: pin_for_modal.title.clone(),
                on_close: move |_| show_pin_to_board_modal.set(false),
            }
        }
        if *show_report_modal.read() {
            ReportModal {
                event_id: pin_for_report.event_id.clone(),
                author_pubkey: author_pubkey.clone(),
                on_close: move |_| show_report_modal.set(false),
            }
        }
    }
}
