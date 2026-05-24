use crate::components::toast::show_queued_toast;
use crate::components::{ComposerBody, DraftDiscardModal, RichContent};
use crate::hooks::use_composer_editor::{use_composer_editor, restore_draft_or_empty, ComposerConfig};
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::relay;
use crate::utils::custom_emoji::build_custom_emoji_tags;
use crate::utils::thread_tree::invalidate_thread_tree_cache;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::time::Duration;

fn get_relay_hint(parent_tags: &nostr_sdk::Tags) -> String {
    for tag in parent_tags.iter() {
        let tag_vec = tag.clone().to_vec();
        if tag_vec.len() >= 3 && tag_vec[0] == "e" && !tag_vec[2].is_empty() {
            return tag_vec[2].clone();
        }
    }
    relay::nip65::get_write_relays()
        .first()
        .cloned()
        .unwrap_or_default()
}

#[component]
pub fn ReplyComposer(
    target: NostrEvent,
    #[props(default = None)] root_event: Option<NostrEvent>,
    on_close: EventHandler<()>,
    on_success: EventHandler<NostrEvent>,
) -> Element {
    let has_signer = *HAS_SIGNER.read();
    let mut show_discard = use_signal(|| false);
    let toast = consume_toast();

    let is_note = target.kind == Kind::TextNote;
    let draft_ctx = format!(
        "{}_{}",
        if is_note { "reply" } else { "comment" },
        target.id.to_hex()
    );
    let initial_content = restore_draft_or_empty(&draft_ctx);

    let editor = use_composer_editor(ComposerConfig {
        draft_context: Some(draft_ctx.clone()),
        initial_content,
    });

    let mut thread_participants = Vec::new();
    thread_participants.push(target.pubkey);
    for pk in target.tags.public_keys() {
        if !thread_participants.contains(pk) {
            thread_participants.push(*pk);
        }
    }

    let publish_label = if is_note {
        "Reply".to_string()
    } else {
        "Comment".to_string()
    };
    let header_title = if is_note {
        "Reply".to_string()
    } else if root_event.is_some() {
        "Reply to Comment".to_string()
    } else {
        "Add Comment".to_string()
    };
    let placeholder = if is_note || root_event.is_some() {
        "Write your reply...".to_string()
    } else {
        "Write your comment...".to_string()
    };

    let short_author = truncate_pubkey(&target.pubkey.to_hex());
    let reply_content = target.content.clone();
    let reply_tags: Vec<_> = target.tags.iter().cloned().collect();

    let draft_ctx_for_publish = draft_ctx.clone();
    let handle_publish = move |_| {
        let content_value = editor.content_value();
        if content_value.is_empty() || *editor.is_over_limit.read() {
            return;
        }
        let mut is_publishing = editor.is_publishing;
        is_publishing.set(true);

        let target_event = target.clone();
        let root = root_event.clone();
        let content_for_publish = content_value.clone();
        let ctx_for_clear = draft_ctx_for_publish.clone();
        let toast_api = toast;

        spawn(async move {
            let content_owned = content_for_publish;
            let emoji_tags = build_custom_emoji_tags(&content_owned);
            let event_builder = if target_event.kind == Kind::TextNote {
                let hint = get_relay_hint(&target_event.tags);
                let relay_url = if hint.is_empty() {
                    None
                } else {
                    RelayUrl::parse(&hint).ok()
                };
                EventBuilder::text_note_reply(
                    &content_owned,
                    &target_event,
                    root.as_ref(),
                    relay_url,
                )
            } else {
                let (comment_to, comment_root) = match &root {
                    Some(parent) => (parent, Some(&target_event)),
                    None => (&target_event, None),
                };
                EventBuilder::comment(content_owned, comment_to, comment_root)
            };
            let event_builder = event_builder.tags(emoji_tags);
            let event_builder = if *editor.is_sensitive.read() {
                let reason = editor.sensitive_reason.read().clone();
                let cw_tag = nostr::Tag::from_standardized_without_cell(
                    nostr::event::tag::TagStandard::ContentWarning {
                        reason: if reason.is_empty() { None } else { Some(reason) },
                    },
                );
                event_builder.tag(cw_tag)
            } else {
                event_builder
            };
            let event_builder = if *editor.is_protected.read() {
                event_builder.tag(nostr::Tag::protected())
            } else {
                event_builder
            };
            let signed_event = match crate::stores::publish_queue::signing::sign_event_builder(
                event_builder,
            )
            .await
            {
                Ok(e) => e,
                Err(e) => {
                    log::error!("Failed to sign event: {}", e);
                    toast_api.error(
                        "Failed to publish".to_string(),
                        ToastOptions::new()
                            .description(e)
                            .duration(Duration::from_secs(3)),
                    );
                    is_publishing.set(false);
                    return;
                }
            };

            let event_id = signed_event.id.to_hex();
            crate::stores::publish_queue::enqueue(
                signed_event.clone(),
                crate::stores::publish_queue::types::QueueEventType::Note,
                None,
                std::collections::HashMap::new(),
            ).await;
            log::info!("Enqueued reply: {}", event_id);
            show_queued_toast(toast_api, "Reply");

            if let Some(ref r) = root {
                invalidate_thread_tree_cache(&r.id);
            } else {
                invalidate_thread_tree_cache(&target_event.id);
            }
            editor.clear();
            if let Some(pk) = crate::stores::auth_store::get_pubkey() {
                crate::stores::note_draft_store::clear_note_draft(&pk, &ctx_for_clear);
            }
            on_success.call(signed_event);

            is_publishing.set(false);
        });
    };

    let mut try_close = move || {
        if !editor.content.read().is_empty() {
            show_discard.set(true);
        } else {
            on_close.call(());
        }
    };

    let ctx_for_discard = draft_ctx.clone();

    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 z-50 flex items-start justify-center overflow-y-auto",
            onclick: move |_| try_close(),
            div {
                class: "bg-background border border-border rounded-lg shadow-xl w-full max-w-2xl m-4 mt-20",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between p-4 border-b border-border",
                    h2 { class: "text-xl font-bold", "{header_title}" }
                    button {
                        class: "text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| try_close(),
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-6 h-6",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M6 18L18 6M6 6l12 12",
                            }
                        }
                    }
                }
                if is_note {
                    div { class: "p-4 bg-muted border-b border-border",
                        div { class: "text-sm text-muted-foreground mb-2",
                            "Replying to @{short_author}"
                        }
                        div { class: "text-sm text-foreground line-clamp-3 overflow-hidden",
                            RichContent {
                                content: reply_content.clone(),
                                tags: reply_tags.clone(),
                            }
                        }
                    }
                }
                if !has_signer {
                    div { class: "text-center py-8 text-muted-foreground p-4",
                        p { "Sign in to reply" }
                    }
                } else {
                    div { class: "p-4",
                        ComposerBody {
                            editor,
                            placeholder,
                            textarea_rows: 6,
                            publish_label,
                            on_publish: handle_publish,
                            on_cancel: move |_| try_close(),
                            thread_participants: Some(thread_participants.clone()),
                        }
                    }
                }
            }
        }
        if *show_discard.read() {
            DraftDiscardModal {
                on_save: {
                    let ctx = ctx_for_discard.clone();
                    move |_| {
                        editor.clear();
                        show_discard.set(false);
                        on_close.call(());
                        let _ = ctx;
                    }
                },
                on_discard: {
                    let ctx = ctx_for_discard.clone();
                    move |_| {
                        editor.clear();
                        editor.clear_draft();
                        show_discard.set(false);
                        on_close.call(());
                        let _ = ctx;
                    }
                },
                on_continue: move |_| {
                    show_discard.set(false);
                },
            }
        }
    }
}
