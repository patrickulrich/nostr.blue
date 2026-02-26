//! Repository Action Bar Component
//!
//! Displays action buttons for a repository: Watch, Star, Fork, Zap, Share.
//! Desktop: horizontal button row. Mobile: dropdown menu.
//! Styled to match gittr's layout-client.tsx action bar pattern.
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use crate::components::code::qr_share_modal::QrShareModal;
use crate::components::code::zap_distribution::ZapDistribution;
use crate::components::icons;
use crate::components::ZapModal;
use crate::services::git_hosting::repository::publish_fork;
use crate::services::git_hosting::stars::{check_user_star, publish_star, remove_star};
use crate::stores::code_store::is_repo_starred;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::stores::profiles::PROFILE_CACHE;
use crate::utils::nip34::Repository;
use crate::utils::truncate_pubkey;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
/// Generate a kebab-case d-tag identifier from the fork name
fn generate_fork_id(name: &str, fallback_id: &str) -> String {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug: String = slug
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        format!("{}-fork-{}", fallback_id, nostr_sdk::Timestamp::now().as_secs())
    } else {
        format!("{}-{}", slug, nostr_sdk::Timestamp::now().as_secs())
    }
}

/// Repository action bar with Watch, Star, Fork, Zap, Share buttons
#[allow(clippy::clone_on_copy)]
#[component]
pub fn RepoActionBar(repo: Repository, naddr: String) -> Element {
    let toast = consume_toast();
    let mut show_zap_modal = use_signal(|| false);
    let mut is_starred = use_signal(|| false);
    let mut star_count = use_signal(|| repo.star_count);
    let mut is_watching = use_signal(|| false);
    let mut star_loading = use_signal(|| false);
    let mut star_gen = use_signal(|| 0u32);
    let mut show_actions_menu = use_signal(|| false);
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
        let gen = star_gen.peek().wrapping_add(1);
        star_gen.set(gen);
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
                        Ok(starred) => {
                            if *star_gen.peek() != gen { return; }
                            is_starred.set(starred);
                        }
                        Err(e) => log::debug!("Failed to check star status: {}", e),
                    }
                });
            }
        }
    });
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            let pubkey = repo_pubkey_signal.read().clone();
            let id = repo_id_signal.read().clone();
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(watched_json)) = storage
                        .get_item("nostr_blue_watched_repos")
                    {
                        if let Ok(watched) = serde_json::from_str::<
                            Vec<String>,
                        >(&watched_json) {
                            let coord_str = format!("{}:{}", pubkey, id);
                            is_watching.set(watched.contains(&coord_str));
                            return;
                        }
                    }
                }
            }
            is_watching.set(false);
        }
    });
    let handle_star = {
        move |_| {
            if *star_loading.read() {
                return;
            }
            star_loading.set(true);
            let current_gen = star_gen.peek().wrapping_add(1);
            star_gen.set(current_gen);
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
                // Stale guard: skip state update if a newer star action started
                if *star_gen.peek() != current_gen {
                    star_loading.set(false);
                    return;
                }
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
            let currently_watching = *is_watching.read();
            #[cfg(target_arch = "wasm32")]
            {
                let repo_coord = format!(
                    "{}:{}",
                    repo_pubkey_signal.read(),
                    repo_id_signal.read(),
                );
                if let Some(window) = web_sys::window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        let mut watched: Vec<String> = storage
                            .get_item("nostr_blue_watched_repos")
                            .ok()
                            .flatten()
                            .and_then(|s| serde_json::from_str(&s).ok())
                            .unwrap_or_default();
                        if currently_watching {
                            watched.retain(|x| x != &repo_coord);
                        } else if !watched.contains(&repo_coord) {
                            watched.push(repo_coord.clone());
                        }
                        if let Ok(json) = serde_json::to_string(&watched) {
                            let _ = storage.set_item("nostr_blue_watched_repos", &json);
                        }
                    }
                }
            }
            is_watching.set(!currently_watching);
            if !currently_watching {
                toast.success("Watching repository".to_string(), ToastOptions::new());
            }
        }
    };
    let mut show_share_modal = use_signal(|| false);
    let handle_share = move |_| {
        show_share_modal.set(true);
    };
    let mut fork_loading = use_signal(|| false);
    let mut show_fork_modal = use_signal(|| false);
    let mut fork_event_id = use_signal(|| repo.event_id.clone());
    let mut fork_repo_id = use_signal(|| repo.id.clone());
    let mut fork_repo_name = use_signal(|| repo.name.clone());
    let mut fork_repo_desc = use_signal(|| repo.description.clone());
    let mut fork_clone_urls = use_signal(|| repo.clone.clone());
    // Sync fork signals when repo prop changes
    {
        if *fork_event_id.peek() != repo.event_id {
            fork_event_id.set(repo.event_id.clone());
        }
        if *fork_repo_id.peek() != repo.id {
            fork_repo_id.set(repo.id.clone());
        }
        if *fork_repo_name.peek() != repo.name {
            fork_repo_name.set(repo.name.clone());
        }
        if *fork_repo_desc.peek() != repo.description {
            fork_repo_desc.set(repo.description.clone());
        }
        if *fork_clone_urls.peek() != repo.clone {
            fork_clone_urls.set(repo.clone.clone());
        }
    }
    // Pre-filled form state for the fork modal
    let mut fork_form_name = use_signal(String::new);
    let mut fork_form_desc = use_signal(String::new);
    let mut fork_form_clone_urls = use_signal(String::new);
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
            let fork_id = generate_fork_id(&form_name, &id);
            let fork_name = if form_name.trim().is_empty() { None } else { Some(form_name.trim().to_string()) };
            let fork_desc = if form_desc.trim().is_empty() { None } else { Some(form_desc.trim().to_string()) };
            let urls: Vec<String> = form_urls_raw
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .map(|url| {
                    // Normalize SCP-style URLs (user@host:path) to ssh://
                    if !url.contains("://") {
                        if let Some(at_pos) = url.find('@') {
                            let after_at = &url[at_pos + 1..];
                            let colon_offset = if after_at.starts_with('[') {
                                after_at.find(']').and_then(|bracket_end| {
                                    after_at[bracket_end + 1..].find(':').map(|c| bracket_end + 1 + c)
                                })
                            } else {
                                after_at.find(':')
                            };
                            if let Some(colon_pos) = colon_offset {
                                let user = &url[..at_pos];
                                let host = &after_at[..colon_pos];
                                let path = &after_at[colon_pos + 1..];
                                return format!("ssh://{}@{}/{}", user, host, path);
                            }
                        }
                    }
                    url
                })
                .collect();
            if urls.is_empty() {
                toast.error("At least one clone URL is required".to_string(), ToastOptions::new());
                fork_loading.set(false);
                return;
            }
            for url in &urls {
                match url::Url::parse(url) {
                    Ok(parsed) if ["http", "https", "git", "ssh"].contains(&parsed.scheme()) => {}
                    Ok(_) => {
                        toast.error(format!("Unsupported URL scheme: {}", url), ToastOptions::new());
                        fork_loading.set(false);
                        return;
                    }
                    Err(_) => {
                        toast.error(format!("Invalid URL format: {}", url), ToastOptions::new());
                        fork_loading.set(false);
                        return;
                    }
                }
            }
            let url_refs: Vec<&str> = urls.iter().map(|s| s.as_str()).collect();
            match publish_fork(
                &event_id,
                &fork_id,
                fork_name.as_deref(),
                fork_desc.as_deref(),
                &url_refs,
            ).await {
                Ok(_event_id) => {
                    toast.success("Repository forked! Redirecting...".to_string(), ToastOptions::new());
                    show_fork_modal.set(false);
                    // Navigate to the new fork
                    if let Some(client) = crate::stores::nostr_client::get_client() {
                        if let Ok(signer) = client.signer().await {
                            if let Ok(pubkey) = signer.get_public_key().await {
                                let coordinate = nostr_sdk::prelude::Coordinate::new(
                                    nostr_sdk::prelude::Kind::GitRepoAnnouncement,
                                    pubkey,
                                ).identifier(&fork_id);
                                if let Ok(naddr) = coordinate.to_bech32() {
                                    let nav = navigator();
                                    nav.push(crate::routes::Route::CodeRepo { naddr });
                                }
                            }
                        }
                    }
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
            div { class: "hidden lg:flex items-center gap-2",
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
            }
            div { class: "lg:hidden relative",
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
                            disabled: !*HAS_SIGNER.read() || *fork_loading.read(),
                            loading: *fork_loading.read(),
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
                    }
                }
            }
            if *show_zap_modal.read() {
                if !repo.zap_splits.is_empty() {
                    ZapDistribution {
                        zap_splits: repo.zap_splits.clone(),
                        repo_event_id: repo.event_id.clone(),
                        on_close: move |_| show_zap_modal.set(false),
                    }
                } else {
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
            }
            if *show_fork_modal.read() {
                div {
                    class: "fixed inset-0 z-50 bg-black/50 backdrop-blur-sm",
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
                        role: "dialog",
                        aria_modal: "true",
                        aria_labelledby: "fork_modal_title",
                        class: "bg-background border border-border rounded-lg p-6 w-full max-w-md shadow-lg max-h-[90vh] overflow-y-auto",
                        onclick: move |evt| evt.stop_propagation(),
                        // Header
                        div { class: "flex justify-between items-center mb-6",
                            h2 { id: "fork_modal_title", class: "text-xl font-bold", "Fork Repository" }
                            button {
                                r#type: "button",
                                class: "p-1 hover:bg-accent rounded transition text-muted-foreground hover:text-foreground disabled:opacity-50",
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
                                    r#type: "button",
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
            if *show_share_modal.read() {
                QrShareModal {
                    naddr: naddr.clone(),
                    repo_name: repo.name.clone().unwrap_or_else(|| repo.id.clone()),
                    on_close: move |_| show_share_modal.set(false),
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
