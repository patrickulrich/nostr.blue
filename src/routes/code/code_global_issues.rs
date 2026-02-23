//! Global Issues Dashboard
//!
//! Cross-repository view of all issues relevant to the current user.
//! Shows issues created by, assigned to, or mentioning the user.
use crate::components::{icons, CodeIssueRow};
use crate::routes::Route;
use crate::services::git_hosting::{fetch_user_issues, fetch_issues_assigned_to, fetch_issues_mentioning};
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::{Issue, IssueStatus};
use dioxus::prelude::*;
use nostr_sdk::prelude::PublicKey;

#[derive(Clone, Copy, PartialEq)]
enum FilterTab {
    Created,
    Assigned,
    Mentioned,
}

#[derive(Clone, Copy, PartialEq)]
enum StatusFilter {
    Open,
    Closed,
}

#[component]
pub fn CodeGlobalIssues() -> Element {
    let mut created_issues = use_signal(Vec::<Issue>::new);
    let mut assigned_issues = use_signal(Vec::<Issue>::new);
    let mut mentioned_issues = use_signal(Vec::<Issue>::new);
    let mut loading = use_signal(|| true);
    let mut active_tab = use_signal(|| FilterTab::Created);
    let mut status_filter = use_signal(|| StatusFilter::Open);
    let mut search_query = use_signal(String::new);
    let mut label_filter = use_signal(|| Option::<String>::None);
    let mut request_gen = use_signal(|| 0u32);

    let user_pubkey = {
        let auth = auth_store::AUTH_STATE.read();
        auth.pubkey.clone().unwrap_or_default()
    };
    let client_init = *nostr_client::CLIENT_INITIALIZED.read();

    use_effect(use_reactive((&user_pubkey, &client_init), move |(pk_hex, initialized)| {
        if !initialized || pk_hex.is_empty() {
            return;
        }
        let gen = request_gen.peek().wrapping_add(1);
        request_gen.set(gen);
        created_issues.set(Vec::new());
        assigned_issues.set(Vec::new());
        mentioned_issues.set(Vec::new());
        label_filter.set(None);
        loading.set(true);
        spawn(async move {
            if let Ok(pk) = PublicKey::from_hex(&pk_hex) {
                let (created_res, assigned_res, mentioned_res) = futures::join!(
                    fetch_user_issues(&pk, 100),
                    fetch_issues_assigned_to(&pk, 100),
                    fetch_issues_mentioning(&pk, 100)
                );
                if *request_gen.peek() != gen { return; }
                match created_res {
                    Ok(fetched) => created_issues.set(fetched),
                    Err(e) => log::warn!("Failed to fetch created issues: {}", e),
                }
                match assigned_res {
                    Ok(fetched) => assigned_issues.set(fetched),
                    Err(e) => log::warn!("Failed to fetch assigned issues: {}", e),
                }
                match mentioned_res {
                    Ok(fetched) => mentioned_issues.set(fetched),
                    Err(e) => log::warn!("Failed to fetch mentioned issues: {}", e),
                }
            }
            loading.set(false);
        });
    }));

    if !auth_store::AUTH_STATE.read().is_authenticated {
        return rsx! { NotAuthenticatedState {} };
    }

    let all_issues_for_tab = use_memo(move || -> Vec<Issue> {
        match *active_tab.read() {
            FilterTab::Created => created_issues.read().clone(),
            FilterTab::Assigned => assigned_issues.read().clone(),
            FilterTab::Mentioned => mentioned_issues.read().clone(),
        }
    });
    let all_issues_for_tab = all_issues_for_tab.read();
    let mut all_labels: Vec<String> = all_issues_for_tab
        .iter()
        .flat_map(|i| i.labels.iter().cloned())
        .collect();
    all_labels.sort();
    all_labels.dedup();
    let query = search_query.read().to_lowercase();
    let filtered: Vec<_> = all_issues_for_tab
        .iter()
        .filter(|i| {
            // Status filter
            match *status_filter.read() {
                StatusFilter::Open => {
                    matches!(i.status, IssueStatus::Open | IssueStatus::Draft)
                }
                StatusFilter::Closed => {
                    matches!(i.status, IssueStatus::Closed | IssueStatus::Applied)
                }
            }
        })
        .filter(|i| {
            // Search filter
            if query.is_empty() {
                return true;
            }
            let title = i.display_title().to_lowercase();
            let content = i.content.to_lowercase();
            title.contains(&query) || content.contains(&query)
                || i.labels.iter().any(|l| l.to_lowercase().contains(&query))
        })
        .filter(|i| match label_filter.read().as_deref() {
            Some(filter) => i.labels.contains(&filter.to_string()),
            None => true,
        })
        .cloned()
        .collect();

    let open_count = all_issues_for_tab
        .iter()
        .filter(|i| matches!(i.status, IssueStatus::Open | IssueStatus::Draft))
        .count();
    let closed_count = all_issues_for_tab
        .iter()
        .filter(|i| matches!(i.status, IssueStatus::Closed | IssueStatus::Applied))
        .count();

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeHome {},
                            class: "text-muted-foreground hover:text-foreground",
                            dangerous_inner_html: icons::ARROW_LEFT,
                        }
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
                    }
                }
                div { class: "px-4 pb-3",
                    input {
                        class: "w-full px-3 py-1.5 bg-muted rounded-lg text-sm focus:outline-none focus:ring-1 focus:ring-primary",
                        r#type: "text",
                        placeholder: "Filter issues...",
                        value: "{search_query}",
                        oninput: move |e| search_query.set(e.value()),
                    }
                }
                div { class: "px-4 pb-3 flex gap-4",
                    button {
                        class: if *active_tab.read() == FilterTab::Created {
                            "text-sm font-medium text-foreground border-b-2 border-primary pb-1"
                        } else {
                            "text-sm text-muted-foreground hover:text-foreground pb-1"
                        },
                        onclick: move |_| { label_filter.set(None); active_tab.set(FilterTab::Created); },
                        "Created"
                    }
                    button {
                        class: if *active_tab.read() == FilterTab::Assigned {
                            "text-sm font-medium text-foreground border-b-2 border-primary pb-1"
                        } else {
                            "text-sm text-muted-foreground hover:text-foreground pb-1"
                        },
                        onclick: move |_| { label_filter.set(None); active_tab.set(FilterTab::Assigned); },
                        "Assigned"
                    }
                    button {
                        class: if *active_tab.read() == FilterTab::Mentioned {
                            "text-sm font-medium text-foreground border-b-2 border-primary pb-1"
                        } else {
                            "text-sm text-muted-foreground hover:text-foreground pb-1"
                        },
                        onclick: move |_| { label_filter.set(None); active_tab.set(FilterTab::Mentioned); },
                        "Mentioned"
                    }
                }
                div { class: "px-4 pb-3 flex gap-2",
                    button {
                        class: if *status_filter.read() == StatusFilter::Open {
                            "flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg bg-accent text-foreground font-medium"
                        } else {
                            "flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg text-muted-foreground hover:text-foreground hover:bg-accent/50 transition"
                        },
                        onclick: move |_| status_filter.set(StatusFilter::Open),
                        svg {
                            class: "w-4 h-4 text-green-500",
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
                        "{open_count} Open"
                    }
                    button {
                        class: if *status_filter.read() == StatusFilter::Closed {
                            "flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg bg-accent text-foreground font-medium"
                        } else {
                            "flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg text-muted-foreground hover:text-foreground hover:bg-accent/50 transition"
                        },
                        onclick: move |_| status_filter.set(StatusFilter::Closed),
                        svg {
                            class: "w-4 h-4 text-purple-500",
                            xmlns: "http://www.w3.org/2000/svg",
                            width: "24",
                            height: "24",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M22 11.08V12a10 10 0 1 1-5.93-9.14" }
                            polyline { points: "22 4 12 14.01 9 11.01" }
                        }
                        "{closed_count} Closed"
                    }
                }
            }
            if !all_labels.is_empty() {
                div { class: "px-4 pb-3 flex flex-wrap gap-2",
                    button {
                        class: if label_filter.read().is_none() { "px-2 py-1 text-xs rounded-full bg-primary text-primary-foreground" } else { "px-2 py-1 text-xs rounded-full bg-accent text-accent-foreground hover:bg-accent/80" },
                        onclick: move |_| label_filter.set(None),
                        "All"
                    }
                    for label in all_labels.iter() {
                        {
                            let l = label.clone();
                            rsx! {
                                button {
                                    key: "{l}",
                                    class: if label_filter.read().as_deref() == Some(&l) { "px-2 py-1 text-xs rounded-full bg-blue-500/20 text-blue-400 ring-1 ring-blue-400" } else { "px-2 py-1 text-xs rounded-full bg-blue-500/20 text-blue-400 hover:ring-1 hover:ring-blue-400/50" },
                                    onclick: {
                                        let l = l.clone();
                                        move |_| label_filter.set(Some(l.clone()))
                                    },
                                    "{l}"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "p-4",
                if *loading.read() {
                    LoadingSkeleton {}
                } else if filtered.is_empty() {
                    div { class: "text-center py-12",
                        p { class: "text-muted-foreground text-sm", "No issues found" }
                    }
                } else {
                    div { class: "border border-border rounded-lg divide-y divide-border",
                        for issue in filtered.iter() {
                            CodeIssueRow { key: "{issue.event_id}", issue: issue.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NotAuthenticatedState() -> Element {
    rsx! {
        div { class: "min-h-screen flex items-center justify-center p-4",
            div { class: "text-center max-w-md",
                div { class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
                    svg {
                        class: "w-10 h-10 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                        circle { cx: "12", cy: "7", r: "4" }
                    }
                }
                h2 { class: "font-semibold text-xl mb-2", "Sign In Required" }
                p { class: "text-muted-foreground mb-6",
                    "Connect with your Nostr identity to view your issues."
                }
                Link { to: Route::CodeHome {}, class: "text-primary hover:underline", "Back to Code" }
            }
        }
    }
}

#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-2 animate-pulse",
            for i in 0..5 {
                div { key: "{i}", class: "p-4 border border-border rounded-lg",
                    div { class: "h-4 bg-muted rounded w-2/3 mb-3" }
                    div { class: "h-3 bg-muted rounded w-1/3" }
                }
            }
        }
    }
}
