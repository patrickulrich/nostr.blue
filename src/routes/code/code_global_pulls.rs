//! Global Pull Requests Dashboard
//!
//! Cross-repository view of all pull requests relevant to the current user.
//! Shows PRs created by the user with status filtering and search.
use crate::components::{icons, CodePullRow};
use crate::routes::Route;
use crate::services::git_hosting::{fetch_user_prs, fetch_prs_assigned_to};
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::{IssueStatus, PullRequest};
use dioxus::prelude::*;
use nostr_sdk::prelude::PublicKey;

#[derive(Clone, Copy, PartialEq)]
enum FilterTab {
    Created,
    Assigned,
    // NIP-34 uses p-tags for both assignment and mentions with no protocol-level
    // distinction, so a separate "Mentioned" tab would duplicate "Assigned".
}

#[derive(Clone, Copy, PartialEq)]
enum StatusFilter {
    Open,
    Closed,
}

#[component]
pub fn CodeGlobalPulls() -> Element {
    // All hooks must be called before any early returns to maintain stable call-order indices.
    let mut created_prs = use_signal(Vec::<PullRequest>::new);
    let mut assigned_prs = use_signal(Vec::<PullRequest>::new);
    let mut loading = use_signal(|| true);
    let mut active_tab = use_signal(|| FilterTab::Created);
    let mut status_filter = use_signal(|| StatusFilter::Open);
    let mut search_query = use_signal(String::new);
    let mut label_filter = use_signal(|| Option::<String>::None);
    let mut fetch_error = use_signal(|| None::<String>);
    let mut fetch_gen = use_signal(|| 0u32);

    let auth = auth_store::AUTH_STATE.read();
    let user_pubkey = auth.pubkey.clone().unwrap_or_default();
    let client_init = *nostr_client::CLIENT_INITIALIZED.read();
    use_effect(
        use_reactive(
            (&user_pubkey, &client_init),
            move |(pk_hex, client_initialized)| {
                if !client_initialized || pk_hex.is_empty() {
                    return;
                }
                let gen = fetch_gen.peek().wrapping_add(1);
                fetch_gen.set(gen);
                created_prs.set(Vec::new());
                assigned_prs.set(Vec::new());
                loading.set(true);
                fetch_error.set(None);
                let captured_pk = pk_hex.clone();
                spawn(async move {
                    if let Ok(pk) = PublicKey::from_hex(&captured_pk) {
                        let (created_res, assigned_res) = futures::join!(
                            fetch_user_prs(&pk, 100),
                            fetch_prs_assigned_to(&pk, 100)
                        );
                        if *fetch_gen.peek() != gen { return; }
                        let mut errors = Vec::new();
                        match created_res {
                            Ok(fetched) => created_prs.set(fetched),
                            Err(e) => {
                                errors.push(format!("Created: {}", e));
                                created_prs.set(Vec::new());
                            }
                        }
                        match assigned_res {
                            Ok(fetched) => assigned_prs.set(fetched),
                            Err(e) => {
                                errors.push(format!("Assigned: {}", e));
                                assigned_prs.set(Vec::new());
                            }
                        }
                        if !errors.is_empty() {
                            fetch_error.set(Some(errors.join("; ")));
                        }
                    }
                    loading.set(false);
                });
            },
        ),
    );

    if !auth.is_authenticated {
        return rsx! { NotAuthenticatedState {} };
    }

    let all_prs_for_tab: Vec<PullRequest> = match *active_tab.read() {
        FilterTab::Created => created_prs.read().clone(),
        FilterTab::Assigned => assigned_prs.read().clone(),
    };
    let mut all_labels: Vec<String> = all_prs_for_tab
        .iter()
        .flat_map(|p| p.labels.iter().cloned())
        .collect();
    all_labels.sort();
    all_labels.dedup();
    let query = search_query.read().to_lowercase();
    let current_label = label_filter.read().clone();
    let filtered: Vec<_> = all_prs_for_tab
        .iter()
        .filter(|p| match *status_filter.read() {
            StatusFilter::Open => {
                matches!(p.status, IssueStatus::Open | IssueStatus::Draft)
            }
            StatusFilter::Closed => {
                matches!(p.status, IssueStatus::Closed | IssueStatus::Applied)
            }
        })
        .filter(|p| {
            if query.is_empty() {
                return true;
            }
            let title = p.display_title().to_lowercase();
            let content = p.content.to_lowercase();
            title.contains(&query) || content.contains(&query)
                || p.labels.iter().any(|l| l.to_lowercase().contains(&query))
        })
        .filter(|p| match current_label.as_deref() {
            Some(filter) => p.labels.iter().any(|l| l == filter),
            None => true,
        })
        .cloned()
        .collect();

    let open_count = all_prs_for_tab
        .iter()
        .filter(|p| matches!(p.status, IssueStatus::Open | IssueStatus::Draft))
        .count();
    let closed_count = all_prs_for_tab
        .iter()
        .filter(|p| matches!(p.status, IssueStatus::Closed | IssueStatus::Applied))
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
                                circle { cx: "18", cy: "18", r: "3" }
                                circle { cx: "6", cy: "6", r: "3" }
                                path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                                line { x1: "6", y1: "9", x2: "6", y2: "21" }
                            }
                            "Pull Requests"
                        }
                    }
                }
                div { class: "px-4 pb-3",
                    input {
                        class: "w-full px-3 py-1.5 bg-muted rounded-lg text-sm focus:outline-none focus:ring-1 focus:ring-primary",
                        r#type: "text",
                        placeholder: "Filter pull requests...",
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
                        onclick: move |_| {
                            active_tab.set(FilterTab::Created);
                            label_filter.set(None);
                        },
                        "Created"
                    }
                    button {
                        class: if *active_tab.read() == FilterTab::Assigned {
                            "text-sm font-medium text-foreground border-b-2 border-primary pb-1"
                        } else {
                            "text-sm text-muted-foreground hover:text-foreground pb-1"
                        },
                        onclick: move |_| {
                            active_tab.set(FilterTab::Assigned);
                            label_filter.set(None);
                        },
                        "Assigned"
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
                            circle { cx: "18", cy: "18", r: "3" }
                            circle { cx: "6", cy: "6", r: "3" }
                            path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                            line { x1: "6", y1: "9", x2: "6", y2: "21" }
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
                                    key: "{label}",
                                    class: if label_filter.read().as_deref() == Some(&l) { "px-2 py-1 text-xs rounded-full bg-blue-500/20 text-blue-400 ring-1 ring-blue-400" } else { "px-2 py-1 text-xs rounded-full bg-blue-500/20 text-blue-400 hover:ring-1 hover:ring-blue-400/50" },
                                    onclick: move |_| label_filter.set(Some(l.clone())),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
            if let Some(err) = fetch_error.read().as_ref() {
                div { class: "mx-4 mt-3 p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-sm text-destructive",
                    "{err}"
                }
            }
            div { class: "p-4",
                if *loading.read() {
                    LoadingSkeleton {}
                } else if filtered.is_empty() {
                    div { class: "text-center py-12",
                        p { class: "text-muted-foreground text-sm", "No pull requests found" }
                    }
                } else {
                    div { class: "border border-border rounded-lg divide-y divide-border",
                        for pr in filtered.iter() {
                            CodePullRow { key: "{pr.event_id}", pr: pr.clone() }
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
                    "Connect with your Nostr identity to view your pull requests."
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
