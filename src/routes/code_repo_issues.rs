//! Repository Issues Page
//!
//! View issues for a repository.
//! Follows patterns from code_issue_detail.rs and gittr design.

use dioxus::prelude::*;
use crate::components::{icons, CodeIssueRow};
use crate::routes::Route;
use crate::services::git_hosting::fetch_repo_issues;
use crate::stores::nostr_client;

/// Repository issues page component
#[component]
pub fn CodeRepoIssues(naddr: String) -> Element {
    let mut issues = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Clone for effect
    let naddr_for_effect = naddr.clone();

    // Fetch issues - wait for client initialization
    use_effect(move || {
        let n = naddr_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        spawn(async move {
            loading.set(true);
            match fetch_repo_issues(&n).await {
                Ok(fetched) => {
                    issues.set(fetched);
                    error.set(None);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "p-4 flex items-center justify-between",
                    div {
                        class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeRepo { naddr: naddr.clone() },
                            class: "text-muted-foreground hover:text-foreground",
                            dangerous_inner_html: icons::ARROW_LEFT
                        }
                        div {
                            h1 {
                                class: "text-xl font-bold flex items-center gap-2",
                                // Circle dot icon for issues
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
                            p {
                                class: "text-sm text-muted-foreground",
                                if !*loading.read() {
                                    "{issues.read().len()} issues"
                                }
                            }
                        }
                    }

                    // New Issue button
                    Link {
                        to: Route::CodeIssueNew { naddr: naddr.clone() },
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
                            line { x1: "12", y1: "5", x2: "12", y2: "19" }
                            line { x1: "5", y1: "12", x2: "19", y2: "12" }
                        }
                        "New Issue"
                    }
                }
            }

            // Content
            div {
                class: "p-4",

                // Error state
                if let Some(err) = error.read().as_ref() {
                    div {
                        class: "text-center py-12",
                        div {
                            class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
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
                                line { x1: "12", y1: "8", x2: "12", y2: "12" }
                                line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                            }
                        }
                        h3 { class: "font-semibold text-lg mb-2", "Failed to load issues" }
                        p { class: "text-muted-foreground text-sm", "{err}" }
                    }
                } else if *loading.read() {
                    // Loading skeleton
                    LoadingSkeleton {}
                } else if issues.read().is_empty() {
                    // Empty state
                    EmptyIssues {}
                } else {
                    // Issues list
                    div {
                        class: "border border-border rounded-lg divide-y divide-border",
                        for issue in issues.read().iter() {
                            CodeIssueRow {
                                key: "{issue.event_id}",
                                issue: issue.clone()
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
        div {
            class: "text-center py-12",
            div {
                class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
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
                    line { x1: "12", y1: "8", x2: "12", y2: "12" }
                    line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Issues" }
            p { class: "text-muted-foreground text-sm", "This repository has no open issues." }
        }
    }
}

#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div {
            class: "space-y-2 animate-pulse",
            for i in 0..3 {
                div {
                    key: "{i}",
                    class: "p-4 border border-border rounded-lg",
                    div { class: "h-4 bg-muted rounded w-2/3 mb-3" }
                    div { class: "flex items-center gap-3" }
                    div { class: "h-3 bg-muted rounded w-1/3" }
                }
            }
        }
    }
}
