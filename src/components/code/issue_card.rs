//! Code Issue Card Component
//!
//! Displays NIP-34 Git issues in cards.
use super::status_badge::{BadgeSize, CodeStatusBadge, StatusDisplayContext};
use crate::components::icons::CommentIcon;
use crate::routes::Route;
use crate::utils::nip34::Issue;
use dioxus::prelude::*;
/// Issue card component for lists
#[component]
pub fn CodeIssueCard(issue: Issue) -> Element {
    let title = issue.display_title();
    rsx! {
        Link {
            to: Route::AddressViewer {
                address: crate::utils::nip19_urls::note_route_id(&issue.event_id, None),
            },
            class: "block p-4 border border-border rounded-lg hover:bg-accent/50 transition",
            div { class: "flex items-start gap-3",
                CodeStatusBadge {
                    status: issue.status,
                    size: BadgeSize::Small,
                    context: StatusDisplayContext::Issue,
                }
                div { class: "flex-1 min-w-0",
                    h3 { class: "font-medium text-foreground line-clamp-2", "{title}" }
                    div { class: "mt-1 flex items-center gap-2 text-sm text-muted-foreground",
                        span { "#{issue.event_id_short()}" }
                        span { "·" }
                        span { "by {issue.pubkey_display()}" }
                        if issue.comment_count > 0 {
                            span { "·" }
                            div { class: "flex items-center gap-1",
                                CommentIcon { class: "w-3.5 h-3.5".to_string() }
                                span { "{issue.comment_count}" }
                            }
                        }
                    }
                }
            }
            if !issue.labels.is_empty() || !issue.assignees.is_empty() {
                div { class: "mt-2 flex flex-wrap gap-1 items-center",
                    for label in issue.labels.iter() {
                        span {
                            key: "{label}",
                            class: "px-2 py-0.5 text-xs rounded-full bg-accent text-accent-foreground",
                            "{label}"
                        }
                    }
                    if !issue.assignees.is_empty() {
                        div { class: format!("flex items-center gap-1{}", if !issue.labels.is_empty() { " ml-auto" } else { "" }),
                            for assignee in issue.assignees.iter() {
                                div {
                                    key: "{assignee}",
                                    class: "w-5 h-5 rounded-full bg-muted flex items-center justify-center text-[10px] text-foreground",
                                    title: "{assignee}",
                                    "{assignee.chars().next().unwrap_or('?')}"
                                }
                            }
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
            to: Route::AddressViewer {
                address: crate::utils::nip19_urls::note_route_id(&issue.event_id, None),
            },
            class: "flex items-center gap-3 p-2 hover:bg-accent/50 transition rounded",
            div { class: format!("w-2 h-2 rounded-full {}", issue.status.bg_class()) }
            span { class: "flex-1 truncate", "{title}" }
            if issue.comment_count > 0 {
                div { class: "flex items-center gap-1 text-sm text-muted-foreground",
                    CommentIcon { class: "w-3.5 h-3.5".to_string() }
                    span { "{issue.comment_count}" }
                }
            }
        }
    }
}
