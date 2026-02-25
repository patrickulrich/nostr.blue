//! New Discussion Page
//!
//! Create a new discussion for a repository.
//! Follows patterns from code_issue_new.rs.
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::discussions::publish_discussion_by_naddr;
use crate::services::git_hosting::fetch_repository;
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
/// New discussion page component
#[component]
pub fn CodeDiscussionNew(naddr: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut subject = use_signal(String::new);
    let mut content = use_signal(String::new);
    let mut category = use_signal(|| "general".to_string());
    let mut is_publishing = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let mut loading = use_signal(|| true);
    let mut fetch_gen = use_signal(|| 0u32);
    let nav = use_navigator();
    let naddr_for_effect = naddr.clone();
    use_effect(move || {
        let n = naddr_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let auth = auth_store::AUTH_STATE.read();
        if !auth.is_authenticated {
            loading.set(false);
            return;
        }
        let gen = fetch_gen.peek().wrapping_add(1);
        fetch_gen.set(gen);
        repo_result.set(None);
        spawn(async move {
            loading.set(true);
            let result = fetch_repository(&n).await;
            if *fetch_gen.peek() != gen { return; }
            repo_result.set(Some(result));
            loading.set(false);
        });
    });
    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState { naddr: naddr.clone() }
        };
    }
    let handle_submit = {
        let naddr = naddr.clone();
        move |_| {
            if *is_publishing.peek() { return; }
            let subject_val = subject.read().clone();
            let content_val = content.read().clone();
            let category_val = category.read().clone();
            let naddr = naddr.clone();
            if content_val.trim().is_empty() {
                error_message.set(Some("Please provide discussion content".to_string()));
                return;
            }
            is_publishing.set(true);
            error_message.set(None);
            spawn(async move {
                let subj = if subject_val.is_empty() {
                    None
                } else {
                    Some(subject_val.as_str())
                };
                let cat = if category_val.is_empty() {
                    None
                } else {
                    Some(category_val.as_str())
                };
                match publish_discussion_by_naddr(&naddr, subj, &content_val, cat, &[]).await {
                    Ok(event_id) => {
                        nav.push(Route::CodeDiscussionDetail {
                            note_id: event_id,
                        });
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
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeRepo {
                                naddr: naddr.clone(),
                            },
                            class: "text-muted-foreground hover:text-foreground",
                            aria_label: "Back to repository",
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
                                path { d: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" }
                            }
                            "New Discussion"
                        }
                    }
                    button {
                        class: "px-4 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                        r#type: "button",
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
                                    stroke_width: "4",
                                }
                                path {
                                    class: "opacity-75",
                                    fill: "currentColor",
                                    d: "M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z",
                                }
                            }
                            "Publishing..."
                        } else {
                            "Start Discussion"
                        }
                    }
                }
            }
            div { class: "p-4 space-y-6",
                if let Some(error) = error_message.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }
                div { class: "p-4 bg-muted rounded-lg",
                    p { class: "text-sm text-muted-foreground",
                        "Starting discussion for "
                        span { class: "font-medium text-foreground", "{repo_name}" }
                    }
                }
                div {
                    label { class: "block text-sm font-medium mb-2",
                        "Subject "
                        span { class: "text-muted-foreground font-normal", "(optional)" }
                    }
                    input {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "text",
                        placeholder: "Discussion topic",
                        value: "{subject}",
                        oninput: move |e| subject.set(e.value()),
                    }
                }
                div {
                    label { class: "block text-sm font-medium mb-2", "Category" }
                    select {
                        class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        value: "{category}",
                        onchange: move |e| category.set(e.value()),
                        option { value: "general", "General" }
                        option { value: "ideas", "Ideas" }
                        option { value: "q-a", "Q&A" }
                        option { value: "show-and-tell", "Show & Tell" }
                    }
                }
                div {
                    label { class: "block text-sm font-medium mb-2",
                        "Content "
                        span { class: "text-destructive", "*" }
                    }
                    textarea {
                        class: "w-full h-48 px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary resize-y",
                        placeholder: "Share your thoughts, ideas, or questions...",
                        value: "{content}",
                        oninput: move |e| content.set(e.value()),
                    }
                    p { class: "text-xs text-muted-foreground mt-1",
                        "Markdown formatting is supported"
                    }
                }
            }
        }
    }
}
#[component]
fn NotAuthenticatedState(naddr: String) -> Element {
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
                    "Connect with your Nostr identity to start discussions."
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
