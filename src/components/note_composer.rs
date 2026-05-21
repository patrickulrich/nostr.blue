use crate::components::{ComposerBody, DraftDiscardModal};
use crate::components::toast::show_queued_toast;
use crate::hooks::use_composer_editor::{use_composer_editor, restore_draft_or_empty, ComposerConfig};
use crate::stores::{auth_store, feed_cache, nostr_client::publish_note_tracked};
use crate::utils::repost::FeedItem;
use dioxus::prelude::*;
use dioxus_primitives::toast::consume_toast;

#[derive(Clone, PartialEq)]
pub enum NoteMode {
    Inline,
    FullPage { quote: Option<String> },
}

#[component]
pub fn NoteComposer(mode: NoteMode) -> Element {
    let navigator = navigator();
    let toast = consume_toast();
    let mut is_focused = use_signal(|| false);
    let mut publish_feedback = use_signal(|| Option::<(bool, String)>::None);
    let mut feedback_version = use_signal(|| 0u32);
    let mut show_discard = use_signal(|| false);
    let is_authenticated = use_memo(move || auth_store::AUTH_STATE.read().is_authenticated);

    let initial_content = match &mode {
        NoteMode::FullPage { quote } => quote
            .as_ref()
            .map(|q| {
                let clean = q.strip_prefix("nostr:").unwrap_or(q);
                format!("\nnostr:{}", clean)
            })
            .unwrap_or_else(|| restore_draft_or_empty("root")),
        NoteMode::Inline => restore_draft_or_empty("root"),
    };

    let editor = use_composer_editor(ComposerConfig {
        draft_context: Some("root".to_string()),
        initial_content,
    });

    let mode_for_publish = mode.clone();
    let handle_publish = move |_| {
        let content_value = editor.content_value();
        if content_value.is_empty() || *editor.is_over_limit.read() {
            return;
        }
        let mut is_publishing = editor.is_publishing;
        is_publishing.set(true);

        let content_warning = if *editor.is_sensitive.read() {
            let reason = editor.sensitive_reason.read().clone();
            Some(reason).filter(|r| !r.is_empty()).or(Some(String::new()))
        } else {
            None
        };

        match mode_for_publish {
            NoteMode::Inline => {
                let mut content = editor.content;
                let mut show_media_uploader = editor.show_media_uploader;
                let toast_api = toast;
                publish_feedback.set(None);
                spawn(async move {
                    match publish_note_tracked(content_value, Vec::new(), content_warning.clone()).await {
                        Ok(result) => {
                            log::info!("Note published: {}", result.event_id);
                            if result.is_success() {
                                if let Some(event) = result.event {
                                    feed_cache::push_optimistic_feed_item(
                                        FeedItem::OriginalPost(event)
                                    );
                                }
                                show_queued_toast(toast_api, "Note");
                                content.set(String::new());
                                editor.clear_draft();
                                show_media_uploader.set(false);
                                is_publishing.set(false);
                            } else {
                                feedback_version.set(feedback_version() + 1);
                                let current_version = feedback_version();
                                publish_feedback.set(Some((
                                    false,
                                    "Failed to publish".to_string(),
                                )));
                                is_publishing.set(false);
                                crate::platform::timer::sleep_ms(3000).await;
                                if feedback_version() == current_version {
                                    publish_feedback.set(None);
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to publish note: {}", e);
                            feedback_version.set(feedback_version() + 1);
                            let current_version = feedback_version();
                            publish_feedback.set(Some((false, format!("Error: {}", e))));
                            is_publishing.set(false);
                            crate::platform::timer::sleep_ms(5000).await;
                            if feedback_version() == current_version {
                                publish_feedback.set(None);
                            }
                        }
                    }
                });
            }
            NoteMode::FullPage { .. } => {
                let nav = navigator;
                let toast_api = toast;
                spawn(async move {
                    match publish_note_tracked(content_value, Vec::new(), content_warning.clone()).await {
                        Ok(result) => {
                            if let Some(event) = result.event {
                                feed_cache::push_optimistic_feed_item(
                                    FeedItem::OriginalPost(event)
                                );
                            }
                            show_queued_toast(toast_api, "Note");
                            editor.clear();
                            editor.clear_draft();
                            nav.push(crate::routes::Route::Home {
                                list: String::new(),
                            });
                        }
                        Err(e) => {
                            log::error!("Failed to publish note: {}", e);
                        }
                    }
                    is_publishing.set(false);
                });
            }
        }
    };

    let textarea_rows: u32 = match &mode {
        NoteMode::Inline => {
            if *is_focused.read() || *editor.char_count.read() > 0 {
                4
            } else {
                2
            }
        }
        NoteMode::FullPage { .. } => 8,
    };

    if matches!(mode, NoteMode::FullPage { .. }) && !*is_authenticated.read() {
        return rsx! {
            div {
                class: "flex items-center justify-center h-screen",
                onmounted: move |_| {
                    navigator.push(crate::routes::Route::Home { list: String::new() });
                },
                "Redirecting..."
            }
        };
    }

    match &mode {
        NoteMode::Inline => rsx! {
            div { class: "border-b border-border p-4 bg-background",
                if !*is_authenticated.read() {
                    div { class: "text-center py-8 text-muted-foreground",
                        p { "Sign in to create posts" }
                    }
                } else {
                    if let Some((is_success, message)) = publish_feedback.read().clone() {
                        div {
                            class: if is_success {
                                "mb-3 p-3 rounded-lg bg-yellow-500/10 border border-yellow-500/20 text-yellow-600 dark:text-yellow-400 text-sm flex items-center gap-2"
                            } else {
                                "mb-3 p-3 rounded-lg bg-red-500/10 border border-red-500/20 text-red-600 dark:text-red-400 text-sm flex items-center gap-2"
                            },
                            span { "{message}" }
                            button {
                                class: "ml-auto text-current opacity-60 hover:opacity-100",
                                onclick: move |_| publish_feedback.set(None),
                                "×"
                            }
                        }
                    }
                    ComposerBody {
                        editor,
                        placeholder: "What's happening?".to_string(),
                        textarea_rows,
                        publish_label: "Post".to_string(),
                        on_publish: handle_publish,
                        on_cancel: move |_| {
                            if !editor.content.read().is_empty() {
                                show_discard.set(true);
                            } else {
                                editor.clear();
                                editor.clear_draft();
                                is_focused.set(false);
                            }
                        },
                        on_focus: move |_| is_focused.set(true),
                        thread_participants: None,
                    }
                }
            }
            if *show_discard.read() {
                DraftDiscardModal {
                    on_save: move |_| {
                        editor.clear();
                        show_discard.set(false);
                        is_focused.set(false);
                    },
                    on_discard: move |_| {
                        editor.clear();
                        editor.clear_draft();
                        show_discard.set(false);
                        is_focused.set(false);
                    },
                    on_continue: move |_| {
                        show_discard.set(false);
                    },
                }
            }
        },
        NoteMode::FullPage { quote } => {
            let mut try_close = move || {
                if !editor.content.read().is_empty() {
                    show_discard.set(true);
                } else {
                    navigator.go_back();
                }
            };
            rsx! {
                div {
                    class: "fixed inset-0 bg-black/50 z-50 flex items-start justify-center overflow-y-auto",
                    onclick: move |_| try_close(),
                    div {
                        class: "bg-background border border-border rounded-lg shadow-xl w-full max-w-2xl m-4 mt-20",
                        onclick: move |e| e.stop_propagation(),
                        div { class: "flex items-center justify-between p-4 border-b border-border",
                            h2 { class: "text-xl font-bold",
                                if quote.is_some() { "Quote Note" } else { "Create Note" }
                            }
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
                        div { class: "p-4",
                            ComposerBody {
                                editor,
                                placeholder: "What's on your mind?".to_string(),
                                textarea_rows,
                                textarea_class: Some("w-full min-h-[200px] p-3 bg-background border border-border rounded-lg resize-y focus:outline-hidden focus:ring-2 focus:ring-blue-500".to_string()),
                                publish_label: "Post".to_string(),
                                on_publish: handle_publish,
                                thread_participants: None,
                            }
                        }
                    }
                }
                if *show_discard.read() {
                    DraftDiscardModal {
                        on_save: move |_| {
                            editor.clear();
                            show_discard.set(false);
                            navigator.go_back();
                        },
                        on_discard: move |_| {
                            editor.clear();
                            editor.clear_draft();
                            show_discard.set(false);
                            navigator.go_back();
                        },
                        on_continue: move |_| {
                            show_discard.set(false);
                        },
                    }
                }
            }
        },
    }
}
