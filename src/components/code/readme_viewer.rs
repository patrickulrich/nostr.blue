//! README Viewer Component
//!
//! Displays repository README with markdown rendering.
//! Uses pulldown-cmark for parsing and ammonia for sanitization.
//! Styled to match gittr's readme-section.tsx pattern.

use dioxus::prelude::*;
use crate::utils::format::truncate_with_word_break;
use crate::utils::markdown::render_markdown;

/// README viewer with loading/error states
#[component]
pub fn ReadmeViewer(
    #[props(default = None)] content: Option<String>,
    #[props(default = false)] loading: bool,
    #[props(default = None)] error: Option<String>,
    #[props(default = "README.md".to_string())] filename: String,
) -> Element {
    rsx! {
        div {
            class: "border border-border rounded-lg overflow-hidden",

            // Header
            div {
                class: "flex items-center gap-2 px-4 py-3 bg-muted/50 border-b border-border",

                // File icon
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
                    path { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }
                    polyline { points: "14 2 14 8 20 8" }
                }

                span {
                    class: "text-sm font-medium",
                    "{filename}"
                }
            }

            // Content
            div {
                class: "p-6",

                if loading {
                    ReadmeSkeleton {}
                } else if let Some(err) = error {
                    div {
                        class: "text-center py-8 text-muted-foreground",

                        svg {
                            class: "w-12 h-12 mx-auto mb-3 text-muted-foreground/50",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "12", cy: "12", r: "10" }
                            line { x1: "12", y1: "8", x2: "12", y2: "12" }
                            line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                        }

                        p {
                            class: "text-sm",
                            "{err}"
                        }
                    }
                } else if let Some(markdown) = content {
                    if markdown.is_empty() {
                        div {
                            class: "text-center py-8 text-muted-foreground",

                            svg {
                                class: "w-12 h-12 mx-auto mb-3 text-muted-foreground/50",
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
                                line { x1: "9", y1: "15", x2: "15", y2: "15" }
                            }

                            p {
                                class: "text-sm",
                                "README is empty"
                            }
                        }
                    } else {
                        // Render markdown content
                        div {
                            class: "prose prose-neutral dark:prose-invert max-w-none prose-headings:font-semibold prose-headings:text-foreground prose-a:text-primary prose-a:no-underline hover:prose-a:underline prose-code:bg-muted prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-code:before:content-[''] prose-code:after:content-[''] prose-pre:bg-muted prose-pre:border prose-pre:border-border prose-img:rounded-lg prose-hr:border-border",
                            dangerous_inner_html: "{render_markdown(&markdown)}"
                        }
                    }
                } else {
                    // No README available
                    NoReadme {}
                }
            }
        }
    }
}

/// Skeleton loader for README
#[component]
pub fn ReadmeSkeleton() -> Element {
    rsx! {
        div {
            class: "animate-pulse space-y-4",

            // Title skeleton
            div {
                class: "h-8 bg-muted rounded w-1/3"
            }

            // Paragraph skeletons
            div {
                class: "space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
                div { class: "h-4 bg-muted rounded w-4/6" }
            }

            // Subheading skeleton
            div {
                class: "h-6 bg-muted rounded w-1/4 mt-6"
            }

            // More paragraphs
            div {
                class: "space-y-2",
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-5/6" }
            }

            // Code block skeleton
            div {
                class: "h-24 bg-muted rounded mt-4"
            }

            // More text
            div {
                class: "space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-4 bg-muted rounded w-1/2" }
            }
        }
    }
}

/// Placeholder when no README exists
#[component]
fn NoReadme() -> Element {
    rsx! {
        div {
            class: "text-center py-12",

            svg {
                class: "w-16 h-16 mx-auto mb-4 text-muted-foreground/30",
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" }
                path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" }
            }

            h3 {
                class: "text-lg font-medium text-muted-foreground mb-2",
                "No README found"
            }

            p {
                class: "text-sm text-muted-foreground",
                "Add a README file to help others understand this repository."
            }
        }
    }
}

/// Inline README preview (for compact displays)
#[component]
pub fn ReadmePreview(
    content: String,
    #[props(default = 200)] max_chars: usize,
) -> Element {
    // Use UTF-8 safe truncation to avoid panic on multi-byte characters
    let preview_text = truncate_with_word_break(&content, max_chars);

    rsx! {
        div {
            class: "text-sm text-muted-foreground line-clamp-3",
            dangerous_inner_html: "{render_markdown(&preview_text)}"
        }
    }
}
