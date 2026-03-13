//! Clone Help Modal Component
//!
//! Shows tabbed clone instructions for SSH, HTTP(S), and Nostr protocols.
use crate::utils::clipboard::copy_to_clipboard;
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};

fn is_ssh_clone_url(url: &str) -> bool {
    if url.starts_with("ssh://") {
        return true;
    }
    if url.contains("://") || url.contains(' ') {
        return false;
    }
    let Some(colon_idx) = url.find(':') else {
        return false;
    };
    let Some(at_idx) = url.find('@') else {
        return false;
    };
    if at_idx == 0 || at_idx > colon_idx {
        return false;
    }
    let host_part = &url[..colon_idx];
    let path_part = &url[(colon_idx + 1)..];
    !path_part.is_empty() && !host_part.contains('/') && !host_part.contains(' ')
}

/// Clone help modal with tabbed URL display
#[component]
pub fn CloneHelpModal(
    clone_urls: Vec<String>,
    naddr: String,
    on_close: EventHandler<()>,
) -> Element {
    let initial_tab = if clone_urls
        .iter()
        .any(|u| u.starts_with("https://") || u.starts_with("http://"))
    {
        "https"
    } else if clone_urls.iter().any(|u| is_ssh_clone_url(u)) {
        "ssh"
    } else if clone_urls.iter().any(|u| u.starts_with("git://")) {
        "git"
    } else if clone_urls.iter().any(|u| u.starts_with("nostr:")) {
        "nostr"
    } else if let Some(first) = clone_urls.first() {
        if is_ssh_clone_url(first) {
            "ssh"
        } else if first.starts_with("git://") {
            "git"
        } else {
            "https"
        }
    } else {
        "nostr"
    };
    let mut active_tab = use_signal(move || initial_tab);

    // Categorize URLs
    let ssh_urls: Vec<&String> = clone_urls.iter().filter(|u| is_ssh_clone_url(u)).collect();
    let https_urls: Vec<&String> = clone_urls
        .iter()
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        .collect();
    let git_urls: Vec<&String> = clone_urls
        .iter()
        .filter(|u| u.starts_with("git://"))
        .collect();
    let nostr_url = format!("nostr:{}", naddr);

    rsx! {
        // Backdrop
        div {
            class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm",
            onclick: move |_| on_close.call(()),
        }
        // Modal
        div {
            class: "fixed inset-0 z-50 flex items-center justify-center p-4",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-background border border-border rounded-lg p-6 w-full max-w-lg shadow-lg",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "clone-help-title",
                onclick: move |evt| evt.stop_propagation(),
                // Header
                div { class: "flex justify-between items-center mb-4",
                    h2 { id: "clone-help-title", class: "text-lg font-bold", "Clone Repository" }
                    button {
                        class: "p-1 hover:bg-accent rounded transition text-muted-foreground hover:text-foreground",
                        aria_label: "Close",
                        r#type: "button",
                        onclick: move |_| on_close.call(()),
                        "X"
                    }
                }
                // Tabs
                div {
                    class: "flex gap-1 mb-4 border-b border-border",
                    role: "tablist",
                    onkeydown: {
                        let has_git = !git_urls.is_empty();
                        move |e: KeyboardEvent| {
                            let tabs: Vec<&str> = if has_git {
                                vec!["https", "ssh", "git", "nostr"]
                            } else {
                                vec!["https", "ssh", "nostr"]
                            };
                            let current = *active_tab.read();
                            let idx = tabs.iter().position(|t| *t == current).unwrap_or(0);
                            let new_idx = match e.key() {
                                Key::ArrowRight => Some((idx + 1) % tabs.len()),
                                Key::ArrowLeft => Some(if idx == 0 { tabs.len() - 1 } else { idx - 1 }),
                                Key::Home => Some(0),
                                Key::End => Some(tabs.len() - 1),
                                _ => None,
                            };
                            if let Some(i) = new_idx {
                                e.prevent_default();
                                active_tab.set(tabs[i]);
                            }
                        }
                    },
                    TabButton { label: "HTTP(S)", tab_id: "https", active_tab: active_tab }
                    TabButton { label: "SSH", tab_id: "ssh", active_tab: active_tab }
                    if !git_urls.is_empty() {
                        TabButton { label: "Git", tab_id: "git", active_tab: active_tab }
                    }
                    TabButton { label: "Nostr", tab_id: "nostr", active_tab: active_tab }
                }
                // Tab content
                div {
                    class: "space-y-3",
                    role: "tabpanel",
                    id: "clone-panel-{active_tab}",
                    aria_labelledby: "clone-tab-{active_tab}",
                    match *active_tab.read() {
                        "ssh" => rsx! {
                            if ssh_urls.is_empty() {
                                p { class: "text-sm text-muted-foreground py-4 text-center", "No SSH clone URLs available." }
                            } else {
                                for url in ssh_urls.iter() {
                                    CloneUrlRow { key: "{url}", url: url.to_string() }
                                }
                            }
                            p { class: "text-xs text-muted-foreground mt-2",
                                "Make sure you have an SSH key configured."
                            }
                        },
                        "git" => rsx! {
                            for url in git_urls.iter() {
                                CloneUrlRow { key: "{url}", url: url.to_string() }
                            }
                            p { class: "text-xs text-muted-foreground mt-2",
                                "Git protocol is read-only and unencrypted."
                            }
                        },
                        "nostr" => rsx! {
                            CloneUrlRow { url: nostr_url.clone() }
                            p { class: "text-xs text-muted-foreground mt-2",
                                "Use a Nostr-aware Git client like ngit to clone via the Nostr network."
                            }
                        },
                        _ => rsx! {
                            if https_urls.is_empty() {
                                p { class: "text-sm text-muted-foreground py-4 text-center", "No HTTP(S) clone URLs available." }
                            } else {
                                for url in https_urls.iter() {
                                    CloneUrlRow { key: "{url}", url: url.to_string() }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn TabButton(
    label: &'static str,
    tab_id: &'static str,
    active_tab: Signal<&'static str>,
) -> Element {
    let is_active = *active_tab.read() == tab_id;
    let class = if is_active {
        "px-3 py-2 text-sm font-medium text-primary border-b-2 border-primary -mb-px"
    } else {
        "px-3 py-2 text-sm text-muted-foreground hover:text-foreground transition -mb-px"
    };
    rsx! {
        button {
            class: class,
            r#type: "button",
            role: "tab",
            id: "clone-tab-{tab_id}",
            aria_controls: "clone-panel-{tab_id}",
            aria_selected: "{is_active}",
            tabindex: if is_active { "0" } else { "-1" },
            onclick: move |_| active_tab.set(tab_id),
            "{label}"
        }
    }
}

#[component]
fn CloneUrlRow(url: String) -> Element {
    let toast = consume_toast();
    let url_for_copy = url.clone();
    rsx! {
        div { class: "flex items-center gap-2",
            code { class: "flex-1 text-sm font-mono bg-muted px-3 py-2 rounded-lg overflow-x-auto",
                "{url}"
            }
            button {
                class: "shrink-0 p-2 hover:bg-accent rounded-lg transition text-muted-foreground hover:text-foreground",
                title: "Copy to clipboard",
                aria_label: "Copy clone URL",
                r#type: "button",
                onclick: move |_| {
                    let url_clone = url_for_copy.clone();
                    spawn(async move {
                        if copy_to_clipboard(&url_clone).await.is_ok() {
                            toast.success("Copied to clipboard".to_string(), ToastOptions::new());
                        } else {
                            toast.error("Failed to copy".to_string(), ToastOptions::new());
                        }
                    });
                },
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
                    rect { x: "9", y: "9", width: "13", height: "13", rx: "2" }
                    path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                }
            }
        }
    }
}
