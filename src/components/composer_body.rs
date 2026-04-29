use crate::components::icons::{BarChartIcon, CameraIcon, EyeOffIcon, MoreHorizontalIcon};
use crate::components::{EmojiPicker, GifPicker, MediaUploader, PollCreatorModal};
use crate::hooks::use_composer_editor::UseComposerEditor;
use dioxus::prelude::*;
use dioxus_primitives::dropdown_menu::{
    DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger,
};

#[derive(Props, Clone, PartialEq)]
pub struct ComposerBodyProps {
    pub editor: UseComposerEditor,
    pub placeholder: String,
    #[props(default = 4)]
    pub textarea_rows: u32,
    #[props(default)]
    pub textarea_class: Option<String>,
    pub publish_label: String,
    pub on_publish: EventHandler<()>,
    #[props(default)]
    pub on_cancel: Option<EventHandler<()>>,
    #[props(default)]
    pub on_focus: Option<EventHandler<()>>,
    #[props(default)]
    pub thread_participants: Option<Vec<nostr_sdk::PublicKey>>,
}

#[component]
pub fn ComposerBody(props: ComposerBodyProps) -> Element {
    let editor = props.editor;
    let content = editor.content;
    let cursor_position = editor.cursor_position;
    let mut show_media_uploader = editor.show_media_uploader;
    let show_poll_modal = editor.show_poll_modal;
    let is_publishing = editor.is_publishing;
    let mut is_sensitive = editor.is_sensitive;
    let mut sensitive_reason = editor.sensitive_reason;
    let char_count = editor.char_count;
    let is_over_limit = editor.is_over_limit;
    let can_publish = editor.can_publish;
    let counter_color = editor.counter_color;

    let textarea_class = props
        .textarea_class
        .clone()
        .unwrap_or_else(|| {
            "w-full p-3 text-lg bg-transparent border border-input rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring resize-none"
                .to_string()
        });

    let on_cancel_prop = props.on_cancel;
    let on_publish_prop = props.on_publish;

    rsx! {
        div { class: "w-full",
            super::MentionAutocomplete {
                content,
                on_input: move |new_value: String| {
                    let mut c = content;
                    c.set(new_value);
                },
                placeholder: props.placeholder.clone(),
                rows: props.textarea_rows,
                class: textarea_class,
                disabled: *is_publishing.read(),
                onfocus: props.on_focus,
                cursor_position: Some(cursor_position),
                thread_participants: props.thread_participants.clone().unwrap_or_default(),
            }
            if *show_media_uploader.read() {
                div { class: "mt-3",
                    MediaUploader {
                        on_upload: move |url: String| editor.handle_media_uploaded(url),
                        button_label: "Upload Media",
                    }
                }
            }
            if *is_sensitive.read() {
                div { class: "mt-2 flex items-center gap-2",
                    input {
                        r#type: "text",
                        class: "flex-1 px-3 py-1.5 text-sm bg-transparent border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-ring",
                        placeholder: "Content warning reason (optional)",
                        value: "{sensitive_reason}",
                        oninput: move |evt| {
                            let mut r = sensitive_reason;
                            r.set(evt.value());
                        },
                    }
                    button {
                        class: "p-1.5 text-muted-foreground hover:text-foreground transition",
                        onclick: move |_| {
                            is_sensitive.set(false);
                            sensitive_reason.set(String::new());
                        },
                        "×"
                    }
                }
            }
            div { class: "mt-3 flex items-center justify-between gap-2",
                div { class: "flex items-center gap-2 min-w-0",
                    button {
                        class: if *show_media_uploader.read() { "p-2 rounded-full bg-primary text-primary-foreground transition shrink-0" } else { "p-2 rounded-full hover:bg-accent transition shrink-0" },
                        title: "Add media",
                        onclick: move |_| {
                            let val = *show_media_uploader.read();
                            show_media_uploader.set(!val);
                        },
                        disabled: *is_publishing.read(),
                        CameraIcon { class: "w-5 h-5".to_string() }
                    }
                    EmojiPicker {
                        on_emoji_selected: move |selection| editor.handle_emoji_selected(selection),
                        icon_only: true,
                    }
                    GifPicker {
                        on_gif_selected: move |url| editor.handle_gif_selected(url),
                        icon_only: true,
                    }
                    div { class: "hidden lg:flex items-center gap-2",
                        button {
                            class: "p-2 rounded-full hover:bg-accent transition",
                            title: "Create poll",
                            onclick: move |_| {
                                let mut s = show_poll_modal;
                                s.set(true);
                            },
                            disabled: *is_publishing.read(),
                            BarChartIcon { class: "w-5 h-5".to_string() }
                        }
                        button {
                            class: if *is_sensitive.read() { "p-2 rounded-full bg-yellow-500/20 text-yellow-600 dark:text-yellow-400 transition" } else { "p-2 rounded-full hover:bg-accent transition" },
                            title: "Mark as sensitive",
                            onclick: move |_| {
                                let val = *is_sensitive.read();
                                is_sensitive.set(!val);
                                if val {
                                    sensitive_reason.set(String::new());
                                }
                            },
                            disabled: *is_publishing.read(),
                            EyeOffIcon { class: "w-5 h-5".to_string() }
                        }
                    }
                    div { class: "lg:hidden",
                        DropdownMenu { default_open: false, class: "relative",
                            DropdownMenuTrigger {
                                class: "p-2 rounded-full hover:bg-accent transition",
                                MoreHorizontalIcon { class: "w-5 h-5".to_string() }
                            }
                            DropdownMenuContent {
                                class: "absolute right-0 mt-2 w-48 bg-background border border-border rounded-lg shadow-lg z-50 py-1",

                                DropdownMenuItem::<String> {
                                    value: "poll".to_string(),
                                    index: 0usize,
                                    disabled: *is_publishing.read(),
                                    on_select: move |_| {
                                        let mut s = show_poll_modal;
                                        s.set(true);
                                    },
                                    class: "flex items-center gap-2 px-3 py-2 text-sm hover:bg-accent transition cursor-pointer rounded-sm mx-1",
                                    BarChartIcon { class: "w-4 h-4".to_string() }
                                    "Create poll"
                                }
                                DropdownMenuItem::<String> {
                                    value: "sensitive".to_string(),
                                    index: 1usize,
                                    disabled: *is_publishing.read(),
                                    on_select: move |_| {
                                        let val = *is_sensitive.read();
                                        is_sensitive.set(!val);
                                        if val {
                                            sensitive_reason.set(String::new());
                                        }
                                    },
                                    class: "flex items-center gap-2 px-3 py-2 text-sm hover:bg-accent transition cursor-pointer rounded-sm mx-1",
                                    EyeOffIcon { class: "w-4 h-4".to_string() }
                                    if *is_sensitive.read() {
                                        "Remove content warning"
                                    } else {
                                        "Mark as sensitive"
                                    }
                                }
                            }
                        }
                    }
                    if *is_over_limit.read() {
                        span { class: "text-sm text-red-500 ml-1 shrink-0", "Over limit by {char_count - 5000}" }
                    } else if *editor.show_warning.read() {
                        span { class: "text-sm {counter_color} ml-1 shrink-0", "{editor.remaining} chars left" }
                    }
                }
                div { class: "flex gap-2 shrink-0",
                    if let Some(on_cancel) = on_cancel_prop {
                        button {
                            class: "px-4 py-2 text-sm font-medium hover:bg-accent rounded-full transition",
                            onclick: move |_| on_cancel.call(()),
                            disabled: *is_publishing.read(),
                            "Cancel"
                        }
                    }
                    button {
                        class: "px-6 py-2 text-sm font-bold text-white bg-blue-500 hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed rounded-full transition flex items-center gap-2",
                        disabled: !*can_publish.read(),
                        onclick: move |_| on_publish_prop.call(()),
                        if *is_publishing.read() {
                            span { class: "inline-block w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" }
                            "{props.publish_label}ing..."
                        } else {
                            "{props.publish_label}"
                        }
                    }
                }
            }
            PollCreatorModal {
                show: show_poll_modal,
                on_poll_created: move |nevent_ref: String| editor.handle_poll_created(nevent_ref),
            }
        }
    }
}
