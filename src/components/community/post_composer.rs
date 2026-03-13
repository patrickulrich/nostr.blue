//! Community Post Composer Component
//! Modal for creating new posts or replies in a community
use crate::components::{ConfirmModal, RichContent};
use crate::stores::community_store::{post_to_community, reply_to_post, Community, CommunityPost};
use crate::stores::nostr_client::HAS_SIGNER;
use crate::utils::validation::is_valid_http_url;
use dioxus::prelude::*;
/// Post composer modal for communities
#[component]
pub fn CommunityPostComposer(
    community: Community,
    #[props(default)] reply_to: Option<CommunityPost>,
    on_close: EventHandler<()>,
    #[props(default)] on_success: Option<EventHandler<String>>,
) -> Element {
    let mut content = use_signal(String::new);
    let mut posting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut show_discard_confirm = use_signal(|| false);
    let has_signer = *HAS_SIGNER.read();
    let is_reply = reply_to.is_some();
    let community_for_post = community.clone();
    let reply_to_for_post = reply_to.clone();
    let on_success_for_post = on_success;
    let on_close_for_post = on_close;
    let handle_submit = move |_| {
        let content_text = content.read().trim().to_string();
        if content_text.is_empty() {
            error.set(Some("Post content cannot be empty".to_string()));
            return;
        }
        let community = community_for_post.clone();
        let reply_to = reply_to_for_post.clone();
        let on_success = on_success_for_post;
        let on_close = on_close_for_post;
        posting.set(true);
        error.set(None);
        spawn(async move {
            let result = if let Some(parent) = reply_to {
                reply_to_post(&community, &parent, &content_text).await
            } else {
                post_to_community(&community, &content_text).await
            };
            match result {
                Ok(event_id) => {
                    log::info!("Community post published: {}", event_id);
                    posting.set(false);
                    content.set(String::new());
                    if let Some(handler) = on_success {
                        handler.call(event_id);
                    }
                    on_close.call(());
                }
                Err(e) => {
                    log::error!("Failed to publish post: {}", e);
                    error.set(Some(e));
                    posting.set(false);
                }
            }
        });
    };
    let on_close_backdrop = on_close;
    let mut request_close = move |handler: EventHandler<()>| {
        if !content.read().trim().is_empty() {
            show_discard_confirm.set(true);
        } else {
            handler.call(());
        }
    };
    let on_close_for_backdrop = on_close_backdrop;
    let on_close_for_button = on_close;
    let on_close_for_cancel = on_close;
    rsx! {
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-center justify-center p-4",
            onclick: move |_| request_close(on_close_for_backdrop),
            div {
                class: "bg-background rounded-lg p-6 max-w-lg w-full shadow-xl max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center justify-between mb-4",
                    h2 { class: "text-lg font-bold",
                        if is_reply {
                            "Reply to Post"
                        } else {
                            "New Post"
                        }
                    }
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition",
                        onclick: move |_| request_close(on_close_for_button),
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            line {
                                x1: "18",
                                y1: "6",
                                x2: "6",
                                y2: "18",
                            }
                            line {
                                x1: "6",
                                y1: "6",
                                x2: "18",
                                y2: "18",
                            }
                        }
                    }
                }
                div { class: "flex items-center gap-2 mb-4 p-2 bg-accent/50 rounded-lg",
                    if let Some(ref image) = community.image {
                        if is_valid_http_url(image) {
                            img {
                                class: "w-8 h-8 rounded-full object-cover",
                                src: "{image}",
                                alt: "Community",
                                referrerpolicy: "no-referrer",
                            }
                        } else {
                            div { class: "w-8 h-8 rounded-full bg-gradient-to-br from-purple-400 to-blue-500 flex items-center justify-center text-white text-sm",
                                svg {
                                    class: "w-4 h-4",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }
                                    circle { cx: "9", cy: "7", r: "4" }
                                    path { d: "M22 21v-2a4 4 0 0 0-3-3.87" }
                                    path { d: "M16 3.13a4 4 0 0 1 0 7.75" }
                                }
                            }
                        }
                    } else {
                        div { class: "w-8 h-8 rounded-full bg-gradient-to-br from-purple-400 to-blue-500 flex items-center justify-center text-white text-sm",
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }
                                circle { cx: "9", cy: "7", r: "4" }
                                path { d: "M22 21v-2a4 4 0 0 0-3-3.87" }
                                path { d: "M16 3.13a4 4 0 0 1 0 7.75" }
                            }
                        }
                    }
                    div {
                        p { class: "font-medium text-sm",
                            "{community.name.as_ref().unwrap_or(&community.d_tag)}"
                        }
                        p { class: "text-xs text-muted-foreground",
                            if is_reply {
                                "Replying to post"
                            } else {
                                "Posting to community"
                            }
                        }
                    }
                }
                if let Some(ref parent) = reply_to {
                    div { class: "mb-4 p-3 border-l-2 border-blue-500 bg-accent/30 rounded-r",
                        p { class: "text-sm text-muted-foreground mb-1", "Replying to:" }
                        div { class: "text-sm line-clamp-2",
                            RichContent {
                                content: parent.content.clone(),
                                tags: parent.event.tags.iter().cloned().collect(),
                            }
                        }
                    }
                }
                if let Some(ref err) = *error.read() {
                    div { class: "mb-4 p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg text-sm",
                        "{err}"
                    }
                }
                if !has_signer {
                    div { class: "mb-4 p-3 bg-yellow-100 dark:bg-yellow-900 text-yellow-800 dark:text-yellow-200 rounded-lg text-sm",
                        "You need to sign in to post to this community."
                    }
                }
                textarea {
                    class: "w-full p-3 border border-border rounded-lg bg-background min-h-[150px] focus:outline-hidden focus:ring-2 focus:ring-blue-500 resize-none",
                    placeholder: if is_reply { "Write your reply..." } else { "What's on your mind?" },
                    value: "{content}",
                    disabled: !has_signer || *posting.read(),
                    oninput: move |evt| content.set(evt.value()),
                }
                div { class: "mt-2 text-xs text-muted-foreground text-right",
                    "{content.read().chars().count()} characters"
                }
                div { class: "mt-4 flex justify-end gap-2",
                    button {
                        class: "px-4 py-2 border border-border rounded-lg hover:bg-accent transition",
                        onclick: move |_| request_close(on_close_for_cancel),
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: !has_signer || *posting.read() || content.read().trim().is_empty(),
                        onclick: handle_submit,
                        if *posting.read() {
                            span { class: "flex items-center gap-2",
                                span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" }
                                "Posting..."
                            }
                        } else if is_reply {
                            span { class: "flex items-center gap-2",
                                svg {
                                    class: "w-4 h-4",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    polyline { points: "9 17 4 12 9 7" }
                                    path { d: "M20 18v-2a4 4 0 0 0-4-4H4" }
                                }
                                "Reply"
                            }
                        } else {
                            span { class: "flex items-center gap-2",
                                svg {
                                    class: "w-4 h-4",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    line {
                                        x1: "22",
                                        y1: "2",
                                        x2: "11",
                                        y2: "13",
                                    }
                                    polygon { points: "22 2 15 22 11 13 2 9 22 2" }
                                }
                                "Post"
                            }
                        }
                    }
                }
            }
            if *show_discard_confirm.read() {
                ConfirmModal {
                    title: "Discard draft?".to_string(),
                    message: "You have unsaved content. Discard it and close this composer?".to_string(),
                    confirm_text: Some("Discard".to_string()),
                    cancel_text: Some("Keep editing".to_string()),
                    on_confirm: move |_| {
                        show_discard_confirm.set(false);
                        on_close.call(());
                    },
                    on_cancel: move |_| show_discard_confirm.set(false),
                }
            }
        }
    }
}
/// Inline composer (non-modal) for community posts
#[component]
pub fn CommunityPostComposerInline(
    community: Community,
    #[props(default)] on_success: Option<EventHandler<String>>,
) -> Element {
    let mut content = use_signal(String::new);
    let mut posting = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let has_signer = *HAS_SIGNER.read();
    let community_for_post = community.clone();
    let on_success_for_post = on_success;
    let handle_submit = move |_| {
        let content_text = content.read().trim().to_string();
        if content_text.is_empty() {
            error.set(Some("Post content cannot be empty".to_string()));
            return;
        }
        let community = community_for_post.clone();
        let on_success = on_success_for_post;
        posting.set(true);
        error.set(None);
        spawn(async move {
            match post_to_community(&community, &content_text).await {
                Ok(event_id) => {
                    log::info!("Community post published: {}", event_id);
                    content.set(String::new());
                    if let Some(handler) = on_success {
                        handler.call(event_id);
                    }
                }
                Err(e) => {
                    log::error!("Failed to publish post: {}", e);
                    error.set(Some(e));
                }
            }
            posting.set(false);
        });
    };
    if !has_signer {
        return rsx! {
            div { class: "p-4 border-b border-border bg-accent/30",
                p { class: "text-sm text-muted-foreground text-center",
                    "Sign in to post to this community"
                }
            }
        };
    }
    rsx! {
        div { class: "p-4 border-b border-border",
            if let Some(ref err) = *error.read() {
                div { class: "mb-3 p-2 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded text-sm",
                    "{err}"
                }
            }
            div { class: "flex gap-3",
                div { class: "flex-1",
                    textarea {
                        class: "w-full p-3 border border-border rounded-lg bg-background min-h-[80px] focus:outline-hidden focus:ring-2 focus:ring-blue-500 resize-none",
                        placeholder: "Share something with the community...",
                        value: "{content}",
                        disabled: *posting.read(),
                        oninput: move |evt| content.set(evt.value()),
                    }
                }
            }
            div { class: "mt-2 flex justify-end",
                button {
                    class: "px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition disabled:opacity-50 disabled:cursor-not-allowed",
                    disabled: *posting.read() || content.read().trim().is_empty(),
                    onclick: handle_submit,
                    if *posting.read() {
                        span { class: "flex items-center gap-2",
                            span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" }
                            "Posting..."
                        }
                    } else {
                        span { class: "flex items-center gap-2",
                            svg {
                                class: "w-4 h-4",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                line {
                                    x1: "22",
                                    y1: "2",
                                    x2: "11",
                                    y2: "13",
                                }
                                polygon { points: "22 2 15 22 11 13 2 9 22 2" }
                            }
                            "Post"
                        }
                    }
                }
            }
        }
    }
}
