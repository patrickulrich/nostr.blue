//! Filter Bar Component
//!
//! Reusable status/label/search filter bar for issues and PRs lists.
use crate::utils::nip34::IssueStatus;
use dioxus::prelude::*;

/// Active filter tab
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StatusFilter {
    Open,
    Closed,
    All,
}

/// Filter bar component for issues/PRs
#[component]
pub fn FilterBar(
    status_filter: StatusFilter,
    on_status_change: EventHandler<StatusFilter>,
    search_query: String,
    on_search_change: EventHandler<String>,
    open_count: usize,
    closed_count: usize,
    #[props(default = vec![])]
    available_labels: Vec<String>,
    #[props(default = vec![])]
    selected_labels: Vec<String>,
    #[props(default = None)]
    on_label_toggle: Option<EventHandler<String>>,
) -> Element {
    let search_query = search_query.clone();
    rsx! {
        div { class: "space-y-3 mb-4",
            // Search + status tabs row
            div { class: "flex flex-col sm:flex-row gap-3",
                // Search input
                div { class: "flex-1",
                    div { class: "relative",
                        svg {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            circle { cx: "11", cy: "11", r: "8" }
                            line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
                        }
                        input {
                            class: "w-full pl-10 pr-3 py-2 bg-muted border border-border rounded-lg text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Filter by title or content...",
                            value: "{search_query}",
                            oninput: move |e| on_search_change.call(e.value()),
                        }
                    }
                }
                // Status tabs
                div { class: "flex border border-border rounded-lg overflow-hidden",
                    button {
                        r#type: "button",
                        class: if status_filter == StatusFilter::Open {
                            "px-3 py-2 text-sm font-medium bg-primary text-primary-foreground transition"
                        } else {
                            "px-3 py-2 text-sm text-muted-foreground hover:bg-accent transition"
                        },
                        onclick: move |_| on_status_change.call(StatusFilter::Open),
                        div { class: "flex items-center gap-1.5",
                            div { class: "w-2 h-2 rounded-full bg-green-500" }
                            "Open"
                            span { class: "text-xs opacity-70 ml-1", "({open_count})" }
                        }
                    }
                    button {
                        r#type: "button",
                        class: if status_filter == StatusFilter::Closed {
                            "px-3 py-2 text-sm font-medium bg-primary text-primary-foreground transition"
                        } else {
                            "px-3 py-2 text-sm text-muted-foreground hover:bg-accent transition"
                        },
                        onclick: move |_| on_status_change.call(StatusFilter::Closed),
                        div { class: "flex items-center gap-1.5",
                            div { class: "w-2 h-2 rounded-full bg-red-500" }
                            "Closed"
                            span { class: "text-xs opacity-70 ml-1", "({closed_count})" }
                        }
                    }
                    button {
                        r#type: "button",
                        class: if status_filter == StatusFilter::All {
                            "px-3 py-2 text-sm font-medium bg-primary text-primary-foreground transition"
                        } else {
                            "px-3 py-2 text-sm text-muted-foreground hover:bg-accent transition"
                        },
                        onclick: move |_| on_status_change.call(StatusFilter::All),
                        "All"
                    }
                }
            }
            // Label filter chips
            if !available_labels.is_empty() {
                div { class: "flex flex-wrap gap-1.5",
                    for label in available_labels.iter() {
                        {
                            let is_selected = selected_labels.contains(label);
                            let label_clone = label.clone();
                            if on_label_toggle.is_some() {
                                rsx! {
                                    button {
                                        r#type: "button",
                                        key: "{label}",
                                        class: if is_selected {
                                            "px-2 py-0.5 text-xs rounded-full border border-primary bg-primary/10 text-primary font-medium"
                                        } else {
                                            "px-2 py-0.5 text-xs rounded-full border border-border text-muted-foreground hover:bg-accent transition"
                                        },
                                        onclick: move |_| {
                                            if let Some(handler) = &on_label_toggle {
                                                handler.call(label_clone.clone());
                                            }
                                        },
                                        "{label}"
                                    }
                                }
                            } else {
                                rsx! {
                                    span {
                                        key: "{label}",
                                        class: if is_selected {
                                            "px-2 py-0.5 text-xs rounded-full border border-primary bg-primary/10 text-primary font-medium"
                                        } else {
                                            "px-2 py-0.5 text-xs rounded-full border border-border text-muted-foreground"
                                        },
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Apply filters to a list of issues
/// Note: filter_issues and filter_prs below share parallel structure (status/search/label).
/// They remain separate because Issue and PullRequest are distinct types with different fields.
pub fn filter_issues(
    issues: &[crate::utils::nip34::Issue],
    status: StatusFilter,
    query: &str,
    selected_labels: &[String],
) -> Vec<crate::utils::nip34::Issue> {
    let q = query.to_lowercase();
    issues
        .iter()
        .filter(|issue| {
            // Status filter
            match status {
                StatusFilter::Open => {
                    issue.status == IssueStatus::Open || issue.status == IssueStatus::Draft
                }
                StatusFilter::Closed => {
                    issue.status == IssueStatus::Closed || issue.status == IssueStatus::Applied
                }
                StatusFilter::All => true,
            }
        })
        .filter(|issue| {
            // Text search
            if q.is_empty() {
                return true;
            }
            let title = issue.display_title().to_lowercase();
            let content = issue.content.to_lowercase();
            title.contains(&q) || content.contains(&q)
        })
        .filter(|issue| {
            // Label filter
            if selected_labels.is_empty() {
                return true;
            }
            selected_labels.iter().any(|sel| issue.labels.iter().any(|il| il.eq_ignore_ascii_case(sel)))
        })
        .cloned()
        .collect()
}

/// Apply filters to a list of PRs
pub fn filter_prs(
    prs: &[crate::utils::nip34::PullRequest],
    status: StatusFilter,
    query: &str,
    selected_labels: &[String],
) -> Vec<crate::utils::nip34::PullRequest> {
    let q = query.to_lowercase();
    prs.iter()
        .filter(|pr| {
            match status {
                StatusFilter::Open => {
                    pr.status == IssueStatus::Open || pr.status == IssueStatus::Draft
                }
                StatusFilter::Closed => {
                    pr.status == IssueStatus::Closed || pr.status == IssueStatus::Applied
                }
                StatusFilter::All => true,
            }
        })
        .filter(|pr| {
            if q.is_empty() {
                return true;
            }
            let title = pr.display_title().to_lowercase();
            let content = pr.content.to_lowercase();
            title.contains(&q) || content.contains(&q)
        })
        .filter(|pr| {
            if selected_labels.is_empty() {
                return true;
            }
            selected_labels.iter().any(|sel| pr.labels.iter().any(|il| il.eq_ignore_ascii_case(sel)))
        })
        .cloned()
        .collect()
}
