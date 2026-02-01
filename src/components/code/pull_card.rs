//! Code Pull Request Card Component
//!
//! Displays NIP-34 Git patches/PRs in cards.
use super::status_badge::{status_color_class, BadgeSize, CodeStatusBadge};
use crate::routes::Route;
use crate::utils::format::truncate_commit;
use crate::utils::nip34::PullRequest;
use dioxus::prelude::*;
/// Pull request card component for lists
#[component]
pub fn CodePullCard(pr: PullRequest) -> Element {
    let title = pr.display_title();
    rsx! {
        Link {
            to: Route::CodePullDetail {
                note_id: pr.event_id.clone(),
            },
            class: "block p-4 border border-border rounded-lg hover:bg-accent/50 transition",
            div { class: "flex items-start gap-3",
                CodeStatusBadge { status: pr.status, size: BadgeSize::Small }
                div { class: "flex-1 min-w-0",
                    h3 { class: "font-medium text-foreground line-clamp-2", "{title}" }
                    div { class: "mt-1 flex items-center gap-2 text-sm text-muted-foreground",
                        span { "#{pr.event_id_short()}" }
                        span { "·" }
                        span { "by {pr.pubkey_display()}" }
                        if let Some(ref commit) = pr.commit {
                            span { "·" }
                            span { class: "font-mono text-xs", "{truncate_commit(commit)}" }
                        }
                    }
                }
            }
            div { class: "mt-2 flex items-center gap-2",
                if pr.is_cover_letter {
                    span { class: "px-2 py-0.5 text-xs rounded-full bg-blue-500/10 text-blue-500 border border-blue-500/20",
                        "Cover Letter"
                    }
                }
                for (idx , label) in pr.labels.iter().enumerate() {
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
/// Compact PR row for tables
#[component]
pub fn CodePullRow(pr: PullRequest) -> Element {
    let title = pr.display_title();
    rsx! {
        Link {
            to: Route::CodePullDetail {
                note_id: pr.event_id.clone(),
            },
            class: "flex items-center gap-3 p-2 hover:bg-accent/50 transition rounded",
            svg {
                class: format!("w-4 h-4 {}", status_color_class(pr.status)),
                xmlns: "http://www.w3.org/2000/svg",
                width: "24",
                height: "24",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "2",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                circle { cx: "18", cy: "18", r: "3" }
                circle { cx: "6", cy: "6", r: "3" }
                path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                line {
                    x1: "6",
                    y1: "9",
                    x2: "6",
                    y2: "21",
                }
            }
            span { class: "flex-1 truncate", "{title}" }
            if pr.is_cover_letter {
                span { class: "text-xs text-blue-500", "CL" }
            }
            if let Some(ref commit) = pr.commit {
                span { class: "font-mono text-xs text-muted-foreground", "{truncate_commit(commit)}" }
            }
        }
    }
}
