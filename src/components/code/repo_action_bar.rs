//! Repository Action Bar Component
//!
//! Displays action buttons for a repository: Watch, Star, Fork, Zap, Share.
//! Desktop: horizontal button row. Mobile: dropdown menu.
//! Styled to match gittr's layout-client.tsx action bar pattern.

use dioxus::prelude::*;
use nostr_sdk::prelude::*;

use crate::components::icons;
use crate::services::git_hosting::stars::{publish_star, remove_star, check_user_star};
use crate::stores::code_store::is_repo_starred;
use crate::stores::nostr_client::HAS_SIGNER;
use crate::utils::clipboard::copy_to_clipboard;
use crate::utils::nip34::Repository;
use dioxus_primitives::toast::{consume_toast, ToastOptions};

/// Repository action bar with Watch, Star, Fork, Zap, Share buttons
#[component]
pub fn RepoActionBar(
    repo: Repository,
    naddr: String,
    #[props(default = false)] compact: bool,
) -> Element {
    let toast = consume_toast();
    let mut is_starred = use_signal(|| false);
    let mut star_count = use_signal(|| repo.star_count);
    let mut is_watching = use_signal(|| false);
    let mut star_loading = use_signal(|| false);
    let mut show_actions_menu = use_signal(|| false);

    // Check if user is authenticated
    let has_signer = *HAS_SIGNER.read();

    // Clone values needed in closures before they get moved
    let repo_pubkey = repo.pubkey.clone();
    let repo_id = repo.id.clone();
    #[cfg(target_arch = "wasm32")]
    let repo_pubkey_for_watch = repo_pubkey.clone();
    #[cfg(target_arch = "wasm32")]
    let repo_id_for_watch = repo_id.clone();
    #[cfg(target_arch = "wasm32")]
    let repo_pubkey_for_handler = repo_pubkey.clone();
    #[cfg(target_arch = "wasm32")]
    let repo_id_for_handler = repo.id.clone();

    // Build coordinate for the repository
    let coordinate = use_memo(move || {
        if let Ok(pk) = PublicKey::from_hex(&repo_pubkey) {
            Some(Coordinate::new(Kind::GitRepoAnnouncement, pk).identifier(&repo_id))
        } else {
            None
        }
    });

    // Check initial star status
    use_effect(move || {
        let coord = coordinate.read().clone();
        if let Some(coord) = coord {
            let coord_str = format!(
                "{}:{}:{}",
                coord.kind.as_u16(),
                coord.public_key.to_hex(),
                coord.identifier
            );
            // Check local cache first
            if is_repo_starred(&coord_str) {
                is_starred.set(true);
            } else {
                // Check from relays
                spawn(async move {
                    if let Ok(starred) = check_user_star(&coord).await {
                        is_starred.set(starred);
                    }
                });
            }
        }
    });

    // Load watch status from localStorage
    use_effect(move || {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(watched_json)) = storage.get_item("nostr_blue_watched_repos") {
                        if let Ok(watched) = serde_json::from_str::<Vec<String>>(&watched_json) {
                            let coord_str = format!("{}:{}", repo_pubkey_for_watch, repo_id_for_watch);
                            is_watching.set(watched.contains(&coord_str));
                        }
                    }
                }
            }
        }
    });

    // Star/Unstar handler
    let handle_star = {
        let coord = coordinate.read().clone();
        move |_| {
            if !has_signer {
                toast.warning("Sign in to star repositories".to_string(), ToastOptions::new());
                return;
            }

            if let Some(coord) = coord.clone() {
                star_loading.set(true);
                let currently_starred = *is_starred.read();

                spawn(async move {
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
                                star_count.set(current + 1);
                            }
                        }
                        Err(e) => {
                            log::error!("Star action failed: {}", e);
                        }
                    }
                    star_loading.set(false);
                });
            }
        }
    };

    // Watch handler (localStorage only)
    let handle_watch = {
        #[cfg(target_arch = "wasm32")]
        let repo_coord = format!("{}:{}", repo_pubkey_for_handler, repo_id_for_handler);
        move |_| {
            let currently_watching = *is_watching.read();

            #[cfg(target_arch = "wasm32")]
            {
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
                        } else {
                            if !watched.contains(&repo_coord) {
                                watched.push(repo_coord.clone());
                            }
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

    // Share handler (copy naddr)
    let handle_share = {
        let naddr = naddr.clone();
        move |_| {
            let naddr = naddr.clone();
            spawn(async move {
                let share_text = format!("nostr:{}", naddr);
                if copy_to_clipboard(&share_text).await.is_ok() {
                    toast.success("Copied to clipboard".to_string(), ToastOptions::new());
                } else {
                    toast.error("Failed to copy".to_string(), ToastOptions::new());
                }
            });
        }
    };

    // Fork handler (placeholder)
    let handle_fork = move |_| {
        toast.info("Fork coming soon".to_string(), ToastOptions::new());
    };

    // Zap handler (placeholder - opens ZapModal later)
    let handle_zap = move |_| {
        if !has_signer {
            toast.warning("Sign in to zap repositories".to_string(), ToastOptions::new());
            return;
        }
        toast.info("Zap modal coming soon".to_string(), ToastOptions::new());
    };

    let star_text = if *is_starred.read() { "Starred" } else { "Star" };
    let watch_text = if *is_watching.read() { "Unwatch" } else { "Watch" };

    rsx! {
        div {
            class: "flex items-center gap-2",

            // Desktop buttons (hidden on small screens)
            div {
                class: "hidden md:flex items-center gap-2",

                // Watch button
                ActionButton {
                    icon: icons::EYE,
                    label: "{watch_text}",
                    count: None,
                    active: *is_watching.read(),
                    disabled: false,
                    loading: false,
                    onclick: handle_watch.clone(),
                }

                // Star button
                ActionButton {
                    icon: if *is_starred.read() { icons::STAR_FILLED } else { icons::STAR },
                    label: "{star_text}",
                    count: Some(*star_count.read()),
                    active: *is_starred.read(),
                    disabled: !has_signer,
                    loading: *star_loading.read(),
                    onclick: handle_star.clone(),
                }

                // Fork button
                ActionButton {
                    icon: icons::GIT_FORK,
                    label: "Fork",
                    count: None,
                    active: false,
                    disabled: false,
                    loading: false,
                    onclick: handle_fork,
                }

                // Zap button
                ActionButton {
                    icon: icons::ZAP,
                    label: "Zap",
                    count: None,
                    active: false,
                    disabled: !has_signer,
                    loading: false,
                    onclick: handle_zap,
                }

                // Share button
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

            // Mobile dropdown (shown on small screens)
            div {
                class: "md:hidden relative",

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

                // Dropdown menu
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
                // Loading spinner
                svg {
                    class: "w-4 h-4 animate-spin",
                    xmlns: "http://www.w3.org/2000/svg",
                    width: "24",
                    height: "24",
                    view_box: "0 0 24 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "2",
                    circle { cx: "12", cy: "12", r: "10", stroke_opacity: "0.25" }
                    path { d: "M12 2a10 10 0 0 1 10 10", stroke_opacity: "1" }
                }
            } else {
                span {
                    class: "w-4 h-4",
                    dangerous_inner_html: "{icon}"
                }
            }

            span { "{label}" }

            if let Some(c) = count {
                span {
                    class: "ml-1 px-1.5 py-0.5 text-xs rounded-full bg-background",
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
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: "w-full flex items-center gap-2 px-3 py-2 text-sm hover:bg-accent transition text-left",
            onclick: move |e| onclick.call(e),

            span {
                class: "w-4 h-4",
                dangerous_inner_html: "{icon}"
            }
            span { "{label}" }
        }
    }
}
