use crate::components::board::item_selector::PinToBoardModal;
use crate::components::edit_post::EditPostView;
use crate::components::icons::MoreHorizontalIcon;
use crate::components::{AddToListModal, ConfirmModal, ReportModal, ShareModal};
use crate::routes::Route;
use crate::stores::ai_chat_seed_store::{queue_ai_chat_seed, AiChatSeedPayload};
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::pin_boards_store::{PinContentType, PinReference};
use crate::stores::pinned_notes;
use crate::stores::{ai_chat_store, auth_store, nostr_client, relay};
use crate::utils::clipboard::copy_to_clipboard;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::nips::nip19::{FromBech32, ToBech32};
use nostr_sdk::prelude::*;
use std::time::Duration;

const NOTE_AI_CONTENT_CHAR_LIMIT: usize = 2_000;

fn format_note_context_for_ai(event: &nostr_sdk::Event) -> String {
    let author = event
        .pubkey
        .to_bech32()
        .unwrap_or_else(|_| event.pubkey.to_string());
    let event_id = event
        .id
        .to_bech32()
        .unwrap_or_else(|_| event.id.to_string());
    let content = if event.content.trim().is_empty() {
        "(no content)".to_string()
    } else {
        event
            .content
            .chars()
            .take(NOTE_AI_CONTENT_CHAR_LIMIT)
            .collect()
    };

    format!(
        "Note for discussion\nAuthor: {author}\nCreated: {}\nEvent ID: {event_id}\n\nContent:\n{content}\n\nYou have nostr tools available. Use get_profile to learn about the author, get_interaction_counts to see replies/likes/zaps for this note, get_contact_list to see who the author follows, and other tools to provide richer context.",
        event.created_at.to_human_datetime()
    )
}

