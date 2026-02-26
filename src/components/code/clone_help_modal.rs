//! Clone Help Modal Component
//!
//! Shows tabbed clone instructions for SSH, HTTPS, and Nostr protocols.
use dioxus::prelude::*;
use crate::utils::clipboard::copy_to_clipboard;
use dioxus_primitives::toast::{consume_toast, ToastOptions};

/// Clone help modal with tabbed URL display
#[component]
pub fn CloneHelpModal(
    clone_urls: Vec<String>,
    naddr: String,
    on_close: EventHandler<()>,
) -> Element {
    let initial_tab = if clone_urls.iter().any(|u| u.starts_with("https://") || u.starts_with("http://")) {
        "https"
    } else if clone_urls.iter().any(|u| u.starts_with("ssh://") || u.starts_with("git@")) {
        "ssh"
    } else if clone_urls.iter().any(|u| u.starts_with("git://")) {
        "git"
    } else {
        "https"
    };
    let active_tab = use_signal(move || initial_tab);

    // Categorize URLs
    let ssh_urls: Vec<&String> = clone_urls.iter().filter(|u| {
        u.starts_with("ssh://")
            || (u.contains('@') && u.contains(':') && !u.contains("://"))
    }).collect();
    let https_urls: Vec<&String> = clone_urls.iter().filter(|u| u.starts_with("https://") || u.starts_with("http://")).collect();
    let git_urls: Vec<&String> = clone_urls.iter().filter(|u| u.starts_with("git://")).collect();
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
                div { class: "flex gap-1 mb-4 border-b border-border",
                    TabButton { label: "HTTPS", tab_id: "https", active_tab: active_tab }
                    TabButton { label: "SSH", tab_id: "ssh", active_tab: active_tab }
                    if !git_urls.is_empty() {
                        TabButton { label: "Git", tab_id: "git", active_tab: active_tab }
                    }
                    TabButton { label: "Nostr", tab_id: "nostr", active_tab: active_tab }
                }
                // Tab content
                div { class: "space-y-3",
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
                                p { class: "text-sm text-muted-foreground py-4 text-center", "No HTTPS clone URLs available." }
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
fn TabButton(label: &'static str, tab_id: &'static str, active_tab: Signal<&'static str>) -> Element {
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
