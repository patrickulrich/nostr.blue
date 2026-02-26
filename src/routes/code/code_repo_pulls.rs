//! Repository Pull Requests Page
//!
//! View pull requests for a repository with filtering by status, search, and labels.
use crate::components::code::{FilterBar, StatusFilter, filter_prs};
use crate::components::{icons, CodePullRow};
use crate::routes::Route;
use crate::services::git_hosting::fetch_repo_prs;
use crate::stores::nostr_client;
use crate::utils::nip34::IssueStatus;
use dioxus::prelude::*;

/// Repository pull requests page component
#[component]
pub fn CodeRepoPulls(naddr: String) -> Element {
    let mut all_prs = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut status_filter = use_signal(|| StatusFilter::Open);
    let mut search_query = use_signal(String::new);
    let mut selected_labels = use_signal(Vec::<String>::new);
    let mut request_gen = use_signal(|| 0u32);

    use_effect(use_reactive(&naddr, move |naddr| {
        error.set(None);
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let gen = request_gen.peek().wrapping_add(1);
        request_gen.set(gen);
        all_prs.set(vec![]);
        selected_labels.set(Vec::new());
        status_filter.set(StatusFilter::Open);
        search_query.set(String::new());
        loading.set(true);
        spawn(async move {
            let result = fetch_repo_prs(&naddr).await;
            if *request_gen.peek() != gen { return; }
            match result {
                Ok(fetched) => {
                    all_prs.set(fetched);
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    }));

    let filtered = use_memo(move || {
        let prs = all_prs.read();
        let status = *status_filter.read();
        let query = search_query.read().clone();
        let labels = selected_labels.read().clone();
        filter_prs(&prs, status, &query, &labels)
    });

    let open_count = use_memo(move || {
        all_prs
            .read()
            .iter()
            .filter(|p| p.status == IssueStatus::Open || p.status == IssueStatus::Draft)
            .count()
    });
    let closed_count = use_memo(move || {
        all_prs
            .read()
            .iter()
            .filter(|p| p.status == IssueStatus::Closed || p.status == IssueStatus::Applied)
            .count()
    });

    let available_labels = use_memo(move || {
        let mut labels: Vec<String> = all_prs
            .read()
            .iter()
            .flat_map(|p| p.labels.iter().cloned())
            .collect();
        labels.sort();
        labels.dedup();
        labels
    });

    // Trim selected_labels to only contain labels available in the current repo
    use_effect(move || {
        let current_selected = selected_labels.read().clone();
        let available = available_labels.read();
        let trimmed: Vec<String> = current_selected
            .into_iter()
            .filter(|l| available.contains(l))
            .collect();
        if trimmed != *selected_labels.peek() {
            selected_labels.set(trimmed);
        }
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
                                "Pull Requests"
                            }
                            p { class: "text-sm text-muted-foreground",
                                if !*loading.read() {
                                    "{all_prs.read().len()} pull requests"
                                }
                            }
                        }
                    }
                    Link {
                        to: Route::CodePullNew {
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
                        "New PR"
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
                        h3 { class: "font-semibold text-lg mb-2", "Failed to load pull requests" }
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
                        open_count: *open_count.read(),
                        closed_count: *closed_count.read(),
                        available_labels: available_labels.read().clone(),
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
                    if filtered.read().is_empty() {
                        EmptyPRs { has_filters: !search_query.read().is_empty() || !selected_labels.read().is_empty() || *status_filter.read() != StatusFilter::Open }
                    } else {
                        div { class: "border border-border rounded-lg divide-y divide-border",
                            for pr in filtered.read().iter() {
                                CodePullRow { key: "{pr.event_id}", pr: pr.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn EmptyPRs(has_filters: bool) -> Element {
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
            }
            h3 { class: "font-semibold text-lg mb-2",
                if has_filters {
                    "No Matching Pull Requests"
                } else {
                    "No Pull Requests"
                }
            }
            p { class: "text-muted-foreground text-sm",
                if has_filters {
                    "No pull requests match the current filters."
                } else {
                    "No pull requests yet."
                }
            }
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
