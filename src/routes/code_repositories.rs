//! Code Repositories Page
//!
//! User's Git repositories list (NIP-34).

use crate::components::{icons, CodeRepoCard};
use crate::routes::Route;
use crate::services::git_hosting::fetch_user_repositories;
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
use nostr_sdk::PublicKey;

/// Code repositories page component
#[component]
pub fn CodeRepositories() -> Element {
    let auth = auth_store::AUTH_STATE.read();

    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState {}
        };
    }

    let pubkey_hex = auth.pubkey.clone().unwrap_or_default();

    // Repos state
    let mut repos_result = use_signal(|| None::<Result<Vec<Repository>, String>>);

    // Clone for effect
    let pubkey_for_effect = pubkey_hex.clone();

    // Fetch repositories - wait for client initialization
    use_effect(move || {
        let pk = pubkey_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        spawn(async move {
            if pk.is_empty() {
                repos_result.set(Some(Err("No public key".to_string())));
                return;
            }
            let result = if let Ok(pubkey) = PublicKey::parse(&pk) {
                fetch_user_repositories(&pubkey, 50).await
            } else {
                Err("Invalid public key".to_string())
            };
            repos_result.set(Some(result));
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
                            to: Route::CodeHome {},
                            class: "text-muted-foreground hover:text-foreground",
                            dangerous_inner_html: icons::ARROW_LEFT
                        }
                        h1 {
                            class: "text-xl font-bold flex items-center gap-2",
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
                                // Repository icon
                                path { d: "M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.28 1.15-.28 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4" }
                                path { d: "M9 18c-4.51 2-5-2-7-2" }
                            }
                            "My Repositories"
                        }
                    }

                    // Import button
                    Link {
                        to: Route::CodeImport {},
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
                            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                            polyline { points: "17 8 12 3 7 8" }
                            line { x1: "12", y1: "3", x2: "12", y2: "15" }
                        }
                        "Import"
                    }
                }
            }

            // Content
            div {
                class: "p-4",

                // NIP-34 info
                div {
                    class: "mb-6 p-4 bg-blue-500/10 rounded-lg border border-blue-500/20",
                    div {
                        class: "flex items-start gap-3",
                        div {
                            class: "w-8 h-8 rounded-lg bg-blue-500/20 flex items-center justify-center shrink-0",
                            svg {
                                class: "w-4 h-4 text-blue-500",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                line { x1: "6", y1: "3", x2: "6", y2: "15" }
                                circle { cx: "18", cy: "6", r: "3" }
                                circle { cx: "6", cy: "18", r: "3" }
                                path { d: "M18 9a9 9 0 0 1-9 9" }
                            }
                        }
                        div {
                            p {
                                class: "text-sm",
                                span { class: "font-medium", "NIP-34 Git Repositories" }
                                span { class: "text-muted-foreground", " - Manage issues, pull requests, and collaborate without centralized servers." }
                            }
                        }
                    }
                }

                // Repositories list
                match &*repos_result.read() {
                    Some(Ok(list)) if !list.is_empty() => rsx! {
                        div {
                            class: "space-y-3",
                            for repo in list.iter() {
                                CodeRepoCard {
                                    key: "{repo.event_id}",
                                    repo: repo.clone()
                                }
                            }
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        EmptyState {}
                    },
                    Some(Err(e)) => rsx! {
                        div {
                            class: "text-center py-12 text-destructive",
                            "Error loading repositories: {e}"
                        }
                    },
                    None => rsx! {
                        LoadingState {}
                    },
                }
            }
        }
    }
}

#[component]
fn NotAuthenticatedState() -> Element {
    rsx! {
        div {
            class: "min-h-screen flex items-center justify-center p-4",
            div {
                class: "text-center max-w-md",
                div {
                    class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
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
                h2 {
                    class: "font-semibold text-xl mb-2",
                    "Sign In Required"
                }
                p {
                    class: "text-muted-foreground mb-6",
                    "Connect with your Nostr identity to view and manage your repositories."
                }
                Link {
                    to: Route::CodeHome {},
                    class: "text-primary hover:underline",
                    "← Back to Code"
                }
            }
        }
    }
}

#[component]
fn EmptyState() -> Element {
    rsx! {
        div {
            class: "text-center py-16",
            div {
                class: "w-20 h-20 mx-auto mb-6 rounded-full bg-muted flex items-center justify-center",
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
                    // Repository icon
                    path { d: "M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.28 1.15-.28 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4" }
                    path { d: "M9 18c-4.51 2-5-2-7-2" }
                }
            }
            h3 {
                class: "font-semibold text-xl mb-2",
                "No Repositories Yet"
            }
            p {
                class: "text-muted-foreground max-w-md mx-auto mb-6",
                "Import a Git repository from GitHub, GitLab, or Codeberg to start hosting your code on Nostr."
            }
            Link {
                to: Route::CodeImport {},
                class: "inline-flex items-center gap-2 px-6 py-3 bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition font-medium",
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
                    path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
                    polyline { points: "17 8 12 3 7 8" }
                    line { x1: "12", y1: "3", x2: "12", y2: "15" }
                }
                "Import Your First Repository"
            }
        }
    }
}

#[component]
fn LoadingState() -> Element {
    rsx! {
        div {
            class: "space-y-3",
            for i in 0..5 {
                div {
                    key: "{i}",
                    class: "p-4 border border-border rounded-lg animate-pulse",
                    div {
                        class: "flex items-start gap-3",
                        div { class: "w-10 h-10 rounded-lg bg-muted" }
                        div {
                            class: "flex-1",
                            div { class: "h-4 bg-muted rounded w-1/3 mb-2" }
                            div { class: "h-3 bg-muted rounded w-1/4" }
                        }
                    }
                    div { class: "h-3 bg-muted rounded w-2/3 mt-3" }
                    div {
                        class: "flex gap-4 mt-3",
                        div { class: "h-3 bg-muted rounded w-12" }
                        div { class: "h-3 bg-muted rounded w-12" }
                    }
                }
            }
        }
    }
}
