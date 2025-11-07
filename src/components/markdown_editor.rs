use dioxus::prelude::*;
use crate::utils::markdown::render_markdown;

#[derive(Clone, Copy, PartialEq)]
pub enum EditorMode {
    Edit,
    Preview,
    Split,
}

#[derive(Props, Clone, PartialEq)]
pub struct MarkdownEditorProps {
    pub content: Signal<String>,
    #[props(default = 400)]
    pub min_height: i32,
    #[props(default = String::from("Write your content here..."))]
    pub placeholder: String,
}

#[component]
pub fn MarkdownEditor(mut props: MarkdownEditorProps) -> Element {
    let mut mode = use_signal(|| EditorMode::Split);

    // Render markdown preview
    let html_content = use_memo(move || render_markdown(&props.content.read()));

    rsx! {
        div {
            class: "flex flex-col h-full",

            // Mode selector tabs
            div {
                class: "flex border-b border-border",
                button {
                    class: format!(
                        "px-4 py-2 font-medium transition {}",
                        if *mode.read() == EditorMode::Edit {
                            "border-b-2 border-blue-500 text-blue-500"
                        } else {
                            "text-muted-foreground hover:text-foreground"
                        }
                    ),
                    onclick: move |_| mode.set(EditorMode::Edit),
                    "Edit"
                }
                button {
                    class: format!(
                        "px-4 py-2 font-medium transition {}",
                        if *mode.read() == EditorMode::Split {
                            "border-b-2 border-blue-500 text-blue-500"
                        } else {
                            "text-muted-foreground hover:text-foreground"
                        }
                    ),
                    onclick: move |_| mode.set(EditorMode::Split),
                    "Split"
                }
                button {
                    class: format!(
                        "px-4 py-2 font-medium transition {}",
                        if *mode.read() == EditorMode::Preview {
                            "border-b-2 border-blue-500 text-blue-500"
                        } else {
                            "text-muted-foreground hover:text-foreground"
                        }
                    ),
                    onclick: move |_| mode.set(EditorMode::Preview),
                    "Preview"
                }
            }

            // Editor area
            div {
                class: "flex-1 overflow-hidden",
                style: format!("min-height: {}px", props.min_height),

                match *mode.read() {
                    EditorMode::Edit => rsx! {
                        textarea {
                            class: "w-full h-full p-4 bg-background border-0 resize-none focus:outline-none focus:ring-0 font-mono text-sm",
                            placeholder: "{props.placeholder}",
                            value: "{props.content}",
                            oninput: move |e| props.content.set(e.value()),
                        }
                    },
                    EditorMode::Preview => rsx! {
                        div {
                            class: "w-full h-full p-4 overflow-y-auto prose prose-lg prose-neutral dark:prose-invert max-w-none",
                            dangerous_inner_html: "{html_content}",
                        }
                    },
                    EditorMode::Split => rsx! {
                        div {
                            class: "flex h-full",

                            // Left: Editor
                            div {
                                class: "w-1/2 border-r border-border",
                                textarea {
                                    class: "w-full h-full p-4 bg-background border-0 resize-none focus:outline-none focus:ring-0 font-mono text-sm",
                                    placeholder: "{props.placeholder}",
                                    value: "{props.content}",
                                    oninput: move |e| props.content.set(e.value()),
                                }
                            }

                            // Right: Preview
                            div {
                                class: "w-1/2 overflow-y-auto",
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

            // Markdown help
            div {
                class: "px-4 py-2 text-xs text-muted-foreground bg-muted border-t border-border",
                "Markdown supported: **bold**, *italic*, [links](url), # headings, - lists, > quotes, `code`, ```code blocks```"
            }
        }
    }
}
