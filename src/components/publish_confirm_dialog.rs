//! Publish Confirmation Dialog Component
//!
//! A dialog for confirming article publication with preview
//! and optional "promote to feed" functionality.

use dioxus::prelude::*;

use crate::utils::markdown::render_markdown;
use super::dialog::{DialogRoot, DialogContent, DialogTitle, DialogDescription};

/// Configuration for what to publish
#[derive(Clone, Debug, Default)]
pub struct PublishConfig {
    /// Whether to create a Kind 1 note promoting the article
    pub promote_to_feed: bool,
}

#[derive(Props, Clone, PartialEq)]
pub struct PublishConfirmDialogProps {
    /// Whether the dialog is open
    pub open: Signal<bool>,
    /// Article title
    pub title: String,
    /// Article summary (optional)
    #[props(default)]
    pub summary: String,
    /// Article content (markdown)
    pub content: String,
    /// Cover image URL (optional)
    #[props(default)]
    pub cover_image: String,
    /// Callback when user confirms publish
    pub on_confirm: EventHandler<PublishConfig>,
    /// Callback when user cancels
    pub on_cancel: EventHandler<()>,
    /// Whether publishing is in progress
    #[props(default = false)]
    pub is_publishing: bool,
}

#[component]
pub fn PublishConfirmDialog(props: PublishConfirmDialogProps) -> Element {
    let mut promote_to_feed = use_signal(|| false);
    let mut open = props.open;

    // Clone content for use in memo and later
    let content_for_preview = props.content.clone();
    let content_for_count = props.content.clone();

    // Reset promote checkbox when dialog opens
    use_effect(move || {
        if *open.read() {
            promote_to_feed.set(false);
        }
    });

    // Generate content preview (first 500 chars of rendered markdown)
    let content_preview = use_memo(move || {
        let content = &content_for_preview;
        if content.len() > 500 {
            let truncated = &content[..500.min(content.len())];
            // Try to end at a word boundary
            let end = truncated.rfind(' ').unwrap_or(truncated.len());
            format!("{}...", render_markdown(&truncated[..end]))
        } else {
            render_markdown(content)
        }
    });

    // Calculate read time
    let word_count = content_for_count.split_whitespace().count();
    let read_time = (word_count as f32 / 200.0).ceil() as usize;

    let handle_confirm = {
        let on_confirm = props.on_confirm;
        move |_| {
            on_confirm.call(PublishConfig {
                promote_to_feed: *promote_to_feed.read(),
            });
        }
    };

    let handle_cancel = {
        let on_cancel = props.on_cancel;
        move |_| {
            open.set(false);
            on_cancel.call(());
        }
    };

    rsx! {
        DialogRoot {
            open: *open.read(),
            on_open_change: move |is_open: bool| {
                open.set(is_open);
                if !is_open {
                    props.on_cancel.call(());
                }
            },

            DialogContent {
                // Dialog container with max width
                div {
                    class: "w-full max-w-2xl max-h-[80vh] flex flex-col",

                    // Header
                    div {
                        class: "flex items-center justify-between p-4 border-b border-border",

                        DialogTitle {
                            h2 {
                                class: "text-xl font-bold",
                                "Publish Article"
                            }
                        }

                        // Close button
                        button {
                            class: "p-2 rounded-full hover:bg-accent text-muted-foreground hover:text-foreground transition",
                            onclick: handle_cancel,
                            disabled: props.is_publishing,
                            svg {
                                class: "w-5 h-5",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                view_box: "0 0 24 24",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    d: "M6 18L18 6M6 6l12 12"
                                }
                            }
                        }
                    }

                    DialogDescription {
                        // Scrollable preview area
                        div {
                            class: "flex-1 overflow-y-auto p-4",

                            // Article preview card
                            div {
                                class: "border border-border rounded-lg overflow-hidden bg-card",

                                // Cover image
                                if !props.cover_image.is_empty() {
                                    img {
                                        src: "{props.cover_image}",
                                        class: "w-full h-40 object-cover",
                                        alt: "Cover image",
                                    }
                                }

                                // Content
                                div {
                                    class: "p-4 space-y-3",

                                    // Title
                                    h3 {
                                        class: "text-lg font-bold line-clamp-2",
                                        "{props.title}"
                                    }

                                    // Summary or content preview
                                    if !props.summary.is_empty() {
                                        p {
                                            class: "text-muted-foreground text-sm line-clamp-3",
                                            "{props.summary}"
                                        }
                                    } else {
                                        div {
                                            class: "text-sm text-muted-foreground prose prose-sm dark:prose-invert max-w-none line-clamp-4",
                                            dangerous_inner_html: "{content_preview}",
                                        }
                                    }

                                    // Metadata
                                    div {
                                        class: "flex items-center gap-4 text-xs text-muted-foreground",

                                        span {
                                            class: "flex items-center gap-1",
                                            svg {
                                                class: "w-3.5 h-3.5",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                view_box: "0 0 24 24",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
                                                }
                                            }
                                            "{read_time} min read"
                                        }

                                        span {
                                            class: "flex items-center gap-1",
                                            svg {
                                                class: "w-3.5 h-3.5",
                                                fill: "none",
                                                stroke: "currentColor",
                                                stroke_width: "2",
                                                view_box: "0 0 24 24",
                                                path {
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    d: "M7 8h10M7 12h4m1 8l-4-4H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-3l-4 4z"
                                                }
                                            }
                                            "{word_count} words"
                                        }
                                    }
                                }
                            }

                            // Promote to feed option
                            div {
                                class: "mt-6 p-4 border border-border rounded-lg bg-muted/30",

                                label {
                                    class: "flex items-start gap-3 cursor-pointer",

                                    input {
                                        class: "mt-1 w-4 h-4 rounded border-border text-primary focus:ring-primary focus:ring-offset-0",
                                        r#type: "checkbox",
                                        checked: *promote_to_feed.read(),
                                        disabled: props.is_publishing,
                                        onchange: move |e| promote_to_feed.set(e.checked()),
                                    }

                                    div {
                                        class: "flex-1",

                                        span {
                                            class: "font-medium text-sm",
                                            "Share to your feed"
                                        }

                                        p {
                                            class: "text-xs text-muted-foreground mt-0.5",
                                            "Post a note linking to this article so your followers see it"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Footer with actions
                    div {
                        class: "flex items-center justify-end gap-3 p-4 border-t border-border",

                        button {
                            class: "px-4 py-2 text-sm font-medium rounded-lg hover:bg-accent text-muted-foreground hover:text-foreground transition",
                            onclick: handle_cancel,
                            disabled: props.is_publishing,
                            "Cancel"
                        }

                        button {
                            class: "px-6 py-2 text-sm font-medium rounded-lg bg-primary text-primary-foreground hover:bg-primary/90 transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                            onclick: handle_confirm,
                            disabled: props.is_publishing,

                            if props.is_publishing {
                                // Loading spinner
                                svg {
                                    class: "w-4 h-4 animate-spin",
                                    fill: "none",
                                    stroke: "currentColor",
                                    view_box: "0 0 24 24",
                                    circle {
                                        class: "opacity-25",
                                        cx: "12",
                                        cy: "12",
                                        r: "10",
                                        stroke: "currentColor",
                                        stroke_width: "4",
                                    }
                                    path {
                                        class: "opacity-75",
                                        fill: "currentColor",
                                        d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                                    }
                                }
                                "Publishing..."
                            } else {
                                // Publish icon
                                svg {
                                    class: "w-4 h-4",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    view_box: "0 0 24 24",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M12 19l9 2-9-18-9 18 9-2zm0 0v-8"
                                    }
                                }
                                "Publish Now"
                            }
                        }
                    }
                }
            }
        }
    }
}
