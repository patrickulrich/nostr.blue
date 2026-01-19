//! Code Repository Card Component
//!
//! Displays a repository card for lists and discovery pages.

use dioxus::prelude::*;
use crate::utils::nip34::Repository;
use crate::routes::Route;

/// Repository card component
#[component]
pub fn CodeRepoCard(
    repo: Repository,
    #[props(default = true)] show_description: bool,
    #[props(default = true)] show_stats: bool,
) -> Element {
    let display_name = repo.name.clone().unwrap_or_else(|| repo.id.clone());
    let description = repo.description.clone().unwrap_or_default();

    rsx! {
        Link {
            to: Route::CodeRepo { naddr: repo.naddr.clone() },
            class: "block p-4 border border-border rounded-lg hover:bg-accent/50 transition",

            // Header with icon and name
            div {
                class: "flex items-start gap-3",

                // Repository icon
                div {
                    class: "w-10 h-10 rounded-lg bg-muted flex items-center justify-center shrink-0",
                    svg {
                        class: "w-5 h-5 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        // Book/repo icon
                        path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" }
                        path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" }
                    }
                }

                // Name and metadata
                div {
                    class: "flex-1 min-w-0",

                    // Repository name
                    h3 {
                        class: "font-semibold text-foreground truncate",
                        "{display_name}"
                    }

                    // Owner (truncated pubkey)
                    p {
                        class: "text-sm text-muted-foreground truncate",
                        "{repo.pubkey_display()}"
                    }
                }
            }

            // Description
            if show_description && !description.is_empty() {
                p {
                    class: "mt-2 text-sm text-muted-foreground line-clamp-2",
                    "{description}"
                }
            }

            // Stats row
            if show_stats {
                div {
                    class: "mt-3 flex items-center gap-4 text-sm text-muted-foreground",

                    // Stars
                    div {
                        class: "flex items-center gap-1",
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
                            polygon { points: "12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" }
                        }
                        span { "{repo.star_count}" }
                    }

                    // Issues
                    div {
                        class: "flex items-center gap-1",
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
                            circle { cx: "12", cy: "12", r: "10" }
                            line { x1: "12", y1: "8", x2: "12", y2: "12" }
                            line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                        }
                        span { "{repo.issue_count}" }
                    }

                    // Clone URLs indicator
                    if !repo.clone.is_empty() {
                        div {
                            class: "flex items-center gap-1",
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
                                // Git branch icon
                                line { x1: "6", y1: "3", x2: "6", y2: "15" }
                                circle { cx: "18", cy: "6", r: "3" }
                                circle { cx: "6", cy: "18", r: "3" }
                                path { d: "M18 9a9 9 0 0 1-9 9" }
                            }
                            span { "Clone" }
                        }
                    }
                }
            }
        }
    }
}

/// Compact repository card for inline display
#[component]
pub fn CodeRepoCardCompact(repo: Repository) -> Element {
    let display_name = repo.name.clone().unwrap_or_else(|| repo.id.clone());

    rsx! {
        Link {
            to: Route::CodeRepo { naddr: repo.naddr.clone() },
            class: "flex items-center gap-2 p-2 rounded-lg hover:bg-accent/50 transition",

            // Repository icon
            div {
                class: "w-8 h-8 rounded bg-muted flex items-center justify-center shrink-0",
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
                    path { d: "M4 19.5A2.5 2.5 0 0 1 6.5 17H20" }
                    path { d: "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" }
                }
            }

            // Name
            span {
                class: "font-medium truncate",
                "{display_name}"
            }
        }
    }
}
