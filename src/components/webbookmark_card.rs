use crate::components::icons::BookmarkIcon;
use crate::stores::webbookmarks::{
    delete_webbookmark, get_display_hashtags, get_domain, get_image, get_published_at,
    get_title, get_url, is_archived, is_favorite, toggle_favorite,
};
use crate::utils::format_relative_time_or;
use dioxus::prelude::*;
use dioxus_primitives::toast::consume_toast;
#[cfg(not(feature = "web"))]
use dioxus_primitives::toast::ToastOptions;
#[cfg(not(feature = "web"))]
use std::time::Duration;
use nostr_sdk::Event as NostrEvent;
#[component]
pub fn WebBookmarkCard(
    event: NostrEvent,
    on_edit: Option<EventHandler<NostrEvent>>,
) -> Element {
    let url = get_url(&event);
    let title = get_title(&event);
    let description = if event.content.is_empty() {
        None
    } else {
        Some(event.content.clone())
    };
    let image_url = get_image(&event);
    let published_at = get_published_at(&event);
    let hashtags = get_display_hashtags(&event);
    let is_fav = is_favorite(&event);
    let is_arch = is_archived(&event);
    let deleting = use_signal(|| false);
    let toggling_favorite = use_signal(|| false);
    let mut show_actions = use_signal(|| false);
    let toast = consume_toast();
    let domain = url.as_ref().map(|u| get_domain(u)).unwrap_or_default();
    let full_url = url
        .as_ref()
        .map(|u| {
            if u.starts_with("http://") || u.starts_with("https://") {
                u.clone()
            } else {
                format!("https://{}", u)
            }
        });
    let display_title = title
        .or_else(|| url.clone())
        .unwrap_or_else(|| "Untitled Bookmark".to_string());
    let timestamp_str = published_at
        .map(|ts| format_relative_time_or(ts.as_secs(), "Unknown"))
        .unwrap_or_else(|| format_relative_time_or(
            event.created_at.as_secs(),
            "Unknown",
        ));
    let handle_delete = {
        let event_clone = event.clone();
        let mut deleting = deleting;
        move |_| {
            let event_for_delete = event_clone.clone();
            deleting.set(true);
            spawn(async move {
                match delete_webbookmark(&event_for_delete).await {
                    Ok(_) => {
                        log::info!("Bookmark deleted successfully");
                    }
                    Err(e) => {
                        log::error!("Failed to delete bookmark: {}", e);
                        deleting.set(false);
                    }
                }
            });
        }
    };
    let handle_toggle_favorite = {
        let event_clone = event.clone();
        let mut toggling_favorite = toggling_favorite;
        move |_| {
            let event_for_toggle = event_clone.clone();
            let current_fav = is_fav;
            toggling_favorite.set(true);
            spawn(async move {
                match toggle_favorite(&event_for_toggle, !current_fav).await {
                    Ok(_) => {
                        log::info!("Favorite toggled successfully");
                        toggling_favorite.set(false);
                    }
                    Err(e) => {
                        log::error!("Failed to toggle favorite: {}", e);
                        toggling_favorite.set(false);
                    }
                }
            });
        }
    };
    let handle_edit = {
        let event_clone = event.clone();
        let mut show_actions_edit = show_actions;
        move |_| {
            if let Some(ref handler) = on_edit {
                handler.call(event_clone.clone());
            }
            show_actions_edit.set(false);
        }
    };
    let handle_open = {
        let full_url_clone = full_url.clone();
        #[allow(unused_variables)]
        let toast_api = toast;
        move |_| {
            if let Some(ref url) = full_url_clone {
                #[cfg(feature = "web")]
                {
                    if let Some(window) = web_sys::window() {
                        let _ = window.open_with_url_and_target(url, "_blank");
                    }
                }
                #[cfg(not(feature = "web"))]
                {
                    log::info!("Open URL: {}", url);
                    toast_api.error(
                        "Open URL not supported on this platform".to_string(),
                        ToastOptions::new()
                            .duration(Duration::from_secs(3))
                            .permanent(false),
                    );
                }
            }
        }
    };
    if *deleting.read() {
        return rsx! {
            div { class: "bg-card rounded-lg border border-border p-4 opacity-50",
                div { class: "text-center text-muted-foreground", "Deleting..." }
            }
        };
    }
    let handle_open_click = handle_open.clone();
    let handle_open_image = handle_open.clone();
    rsx! {
        div {
            class: "group bg-card rounded-lg border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-lg relative",
            onmouseleave: move |_| show_actions.set(false),
            if is_fav {
                div { class: "absolute top-2 right-2 z-10 bg-yellow-500/90 text-white px-2 py-1 rounded-full text-xs font-medium",
                    "⭐ Favorite"
                }
            }
            if is_arch {
                div { class: "absolute top-2 left-2 z-10 bg-muted/90 text-muted-foreground px-2 py-1 rounded-full text-xs font-medium",
                    "📦 Archived"
                }
            }
            if let Some(img_url) = image_url {
                div {
                    class: "aspect-video w-full bg-muted overflow-hidden cursor-pointer",
                    onclick: handle_open_image,
                    img {
                        src: "{img_url}",
                        alt: "{display_title}",
                        class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200",
                        loading: "lazy",
                    }
                }
            }
            div { class: "p-4 space-y-3",
                div { class: "flex items-center justify-between",
                    div { class: "flex items-center gap-2 text-xs text-muted-foreground",
                        span { "🔗" }
                        span { class: "truncate", "{domain}" }
                    }
                    div { class: "relative",
                        button {
                            class: "p-1 hover:bg-accent rounded transition opacity-0 group-hover:opacity-100",
                            onclick: move |e| {
                                e.stop_propagation();
                                let current = *show_actions.read();
                                show_actions.set(!current);
                            },
                            "⋮"
                        }
                        if *show_actions.read() {
                            div { class: "absolute right-0 top-full mt-1 bg-card border border-border rounded-lg shadow-lg min-w-[150px] z-20",
                                button {
                                    class: "w-full px-4 py-2 text-left text-sm hover:bg-accent transition flex items-center gap-2",
                                    onclick: handle_open,
                                    span { "🔗" }
                                    "Open URL"
                                }
                                if on_edit.is_some() {
                                    button {
                                        class: "w-full px-4 py-2 text-left text-sm hover:bg-accent transition flex items-center gap-2 border-t border-border",
                                        onclick: handle_edit,
                                        span { "✏️" }
                                        "Edit"
                                    }
                                }
                                button {
                                    class: "w-full px-4 py-2 text-left text-sm hover:bg-accent transition flex items-center gap-2 border-t border-border",
                                    onclick: handle_toggle_favorite,
                                    disabled: *toggling_favorite.read(),
                                    span {
                                        if is_fav {
                                            "☆"
                                        } else {
                                            "⭐"
                                        }
                                    }
                                    if is_fav {
                                        "Remove Favorite"
                                    } else {
                                        "Add Favorite"
                                    }
                                }
                                button {
                                    class: "w-full px-4 py-2 text-left text-sm hover:bg-destructive/10 text-destructive transition flex items-center gap-2 border-t border-border",
                                    onclick: handle_delete,
                                    span { "🗑️" }
                                    "Delete"
                                }
                            }
                        }
                    }
                }
                if !hashtags.is_empty() {
                    div { class: "flex flex-wrap gap-2",
                        for tag in hashtags.iter().take(5) {
                            span { class: "px-2 py-1 text-xs rounded-full bg-primary/10 text-primary font-medium",
                                "#{tag}"
                            }
                        }
                        if hashtags.len() > 5 {
                            span { class: "px-2 py-1 text-xs rounded-full bg-muted text-muted-foreground font-medium",
                                "+{hashtags.len() - 5} more"
                            }
                        }
                    }
                }
                div { class: "cursor-pointer", onclick: handle_open_click,
                    h3 { class: "text-lg font-bold line-clamp-2 group-hover:text-primary transition-colors",
                        "{display_title}"
                    }
                }
                if let Some(desc) = description {
                    p { class: "text-sm text-muted-foreground line-clamp-3", "{desc}" }
                }
                div { class: "flex items-center justify-between pt-2 text-xs text-muted-foreground",
                    span { "{timestamp_str}" }
                    div { class: "flex items-center gap-1",
                        BookmarkIcon { class: "w-4 h-4".to_string(), filled: true }
                        span { "Saved" }
                    }
                }
            }
        }
    }
}
/// Skeleton loader for web bookmark cards
#[component]
pub fn WebBookmarkCardSkeleton() -> Element {
    rsx! {
        div { class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse",
            div { class: "aspect-video w-full bg-muted" }
            div { class: "p-4 space-y-3",
                div { class: "h-4 w-32 bg-muted rounded" }
                div { class: "flex gap-2",
                    div { class: "h-6 w-16 bg-muted rounded-full" }
                    div { class: "h-6 w-20 bg-muted rounded-full" }
                }
                div { class: "h-6 bg-muted rounded w-3/4" }
                div { class: "h-6 bg-muted rounded w-1/2" }
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-2/3" }
                div { class: "flex items-center justify-between pt-2",
                    div { class: "h-4 w-24 bg-muted rounded" }
                    div { class: "h-4 w-16 bg-muted rounded" }
                }
            }
        }
    }
}
