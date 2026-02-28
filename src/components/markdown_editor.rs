//! Markdown Editor Component
//!
//! A full-featured markdown editor with preview modes, formatting toolbar,
//! and keyboard shortcut support.
use dioxus::prelude::*;
use std::rc::Rc;
use super::markdown_toolbar::{
    apply_markdown_format, get_textarea_cursor, set_textarea_cursor, MarkdownFormat,
    MarkdownToolbar,
};
use crate::utils::markdown::render_markdown;
#[derive(Clone, Copy, PartialEq)]
pub enum EditorMode {
    Edit,
    Preview,
    Split,
}
#[derive(Props, Clone, PartialEq)]
pub struct MarkdownEditorProps {
    /// The content signal to edit
    pub content: Signal<String>,
    /// Minimum height in pixels
    #[props(default = 400)]
    pub min_height: i32,
    /// Placeholder text
    #[props(default = String::from("Write your content here..."))]
    pub placeholder: String,
    /// Whether to show the formatting toolbar
    #[props(default = true)]
    pub show_toolbar: bool,
    /// Callback when user requests to upload an image
    #[props(default)]
    pub on_image_upload_request: Option<EventHandler<()>>,
    /// Callback when user requests to insert a Nostr mention
    #[props(default)]
    pub on_mention_request: Option<EventHandler<()>>,
    /// Optional ID to use for the textarea (for external cursor manipulation)
    #[props(default)]
    pub textarea_id: Option<String>,
}
#[component]
pub fn MarkdownEditor(mut props: MarkdownEditorProps) -> Element {
    let mut mode = use_signal(|| EditorMode::Split);
    let textarea_id = use_signal(|| {
        Rc::new(
            props
                .textarea_id
                .clone()
                .unwrap_or_else(|| format!("md-editor-{}", uuid::Uuid::new_v4())),
        )
    });
    let html_content = use_memo(move || render_markdown(&props.content.read()));
    let handle_format = {
        let mut content = props.content;
        move |format: MarkdownFormat| {
            let current_content = content.read().clone();
            let id_str = (*textarea_id.read()).clone();
            let (cursor_start, cursor_end) = get_textarea_cursor(
                &id_str,
                &current_content,
            );
            let (new_content, new_cursor) = apply_markdown_format(
                &current_content,
                cursor_start,
                cursor_end,
                format,
            );
            content.set(new_content.clone());
            let id_clone = id_str.clone();
            spawn(async move {
                #[cfg(feature = "web")]
                crate::platform::timer::sleep_ms(10).await;
                set_textarea_cursor(&id_clone, new_cursor, &new_content);
            });
        }
    };
    let handle_keydown = {
        let mut handle_format = handle_format;
        move |evt: Event<KeyboardData>| {
            let key = evt.key();
            let modifiers = evt.modifiers();
            let ctrl_or_cmd = modifiers.ctrl() || modifiers.meta();
            if !ctrl_or_cmd {
                return;
            }
            let format = match key {
                Key::Character(ref c) if c.to_lowercase() == "b" => {
                    Some(MarkdownFormat::Bold)
                }
                Key::Character(ref c) if c.to_lowercase() == "i" => {
                    Some(MarkdownFormat::Italic)
                }
                Key::Character(ref c) if c.to_lowercase() == "k" => {
                    Some(MarkdownFormat::Link)
                }
                Key::Character(ref c) if c.to_lowercase() == "`" => {
                    Some(MarkdownFormat::InlineCode)
                }
                _ => None,
            };
            if let Some(f) = format {
                evt.prevent_default();
                handle_format(f);
            }
        }
    };
    let mode_btn_class = |current: EditorMode, target: EditorMode| -> String {
        format!(
            "px-4 py-2 font-medium transition {}",
            if current == target {
                "border-b-2 border-primary text-primary"
            } else {
                "text-muted-foreground hover:text-foreground"
            },
        )
    };
    let textarea_class = "w-full h-full p-4 bg-background border-0 resize-none focus:outline-hidden focus:ring-0 font-mono text-sm";
    rsx! {
        div { class: "flex flex-col h-full border border-border rounded-lg overflow-hidden",
            if props.show_toolbar {
                MarkdownToolbar {
                    on_format: handle_format,
                    on_image_upload_request: props.on_image_upload_request,
                    on_mention_request: props.on_mention_request,
                    disabled: *mode.read() == EditorMode::Preview,
                }
            }
            div { class: "flex border-b border-border bg-muted/20",
                button {
                    class: mode_btn_class(*mode.read(), EditorMode::Edit),
                    onclick: move |_| mode.set(EditorMode::Edit),
                    "Edit"
                }
                button {
                    class: mode_btn_class(*mode.read(), EditorMode::Split),
                    onclick: move |_| mode.set(EditorMode::Split),
                    "Split"
                }
                button {
                    class: mode_btn_class(*mode.read(), EditorMode::Preview),
                    onclick: move |_| mode.set(EditorMode::Preview),
                    "Preview"
                }
            }
            div {
                class: "flex-1 overflow-hidden",
                style: format!("min-height: {}px", props.min_height),
                match *mode.read() {
                    EditorMode::Edit => rsx! {
                        textarea {
                            id: "{textarea_id}",
                            class: textarea_class,
                            placeholder: "{props.placeholder}",
                            value: "{props.content}",
                            oninput: move |e| props.content.set(e.value()),
                            onkeydown: handle_keydown,
                        }
                    },
                    EditorMode::Preview => rsx! {
                        div {
                            class: "w-full h-full p-4 overflow-y-auto prose prose-lg prose-neutral dark:prose-invert max-w-none
                                                                                                                                                                                                                                                                                                                                                                                   [&_h1]:text-4xl [&_h1]:font-bold [&_h1]:mt-8 [&_h1]:mb-4
                                                                                                                                                                                                                                                                                                                                                                                   [&_h2]:text-3xl [&_h2]:font-bold [&_h2]:mt-6 [&_h2]:mb-3
                                                                                                                                                                                                                                                                                                                                                                                   [&_h3]:text-2xl [&_h3]:font-semibold [&_h3]:mt-5 [&_h3]:mb-2
                                                                                                                                                                                                                                                                                                                                                                                   [&_p]:my-4 [&_p]:leading-relaxed
                                                                                                                                                                                                                                                                                                                                                                                   [&_a]:text-primary [&_a]:underline hover:[&_a]:text-primary/80
                                                                                                                                                                                                                                                                                                                                                                                   [&_ul]:my-4 [&_ul]:pl-6 [&_ul]:list-disc
                                                                                                                                                                                                                                                                                                                                                                                   [&_ol]:my-4 [&_ol]:pl-6 [&_ol]:list-decimal
                                                                                                                                                                                                                                                                                                                                                                                   [&_li]:my-2
                                                                                                                                                                                                                                                                                                                                                                                   [&_blockquote]:border-l-4 [&_blockquote]:border-primary [&_blockquote]:pl-4 [&_blockquote]:my-4 [&_blockquote]:italic
                                                                                                                                                                                                                                                                                                                                                                                   [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-sm
                                                                                                                                                                                                                                                                                                                                                                                   [&_pre]:bg-muted [&_pre]:p-4 [&_pre]:rounded-lg [&_pre]:overflow-x-auto [&_pre]:my-4
                                                                                                                                                                                                                                                                                                                                                                                   [&_img]:max-w-full [&_img]:h-auto [&_img]:rounded-lg [&_img]:my-6
                                                                                                                                                                                                                                                                                                                                                                                   [&_table]:w-full [&_table]:my-4
                                                                                                                                                                                                                                                                                                                                                                                   [&_th]:border [&_th]:border-border [&_th]:bg-muted [&_th]:px-4 [&_th]:py-2 [&_th]:font-semibold
                                                                                                                                                                                                                                                                                                                                                                                   [&_td]:border [&_td]:border-border [&_td]:px-4 [&_td]:py-2",
                            dangerous_inner_html: "{html_content}",
                        }
                    },
                    EditorMode::Split => rsx! {
                        div { class: "flex h-full",
                            div { class: "w-1/2 border-r border-border",
                                textarea {
                                    id: "{textarea_id}",
                                    class: textarea_class,
                                    placeholder: "{props.placeholder}",
                                    value: "{props.content}",
                                    oninput: move |e| props.content.set(e.value()),
                                    onkeydown: handle_keydown,
                                }
                            }
                            div { class: "w-1/2 overflow-y-auto",
                                div {
                                    class: "p-4 prose prose-lg prose-neutral dark:prose-invert max-w-none
                                                                                                                                                                                                                                                                                                                                                                                           [&_h1]:text-4xl [&_h1]:font-bold [&_h1]:mt-8 [&_h1]:mb-4
                                                                                                                                                                                                                                                                                                                                                                                           [&_h2]:text-3xl [&_h2]:font-bold [&_h2]:mt-6 [&_h2]:mb-3
                                                                                                                                                                                                                                                                                                                                                                                           [&_h3]:text-2xl [&_h3]:font-semibold [&_h3]:mt-5 [&_h3]:mb-2
                                                                                                                                                                                                                                                                                                                                                                                           [&_p]:my-4 [&_p]:leading-relaxed
                                                                                                                                                                                                                                                                                                                                                                                           [&_a]:text-primary [&_a]:underline hover:[&_a]:text-primary/80
                                                                                                                                                                                                                                                                                                                                                                                           [&_ul]:my-4 [&_ul]:pl-6 [&_ul]:list-disc
                                                                                                                                                                                                                                                                                                                                                                                           [&_ol]:my-4 [&_ol]:pl-6 [&_ol]:list-decimal
                                                                                                                                                                                                                                                                                                                                                                                           [&_li]:my-2
                                                                                                                                                                                                                                                                                                                                                                                           [&_blockquote]:border-l-4 [&_blockquote]:border-primary [&_blockquote]:pl-4 [&_blockquote]:my-4 [&_blockquote]:italic
                                                                                                                                                                                                                                                                                                                                                                                           [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-sm
                                                                                                                                                                                                                                                                                                                                                                                           [&_pre]:bg-muted [&_pre]:p-4 [&_pre]:rounded-lg [&_pre]:overflow-x-auto [&_pre]:my-4
                                                                                                                                                                                                                                                                                                                                                                                           [&_img]:max-w-full [&_img]:h-auto [&_img]:rounded-lg [&_img]:my-6
                                                                                                                                                                                                                                                                                                                                                                                           [&_table]:w-full [&_table]:my-4
                                                                                                                                                                                                                                                                                                                                                                                           [&_th]:border [&_th]:border-border [&_th]:bg-muted [&_th]:px-4 [&_th]:py-2 [&_th]:font-semibold
                                                                                                                                                                                                                                                                                                                                                                                           [&_td]:border [&_td]:border-border [&_td]:px-4 [&_td]:py-2",
                                    dangerous_inner_html: "{html_content}",
                                }
                            }
                        }
                    },
                }
            }
            div { class: "px-4 py-2 text-xs text-muted-foreground bg-muted/30 border-t border-border flex items-center justify-between",
                span { "Markdown supported" }
                div { class: "flex items-center gap-3",
                    span { class: "hidden sm:inline-flex items-center gap-1",
                        kbd { class: "px-1.5 py-0.5 bg-muted rounded text-[10px] font-mono",
                            "Ctrl+B"
                        }
                        span { "Bold" }
                    }
                    span { class: "hidden sm:inline-flex items-center gap-1",
                        kbd { class: "px-1.5 py-0.5 bg-muted rounded text-[10px] font-mono",
                            "Ctrl+I"
                        }
                        span { "Italic" }
                    }
                    span { class: "hidden md:inline-flex items-center gap-1",
                        kbd { class: "px-1.5 py-0.5 bg-muted rounded text-[10px] font-mono",
                            "Ctrl+K"
                        }
                        span { "Link" }
                    }
                }
            }
        }
    }
}
/// Insert text at the current cursor position in the editor
///
/// This is useful for inserting content from external sources (e.g., image upload, mention dialog)
pub fn insert_at_cursor(
    content: &mut Signal<String>,
    textarea_id: &str,
    text_to_insert: &str,
) {
    let current_content = content.read().clone();
    let (cursor_start, _cursor_end) = get_textarea_cursor(textarea_id, &current_content);
    let before = &current_content[..cursor_start];
    let after = &current_content[cursor_start..];
    let new_content = format!("{}{}{}", before, text_to_insert, after);
    let new_cursor = cursor_start + text_to_insert.len();
    content.set(new_content.clone());
    let id = textarea_id.to_string();
    spawn(async move {
        #[cfg(feature = "web")] crate::platform::timer::sleep_ms(10).await;
        set_textarea_cursor(&id, new_cursor, &new_content);
    });
}
