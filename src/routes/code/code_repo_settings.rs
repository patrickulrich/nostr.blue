//! Repository Settings Page
//!
//! Manage repository settings for NIP-34 repositories.
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::{fetch_repository, publish_repository};
use crate::stores::{auth_store, nostr_client};
use crate::utils::nip34::Repository;
use dioxus::prelude::*;
/// Repository settings page component
#[component]
pub fn CodeRepoSettings(naddr: String) -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let nav = use_navigator();
    let mut repo_result = use_signal(|| None::<Result<Repository, String>>);
    let naddr_for_effect = naddr.clone();
    use_effect(move || {
        let n = naddr_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            let result = fetch_repository(&n).await;
            repo_result.set(Some(result));
        });
    });
    let mut repo_name = use_signal(String::new);
    let mut repo_description = use_signal(String::new);
    let mut clone_url = use_signal(String::new);
    let mut web_url = use_signal(String::new);
    let mut form_initialized = use_signal(|| false);
    let mut is_saving = use_signal(|| false);
    let mut save_error = use_signal(|| None::<String>);
    let mut save_success = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);
    if let Some(Ok(r)) = repo_result.read().as_ref() {
        if !*form_initialized.read() {
            repo_name.set(r.name.clone().unwrap_or_default());
            repo_description.set(r.description.clone().unwrap_or_default());
            clone_url.set(r.clone.first().cloned().unwrap_or_default());
            web_url.set(r.web.first().cloned().unwrap_or_default());
            form_initialized.set(true);
        }
    }
    let is_owner = if let Some(Ok(r)) = repo_result.read().as_ref() {
        auth.pubkey.as_ref().map(|pk| pk == &r.pubkey).unwrap_or(false)
    } else {
        false
    };
    if !auth.is_authenticated {
        return rsx! {
            NotAuthenticatedState { naddr: naddr.clone() }
        };
    }
    if !is_owner && repo_result.read().is_some() {
        return rsx! {
            NotOwnerState { naddr: naddr.clone() }
        };
    }
    let handle_save = {
        let _naddr = naddr.clone();
        move |_| {
            let repo_data = match repo_result.read().as_ref() {
                Some(Ok(r)) => r.clone(),
                _ => return,
            };
            let name = repo_name.read().clone();
            let description = repo_description.read().clone();
            let clone = clone_url.read().clone();
            let web = web_url.read().clone();
            spawn(async move {
                is_saving.set(true);
                save_error.set(None);
                save_success.set(false);
                let clone_urls: Vec<&str> = if clone.is_empty() {
                    vec![]
                } else {
                    vec![clone.as_str()]
                };
                let web_urls: Vec<&str> = if web.is_empty() {
                    vec![]
                } else {
                    vec![web.as_str()]
                };
                let relays: Vec<&str> = repo_data
                    .relays
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                let name_opt = if name.is_empty() { None } else { Some(name.as_str()) };
                let desc_opt = if description.is_empty() {
                    None
                } else {
                    Some(description.as_str())
                };
                match publish_repository(
                        &repo_data.id,
                        name_opt,
                        desc_opt,
                        &clone_urls,
                        &web_urls,
                        &relays,
                        &[],
                    )
                    .await
                {
                    Ok(_) => {
                        save_success.set(true);
                    }
                    Err(e) => {
                        save_error.set(Some(e));
                    }
                }
                is_saving.set(false);
            });
        }
    };
    let repo_id = match &*repo_result.read() {
        Some(Ok(r)) => r.id.clone(),
        _ => "".to_string(),
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
                                circle { cx: "12", cy: "12", r: "3" }
                                path { d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" }
                            }
                            "Settings"
                        }
                    }
                    button {
                        class: "px-4 py-1.5 text-sm bg-primary text-primary-foreground rounded-lg hover:opacity-90 transition disabled:opacity-50 disabled:cursor-not-allowed flex items-center gap-2",
                        disabled: *is_saving.read(),
                        onclick: handle_save,
                        if *is_saving.read() {
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
                            "Saving..."
                        } else {
                            "Save Changes"
                        }
                    }
                }
            }
            div { class: "p-4 max-w-2xl mx-auto space-y-6",
                if *save_success.read() {
                    div { class: "p-4 bg-green-500/10 border border-green-500/20 rounded-lg text-green-600 dark:text-green-400 text-sm flex items-center gap-2",
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
                            path { d: "M22 11.08V12a10 10 0 1 1-5.93-9.14" }
                            polyline { points: "22 4 12 14.01 9 11.01" }
                        }
                        "Settings saved successfully!"
                    }
                }
                if let Some(error) = save_error.read().as_ref() {
                    div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                        "{error}"
                    }
                }
                match &*repo_result.read() {
                    None => rsx! {
                        LoadingSkeleton {}
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "p-4 bg-destructive/10 border border-destructive/20 rounded-lg text-destructive text-sm",
                            "Failed to load repository: {e}"
                        }
                    },
                    Some(Ok(_)) => rsx! {
                        div { class: "space-y-4",
                            h2 { class: "font-semibold text-lg", "General" }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Repository ID" }
                                input {
                                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono text-muted-foreground cursor-not-allowed",
                                    r#type: "text",
                                    value: "{repo_id}",
                                    disabled: true,
                                }
                                p { class: "text-xs text-muted-foreground mt-1",
                                    "This is your repository's unique identifier and cannot be changed."
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Display Name" }
                                input {
                                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "My Project",
                                    value: "{repo_name}",
                                    oninput: move |e| repo_name.set(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Description" }
                                textarea {
                                    class: "w-full h-24 px-3 py-2 bg-muted rounded-lg text-sm focus:outline-hidden focus:ring-2 focus:ring-primary resize-y",
                                    placeholder: "A brief description of your project...",
                                    value: "{repo_description}",
                                    oninput: move |e| repo_description.set(e.value()),
                                }
                            }
                        }
                        div { class: "space-y-4 pt-6 border-t border-border",
                            h2 { class: "font-semibold text-lg", "URLs" }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Clone URL" }
                                input {
                                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "https://github.com/user/repo.git",
                                    value: "{clone_url}",
                                    oninput: move |e| clone_url.set(e.value()),
                                }
                            }
                            div {
                                label { class: "block text-sm font-medium mb-2", "Web URL" }
                                input {
                                    class: "w-full px-3 py-2 bg-muted rounded-lg text-sm font-mono focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "https://github.com/user/repo",
                                    value: "{web_url}",
                                    oninput: move |e| web_url.set(e.value()),
                                }
                            }
                        }
                        div { class: "space-y-4 pt-6 border-t border-border",
                            h2 { class: "font-semibold text-lg text-destructive", "Danger Zone" }
                            div { class: "p-4 border border-destructive/20 rounded-lg",
                                div { class: "flex items-center justify-between",
                                    div {
                                        h3 { class: "font-medium", "Delete Repository" }
                                        p { class: "text-sm text-muted-foreground",
                                            "Permanently delete this repository announcement. This cannot be undone."
                                        }
                                    }
                                    button {
                                        class: "px-4 py-2 bg-destructive text-destructive-foreground rounded-lg text-sm hover:opacity-90 transition",
                                        onclick: move |_| show_delete_confirm.set(true),
                                        "Delete"
                                    }
                                }
                            }
                        }
                    },
                }
            }
            if *show_delete_confirm.read() {
                DeleteConfirmModal {
                    repo_name: repo_name.read().clone(),
                    on_cancel: move |_| show_delete_confirm.set(false),
                    on_confirm: move |_| {
                        let _ = nav.push(Route::CodeRepositories {});
                    },
                }
            }
        }
    }
}
#[component]
fn DeleteConfirmModal(
    repo_name: String,
    on_cancel: EventHandler<MouseEvent>,
    on_confirm: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center bg-background/80 backdrop-blur-sm",
            onclick: move |e| on_cancel.call(e),
            div {
                class: "bg-background border border-border rounded-lg p-6 max-w-md w-full mx-4 shadow-xl",
                onclick: move |e| e.stop_propagation(),
                div { class: "w-12 h-12 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
                    svg {
                        class: "w-6 h-6 text-destructive",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M3 6h18" }
                        path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" }
                        path { d: "M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
                    }
                }
                h3 { class: "text-lg font-semibold text-center mb-2", "Remove Repository?" }
                p { class: "text-sm text-muted-foreground text-center mb-6",
                    "Remove \""
                    span { class: "font-medium text-foreground", "{repo_name}" }
                    "\" from your list? The repository will remain on the Nostr network."
                }
                div { class: "flex gap-3",
                    button {
                        class: "flex-1 py-2 border border-border rounded-lg font-medium hover:bg-muted transition",
                        onclick: move |e| on_cancel.call(e),
                        "Cancel"
                    }
                    button {
                        class: "flex-1 py-2 bg-destructive text-destructive-foreground rounded-lg font-medium hover:opacity-90 transition",
                        onclick: move |e| on_confirm.call(e),
                        "Remove"
                    }
                }
            }
        }
    }
}
#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-6 animate-pulse",
            div { class: "h-6 bg-muted rounded w-24" }
            div { class: "space-y-3" }
            div { class: "h-10 bg-muted rounded" }
            div { class: "h-24 bg-muted rounded" }
            div { class: "h-10 bg-muted rounded" }
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
                    "Connect with your Nostr identity to manage repository settings."
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
#[component]
fn NotOwnerState(naddr: String) -> Element {
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
                        rect {
                            x: "3",
                            y: "11",
                            width: "18",
                            height: "11",
                            rx: "2",
                            ry: "2",
                        }
                        path { d: "M7 11V7a5 5 0 0 1 10 0v4" }
                    }
                }
                h2 { class: "font-semibold text-xl mb-2", "Access Denied" }
                p { class: "text-muted-foreground mb-6",
                    "You don't have permission to manage this repository's settings."
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
