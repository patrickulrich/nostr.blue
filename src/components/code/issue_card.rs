//! Code Issue Card Component
//!
//! Displays NIP-34 Git issues in cards.

use dioxus::prelude::*;
use crate::utils::nip34::Issue;
use crate::routes::Route;
use super::status_badge::{CodeStatusBadge, BadgeSize};
use crate::components::icons::CommentIcon;

/// Issue card component for lists
#[component]
pub fn CodeIssueCard(
    issue: Issue,
) -> Element {
    let title = issue.display_title();

    rsx! {
        Link {
            to: Route::CodeIssueDetail { note_id: issue.event_id.clone() },
            class: "block p-4 border border-border rounded-lg hover:bg-accent/50 transition",

            // Header with status and title
            div {
                class: "flex items-start gap-3",

                // Status badge
                CodeStatusBadge {
                    status: issue.status,
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
                                CommentIcon { class: "w-3.5 h-3.5".to_string() }
                                span { "{issue.comment_count}" }
                            }
                        }
                    }
                }
            }

            // Labels (use index for unique keys in case of duplicate labels)
            if !issue.labels.is_empty() {
                div {
                    class: "mt-2 flex flex-wrap gap-1",
                    for (idx, label) in issue.labels.iter().enumerate() {
                        span {
                            key: "{idx}_{label}",
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
    let title = issue.display_title();

    rsx! {
        Link {
            to: Route::CodeIssueDetail { note_id: issue.event_id.clone() },
            class: "flex items-center gap-3 p-2 hover:bg-accent/50 transition rounded",

            // Status indicator
            div {
                class: format!("w-2 h-2 rounded-full {}", issue.status.bg_class()),
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
                    CommentIcon { class: "w-3.5 h-3.5".to_string() }
                    span { "{issue.comment_count}" }
                }
            }
        }
    }
}
