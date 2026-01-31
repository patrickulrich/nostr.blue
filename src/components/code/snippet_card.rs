//! Code Snippet Card Component
//!
//! Displays NIP-C0 code snippets in cards for lists and feeds.
use crate::routes::Route;
use crate::utils::nip34::DisplaySnippet;
use dioxus::prelude::*;
/// Code snippet card component for lists
#[component]
pub fn CodeSnippetCard(
    snippet: DisplaySnippet,
    #[props(default = true)]
    show_author: bool,
    #[props(default = 10)]
    max_lines: usize,
) -> Element {
    let language = snippet.language.clone().unwrap_or_else(|| "text".to_string());
    let display_name = snippet.name.clone().unwrap_or_else(|| "Untitled".to_string());
    let description = snippet.description.clone().unwrap_or_default();
    let code_lines: Vec<&str> = snippet.code.lines().collect();
    let truncated = code_lines.len() > max_lines;
    let display_code = if truncated {
        code_lines[..max_lines].join("\n")
    } else {
        snippet.code.clone()
    };
    rsx! {
        Link {
            to: Route::CodeSnippetDetail {
                note_id: snippet.event_id.clone(),
            },
            class: "block border border-border rounded-lg overflow-hidden hover:border-accent transition",
            div { class: "flex items-center justify-between px-4 py-2 bg-muted/50 border-b border-border",
                div { class: "flex items-center gap-2 min-w-0",
                    svg {
                        class: "w-4 h-4 text-muted-foreground shrink-0",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
                        polyline { points: "14 2 14 8 20 8" }
                    }
                    span { class: "font-medium truncate", "{display_name}" }
                }
                span { class: "px-2 py-0.5 text-xs font-medium rounded bg-accent text-accent-foreground",
                    "{language}"
                }
            }
            div { class: "relative",
                pre {
                    class: "p-4 text-sm font-mono overflow-x-auto bg-background",
                    style: "max-height: 300px;",
                    code { "{display_code}" }
                }
                if truncated {
                    div { class: "absolute bottom-0 left-0 right-0 h-12 bg-gradient-to-t from-background to-transparent pointer-events-none" }
                }
            }
            if !description.is_empty() || show_author {
                div { class: "px-4 py-2 border-t border-border bg-muted/30",
                    if !description.is_empty() {
                        p { class: "text-sm text-muted-foreground line-clamp-2 mb-1",
                            "{description}"
                        }
                    }
                    if show_author {
                        p { class: "text-xs text-muted-foreground", "by {snippet.pubkey_display()}" }
                    }
                }
            }
        }
    }
}
/// Inline/embed version for rendering in notes
#[component]
pub fn CodeSnippetEmbed(snippet: DisplaySnippet) -> Element {
    let language = snippet.language.clone().unwrap_or_else(|| "text".to_string());
    let display_name = snippet.name.clone();
    let code_lines: Vec<&str> = snippet.code.lines().collect();
    let truncated = code_lines.len() > 15;
    let display_code = if truncated {
        format!("{}\n...", code_lines[..15].join("\n"))
    } else {
        snippet.code.clone()
    };
    rsx! {
        div { class: "my-2 border border-border rounded-lg overflow-hidden",
            div { class: "flex items-center justify-between px-3 py-1.5 bg-muted/50 border-b border-border",
                div { class: "flex items-center gap-2",
                    svg {
                        class: "w-4 h-4 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        polyline { points: "16 18 22 12 16 6" }
                        polyline { points: "8 6 2 12 8 18" }
                    }
                    if let Some(name) = display_name {
                        span { class: "text-sm font-medium", "{name}" }
                    }
                    span { class: "px-1.5 py-0.5 text-xs font-medium rounded bg-accent text-accent-foreground",
                        "{language}"
                    }
                }
                Link {
                    to: Route::CodeSnippetDetail {
                        note_id: snippet.event_id.clone(),
                    },
                    class: "text-xs text-blue-500 hover:underline",
                    "View full"
                }
            }
            pre {
                class: "p-3 text-sm font-mono overflow-x-auto bg-background",
                style: "max-height: 400px;",
                code { "{display_code}" }
            }
        }
    }
}
/// Minimal snippet reference for inline mentions
#[component]
pub fn CodeSnippetRef(event_id: String) -> Element {
    rsx! {
        Link {
            to: Route::CodeSnippetDetail {
                note_id: event_id.clone(),
            },
            class: "inline-flex items-center gap-1 px-2 py-0.5 rounded bg-accent/50 hover:bg-accent text-sm transition",
            svg {
                class: "w-3.5 h-3.5",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                polyline { points: "16 18 22 12 16 6" }
                polyline { points: "8 6 2 12 8 18" }
            }
            span { "code snippet" }
        }
    }
}
