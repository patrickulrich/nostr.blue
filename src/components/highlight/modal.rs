//! Highlight Modal Component
//!
//! A modal that allows users to add an optional comment before publishing a highlight.
use dioxus::events::MouseData;
use dioxus::html::input_data::keyboard_types::Key;
use dioxus::prelude::*;
/// Props for the HighlightModal component
#[derive(Props, Clone, PartialEq)]
pub struct HighlightModalProps {
    /// The text being highlighted
    pub content: String,
    /// Human-readable reference (e.g., "John 3:16 (BSB)")
    pub reference: String,
    /// Called when user confirms (with optional comment)
    pub on_confirm: EventHandler<Option<String>>,
    /// Called when user cancels
    pub on_cancel: EventHandler<()>,
    /// Optional external submitting state controlled by parent.
    /// When Some(false), resets local is_submitting to false.
    /// Use this when the parent closes and reopens the modal to ensure fresh state.
    #[props(default)]
    pub submitting: Option<bool>,
}
/// Modal for creating highlights with optional commentary
#[component]
pub fn HighlightModal(props: HighlightModalProps) -> Element {
    let mut comment = use_signal(String::new);
    let mut is_submitting = use_signal(|| false);
    let parent_submitting = props.submitting;
    use_effect(use_reactive!(|parent_submitting| {
        if parent_submitting == Some(false) {
            is_submitting.set(false);
        }
    }));
    let effective_submitting = props.submitting.unwrap_or(*is_submitting.read());
    let do_cancel = {
        let on_cancel = props.on_cancel;
        let parent_submitting = props.submitting;
        move || {
            let currently_submitting = parent_submitting.unwrap_or(*is_submitting.read());
            if currently_submitting {
                return;
            }
            is_submitting.set(false);
            on_cancel.call(());
        }
    };
    let do_submit = {
        let on_confirm = props.on_confirm;
        let parent_submitting = props.submitting;
        move || {
            let currently_submitting = parent_submitting.unwrap_or(*is_submitting.read());
            if currently_submitting {
                return;
            }
            is_submitting.set(true);
            let comment_value = comment.read().clone();
            let comment_opt = if comment_value.trim().is_empty() {
                None
            } else {
                Some(comment_value)
            };
            on_confirm.call(comment_opt);
        }
    };
    let on_submit_click = {
        let mut do_submit = do_submit;
        move |_: Event<MouseData>| {
            do_submit();
        }
    };
    let handle_keydown = {
        let mut do_cancel = do_cancel;
        move |evt: KeyboardEvent| {
            if evt.key() == Key::Escape {
                evt.stop_propagation();
                do_cancel();
            }
        }
    };
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 backdrop-blur-sm z-50 flex items-center justify-center p-4",
            onclick: {
                let mut do_cancel = do_cancel;
                move |_| do_cancel()
            },
            onkeydown: handle_keydown,
            div {
                class: "bg-card rounded-lg border border-border max-w-md w-full max-h-[80vh] overflow-hidden shadow-xl",
                onclick: move |e| e.stop_propagation(),
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "highlight-modal-title",
                tabindex: "-1",
                onmounted: move |_evt| {
                    #[cfg(feature = "web")]
                    {
                        if let Some(html_element) = _evt.data().downcast::<web_sys::HtmlElement>() {
                            let _ = html_element.focus();
                        }
                    }
                },
                div { class: "flex items-center justify-between p-4 border-b border-border",
                    h2 {
                        id: "highlight-modal-title",
                        class: "text-lg font-semibold",
                        "Create Highlight"
                    }
                    button {
                        class: "p-1 hover:bg-accent rounded transition",
                        "aria-label": "Close",
                        onclick: {
                            let mut do_cancel = do_cancel;
                            move |_| do_cancel()
                        },
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-5 h-5",
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
                div { class: "p-4 space-y-4 overflow-y-auto",
                    div { class: "border-l-4 border-primary pl-4 py-2",
                        p { class: "italic text-foreground leading-relaxed", "{props.content}" }
                        p { class: "text-sm text-muted-foreground mt-2", "— {props.reference}" }
                    }
                    div { class: "space-y-2",
                        label {
                            class: "text-sm font-medium",
                            r#for: "highlight-comment",
                            "Add a comment (optional)"
                        }
                        textarea {
                            id: "highlight-comment",
                            class: "w-full p-3 bg-background border border-border rounded-lg resize-none focus:outline-none focus:ring-2 focus:ring-primary transition",
                            rows: 3,
                            placeholder: "Share your thoughts about this passage...",
                            value: "{comment}",
                            oninput: move |e| comment.set(e.value()),
                            onkeydown: {
                                let mut do_submit = do_submit;
                                move |evt: KeyboardEvent| {
                                    if evt.key() == Key::Enter
                                        && (evt.modifiers().ctrl() || evt.modifiers().meta())
                                    {
                                        evt.stop_propagation();
                                        evt.prevent_default();
                                        do_submit();
                                    }
                                }
                            },
                        }
                        p { class: "text-xs text-muted-foreground", "Press Ctrl/⌘+Enter to save" }
                    }
                }
                div { class: "flex justify-end gap-3 p-4 border-t border-border",
                    button {
                        class: "px-4 py-2 rounded-lg hover:bg-accent transition-colors",
                        disabled: effective_submitting,
                        onclick: {
                            let mut do_cancel = do_cancel;
                            move |_| do_cancel()
                        },
                        "Cancel"
                    }
                    button {
                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors disabled:opacity-50",
                        disabled: effective_submitting,
                        onclick: on_submit_click,
                        if effective_submitting {
                            "Saving..."
                        } else {
                            "Save Highlight"
                        }
                    }
                }
            }
        }
    }
}