#[derive(Props, Clone, PartialEq)]
pub struct NoteMenuProps {
    /// Public key of the note author
    pub author_pubkey: String,
    /// Event ID of the note
    pub event_id: String,
    /// Full signed event for manual rebroadcasting
    pub event: nostr_sdk::Event,
}
#[component]
pub fn NoteMenu(props: NoteMenuProps) -> Element {
    let navigator = navigator();
    let mut is_open = use_signal(|| false);
    let mut is_following = use_signal(|| false);
    let mut is_loading_follow_state = use_signal(|| true);
    let mut is_updating_follow = use_signal(|| false);
    let mut show_report_modal = use_signal(|| false);
    let mut show_share_modal = use_signal(|| false);
    let mut show_add_to_list_modal = use_signal(|| false);
    let mut show_pin_to_board_modal = use_signal(|| false);
    let mut is_pinned = use_signal(|| false);
    let mut is_updating_pin = use_signal(|| false);
    let mut is_broadcasting = use_signal(|| false);
    let mut show_ai_chat_confirm = use_signal(|| false);
    let mut pending_ai_chat_seed = use_signal(|| None::<AiChatSeedPayload>);
    let mut show_edit_modal = use_signal(|| false);
    let mut show_propose_modal = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);
    let mut is_deleting = use_signal(|| false);
    let toast = consume_toast();
    let event = props.event.clone();
    let parsed_author = PublicKey::parse(&props.author_pubkey).ok();
    let parsed_event_id = EventId::from_hex(&props.event_id)
        .ok()
        .or_else(|| EventId::from_bech32(&props.event_id).ok());
    let identities_match = parsed_author == Some(event.pubkey) && parsed_event_id == Some(event.id);
    if !identities_match {
        log::error!(
            "NoteMenu identity mismatch: props=({}, {}), event=({}, {})",
            props.author_pubkey,
            props.event_id,
            event.pubkey,
            event.id
        );
        return rsx! {
            div { class: "relative",
                button {
                    class: "p-2 rounded-full text-muted-foreground/50 cursor-not-allowed",
                    disabled: true,
                    title: "Note actions unavailable due to mismatched event identity",
                    MoreHorizontalIcon { class: "h-5 w-5".to_string(), filled: false }
                }
            }
        };
    }
    let author_pubkey = props.author_pubkey.clone();
    let event_id = props.event_id.clone();
    let author_pubkey_follow_check = author_pubkey.clone();
    let author_pubkey_follow_action = author_pubkey.clone();
    let author_pubkey_block = author_pubkey.clone();
    let author_pubkey_modal = author_pubkey.clone();
    let author_pubkey_modal_list = author_pubkey.clone();
    let event_id_list = event_id.clone();
    let event_id_mute = event_id.clone();
    let event_id_report = event_id.clone();
    let event_id_modal_report = event_id.clone();
    let event_id_modal_list = event_id.clone();
    let event_id_copy = event_id.clone();
    let event_content_copy = event.content.clone();
    let event_share = event.clone();
    let event_nevent_copy = event.clone();
    let event_id_pin = event_id.clone();
    let event_id_pin_check = event_id.clone();
    let event_id_pin_board = event_id.clone();
    let event_broadcast = event.clone();
    let event_edit = event.clone();
    let event_id_delete = event_id.clone();
    let mut follow_state_gen = use_signal(|| 0u32);
    let is_own_note = auth_store::get_pubkey()
        .and_then(|pubkey| PublicKey::parse(&pubkey).ok())
        .map(|pubkey| pubkey == event.pubkey)
        .unwrap_or(false);
    use_effect(use_reactive(
        (&author_pubkey_follow_check, &*HAS_SIGNER.read()),
        move |(pubkey, signer)| {
            let gen = follow_state_gen.with_mut(|g| {
                *g = g.wrapping_add(1);
                *g
            });
            if !signer {
                is_following.set(false);
                is_loading_follow_state.set(false);
                return;
            }
            is_loading_follow_state.set(true);
            spawn(async move {
                match nostr_client::is_following(pubkey).await {
                    Ok(following) => {
                        if *follow_state_gen.peek() == gen {
                            is_following.set(following);
                            is_loading_follow_state.set(false);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to check follow status: {}", e);
                        if *follow_state_gen.peek() == gen {
                            is_loading_follow_state.set(false);
                        }
                    }
                }
            });
        },
    ));
    use_effect(use_reactive(&event_id_pin_check, move |eid| {
        let pinned = pinned_notes::is_pinned(&eid);
        is_pinned.set(pinned);
    }));
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
                                    "Unfollow user".to_string()
                                } else {
                                    "Follow user".to_string()
                                }
                            }
                        }
                    }
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            log::info!("Add to list: {}", event_id_list);
                            show_add_to_list_modal.set(true);
                            is_open.set(false);
                        },
                        span { class: "text-sm", "Add to list" }
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
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        disabled: !*HAS_SIGNER.read() || *is_updating_pin.read(),
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            if !*HAS_SIGNER.read() {
                                log::warn!("Cannot pin/unpin note: No signer connected");
                                return;
                            }
                            let eid = event_id_pin.clone();
                            let currently_pinned = *is_pinned.read();
                            let toast_api = toast;
                            is_updating_pin.set(true);
                            is_open.set(false);
                            spawn(async move {
                                let result = if currently_pinned {
                                    pinned_notes::unpin_event(eid.clone()).await
                                } else {
                                    pinned_notes::pin_event(eid.clone()).await
                                };
                                match result {
                                    Ok(_) => {
                                        is_pinned.set(!currently_pinned);
                                        log::info!(
                                            "{} note: {}", if currently_pinned { "Unpinned" } else { "Pinned"
                                            }, eid
                                        );
                                    }
                                    Err(e) => {
                                        let action = if currently_pinned { "unpin" } else { "pin" };
                                        log::error!("Failed to {} note: {}", action, e);
                                        toast_api
                                            .error(
                                                format!("Failed to {} note", action),
                                                ToastOptions::new()
                                                    .duration(Duration::from_secs(3))
                                                    .permanent(false),
                                            );
                                    }
                                }
                                is_updating_pin.set(false);
                            });
                        },
                        span { class: "text-sm",
                            {
                                if *is_updating_pin.read() {
                                    if *is_pinned.read() { "Unpinning..." } else { "Pinning..." }
                                } else if *is_pinned.read() {
                                    "Unpin note"
                                } else {
                                    "Pin to profile"
                                }
                            }
                        }
                    }
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            is_open.set(false);
                            let event_id = event_id_copy.clone();
                            let toast_api = toast;
                            let event_for_nevent = event_nevent_copy.clone();
                            let event_id_parsed = EventId::from_hex(&event_id)
                                .or_else(|_| EventId::from_bech32(&event_id));
                            if let Ok(eid) = event_id_parsed {
                                // nevent (author + kind + relay hints) so receiving
                                // clients can resolve the note without a lookup;
                                // falls back to note1/hex inside the helper.
                                let nevent = crate::utils::nip19_urls::note_route_id_with_kind(
                                    &eid.to_hex(),
                                    Some(&event_for_nevent.pubkey.to_hex()),
                                    Some(event_for_nevent.kind),
                                );
                                let note_uri = format!("nostr:{}", nevent);
                                spawn(async move {
                                    match copy_to_clipboard(&note_uri).await {
                                        Ok(_) => {
                                            toast_api
                                                .success(
                                                    "Copied!".to_string(),
                                                    ToastOptions::new()
                                                        .description("Note ID (nevent) copied to clipboard")
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
                                            .description("Invalid note ID format")
                                            .duration(Duration::from_secs(2))
                                            .permanent(false),
                                    );
                            }
                        },
                        span { class: "text-sm", "Copy Note ID" }
                    }
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            is_open.set(false);
                            let content = event_content_copy.clone();
                            let toast_api = toast;
                            spawn(async move {
                                match copy_to_clipboard(&content).await {
                                    Ok(_) => {
                                        toast_api
                                            .success(
                                                "Copied!".to_string(),
                                                ToastOptions::new()
                                                    .description("Note content copied to clipboard")
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
                        },
                        span { class: "text-sm", "Copy Note" }
                    }
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            show_share_modal.set(true);
                            is_open.set(false);
                        },
                        span { class: "text-sm", "Share..." }
                    }
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            is_open.set(false);
                            let payload = AiChatSeedPayload {
                                source: "note".to_string(),
                                title_hint: Some("Note discussion".to_string()),
                                message: format_note_context_for_ai(&event),
                            };
                            let event_for_prefetch = event.clone();
                            let nav = navigator;
                            spawn(async move {
                                let author_hex = event_for_prefetch.pubkey.to_hex();
                                let event_id = event_for_prefetch.id;
                                spawn(async move {
                                    let _ = crate::stores::profiles::fetch_profile(author_hex).await;
                                    let _ = crate::services::aggregation::fetch_interaction_counts_batch(
                                        vec![event_id],
                                        std::time::Duration::from_secs(5),
                                    )
                                    .await;
                                });
                                let account_key = ai_chat_store::current_account_key();
                                match ai_chat_store::load_chat_state(&account_key).await {
                                    Ok(state) if ai_chat_store::has_saved_conversation_context(&state) => {
                                        pending_ai_chat_seed.set(Some(payload));
                                        show_ai_chat_confirm.set(true);
                                    }
                                    Ok(_) => {
                                        queue_ai_chat_seed(payload);
                                        nav.push(Route::AIChat {});
                                    }
                                    Err(err) => {
                                        log::warn!("Failed to load AI chat state before note seed: {err}");
                                        queue_ai_chat_seed(payload);
                                        nav.push(Route::AIChat {});
                                    }
                                }
                            });
                        },
                         span { class: "text-sm", "AI Chat" }
                     }
                     if is_own_note {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                            disabled: *is_broadcasting.read(),
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                if *is_broadcasting.read() {
                                    return;
                                }
                                let toast_api = toast;
                                let mut relay_urls = relay::get_write_relays();
                                relay_urls.extend(relay::BROADCAST_RELAYS.read().clone());
                                relay_urls.retain(|url| !relay::is_relay_blocked(url));
                                let mut seen = std::collections::HashSet::new();
                                relay_urls.retain(|url| seen.insert(url.trim_end_matches('/').to_string()));
                                if relay_urls.is_empty() {
                                    is_open.set(false);
                                    toast_api.warning(
                                        "No relays configured".to_string(),
                                        ToastOptions::new()
                                            .description("Add write relays or broadcast relays in Settings")
                                            .duration(Duration::from_secs(3))
                                            .permanent(false),
                                    );
                                    return;
                                }
                                let event = event_broadcast.clone();
                                is_broadcasting.set(true);
                                is_open.set(false);
                                spawn(async move {
                                    match nostr_client::broadcast_presigned_event(event, relay_urls).await {
                                        Ok(result) => {
                                            if result.is_success() {
                                                toast_api.success(
                                                    "Broadcast queued".to_string(),
                                                    ToastOptions::new()
                                                        .duration(Duration::from_secs(3))
                                                        .permanent(false),
                                                );
                                            } else {
                                                toast_api.error(
                                                    "Broadcast failed".to_string(),
                                                    ToastOptions::new()
                                                        .duration(Duration::from_secs(3))
                                                        .permanent(false),
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            toast_api.error(
                                                "Broadcast failed".to_string(),
                                                ToastOptions::new()
                                                    .description(e)
                                                    .duration(Duration::from_secs(3))
                                                    .permanent(false),
                                            );
                                        }
                                    }
                                    is_broadcasting.set(false);
                                });
                            },
                            span { class: "text-sm",
                                if *is_broadcasting.read() {
                                    "Broadcasting..."
                                } else {
                                    "Broadcast"
                                }
                            }
                        }
                    }
                    div { class: "h-px bg-border my-1" }
                    if is_own_note && event.kind == Kind::TextNote {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_edit_modal.set(true);
                                is_open.set(false);
                            },
                            span { class: "text-sm", "Edit Post" }
                        }
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                            disabled: *is_deleting.read(),
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_delete_confirm.set(true);
                                is_open.set(false);
                            },
                            span { class: "text-sm",
                                if *is_deleting.read() { "Deleting..." } else { "Delete Note" }
                            }
                        }
                    }
                    if !is_own_note && event.kind == Kind::TextNote {
                        button {
                            class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_propose_modal.set(true);
                                is_open.set(false);
                            },
                            span { class: "text-sm", "Propose an Edit" }
                        }
                    }
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-muted-foreground",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            log::info!("Mute post: {}", event_id_mute);
                            is_open.set(false);
                            let event_id = event_id_mute.clone();
                            spawn(async move {
                                match nostr_client::mute_post(event_id).await {
                                    Ok(_) => log::info!("Post muted successfully"),
                                    Err(e) => log::error!("Failed to mute post: {}", e),
                                }
                            });
                        },
                        span { class: "text-sm", "Mute post" }
                    }
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-muted-foreground",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            log::info!("Block user: {}", author_pubkey_block);
                            is_open.set(false);
                            let pubkey = author_pubkey_block.clone();
                            spawn(async move {
                                match nostr_client::block_user(pubkey).await {
                                    Ok(_) => log::info!("User blocked successfully"),
                                    Err(e) => log::error!("Failed to block user: {}", e),
                                }
                            });
                        },
                        span { class: "text-sm", "Block user" }
                    }
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-red-500 hover:text-red-600",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            log::info!("Report post: {}", event_id_report);
                            show_report_modal.set(true);
                            is_open.set(false);
                        },
                        span { class: "text-sm", "Report post" }
                    }
                }
            }
        }
        if *show_report_modal.read() {
            ReportModal {
                event_id: event_id_modal_report.clone(),
                author_pubkey: author_pubkey_modal.clone(),
                on_close: move |_| {
                    show_report_modal.set(false);
                },
            }
        }
        if *show_share_modal.read() {
            ShareModal {
                event: event_share.clone(),
                on_close: move |_| show_share_modal.set(false),
            }
        }
        if *show_add_to_list_modal.read() {
            AddToListModal {
                event_id: event_id_modal_list.clone(),
                author_pubkey: author_pubkey_modal_list.clone(),
                on_close: move |_| show_add_to_list_modal.set(false),
            }
        }
        if *show_pin_to_board_modal.read() {
            PinToBoardModal {
                reference: PinReference::Event {
                    id: event_id_pin_board.clone(),
                    relay_hint: None,
                },
                content_type: PinContentType::Note,
                on_close: move |_| show_pin_to_board_modal.set(false),
            }
        }
        if *show_ai_chat_confirm.read() {
            ConfirmModal {
                title: "Start a new AI chat?".to_string(),
                message: "This opens AI Chat with this note as context and switches away from your current saved conversation.".to_string(),
                confirm_text: Some("Start new chat".to_string()),
                cancel_text: Some("Cancel".to_string()),
                on_confirm: move |_| {
                    if let Some(payload) = pending_ai_chat_seed.read().clone() {
                        queue_ai_chat_seed(payload);
                        navigator.push(Route::AIChat {});
                    }
                    pending_ai_chat_seed.set(None);
                    show_ai_chat_confirm.set(false);
                },
                on_cancel: move |_| {
                    pending_ai_chat_seed.set(None);
                    show_ai_chat_confirm.set(false);
                },
            }
        }
        if *show_edit_modal.read() {
            EditPostView {
                original_event: event_edit.clone(),
                on_close: move |_| show_edit_modal.set(false),
                on_success: move |_| show_edit_modal.set(false),
            }
        }
        if *show_propose_modal.read() {
            EditPostView {
                original_event: event_edit.clone(),
                is_proposal: true,
                on_close: move |_| show_propose_modal.set(false),
                on_success: move |_| show_propose_modal.set(false),
            }
        }
        if *show_delete_confirm.read() {
            ConfirmModal {
                title: "Delete Note?".to_string(),
                message: "This will publish a deletion request to your relays. There is no guarantee that all relays will honor this request or that the note will be permanently removed.".to_string(),
                confirm_text: Some("Delete".to_string()),
                cancel_text: Some("Cancel".to_string()),
                on_confirm: move |_| {
                    show_delete_confirm.set(false);
                    is_deleting.set(true);
                    let event_id = event_id_delete.clone();
                    let toast_api = toast;
                    spawn(async move {
                        use nostr_sdk::nips::nip09::EventDeletionRequest;
                        use nostr_sdk::{EventBuilder, Tag, TagStandard};
                        let eid = EventId::from_hex(&event_id)
                            .or_else(|_| EventId::from_bech32(&event_id));
                        let event_id_parsed = match eid {
                            Ok(id) => id,
                            Err(e) => {
                                toast_api.error(
                                    "Error".to_string(),
                                    ToastOptions::new()
                                        .description(format!("Invalid event ID: {e}"))
                                        .duration(Duration::from_secs(3))
                                        .permanent(false),
                                );
                                is_deleting.set(false);
                                return;
                            }
                        };
                        let request = EventDeletionRequest::new().id(event_id_parsed);
                        let builder = EventBuilder::delete(request).tag(
                            Tag::from_standardized(TagStandard::Kind {
                                kind: Kind::TextNote,
                                uppercase: false,
                            }),
                        );
                        match crate::stores::publish_queue::signing::sign_event_builder(builder)
                            .await
                        {
                            Ok(signed_event) => {
                                let write_relays: Vec<String> =
                                    crate::stores::relay::get_write_relays();
                                crate::stores::publish_queue::enqueue(
                                    signed_event,
                                    crate::stores::publish_queue::types::QueueEventType::Other(
                                        "delete".to_string(),
                                    ),
                                    Some(write_relays),
                                    std::collections::HashMap::new(),
                                )
                                .await;
                                toast_api.success(
                                    "Deletion requested".to_string(),
                                    ToastOptions::new()
                                        .description(
                                            "A deletion request has been sent to your relays",
                                        )
                                        .duration(Duration::from_secs(3))
                                        .permanent(false),
                                );
                            }
                            Err(e) => {
                                toast_api.error(
                                    "Error".to_string(),
                                    ToastOptions::new()
                                        .description(format!("Failed to sign deletion: {e}"))
                                        .duration(Duration::from_secs(3))
                                        .permanent(false),
                                );
                            }
                        }
                        is_deleting.set(false);
                    });
                },
                on_cancel: move |_| {
                    show_delete_confirm.set(false);
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{format_note_context_for_ai, NOTE_AI_CONTENT_CHAR_LIMIT};
    use nostr_sdk::{EventBuilder, Keys, Kind, Timestamp};

    #[test]
    fn formats_note_context_for_ai_with_bech32_and_metadata() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "hello from nostr.blue")
            .custom_created_at(Timestamp::from(1_700_000_000))
            .sign_with_keys(&keys)
            .unwrap();

        let formatted = format_note_context_for_ai(&event);

        assert!(formatted.starts_with("Note for discussion\nAuthor: npub"));
        assert!(formatted.contains("\nCreated: "));
        assert!(formatted.contains("\nEvent ID: note1"));
        assert!(formatted.contains("\n\nContent:\nhello from nostr.blue"));
        assert!(formatted.contains("get_interaction_counts"));
    }

    #[test]
    fn truncates_long_note_content_for_ai_context() {
        let keys = Keys::generate();
        let content = "a".repeat(NOTE_AI_CONTENT_CHAR_LIMIT + 25);
        let expected = "a".repeat(NOTE_AI_CONTENT_CHAR_LIMIT);
        let event = EventBuilder::new(Kind::TextNote, content)
            .custom_created_at(Timestamp::from(1_700_000_000))
            .sign_with_keys(&keys)
            .unwrap();

        let formatted = format_note_context_for_ai(&event);

        assert!(formatted.contains(&format!("\n\nContent:\n{expected}")));
        assert!(formatted.contains("get_interaction_counts"));
    }
}
