//! Repository Action Bar Component
//!
//! Displays action buttons for a repository: Watch, Star, Fork, Zap, Share.
//! Desktop: horizontal button row. Mobile: dropdown menu.
//! Styled to match gittr's layout-client.tsx action bar pattern.
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use crate::components::icons;
use crate::components::ZapModal;
use crate::services::git_hosting::repository::publish_fork;
use crate::services::git_hosting::stars::{check_user_star, publish_star, remove_star};
use crate::services::git_hosting::watches;
use crate::stores::code_store::is_repo_starred;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::clipboard::copy_to_clipboard;
use crate::utils::nip34::Repository;
use crate::utils::truncate_pubkey;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
/// Repository action bar with Watch, Star, Fork, Zap, Share buttons
#[allow(clippy::clone_on_copy)]
#[component]
pub fn RepoActionBar(repo: Repository, naddr: String) -> Element {
    let toast = consume_toast();
    let _nav = use_navigator();
    let mut show_zap_modal = use_signal(|| false);
    let mut is_starred = use_signal(|| false);
    let mut star_count = use_signal(|| repo.star_count);
    let mut is_watching = use_signal(|| false);
    let mut star_loading = use_signal(|| false);
    let mut show_actions_menu = use_signal(|| false);
    let mut show_qr_modal = use_signal(|| false);
    let mut repo_pubkey_signal = use_signal(|| repo.pubkey.clone());
    let mut repo_id_signal = use_signal(|| repo.id.clone());
    {
        let current_pubkey = repo.pubkey.clone();
        let current_id = repo.id.clone();
        if *repo_pubkey_signal.read() != current_pubkey {
            repo_pubkey_signal.set(current_pubkey);
        }
        if *repo_id_signal.read() != current_id {
            repo_id_signal.set(current_id);
        }
        if *star_count.peek() != repo.star_count {
            star_count.set(repo.star_count);
        }
    }
    let coordinate = use_memo(move || {
        let pubkey_str = repo_pubkey_signal.read();
        let id_str = repo_id_signal.read();
        if let Ok(pk) = PublicKey::from_hex(&pubkey_str) {
            Some(Coordinate::new(Kind::GitRepoAnnouncement, pk).identifier(&*id_str))
        } else {
            None
        }
    });
    use_effect(move || {
        is_starred.set(false);
        let coord = coordinate.read().clone();
        if let Some(coord) = coord {
            let coord_str = format!(
                "{}:{}:{}",
                coord.kind.as_u16(),
                coord.public_key.to_hex(),
                coord.identifier,
            );
            if is_repo_starred(&coord_str) {
                is_starred.set(true);
            } else {
                spawn(async move {
                    match check_user_star(&coord).await {
                        Ok(starred) => is_starred.set(starred),
                        Err(e) => log::debug!("Failed to check star status: {}", e),
                    }
                });
            }
        }
    });
    use_effect(move || {
        let coord = coordinate.read().clone();
        if let Some(coord) = coord {
            let coord_str = format!(
                "{}:{}:{}",
                coord.kind.as_u16(),
                coord.public_key.to_hex(),
                coord.identifier,
            );
            spawn(async move {
                match watches::fetch_watched_repos().await {
                    Ok(watched) => is_watching.set(watched.contains(&coord_str)),
                    Err(e) => {
                        log::debug!("Failed to check watch status: {}", e);
                        is_watching.set(false);
                    }
                }
            });
        } else {
            is_watching.set(false);
        }
    });
    let handle_star = {
        move |_| {
            if *star_loading.read() {
                return;
            }
            star_loading.set(true);
            if !*HAS_SIGNER.read() {
                star_loading.set(false);
                toast
                    .warning(
                        "Sign in to star repositories".to_string(),
                        ToastOptions::new(),
                    );
                return;
            }
            let coord = match coordinate.read().clone() {
                Some(c) => c,
                None => {
                    star_loading.set(false);
                    return;
                }
            };
            spawn(async move {
                let currently_starred = *is_starred.read();
                let result = if currently_starred {
                    remove_star(&coord).await
                } else {
                    publish_star(&coord).await.map(|_| ())
                };
                match result {
                    Ok(_) => {
                        is_starred.set(!currently_starred);
                        let current = *star_count.read();
                        if currently_starred {
                            star_count.set(current.saturating_sub(1));
                        } else {
                            star_count.set(current.saturating_add(1));
                        }
                    }
                    Err(e) => {
                        log::error!("Star action failed: {}", e);
                        toast
                            .error(
                                format!("Failed to update star: {}", e),
                                ToastOptions::new(),
                            );
                    }
                }
                star_loading.set(false);
            });
        }
    };
    let handle_watch = {
        move |_| {
            if !*HAS_SIGNER.read() {
                toast.warning("Sign in to watch repositories".to_string(), ToastOptions::new());
                return;
            }
            let coord = match coordinate.read().clone() {
                Some(c) => c,
                None => return,
            };
            let currently_watching = *is_watching.read();
            // Optimistic update
            is_watching.set(!currently_watching);
            spawn(async move {
                match watches::toggle_watch_repo(&coord, currently_watching).await {
                    Ok(_) => {
                        if !currently_watching {
                            toast.success("Watching repository".to_string(), ToastOptions::new());
                        }
                    }
                    Err(e) => {
                        // Revert on failure
                        is_watching.set(currently_watching);
                        log::error!("Watch toggle failed: {}", e);
                        toast.error(format!("Failed to update watch: {}", e), ToastOptions::new());
                    }
                }
            });
        }
    };
    let handle_share = {
        let naddr = naddr.clone();
        move |_| {
            let naddr = naddr.clone();
            spawn(async move {
                let share_text = format!("nostr:{}", naddr);
                if copy_to_clipboard(&share_text).await.is_ok() {
                    toast
                        .success("Copied to clipboard".to_string(), ToastOptions::new());
                } else {
                    toast.error("Failed to copy".to_string(), ToastOptions::new());
                }
            });
        }
    };
    let mut fork_loading = use_signal(|| false);
    let mut show_fork_modal = use_signal(|| false);
    let fork_event_id = use_signal(|| repo.event_id.clone());
    let fork_repo_id = use_signal(|| repo.id.clone());
    let fork_repo_name = use_signal(|| repo.name.clone());
    let fork_repo_desc = use_signal(|| repo.description.clone());
    let fork_clone_urls = use_signal(|| repo.clone.clone());
    // Pre-filled form state for the fork modal
    let mut fork_form_name = use_signal(|| {
        repo.name.as_deref()
            .map(|n| format!("{} (fork)", n))
            .unwrap_or_else(|| format!("{}-fork", repo.id))
    });
    let mut fork_form_desc = use_signal(|| repo.description.clone().unwrap_or_default());
    let mut fork_form_clone_urls = use_signal(|| {
        repo.clone.iter().map(|s| s.to_string()).collect::<Vec<String>>().join("\n")
    });
    let handle_fork = move |_| {
        if !*HAS_SIGNER.read() {
            toast.warning("Sign in to fork repositories".to_string(), ToastOptions::new());
            return;
        }
        // Pre-fill form fields from parent repo when opening modal
        let name = fork_repo_name.read().clone();
        let id = fork_repo_id.read().clone();
        let desc = fork_repo_desc.read().clone();
        let urls = fork_clone_urls.read().clone();
        fork_form_name.set(
            name.as_deref()
                .map(|n| format!("{} (fork)", n))
                .unwrap_or_else(|| format!("{}-fork", id))
        );
        fork_form_desc.set(desc.unwrap_or_default());
        fork_form_clone_urls.set(urls.join("\n"));
        show_fork_modal.set(true);
    };
    let handle_fork_submit = move |_| {
        if *fork_loading.read() {
            return;
        }
        fork_loading.set(true);
        let event_id = fork_event_id.read().clone();
        let id = fork_repo_id.read().clone();
        let form_name = fork_form_name.read().clone();
        let form_desc = fork_form_desc.read().clone();
        let form_urls_raw = fork_form_clone_urls.read().clone();
        spawn(async move {
            let fork_id = format!("{}-fork", id);
            let fork_name = if form_name.trim().is_empty() { None } else { Some(form_name.trim().to_string()) };
            let fork_desc = if form_desc.trim().is_empty() { None } else { Some(form_desc.trim().to_string()) };
            let urls: Vec<String> = form_urls_raw
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            let url_refs: Vec<&str> = urls.iter().map(|s| s.as_str()).collect();
            match publish_fork(
                &event_id,
                &fork_id,
                fork_name.as_deref(),
                fork_desc.as_deref(),
                &url_refs,
            ).await {
                Ok(_) => {
                    toast.success("Repository forked!".to_string(), ToastOptions::new());
                    show_fork_modal.set(false);
                }
                Err(e) => {
                    toast.error(format!("Fork failed: {}", e), ToastOptions::new());
                }
            }
            fork_loading.set(false);
        });
    };
    let handle_zap = move |_| {
        if !*HAS_SIGNER.read() {
            toast
                .warning("Sign in to zap repositories".to_string(), ToastOptions::new());
            return;
        }
        show_zap_modal.set(true);
    };
    let star_text = if *is_starred.read() { "Starred" } else { "Star" };
    let watch_text = if *is_watching.read() { "Unwatch" } else { "Watch" };
    rsx! {
        div { class: "flex items-center gap-2",
            div { class: "hidden md:flex items-center gap-2",
                ActionButton {
                    icon: icons::EYE,
                    label: "{watch_text}",
                    count: None,
                    active: *is_watching.read(),
                    disabled: false,
                    loading: false,
                    onclick: handle_watch.clone(),
                }
                ActionButton {
                    icon: if *is_starred.read() { icons::STAR_FILLED } else { icons::STAR },
                    label: "{star_text}",
                    count: Some(*star_count.read()),
                    active: *is_starred.read(),
                    disabled: !*HAS_SIGNER.read(),
                    loading: *star_loading.read(),
                    onclick: handle_star.clone(),
                }
                ActionButton {
                    icon: icons::GIT_FORK,
                    label: "Fork",
                    count: None,
                    active: false,
                    disabled: !*HAS_SIGNER.read(),
                    loading: *fork_loading.read(),
                    onclick: handle_fork,
                }
                ActionButton {
                    icon: icons::ZAP,
                    label: "Zap",
                    count: None,
                    active: false,
                    disabled: !*HAS_SIGNER.read(),
                    loading: false,
                    onclick: handle_zap,
                }
                ActionButton {
                    icon: icons::SHARE,
                    label: "Share",
                    count: None,
                    active: false,
                    disabled: false,
                    loading: false,
                    onclick: handle_share.clone(),
                }
                ActionButton {
                    icon: icons::QR_CODE,
                    label: "QR",
                    count: None,
                    active: false,
                    disabled: false,
                    loading: false,
                    onclick: move |_| show_qr_modal.set(true),
                }
            }
            div { class: "md:hidden relative",
                button {
                    class: "flex items-center gap-2 px-3 py-1.5 text-sm border border-border rounded-lg bg-muted hover:bg-accent transition",
                    onclick: move |_| {
                        let current = *show_actions_menu.read();
                        show_actions_menu.set(!current);
                    },
                    "Actions"
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
                        polyline { points: "6 9 12 15 18 9" }
                    }
                }
                if *show_actions_menu.read() {
                    div {
                        class: "absolute right-0 top-full mt-1 w-48 bg-background border border-border rounded-lg shadow-lg z-50",
                        onclick: move |_| show_actions_menu.set(false),
                        MobileMenuItem {
                            icon: icons::EYE,
                            label: "{watch_text}",
                            onclick: handle_watch,
                        }
                        MobileMenuItem {
                            icon: if *is_starred.read() { icons::STAR_FILLED } else { icons::STAR },
                            label: "{star_text} ({star_count})",
                            loading: *star_loading.read(),
                            disabled: !*HAS_SIGNER.read(),
                            onclick: handle_star,
                        }
                        MobileMenuItem {
                            icon: icons::GIT_FORK,
                            label: "Fork",
                            onclick: handle_fork,
                        }
                        MobileMenuItem {
                            icon: icons::ZAP,
                            label: "Zap",
                            disabled: !*HAS_SIGNER.read(),
                            onclick: handle_zap,
                        }
                        MobileMenuItem {
                            icon: icons::SHARE,
                            label: "Share",
                            onclick: handle_share,
                        }
                        MobileMenuItem {
                            icon: icons::QR_CODE,
                            label: "QR Code",
                            onclick: move |_| show_qr_modal.set(true),
                        }
                    }
                }
            }
            if *show_zap_modal.read() {
                {
                    let profile = PROFILE_CACHE.read().peek(&repo.pubkey).cloned();
                    let recipient_name = profile
                        .as_ref()
                        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
                        .unwrap_or_else(|| truncate_pubkey(&repo.pubkey));
                    let lud16 = profile.as_ref().and_then(|p| p.lud16.clone());
                    rsx! {
                        ZapModal {
                            recipient_pubkey: repo.pubkey.clone(),
                            recipient_name: recipient_name,
                            lud16: lud16,
                            lud06: None::<String>,
                            event_id: Some(repo.event_id.clone()),
                            on_close: move |_| show_zap_modal.set(false),
                        }
                    }
                }
            }
            if *show_qr_modal.read() {
                {
                    let naddr_for_qr = naddr.clone();
                    let repo_url = format!("https://nostr.blue/code/repo/{}", naddr_for_qr);
                    let encoded_url = urlencoding::encode(&repo_url);
                    let qr_img_url = format!("https://api.qrserver.com/v1/create-qr-code/?size=200x200&data={}", encoded_url);
                    rsx! {
                        div {
                            class: "fixed inset-0 z-40 bg-black/50 backdrop-blur-sm",
                            onclick: move |_| show_qr_modal.set(false),
                        }
                        div {
                            class: "fixed inset-0 z-50 flex items-center justify-center p-4",
                            onclick: move |_| show_qr_modal.set(false),
                            div {
                                class: "bg-background border border-border rounded-lg p-6 w-full max-w-sm shadow-lg",
                                onclick: move |evt| evt.stop_propagation(),
                                div { class: "flex justify-between items-center mb-4",
                                    h2 { class: "text-lg font-bold", "QR Code" }
                                    button {
                                        class: "text-muted-foreground hover:text-foreground",
                                        onclick: move |_| show_qr_modal.set(false),
                                        "✕"
                                    }
                                }
                                div { class: "flex flex-col items-center gap-4",
                                    img {
                                        src: "{qr_img_url}",
                                        alt: "Repository QR Code",
                                        class: "w-[200px] h-[200px] rounded bg-white p-2",
                                    }
                                    p { class: "text-xs text-muted-foreground text-center break-all font-mono",
                                        "{naddr_for_qr}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if *show_fork_modal.read() {
                div {
                    class: "fixed inset-0 z-40 bg-black/50 backdrop-blur-sm",
                    onclick: move |_| {
                        if !*fork_loading.peek() {
                            show_fork_modal.set(false);
                        }
                    },
                }
                div {
                    class: "fixed inset-0 z-50 flex items-center justify-center p-4",
                    onclick: move |_| {
                        if !*fork_loading.peek() {
                            show_fork_modal.set(false);
                        }
                    },
                    div {
                        class: "bg-background border border-border rounded-lg p-6 w-full max-w-md shadow-lg max-h-[90vh] overflow-y-auto",
                        onclick: move |evt| evt.stop_propagation(),
                        // Header
                        div { class: "flex justify-between items-center mb-6",
                            h2 { class: "text-xl font-bold", "Fork Repository" }
                            button {
                                class: "text-muted-foreground hover:text-foreground disabled:opacity-50",
                                disabled: *fork_loading.read(),
                                onclick: move |_| {
                                    if !*fork_loading.peek() {
                                        show_fork_modal.set(false);
                                    }
                                },
                                "✕"
                            }
                        }
                        // Form
                        div { class: "space-y-4",
                            // Fork name
                            div {
                                label { class: "block text-sm font-medium mb-2", "Fork Name" }
                                input {
                                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    r#type: "text",
                                    placeholder: "my-project-fork",
                                    maxlength: "200",
                                    value: "{fork_form_name}",
                                    oninput: move |e| fork_form_name.set(e.value().clone()),
                                }
                            }
                            // Description
                            div {
                                label { class: "block text-sm font-medium mb-2", "Description (optional)" }
                                textarea {
                                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                                    rows: "3",
                                    placeholder: "A brief description of this fork",
                                    maxlength: "500",
                                    value: "{fork_form_desc}",
                                    oninput: move |e| fork_form_desc.set(e.value().clone()),
                                }
                            }
                            // Clone URLs
                            div {
                                label { class: "block text-sm font-medium mb-2", "Clone URLs (one per line)" }
                                textarea {
                                    class: "w-full px-3 py-2 bg-background border border-border rounded-lg focus:outline-hidden focus:ring-2 focus:ring-primary resize-none font-mono text-sm",
                                    rows: "3",
                                    placeholder: "https://example.com/repo.git",
                                    value: "{fork_form_clone_urls}",
                                    oninput: move |e| fork_form_clone_urls.set(e.value().clone()),
                                }
                                p { class: "text-xs text-muted-foreground mt-1",
                                    "Enter the clone URLs for your fork, one per line."
                                }
                            }
                            // Actions
                            div { class: "flex gap-3 justify-end pt-2",
                                button {
                                    class: "px-4 py-2 text-muted-foreground hover:text-foreground",
                                    disabled: *fork_loading.read(),
                                    onclick: move |_| {
                                        if !*fork_loading.peek() {
                                            show_fork_modal.set(false);
                                        }
                                    },
                                    "Cancel"
                                }
                                button {
                                    class: "px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 disabled:opacity-50 flex items-center gap-2",
                                    disabled: *fork_loading.read(),
                                    onclick: handle_fork_submit,
                                    if *fork_loading.read() {
                                        svg {
                                            class: "w-4 h-4 animate-spin",
                                            xmlns: "http://www.w3.org/2000/svg",
                                            width: "24",
                                            height: "24",
                                            view_box: "0 0 24 24",
                                            fill: "none",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            circle {
                                                cx: "12",
                                                cy: "12",
                                                r: "10",
                                                stroke_opacity: "0.25",
                                            }
                                            path { d: "M12 2a10 10 0 0 1 10 10", stroke_opacity: "1" }
                                        }
                                        "Forking..."
                                    } else {
                                        span { class: "w-4 h-4", dangerous_inner_html: "{icons::GIT_FORK}" }
                                        "Fork Repository"
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
/// Desktop action button with icon, label, and optional count
#[component]
fn ActionButton(
    icon: &'static str,
    label: String,
    count: Option<u32>,
    active: bool,
    disabled: bool,
    loading: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let base_class = if active {
        "flex items-center gap-1.5 px-3 py-1.5 text-sm border border-primary bg-primary/10 text-primary rounded-lg transition"
    } else if disabled {
        "flex items-center gap-1.5 px-3 py-1.5 text-sm border border-border bg-muted text-muted-foreground rounded-lg cursor-not-allowed opacity-50"
    } else {
        "flex items-center gap-1.5 px-3 py-1.5 text-sm border border-border bg-muted hover:bg-accent text-foreground rounded-lg transition cursor-pointer"
    };
    rsx! {
        button {
            class: "{base_class}",
            disabled: disabled || loading,
            onclick: move |e| {
                if !disabled && !loading {
                    onclick.call(e);
                }
            },
            if loading {
                svg {
                    class: "w-4 h-4 animate-spin",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    circle {
                        cx: "12",
                        cy: "12",
                        r: "10",
                        stroke_opacity: "0.25",
                    }
                    path { d: "M12 2a10 10 0 0 1 10 10", stroke_opacity: "1" }
                }
            } else {
                span { class: "w-4 h-4", dangerous_inner_html: "{icon}" }
            }
            span { "{label}" }
            if let Some(c) = count {
                span { class: "ml-1 px-1.5 py-0.5 text-xs rounded-full bg-background",
                    "{c}"
                }
            }
        }
    }
}
/// Mobile menu item
#[component]
fn MobileMenuItem(
    icon: &'static str,
    label: String,
    #[props(default = false)]
    disabled: bool,
    #[props(default = false)]
    loading: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let is_disabled = disabled || loading;
    let button_class = if is_disabled {
        "w-full flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground opacity-50 cursor-not-allowed text-left"
    } else {
        "w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-accent transition text-left"
    };
    rsx! {
        button {
            class: "{button_class}",
            disabled: is_disabled,
            onclick: move |e| {
                if !is_disabled {
                    onclick.call(e);
                }
            },
            if loading {
                svg {
                    class: "w-4 h-4 animate-spin",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    circle {
                        cx: "12",
                        cy: "12",
                        r: "10",
                        stroke_opacity: "0.25",
                    }
                    path { d: "M12 2a10 10 0 0 1 10 10", stroke_opacity: "1" }
                }
            } else {
                span { class: "w-4 h-4", dangerous_inner_html: "{icon}" }
            }
            span { "{label}" }
        }
    }
}
