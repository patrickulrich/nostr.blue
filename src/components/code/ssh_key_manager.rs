//! SSH Key Manager Component
//!
//! Displays and manages SSH public keys stored as Kind 52 events.
//! Provides UI for listing, adding, and deleting SSH keys.
use crate::services::git_hosting::ssh_keys::{self, SshKey};
use crate::stores::auth_store;
use crate::utils::format_time_ago;
use dioxus::prelude::*;
use nostr_sdk::prelude::*;

/// SSH key management panel for user settings
#[component]
pub fn SshKeyManager() -> Element {
    let mut keys = use_signal(Vec::<SshKey>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut show_add_form = use_signal(|| false);
    let mut new_title = use_signal(String::new);
    let mut new_key = use_signal(String::new);
    let mut adding = use_signal(|| false);
    let mut deleting_id = use_signal(|| None::<String>);
    let mut confirm_delete = use_signal(|| None::<String>);

    // Fetch keys on mount
    use_effect(move || {
        if let Some(pubkey_hex) = auth_store::get_pubkey() {
            spawn(async move {
                match PublicKey::from_hex(&pubkey_hex) {
                    Ok(pk) => match ssh_keys::fetch_ssh_keys(&pk).await {
                        Ok(k) => {
                            keys.set(k);
                            loading.set(false);
                        }
                        Err(e) => {
                            error.set(Some(e));
                            loading.set(false);
                        }
                    },
                    Err(e) => {
                        error.set(Some(format!("Invalid public key: {}", e)));
                        loading.set(false);
                    }
                }
            });
        } else {
            loading.set(false);
        }
    });

    let handle_add = move |_| {
        if *adding.peek() {
            return;
        }
        let title = new_title.read().clone();
        let key = new_key.read().clone();
        if title.trim().is_empty() || key.trim().is_empty() {
            error.set(Some("Title and SSH key are required".to_string()));
            return;
        }
        // Basic SSH key format validation
        let trimmed = key.trim().to_string();
        if !trimmed.starts_with("ssh-rsa")
            && !trimmed.starts_with("ssh-ed25519")
            && !trimmed.starts_with("ecdsa-sha2")
            && !trimmed.starts_with("ssh-dss")
            && !trimmed.starts_with("sk-ssh-ed25519")
            && !trimmed.starts_with("sk-ecdsa-sha2")
        {
            error.set(Some(
                "Key must start with ssh-rsa, ssh-ed25519, ecdsa-sha2, or similar".to_string(),
            ));
            return;
        }
        adding.set(true);
        error.set(None);
        spawn(async move {
            match ssh_keys::publish_ssh_key(title.trim(), &trimmed).await {
                Ok(_) => {
                    // Refresh the key list
                    if let Some(pubkey_hex) = auth_store::get_pubkey() {
                        if let Ok(pk) = PublicKey::from_hex(&pubkey_hex) {
                            if let Ok(k) = ssh_keys::fetch_ssh_keys(&pk).await {
                                keys.set(k);
                            }
                        }
                    }
                    new_title.set(String::new());
                    new_key.set(String::new());
                    show_add_form.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            adding.set(false);
        });
    };

    let handle_delete = move |eid_hex: String| {
        spawn(async move {
            deleting_id.set(Some(eid_hex.clone()));
            error.set(None);
            match EventId::from_hex(&eid_hex) {
                Ok(eid) => match ssh_keys::delete_ssh_key(eid).await {
                    Ok(()) => {
                        keys.write().retain(|k| k.event_id != eid_hex);
                        confirm_delete.set(None);
                    }
                    Err(e) => {
                        error.set(Some(e));
                    }
                },
                Err(e) => {
                    error.set(Some(format!("Invalid event ID: {}", e)));
                }
            }
            deleting_id.set(None);
        });
    };

    rsx! {
        div { class: "space-y-4",
            // Header
            div { class: "flex items-center justify-between",
                div { class: "flex items-center gap-2",
                    svg {
                        class: "w-5 h-5 text-muted-foreground",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4" }
                    }
                    h2 { class: "text-lg font-semibold text-foreground", "SSH Keys" }
                }
                if !*show_add_form.read() {
                    button {
                        class: "px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                        onclick: move |_| show_add_form.set(true),
                        "New SSH Key"
                    }
                }
            }
            p { class: "text-sm text-muted-foreground",
                "SSH keys are used for git operations. Add your public key to authenticate with git repositories."
            }
            // Error display
            if let Some(err) = error.read().as_ref() {
                div { class: "p-3 bg-destructive/10 border border-destructive/20 rounded-lg text-sm text-destructive",
                    "{err}"
                }
            }
            // Add key form
            if *show_add_form.read() {
                div { class: "bg-card border border-border rounded-lg p-4 space-y-3",
                    h3 { class: "text-sm font-semibold text-foreground", "Add new SSH key" }
                    div { class: "space-y-1",
                        label { class: "text-sm text-muted-foreground", r#for: "ssh-key-title", "Title" }
                        input {
                            id: "ssh-key-title",
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring",
                            r#type: "text",
                            placeholder: "e.g. My Laptop",
                            value: "{new_title}",
                            oninput: move |e| new_title.set(e.value()),
                        }
                    }
                    div { class: "space-y-1",
                        label { class: "text-sm text-muted-foreground", r#for: "ssh-key-value", "Key" }
                        textarea {
                            id: "ssh-key-value",
                            class: "w-full px-3 py-2 bg-background border border-border rounded-lg text-sm text-foreground placeholder:text-muted-foreground font-mono resize-y focus:outline-none focus:ring-2 focus:ring-ring",
                            rows: "4",
                            placeholder: "ssh-ed25519 AAAA...",
                            value: "{new_key}",
                            oninput: move |e| new_key.set(e.value()),
                        }
                        p { class: "text-xs text-muted-foreground",
                            "Paste your public key. Begins with ssh-rsa, ssh-ed25519, ecdsa-sha2, etc."
                        }
                    }
                    div { class: "flex items-center gap-2 justify-end",
                        button {
                            class: "px-4 py-2 text-sm border border-border rounded-lg hover:bg-accent transition",
                            disabled: *adding.read(),
                            onclick: move |_| {
                                show_add_form.set(false);
                                new_title.set(String::new());
                                new_key.set(String::new());
                                error.set(None);
                            },
                            "Cancel"
                        }
                        button {
                            class: "px-4 py-2 text-sm font-medium bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition disabled:opacity-50",
                            disabled: *adding.read(),
                            onclick: handle_add,
                            if *adding.read() { "Adding..." } else { "Add SSH Key" }
                        }
                    }
                }
            }
            // Key list
            if *loading.read() {
                div { class: "space-y-3",
                    for _ in 0..3 {
                        div { class: "bg-card border border-border rounded-lg p-4 animate-pulse",
                            div { class: "h-4 w-1/3 bg-muted rounded mb-2" }
                            div { class: "h-3 w-2/3 bg-muted rounded" }
                        }
                    }
                }
            } else if keys.read().is_empty() {
                div { class: "bg-card border border-border rounded-lg p-8 text-center",
                    svg {
                        class: "w-10 h-10 text-muted-foreground mx-auto mb-3",
                        xmlns: "http://www.w3.org/2000/svg",
                        width: "24",
                        height: "24",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4" }
                    }
                    p { class: "text-sm text-muted-foreground",
                        "No SSH keys found. Add one to get started with git operations."
                    }
                }
            } else {
                div { class: "space-y-2",
                    for key in keys.read().iter() {
                        {
                            let eid = key.event_id.clone();
                            let eid_for_confirm = key.event_id.clone();
                            let eid_for_delete = key.event_id.clone();
                            let is_confirming = confirm_delete.read().as_ref() == Some(&eid);
                            let is_deleting = deleting_id.read().as_ref() == Some(&eid);
                            let time_ago = format_time_ago(key.created_at);
                            let key_preview = truncate_key(&key.public_key);
                            rsx! {
                                div {
                                    key: "{eid}",
                                    class: "bg-card border border-border rounded-lg p-4",
                                    div { class: "flex items-start justify-between gap-3",
                                        div { class: "flex-1 min-w-0",
                                            div { class: "flex items-center gap-2",
                                                svg {
                                                    class: "w-4 h-4 text-muted-foreground shrink-0",
                                                    xmlns: "http://www.w3.org/2000/svg",
                                                    width: "24",
                                                    height: "24",
                                                    view_box: "0 0 24 24",
                                                    fill: "none",
                                                    stroke: "currentColor",
                                                    stroke_width: "2",
                                                    stroke_linecap: "round",
                                                    stroke_linejoin: "round",
                                                    path { d: "M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4" }
                                                }
                                                h3 { class: "font-medium text-foreground truncate", "{key.title}" }
                                            }
                                            p { class: "text-xs text-muted-foreground font-mono mt-1 truncate",
                                                "{key_preview}"
                                            }
                                            if !key.fingerprint.is_empty() {
                                                p { class: "text-xs text-muted-foreground mt-1",
                                                    "Fingerprint: {key.fingerprint}"
                                                }
                                            }
                                            p { class: "text-xs text-muted-foreground mt-1",
                                                "Added {time_ago}"
                                            }
                                        }
                                        div { class: "shrink-0",
                                            if is_confirming {
                                                div { class: "flex items-center gap-2",
                                                    span { class: "text-xs text-muted-foreground", "Delete?" }
                                                    button {
                                                        class: "px-3 py-1.5 text-xs font-medium bg-destructive text-destructive-foreground rounded-lg hover:bg-destructive/90 transition disabled:opacity-50",
                                                        disabled: is_deleting,
                                                        onclick: move |_| {
                                                            let eid = eid_for_delete.clone();
                                                            handle_delete(eid);
                                                        },
                                                        if is_deleting { "..." } else { "Yes" }
                                                    }
                                                    button {
                                                        class: "px-3 py-1.5 text-xs border border-border rounded-lg hover:bg-accent transition",
                                                        disabled: is_deleting,
                                                        onclick: move |_| confirm_delete.set(None),
                                                        "No"
                                                    }
                                                }
                                            } else {
                                                button {
                                                    class: "p-1 hover:bg-accent rounded transition text-muted-foreground hover:text-destructive",
                                                    title: "Delete key",
                                                    onclick: move |_| confirm_delete.set(Some(eid_for_confirm.clone())),
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
                                                        polyline { points: "3 6 5 6 21 6" }
                                                        path { d: "M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }
                                                        line { x1: "10", y1: "11", x2: "10", y2: "17" }
                                                        line { x1: "14", y1: "11", x2: "14", y2: "17" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Truncate an SSH public key for display, showing type + first/last few chars
#[allow(dead_code)]
fn truncate_key(key: &str) -> String {
    let parts: Vec<&str> = key.split_whitespace().collect();
    if parts.len() >= 2 {
        let key_type = parts[0];
        let key_data = parts[1];
        if key_data.len() > 20 {
            format!(
                "{} {}...{}",
                key_type,
                &key_data[..10],
                &key_data[key_data.len() - 10..]
            )
        } else {
            key.to_string()
        }
    } else if key.len() > 40 {
        format!("{}...{}", &key[..20], &key[key.len() - 10..])
    } else {
        key.to_string()
    }
}
