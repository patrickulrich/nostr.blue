//! Repository Pull Requests Page
//!
//! View pull requests for a repository.
//! Follows patterns from code_issue_detail.rs and gittr design.

use dioxus::prelude::*;
use crate::components::{icons, CodePullRow};
use crate::routes::Route;
use crate::services::git_hosting::fetch_repo_prs;
use crate::stores::nostr_client;

/// Repository pull requests page component
#[component]
pub fn CodeRepoPulls(naddr: String) -> Element {
    let mut prs = use_signal(Vec::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Clone for effect
    let naddr_for_effect = naddr.clone();

    // Fetch PRs - wait for client initialization
    use_effect(move || {
        let n = naddr_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        spawn(async move {
            loading.set(true);
            match fetch_repo_prs(&n).await {
                Ok(fetched) => {
                    prs.set(fetched);
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
                                // Git pull request icon
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
                            p {
                                class: "text-sm text-muted-foreground",
                                if !*loading.read() {
                                    "{prs.read().len()} pull requests"
                                }
                            }
                        }
                    }

                    // New PR button
                    Link {
                        to: Route::CodePullNew { naddr: naddr.clone() },
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
                        "New PR"
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
                        h3 { class: "font-semibold text-lg mb-2", "Failed to load pull requests" }
                        p { class: "text-muted-foreground text-sm", "{err}" }
                    }
                } else if *loading.read() {
                    // Loading skeleton
                    LoadingSkeleton {}
                } else if prs.read().is_empty() {
                    // Empty state
                    EmptyPRs {}
                } else {
                    // PRs list
                    div {
                        class: "border border-border rounded-lg divide-y divide-border",
                        for pr in prs.read().iter() {
                            CodePullRow {
                                key: "{pr.event_id}",
                                pr: pr.clone()
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EmptyPRs() -> Element {
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
                    circle { cx: "18", cy: "18", r: "3" }
                    circle { cx: "6", cy: "6", r: "3" }
                    path { d: "M13 6h3a2 2 0 0 1 2 2v7" }
                    line { x1: "6", y1: "9", x2: "6", y2: "21" }
                }
            }
            h3 { class: "font-semibold text-lg mb-2", "No Pull Requests" }
            p { class: "text-muted-foreground text-sm", "This repository has no open pull requests." }
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
