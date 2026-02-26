//! Topic Post Composer Component
//! Textarea with topic selector for creating new topic posts
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::topic_store::{create_topic_post, reply_to_topic_post, TopicPost};
use dioxus::prelude::*;

/// Composer for creating new topic posts or replies
#[component]
pub fn TopicPostComposer(
    #[props(default)]
    topic: Option<String>,
    #[props(default)]
    reply_to: Option<TopicPost>,
    #[props(default)]
    on_success: Option<EventHandler<String>>,
) -> Element {
    let has_signer = *HAS_SIGNER.read();
    let mut content = use_signal(String::new);
    let mut topic_input = use_signal(|| topic.clone().unwrap_or_default());
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let is_reply = reply_to.is_some();
    let topic_locked = topic.is_some();

    if !has_signer {
        return rsx! {
            div {
                class: "bg-muted border border-border rounded-lg p-4 text-center text-muted-foreground",
                "Sign in to post in topics"
            }
        };
    }

    rsx! {
        div {
            class: "bg-card border border-border rounded-lg p-4",
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
            textarea {
                class: "w-full bg-muted border border-border rounded-md px-3 py-2 text-sm resize-y min-h-[80px] focus:outline-none focus:ring-2 focus:ring-primary/50",
                placeholder: if is_reply { "Write a reply..." } else { "What's on your mind?" },
                value: "{content}",
                oninput: move |e| content.set(e.value()),
            }
            // Error display
            if let Some(err) = &*error.read() {
                div {
                    class: "mt-2 text-sm text-destructive",
                    "{err}"
                }
            }
            // Submit button
            div {
                class: "mt-2 flex justify-end",
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
                            String::new() // Not used for replies
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
                            // Collapse consecutive hyphens and trim
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

                        submitting.set(true);
                        error.set(None);

                        spawn(async move {
                            let result = if let Some(parent) = &reply {
                                reply_to_topic_post(parent, &text).await
                            } else {
                                create_topic_post(&topic_name, &text).await
                            };

                            match result {
                                Ok(event_id) => {
                                    content.set(String::new());
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
        }
    }
}
