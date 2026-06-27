//! Quick action sheet for nest cards. Triggered by long-press (mobile)
//! or right-click (wasm/desktop) via the standard `oncontextmenu` event —
//! the same nostr.blue pattern used by the reaction button at
//! `components/reaction/button.rs:111`. Actions: Share, Copy link, Open
//! host profile, Mute host, Delete room (if author). Mirrors Amethyst's
//! `LongPressToQuickAction` wrapper on `NestFeedCard`.

use crate::components::icons::{CopyIcon, ShareIcon, TrashIcon};
use crate::components::{Sheet, SheetContent, SheetSide};
use crate::platform::clipboard::copy_to_clipboard;
use crate::stores::auth_store;
use crate::utils::nips::nip53::MeetingSpace;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct NestQuickActionSheetProps {
    pub space: MeetingSpace,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn NestQuickActionSheet(props: NestQuickActionSheetProps) -> Element {
    let naddr_uri = format!("nostr:{}", props.space.naddr);
    let room_name = props.space.room_name.clone();
    let my_pubkey = auth_store::get_pubkey();
    let is_author = my_pubkey.as_ref() == Some(&props.space.pubkey);
    let host_pk = props.space.pubkey.clone();
    let space_pk = props.space.pubkey.clone();
    let space_d_tag = props.space.d_tag.clone();

    let handle_share = {
        let uri = naddr_uri.clone();
        let name = room_name.clone();
        move |_| {
            let text = if name.is_empty() { uri.clone() } else { format!("{name} — {uri}") };
            spawn(async move {
                if let Err(e) = copy_to_clipboard(&text).await {
                    log::warn!("Failed to copy room link: {e}");
                }
            });
            props.on_close.call(());
        }
    };

    let handle_copy_link = {
        let uri = naddr_uri.clone();
        move |_| {
            let uri = uri.clone();
            spawn(async move {
                if let Err(e) = copy_to_clipboard(&uri).await {
                    log::warn!("Failed to copy link: {e}");
                }
            });
            props.on_close.call(());
        }
    };

    let handle_block_host = {
        let pk = host_pk.clone();
        move |_| {
            let pk = pk.clone();
            spawn(async move {
                if let Err(e) = crate::stores::nostr_client::block_user(pk).await {
                    log::warn!("Failed to block user: {e}");
                }
            });
            props.on_close.call(());
        }
    };

    let handle_delete_room = {
        let ms_pubkey = space_pk.clone();
        let ms_d_tag = space_d_tag.clone();
        move |_| {
            let ms_pubkey = ms_pubkey.clone();
            let ms_d_tag = ms_d_tag.clone();
            spawn(async move {
                let pk = match PublicKey::parse(&ms_pubkey) {
                    Ok(p) => p,
                    Err(e) => {
                        log::warn!("Cannot parse pubkey for delete: {e}");
                        return;
                    }
                };
                let coord = Coordinate::new(Kind::Custom(30312), pk).identifier(&ms_d_tag);
                let request = nostr_sdk::nips::nip09::EventDeletionRequest::new()
                    .coordinate(coord);
                let builder = EventBuilder::delete(request);
                let _ = crate::stores::publish_queue::signing::sign_event_builder(builder)
                    .await
                    .map(|event| {
                        crate::stores::publish_queue::enqueue(
                            event,
                            crate::stores::publish_queue::types::QueueEventType::Other(
                                "nest-delete".to_string(),
                            ),
                            None,
                            std::collections::HashMap::new(),
                        )
                    });
            });
            props.on_close.call(());
        }
    };

    rsx! {
        Sheet {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    props.on_close.call(());
                }
            },
            SheetContent {
                side: SheetSide::Bottom,
                class: "p-0",
                div { class: "p-4 space-y-1",
                    div { class: "pb-3 mb-2 border-b border-border",
                        p { class: "text-sm font-semibold truncate",
                            "{room_name}"
                        }
                        p { class: "text-xs text-muted-foreground truncate",
                            "Quick actions"
                        }
                    }
                    button {
                        class: "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-accent transition text-left",
                        onclick: handle_share,
                        ShareIcon { class: "w-4 h-4 shrink-0".to_string() }
                        span { class: "text-sm", "Share room link" }
                    }
                    button {
                        class: "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-accent transition text-left",
                        onclick: handle_copy_link,
                        CopyIcon { class: "w-4 h-4 shrink-0".to_string() }
                        span { class: "text-sm", "Copy naddr" }
                    }
                    if !is_author {
                        button {
                            class: "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-accent transition text-left text-destructive",
                            onclick: handle_block_host,
                            span { class: "w-4 h-4 shrink-0 flex items-center justify-center text-base", "🚫" }
                            span { class: "text-sm", "Block host" }
                        }
                    }
                    if is_author {
                        button {
                            class: "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-accent transition text-left text-destructive",
                            onclick: handle_delete_room,
                            TrashIcon { class: "w-4 h-4 shrink-0".to_string() }
                            span { class: "text-sm", "Delete room" }
                        }
                    }
                }
            }
        }
    }
}
