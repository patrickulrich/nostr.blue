use crate::components::icons::{BarChartIcon, CameraIcon};
use crate::components::{EmojiPicker, GifPicker, MediaUploader, PollCreatorModal};
use crate::hooks::use_composer_editor::UseComposerEditor;
use dioxus::prelude::*;

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
    let char_count = editor.char_count;
    let _remaining = editor.remaining;
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
            div { class: "mt-3 flex items-center justify-between",
                div { class: "flex items-center gap-2",
                    button {
                        class: if *show_media_uploader.read() { "p-2 rounded-full bg-primary text-primary-foreground transition" } else { "p-2 rounded-full hover:bg-accent transition" },
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
                    div { class: "text-sm {counter_color} ml-2",
                        if *is_over_limit.read() {
                            span { "Over limit by {char_count - 5000}" }
                        } else {
                            span { "{char_count} / 5000" }
                        }
                    }
                }
                div { class: "flex gap-2",
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
