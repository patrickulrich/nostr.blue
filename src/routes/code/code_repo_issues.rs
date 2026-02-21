//! Repository Issues Page
//!
//! View issues for a repository with filtering by status, search, and labels.
use crate::components::code::{FilterBar, StatusFilter, filter_issues};
use crate::components::{icons, CodeIssueRow};
use crate::routes::Route;
use crate::services::git_hosting::fetch_repo_issues;
use crate::stores::nostr_client;
use crate::utils::nip34::IssueStatus;
use dioxus::prelude::*;

/// Repository issues page component
#[component]
pub fn CodeRepoIssues(naddr: String) -> Element {
    let mut all_issues = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut status_filter = use_signal(|| StatusFilter::Open);
    let mut search_query = use_signal(String::new);
    let mut selected_labels = use_signal(Vec::<String>::new);

    use_effect(use_reactive(&naddr, move |n| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            match fetch_repo_issues(&n).await {
                Ok(fetched) => {
                    all_issues.set(fetched);
                    error.set(None);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    }));

    // Derive filtered list (memoized)
    let filtered = use_memo(move || {
        let issues = all_issues.read();
        let status = *status_filter.read();
        let query = search_query.read().clone();
        let labels = selected_labels.read().clone();
        filter_issues(&issues, status, &query, &labels)
    });

    // Count open/closed for tabs (memoized)
    let open_count = use_memo(move || {
        all_issues
            .read()
            .iter()
            .filter(|i| i.status == IssueStatus::Open || i.status == IssueStatus::Draft)
            .count()
    });
    let closed_count = use_memo(move || {
        all_issues
            .read()
            .iter()
            .filter(|i| i.status == IssueStatus::Closed || i.status == IssueStatus::Applied)
            .count()
    });

    // Collect unique labels for filter chips (memoized)
    let available_labels = use_memo(move || {
        let mut labels: Vec<String> = all_issues
            .read()
            .iter()
            .flat_map(|i| i.labels.clone())
            .collect();
        labels.sort();
        labels.dedup();
        labels
    });

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeRepo {
                                naddr: naddr.clone(),
                            },
                            class: "text-muted-foreground hover:text-foreground",
                            dangerous_inner_html: icons::ARROW_LEFT,
                        }
                        div {
                            h1 { class: "text-xl font-bold flex items-center gap-2",
                                svg {
                                    class: "w-5 h-5",
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
                                    circle { cx: "12", cy: "12", r: "1" }
                                }
                                "Issues"
                            }
                            p { class: "text-sm text-muted-foreground",
                                if !*loading.read() {
                                    "{all_issues.read().len()} issues"
                                }
                            }
                        }
                    }
                    Link {
                        to: Route::CodeIssueNew {
                            naddr: naddr.clone(),
                        },
                        class: "px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition flex items-center gap-1",
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
                            line {
                                x1: "12",
                                y1: "5",
                                x2: "12",
                                y2: "19",
                            }
                            line {
                                x1: "5",
                                y1: "12",
                                x2: "19",
                                y2: "12",
                            }
                        }
                        "New Issue"
                    }
                }
            }
            div { class: "p-4",
                if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-12",
                        div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
                            svg {
                                class: "w-8 h-8 text-destructive",
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
                                line {
                                    x1: "12",
                                    y1: "8",
                                    x2: "12",
                                    y2: "12",
                                }
                                line {
                                    x1: "12",
                                    y1: "16",
                                    x2: "12.01",
                                    y2: "16",
                                }
                            }
                        }
                        h3 { class: "font-semibold text-lg mb-2", "Failed to load issues" }
                        p { class: "text-muted-foreground text-sm", "{err}" }
                    }
                } else if *loading.read() {
                    LoadingSkeleton {}
                } else {
                    FilterBar {
                        status_filter: *status_filter.read(),
                        on_status_change: move |s| status_filter.set(s),
                        search_query: search_query.read().clone(),
                        on_search_change: move |q| search_query.set(q),
                        open_count: open_count(),
                        closed_count: closed_count(),
                        available_labels: available_labels(),
                        selected_labels: selected_labels.read().clone(),
                        on_label_toggle: move |label: String| {
                            let mut labels = selected_labels.write();
                            if labels.contains(&label) {
                                labels.retain(|l| *l != label);
                            } else {
                                labels.push(label);
                            }
                        },
                    }
                    {
                        let filtered_issues = filtered();
                        if filtered_issues.is_empty() {
                            rsx! { EmptyIssues {} }
                        } else {
                            rsx! {
                                div { class: "border border-border rounded-lg divide-y divide-border",
                                    for issue in filtered_issues.iter() {
                                        CodeIssueRow { key: "{issue.event_id}", issue: issue.clone() }
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
#[component]
fn EmptyIssues() -> Element {
    rsx! {
        div { class: "text-center py-12",
            div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                svg {
                    class: "w-8 h-8 text-muted-foreground",
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
                    line {
                        x1: "12",
                        y1: "8",
                        x2: "12",
                        y2: "12",
                    }
                    line {
                        x1: "12",
                        y1: "16",
                        x2: "12.01",
                        y2: "16",
                    }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Issues" }
            p { class: "text-muted-foreground text-sm", "No issues match the current filters." }
        }
    }
}
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-2 animate-pulse",
            for i in 0..3 {
                div { key: "{i}", class: "p-4 border border-border rounded-lg",
                    div { class: "h-4 bg-muted rounded w-2/3 mb-3" }
                    div { class: "flex items-center gap-3" }
                    div { class: "h-3 bg-muted rounded w-1/3" }
                }
            }
        }
    }
}
