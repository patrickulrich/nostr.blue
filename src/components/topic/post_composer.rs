//! Topic Post Composer Component
//! Textarea with topic selector for creating new topic posts
use crate::components::{MediaUploader, MentionAutocomplete};
use crate::hooks::use_composer_editor::new_textarea_id;
use crate::platform::editor_dom;
use crate::stores::auth_store;
use crate::stores::content::topic_draft_store;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::topic_store::{
    create_topic_post, create_topic_post_with_media, reply_to_topic_post,
    reply_to_topic_post_with_media, TopicPost,
};
use dioxus::prelude::*;
use nostr_sdk::PublicKey;

#[component]
pub fn TopicPostComposer(
    #[props(default)] topic: Option<String>,
    #[props(default)] reply_to: Option<TopicPost>,
    #[props(default)] on_success: Option<EventHandler<String>>,
) -> Element {
    let has_signer = *HAS_SIGNER.read();
    let mut draft_restored = use_signal(|| false);
    let mut content = use_signal(String::new);
    let mut topic_input = use_signal(|| topic.clone().unwrap_or_default());
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut show_media_uploader = use_signal(|| false);
    let mut media_urls: Signal<Vec<String>> = use_signal(Vec::new);
    let mut last_draft_save = use_signal(|| 0u64);
    let mut draft_restored_flag = use_signal(|| false);
    let textarea_id = use_signal(|| new_textarea_id("topic"));

    let is_reply = reply_to.is_some();
    let topic_locked = topic.is_some();
    let mut thread_participants = Vec::new();
    if let Some(ref parent) = reply_to {
        if let Ok(pk) = PublicKey::parse(&parent.pubkey) {
            thread_participants.push(pk);
        }
    }

    if !has_signer {
        return rsx! {
            div {
                class: "bg-muted border border-border rounded-lg p-4 text-center text-muted-foreground",
                "Sign in to post in topics"
            }
        };
    }

    if !*draft_restored.read() && !is_reply {
        if let Some(pk) = auth_store::get_pubkey() {
            if let Some(draft) = topic_draft_store::read_topic_draft(&pk) {
                if !draft.content.is_empty() && !topic_locked {
                    topic_input.set(draft.topic);
                    content.set(draft.content);
                    draft_restored_flag.set(true);
                }
            }
        }
        draft_restored.set(true);
    }

    let content_for_save = content.read().clone();
    let topic_for_save = topic_input.read().clone();
    let now = crate::platform::timestamp::now_secs();
    if !is_reply
        && !content_for_save.trim().is_empty()
        && now.saturating_sub(*last_draft_save.peek()) >= 2
    {
        if let Some(pk) = auth_store::get_pubkey() {
            last_draft_save.set(now);
            topic_draft_store::save_topic_draft(
                &pk,
                &topic_draft_store::TopicPostDraft {
                    topic: topic_for_save,
                    content: content_for_save,
                    saved_at: now,
                },
            );
        }
    }

    rsx! {
        div {
            class: "bg-card border border-border rounded-lg p-4",
            if *draft_restored_flag.read() && !content.read().trim().is_empty() {
                div {
                    class: "mb-2 text-xs text-muted-foreground italic",
                    "Draft restored"
                }
            }
            // Topic input (if not locked to a specific topic)
            if !topic_locked && !is_reply {
                div {
                    class: "mb-3",
                    label {
                        class: "block text-sm font-medium text-foreground mb-1",
                        "Topic"
                    }
                    div {
                        class: "flex items-center gap-1",
                        span { class: "text-muted-foreground font-medium", "#" }
                        input {
                            class: "flex-1 bg-muted border border-border rounded-md px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50",
                            r#type: "text",
                            placeholder: "topic name",
                            value: "{topic_input}",
                            oninput: move |e| topic_input.set(e.value()),
                        }
                    }
                }
            }
            // Content textarea
            {
                let participants = thread_participants.clone();
                rsx! {
                    MentionAutocomplete {
                        content,
                        on_input: move |new_value: String| {
                            content.set(new_value);
                        },
                        placeholder: if is_reply {
                            "Write a reply...".to_string()
                        } else {
                            "What's on your mind?".to_string()
                        },
                        rows: 3,
                        class: "w-full bg-muted border border-border rounded-md px-3 py-2 text-sm resize-y min-h-[80px] focus:outline-none focus:ring-2 focus:ring-primary/50".to_string(),
                        disabled: *submitting.read(),
                        textarea_id: Some(textarea_id),
                        thread_participants: participants,
                    }
                }
            }
            // Media previews
            if !media_urls.read().is_empty() {
                div {
                    class: "flex gap-2 mt-2 flex-wrap",
                    for (idx, url) in media_urls.read().iter().enumerate() {
                        div {
                            key: "{idx}",
                            class: "relative group",
                            img {
                                src: "{url}",
                                class: "w-16 h-16 object-cover rounded border border-border",
                            }
                            button {
                                class: "absolute -top-1 -right-1 w-5 h-5 bg-destructive text-white rounded-full text-xs flex items-center justify-center opacity-0 group-hover:opacity-100 transition",
                                onclick: {
                                    let mut urls_signal = media_urls;
                                    move |_| {
                                        let mut urls = urls_signal.write();
                                        let mut new_urls: Vec<String> = urls.drain(..).collect();
                                        new_urls.remove(idx);
                                        urls.extend(new_urls);
                                    }
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            // Action bar: media upload + submit
            div {
                class: "mt-2 flex items-center justify-between",
                div {
                    class: "flex items-center gap-2",
                    button {
                        class: if *show_media_uploader.read() {
                            "p-1.5 rounded-md bg-primary/10 text-primary transition"
                        } else {
                            "p-1.5 rounded-md hover:bg-accent text-muted-foreground transition"
                        },
                        onclick: move |_| show_media_uploader.set(!show_media_uploader()),
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            rect { x: "3", y: "3", width: "18", height: "18", rx: "2", ry: "2" }
                            circle { cx: "9", cy: "9", r: "2" }
                            path { d: "m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" }
                        }
                    }
                }
                // Error display
                if let Some(err) = &*error.read() {
                    span { class: "text-xs text-destructive flex-1 text-center", "{err}" }
                }
                // Submit button
                button {
                    class: "px-4 py-1.5 text-sm font-medium rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50",
                    disabled: content.read().trim().is_empty()
                        || (!is_reply && {
                            let raw = topic_input.read().trim().to_string();
                            let stripped = raw.strip_prefix('#').unwrap_or(&raw);
                            !stripped.chars().any(|c| c.is_ascii_alphanumeric())
                        })
                        || *submitting.read(),
                    onclick: move |_| {
                        let text = content.read().trim().to_string();
                        let topic_name = if is_reply {
                            String::new()
                        } else if topic_locked {
                            topic.clone().unwrap_or_default()
                        } else {
                            let raw = topic_input.read().trim().to_string();
                            let stripped = raw.strip_prefix('#').unwrap_or(&raw);
                            let sanitized: String = stripped
                                .to_lowercase()
                                .chars()
                                .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
                                .collect();
                            let topic_name = sanitized.split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
                            if topic_name.is_empty() {
                                String::new()
                            } else {
                                topic_name
                            }
                        };

                        if text.is_empty() {
                            return;
                        }

                        let reply = reply_to.clone();
                        let on_success = on_success;
                        let urls = media_urls.read().clone();
                        let clear_id = (*textarea_id.read()).clone();

                        submitting.set(true);
                        error.set(None);

                        spawn(async move {
                            let result = if let Some(parent) = &reply {
                                if urls.is_empty() {
                                    reply_to_topic_post(parent, &text).await
                                } else {
                                    reply_to_topic_post_with_media(parent, &text, urls).await
                                }
                            } else if urls.is_empty() {
                                create_topic_post(&topic_name, &text).await
                            } else {
                                create_topic_post_with_media(&topic_name, &text, urls).await
                            };

                            match result {
                                Ok(event_id) => {
                                    content.set(String::new());
                                    // Uncontrolled textarea: clear the DOM imperatively.
                                    editor_dom::write_value_and_caret(&clear_id, "", 0).await;
                                    media_urls.write().clear();
                                    show_media_uploader.set(false);
                                    if !is_reply {
                                        if let Some(pk) = auth_store::get_pubkey() {
                                            topic_draft_store::clear_topic_draft(&pk);
                                        }
                                    }
                                    draft_restored_flag.set(false);
                                    if let Some(handler) = on_success {
                                        handler.call(event_id);
                                    }
                                }
                                Err(e) => {
                                    error.set(Some(e));
                                }
                            }
                            submitting.set(false);
                        });
                    },
                    if *submitting.read() {
                        "Posting..."
                    } else if is_reply {
                        "Reply"
                    } else {
                        "Post"
                    }
                }
            }
            // Media uploader (collapsible)
            if *show_media_uploader.read() {
                div {
                    class: "mt-2",
                    MediaUploader {
                        on_upload: move |url: String| {
                            media_urls.write().push(url);
                            show_media_uploader.set(false);
                        },
                        button_label: "Upload Image".to_string(),
                        show_server_selector: false,
                    }
                }
            }
        }
    }
}
