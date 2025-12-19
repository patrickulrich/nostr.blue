//! Code Issue Card Component
//!
//! Displays NIP-34 Git issues in cards.

use dioxus::prelude::*;
use crate::utils::nip34::Issue;
use crate::routes::Route;
use crate::components::code_status_badge::{CodeStatusBadge, BadgeSize};

/// Issue card component for lists
#[component]
pub fn CodeIssueCard(
    issue: Issue,
) -> Element {
    let title = issue.subject.clone().unwrap_or_else(|| {
        // Use first line of content as title if no subject
        issue.content.lines().next().unwrap_or("Untitled issue").to_string()
    });

    rsx! {
        Link {
            to: Route::CodeIssueDetail { note_id: issue.event_id.clone() },
            class: "block p-4 border border-border rounded-lg hover:bg-accent/50 transition",

            // Header with status and title
            div {
                class: "flex items-start gap-3",

                // Status badge
                CodeStatusBadge {
                    status: issue.status.clone(),
                    size: BadgeSize::Small,
                }

                // Title and metadata
                div {
                    class: "flex-1 min-w-0",

                    h3 {
                        class: "font-medium text-foreground line-clamp-2",
                        "{title}"
                    }

                    // Metadata row
                    div {
                        class: "mt-1 flex items-center gap-2 text-sm text-muted-foreground",

                        span { "#{issue.event_id_short()}" }
                        span { "·" }
                        span { "by {issue.pubkey_display()}" }

                        if issue.comment_count > 0 {
                            span { "·" }
                            div {
                                class: "flex items-center gap-1",
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
                                    path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                                }
                                span { "{issue.comment_count}" }
                            }
                        }
                    }
                }
            }

            // Labels
            if !issue.labels.is_empty() {
                div {
                    class: "mt-2 flex flex-wrap gap-1",
                    for label in issue.labels.iter() {
                        span {
                            class: "px-2 py-0.5 text-xs rounded-full bg-accent text-accent-foreground",
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

/// Compact issue row for tables
#[component]
pub fn CodeIssueRow(issue: Issue) -> Element {
    let title = issue.subject.clone().unwrap_or_else(|| {
        issue.content.lines().next().unwrap_or("Untitled issue").to_string()
    });

    rsx! {
        Link {
            to: Route::CodeIssueDetail { note_id: issue.event_id.clone() },
            class: "flex items-center gap-3 p-2 hover:bg-accent/50 transition rounded",

            // Status indicator
            div {
                class: match issue.status {
                    crate::utils::nip34::IssueStatus::Open => "w-2 h-2 rounded-full bg-green-500",
                    crate::utils::nip34::IssueStatus::Closed => "w-2 h-2 rounded-full bg-red-500",
                    crate::utils::nip34::IssueStatus::Applied => "w-2 h-2 rounded-full bg-purple-500",
                    crate::utils::nip34::IssueStatus::Draft => "w-2 h-2 rounded-full bg-gray-500",
                },
            }

            // Title
            span {
                class: "flex-1 truncate",
                "{title}"
            }

            // Comment count
            if issue.comment_count > 0 {
                div {
                    class: "flex items-center gap-1 text-sm text-muted-foreground",
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
                        path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                    }
                    span { "{issue.comment_count}" }
                }
            }
        }
    }
}
