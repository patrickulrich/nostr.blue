//! New Issue Page
//!
//! Create a new NIP-34 Git issue (Kind 1621) for a repository.

use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, publish_issue_by_naddr};
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::Repository;
use dioxus::prelude::*;

/// New issue page component
#[component]
pub fn CodeIssueNew(naddr: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();

    // Form state
    let mut title = use_signal(String::new);
    let mut content = use_signal(String::new);
    let mut labels = use_signal(String::new);
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);

    // Repository state
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let mut loading = use_signal(|| true);

    let nav = use_navigator();

    // Clone for effect
    let naddr_for_effect = naddr.clone();

    // Fetch repository info - wait for client initialization
    use_effect(move || {
        let n = naddr_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        spawn(async move {
            loading.set(true);
            let result = fetch_repository(&n).await;
            repo_result.set(Some(result));
            loading.set(false);
        });
    });

    // Check authentication
    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState { naddr: naddr.clone() }
        };
    }

    let handle_submit = {
        let naddr = naddr.clone();
        move |_| {
            let title_val = title.read().clone();
            let content_val = content.read().clone();
            let labels_val = labels.read().clone();
            let naddr = naddr.clone();

            spawn(async move {
                // Validate
                if content_val.trim().is_empty() {
                    error_message.set(Some("Please describe the issue".to_string()));
                    return;
                }

                is_publishing.set(true);
                error_message.set(None);

                // Parse labels
                let label_list: Vec<&str> = if labels_val.is_empty() {
                    vec![]
                } else {
                    labels_val.split(',').map(|s| s.trim()).collect()
                };

                let subject = if title_val.is_empty() {
                    None
                } else {
                    Some(title_val.as_str())
                };

                match publish_issue_by_naddr(&naddr, subject, &content_val, &label_list).await {
                    Ok(event_id) => {
                        // Navigate to the new issue
                        nav.push(Route::CodeIssueDetail { note_id: event_id });
                    }
                    Err(e) => {
                        error_message.set(Some(e));
                        is_publishing.set(false);
                    }
                }
            });
        }
    };

    let repo_name = match &*repo_result.read() {
        Some(Ok(r)) => r.name.clone().unwrap_or_else(|| r.id.clone()),
        _ => "Repository".to_string(),
    };

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
                                circle { cx: "12", cy: "12", r: "10" }
                                line { x1: "12", y1: "8", x2: "12", y2: "12" }
                                line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                            }
                            "New Issue"
                        }
                    }

                    // Submit button
                    button {
                        class: "px-4 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                        disabled: *is_publishing.read(),
                        onclick: handle_submit,
                        if *is_publishing.read() {
                            svg {
                                class: "w-4 h-4 animate-spin",
                                xmlns: "http://www.w3.org/2000/svg",
                                fill: "none",
                                view_box: "0 0 24 24",
                                circle {
                                    class: "opacity-25",
                                    cx: "12",
                                    cy: "12",
                                    r: "10",
                                    stroke: "currentColor",
                                    stroke_width: "4"
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                                }
                            }
                            "Publishing..."
                        } else {
                            "Submit Issue"
                        }
                    }
                }
            }

            // Content
            div {
                class: "p-4 space-y-6",

                // Error message
                if let Some(error) = error_message.read().as_ref() {
                    div {
                        class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }

                // Repository info
                div {
                    class: "p-4 bg-muted rounded-lg",
                    p {
                        class: "text-sm text-muted-foreground",
                        "Creating issue for "
                        span { class: "font-medium text-foreground", "{repo_name}" }
                    }
                }

                // NIP-34 info
                div {
                    class: "p-4 bg-blue-500/10 rounded-lg border border-blue-500/20",
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
                                circle { cx: "12", cy: "12", r: "10" }
                                path { d: "M12 16v-4" }
                                path { d: "M12 8h.01" }
                            }
                        }
                        div {
                            p {
                                class: "text-sm",
                                span { class: "font-medium", "NIP-34 Git Issue" }
                                span { class: "text-muted-foreground", " - Your issue will be published as a Kind 1621 event on Nostr. It will be publicly visible and linked to the repository." }
                            }
                        }
                    }
                }

                // Title
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Title "
                        span { class: "text-muted-foreground font-normal", "(optional)" }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "Brief summary of the issue",
                        value: "{title}",
                        oninput: move |e| title.set(e.value())
                    }
                }

                // Content
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Description "
                        span { class: "text-destructive", "*" }
                    }
                    textarea {
                        class: "w-full h-48 px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary resize-y",
                        placeholder: "Describe the issue in detail. What happened? What did you expect to happen?",
                        value: "{content}",
                        oninput: move |e| content.set(e.value())
                    }
                    p {
                        class: "text-xs text-muted-foreground mt-1",
                        "Markdown formatting is supported"
                    }
                }

                // Labels
                div {
                    label {
                        class: "block text-sm font-medium mb-2",
                        "Labels "
                        span { class: "text-muted-foreground font-normal", "(optional)" }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "e.g., bug, enhancement, documentation",
                        value: "{labels}",
                        oninput: move |e| labels.set(e.value())
                    }
                    p {
                        class: "text-xs text-muted-foreground mt-1",
                        "Comma-separated list of labels"
                    }
                }
            }
        }
    }
}

#[component]
fn NotAuthenticatedState(naddr: String) -> Element {
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
                    "Connect with your Nostr identity to create issues."
                }
                Link {
                    to: Route::CodeRepo { naddr },
                    class: "text-primary hover:underline",
                    "Back to Repository"
                }
            }
        }
    }
}
