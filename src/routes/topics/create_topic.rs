use crate::components::ClientInitializing;
use crate::stores::nostr_client::{CLIENT_INITIALIZED, HAS_SIGNER};
use crate::stores::topic_store::{create_topic_metadata, create_topic_post, subscribe_to_topic};
use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn TopicCreate() -> Element {
    let has_signer = *HAS_SIGNER.read();
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut rules = use_signal(String::new);
    let mut first_post = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut step = use_signal(|| 1u8);
    let nav = navigator();

    if !*CLIENT_INITIALIZED.read() {
        return rsx! { ClientInitializing {} };
    }

    if !has_signer {
        return rsx! {
            div {
                class: "w-full max-w-2xl px-4 py-4",
                div {
                    class: "bg-muted border border-border rounded-lg p-8 text-center",
                    p { class: "text-muted-foreground", "Sign in to create a topic." }
                }
            }
        };
    }

    let sanitized_name: String = {
        let raw = name.read().trim().to_string();
        let stripped = raw.strip_prefix('#').unwrap_or(&raw);
        stripped
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    };

    rsx! {
        div {
            class: "w-full max-w-2xl px-4 py-4",
            Link {
                to: Route::TopicsHome {},
                class: "text-sm text-muted-foreground hover:text-foreground mb-4 inline-block",
                "← Back to Topics"
            }
            h1 { class: "text-2xl font-bold text-foreground mb-4", "Create Topic" }
            if *step.read() == 1 {
                div {
                    class: "bg-card border border-border rounded-lg p-4 space-y-4",
                    div {
                        label { class: "block text-sm font-medium text-foreground mb-1", "Topic name" }
                        div {
                            class: "flex items-center gap-1",
                            span { class: "text-muted-foreground font-medium text-lg", "#" }
                            input {
                                class: "flex-1 bg-muted border border-border rounded-md px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/50",
                                r#type: "text",
                                placeholder: "my-awesome-topic",
                                maxlength: "32",
                                value: "{name}",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                        p {
                            class: "text-xs text-muted-foreground mt-1",
                            "Lowercase alphanumeric and hyphens, 2-32 characters"
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-foreground mb-1", "Description (optional)" }
                        textarea {
                            class: "w-full bg-muted border border-border rounded-md px-3 py-2 text-sm resize-y min-h-[60px] focus:outline-none focus:ring-2 focus:ring-primary/50",
                            placeholder: "What is this topic about?",
                            maxlength: "500",
                            value: "{description}",
                            oninput: move |e| description.set(e.value()),
                        }
                        p {
                            class: "text-xs text-muted-foreground text-right",
                            "{description.read().len()}/500"
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-foreground mb-1", "Rules (optional)" }
                        textarea {
                            class: "w-full bg-muted border border-border rounded-md px-3 py-2 text-sm resize-y min-h-[60px] focus:outline-none focus:ring-2 focus:ring-primary/50",
                            placeholder: "1. Be respectful\n2. No spam\n3. Stay on topic",
                            maxlength: "1000",
                            value: "{rules}",
                            oninput: move |e| rules.set(e.value()),
                        }
                        p {
                            class: "text-xs text-muted-foreground text-right",
                            "{rules.read().len()}/1000"
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium text-foreground mb-1", "First post (optional)" }
                        textarea {
                            class: "w-full bg-muted border border-border rounded-md px-3 py-2 text-sm resize-y min-h-[60px] focus:outline-none focus:ring-2 focus:ring-primary/50",
                            placeholder: "Write the first post in this topic...",
                            value: "{first_post}",
                            oninput: move |e| first_post.set(e.value()),
                        }
                    }
                    if let Some(err) = &*error.read() {
                        div { class: "text-sm text-destructive", "{err}" }
                    }
                    div {
                        class: "flex justify-end",
                        button {
                            class: "px-4 py-2 text-sm font-medium rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50",
                            disabled: sanitized_name.len() < 2 || sanitized_name.len() > 32 || !sanitized_name.chars().any(|c| c.is_ascii_alphanumeric()),
                            onclick: move |_| {
                                error.set(None);
                                step.set(2);
                            },
                            "Review →"
                        }
                    }
                }
            } else {
                div {
                    class: "bg-card border border-border rounded-lg p-4 space-y-4",
                    h2 { class: "text-lg font-semibold", "Review" }
                    div {
                        class: "p-3 bg-muted rounded-md",
                        p { class: "text-lg font-bold", "#{sanitized_name}" }
                        if !description.read().is_empty() {
                            p { class: "text-sm text-muted-foreground mt-1", "{description}" }
                        }
                        if !rules.read().is_empty() {
                            div {
                                class: "mt-2",
                                p { class: "text-xs font-medium text-muted-foreground", "Rules:" }
                                p { class: "text-sm text-muted-foreground whitespace-pre-wrap", "{rules}" }
                            }
                        }
                    }
                    if !first_post.read().trim().is_empty() {
                        div {
                            class: "p-3 bg-muted rounded-md",
                            p { class: "text-xs font-medium text-muted-foreground mb-1", "First post:" }
                            p { class: "text-sm", "{first_post}" }
                        }
                    }
                    div {
                        class: "text-xs text-muted-foreground space-y-1",
                        p { "✓ Topic metadata will be published to your relays" }
                        p { "✓ You'll be auto-subscribed to #{sanitized_name}" }
                    }
                    if let Some(err) = &*error.read() {
                        div { class: "text-sm text-destructive", "{err}" }
                    }
                    div {
                        class: "flex justify-between",
                        button {
                            class: "px-4 py-2 text-sm font-medium rounded-md border border-border hover:bg-accent transition",
                            disabled: *submitting.read(),
                            onclick: move |_| {
                                error.set(None);
                                step.set(1);
                            },
                            "← Back"
                        }
                        button {
                            class: "px-4 py-2 text-sm font-medium rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50",
                            disabled: *submitting.read(),
                            onclick: move |_| {
                                let topic = sanitized_name.clone();
                                let desc = description.read().clone();
                                let rul = rules.read().clone();
                                let fp = first_post.read().trim().to_string();
                                submitting.set(true);
                                error.set(None);
                                spawn(async move {
                                    if !desc.is_empty() || !rul.is_empty() {
                                        if let Err(e) = create_topic_metadata(&topic, &desc, &rul).await {
                                            log::warn!("Failed to publish metadata: {}", e);
                                        }
                                    }
                                    let _ = subscribe_to_topic(&topic).await;
                                    if !fp.is_empty() {
                                        if let Err(e) = create_topic_post(&topic, &fp).await {
                                            error.set(Some(format!("Post failed: {}", e)));
                                            submitting.set(false);
                                            return;
                                        }
                                    }
                                    submitting.set(false);
                                    nav.push(Route::TopicFeed { topic });
                                });
                            },
                            if *submitting.read() { "Creating..." } else { "Create Topic" }
                        }
                    }
                }
            }
        }
    }
}
