use dioxus::prelude::*;
use nostr_sdk::Event as NostrEvent;
use crate::stores::webbookmarks::{
    get_url, get_title, get_display_hashtags, get_image, get_published_at,
    get_domain, is_favorite, is_archived, toggle_favorite, delete_webbookmark
};
use crate::components::icons::BookmarkIcon;
use chrono::{DateTime, Utc, Local};

#[component]
pub fn WebBookmarkCard(event: NostrEvent, on_edit: Option<EventHandler<NostrEvent>>) -> Element {
    // Extract bookmark metadata
    let url = get_url(&event);
    let title = get_title(&event);
    let description = if event.content.is_empty() { None } else { Some(event.content.clone()) };
    let image_url = get_image(&event);
    let published_at = get_published_at(&event);
    let hashtags = get_display_hashtags(&event);
    let is_fav = is_favorite(&event);
    let is_arch = is_archived(&event);

    // State for loading actions
    let deleting = use_signal(|| false);
    let toggling_favorite = use_signal(|| false);
    let mut show_actions = use_signal(|| false);

    // Domain for display
    let domain = url.as_ref().map(|u| get_domain(u)).unwrap_or_default();

    // Full URL with scheme for opening
    let full_url = url.as_ref().map(|u| {
        if u.starts_with("http://") || u.starts_with("https://") {
            u.clone()
        } else {
            format!("https://{}", u)
        }
    });

    // Display title with fallback
    let display_title = title
        .or_else(|| url.clone())
        .unwrap_or_else(|| "Untitled Bookmark".to_string());

    // Format timestamp
    let timestamp_str = published_at
        .map(|ts| format_timestamp(ts.as_secs()))
        .unwrap_or_else(|| format_timestamp(event.created_at.as_secs()));

    // Handle delete
    let handle_delete = {
        let event_clone = event.clone();
        let mut deleting = deleting.clone();
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

    // Handle toggle favorite
    let handle_toggle_favorite = {
        let event_clone = event.clone();
        let mut toggling_favorite = toggling_favorite.clone();
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

    // Handle edit
    let handle_edit = {
        let event_clone = event.clone();
        let mut show_actions_edit = show_actions.clone();
        move |_| {
            if let Some(ref handler) = on_edit {
                handler.call(event_clone.clone());
            }
            show_actions_edit.set(false);
        }
    };

    // Handle open URL
    let handle_open = {
        let full_url_clone = full_url.clone();
        move |_| {
            if let Some(ref url) = full_url_clone {
                #[cfg(target_arch = "wasm32")]
                {
                    if let Some(window) = web_sys::window() {
                        let _ = window.open_with_url_and_target(url, "_blank");
                    }
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    log::info!("Open URL: {}", url);
                }
            }
        }
    };

    if *deleting.read() {
        return rsx! {
            div {
                class: "bg-card rounded-lg border border-border p-4 opacity-50",
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

            // Favorite indicator (corner badge)
            if is_fav {
                div {
                    class: "absolute top-2 right-2 z-10 bg-yellow-500/90 text-white px-2 py-1 rounded-full text-xs font-medium",
                    "⭐ Favorite"
                }
            }

            // Archived indicator
            if is_arch {
                div {
                    class: "absolute top-2 left-2 z-10 bg-muted/90 text-muted-foreground px-2 py-1 rounded-full text-xs font-medium",
                    "📦 Archived"
                }
            }

            // Cover image (if available)
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

            // Card content
            div {
                class: "p-4 space-y-3",

                // Domain and actions header
                div {
                    class: "flex items-center justify-between",

                    // Domain
                    div {
                        class: "flex items-center gap-2 text-xs text-muted-foreground",
                        span { "🔗" }
                        span { class: "truncate", "{domain}" }
                    }

                    // Actions menu button
                    div {
                        class: "relative",
                        button {
                            class: "p-1 hover:bg-accent rounded transition opacity-0 group-hover:opacity-100",
                            onclick: move |e| {
                                e.stop_propagation();
                                let current = *show_actions.read();
                                show_actions.set(!current);
                            },
                            "⋮"
                        }

                        // Actions dropdown
                        if *show_actions.read() {
                            div {
                                class: "absolute right-0 top-full mt-1 bg-card border border-border rounded-lg shadow-lg min-w-[150px] z-20",

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
                                    span { if is_fav { "☆" } else { "⭐" } }
                                    if is_fav { "Remove Favorite" } else { "Add Favorite" }
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

                // Hashtags
                if !hashtags.is_empty() {
                    div {
                        class: "flex flex-wrap gap-2",
                        for tag in hashtags.iter().take(5) {
                            span {
                                class: "px-2 py-1 text-xs rounded-full bg-primary/10 text-primary font-medium",
                                "#{tag}"
                            }
                        }
                        if hashtags.len() > 5 {
                            span {
                                class: "px-2 py-1 text-xs rounded-full bg-muted text-muted-foreground font-medium",
                                "+{hashtags.len() - 5} more"
                            }
                        }
                    }
                }

                // Title
                div {
                    class: "cursor-pointer",
                    onclick: handle_open_click,
                    h3 {
                        class: "text-lg font-bold line-clamp-2 group-hover:text-primary transition-colors",
                        "{display_title}"
                    }
                }

                // Description
                if let Some(desc) = description {
                    p {
                        class: "text-sm text-muted-foreground line-clamp-3",
                        "{desc}"
                    }
                }

                // Footer: timestamp and bookmark indicator
                div {
                    class: "flex items-center justify-between pt-2 text-xs text-muted-foreground",

                    // Timestamp
                    span { "{timestamp_str}" }

                    // Bookmark icon
                    div {
                        class: "flex items-center gap-1",
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
        div {
            class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse",

            // Image skeleton
            div {
                class: "aspect-video w-full bg-muted",
            }

            // Content skeleton
            div {
                class: "p-4 space-y-3",

                // Domain skeleton
                div { class: "h-4 w-32 bg-muted rounded" }

                // Tags skeleton
                div {
                    class: "flex gap-2",
                    div { class: "h-6 w-16 bg-muted rounded-full" }
                    div { class: "h-6 w-20 bg-muted rounded-full" }
                }

                // Title skeleton
                div { class: "h-6 bg-muted rounded w-3/4" }
                div { class: "h-6 bg-muted rounded w-1/2" }

                // Description skeleton
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-2/3" }

                // Footer skeleton
                div {
                    class: "flex items-center justify-between pt-2",
                    div { class: "h-4 w-24 bg-muted rounded" }
                    div { class: "h-4 w-16 bg-muted rounded" }
                }
            }
        }
    }
}

/// Format timestamp to relative time
fn format_timestamp(timestamp: u64) -> String {
    let dt = DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or_else(|| Utc::now());
    let local_dt = dt.with_timezone(&Local);
    let now = Local::now();
    let duration = now.signed_duration_since(local_dt);

    if duration.num_seconds() < 60 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        let mins = duration.num_minutes();
        format!("{}m ago", mins)
    } else if duration.num_hours() < 24 {
        let hours = duration.num_hours();
        format!("{}h ago", hours)
    } else if duration.num_days() < 30 {
        let days = duration.num_days();
        format!("{}d ago", days)
    } else if duration.num_days() < 365 {
        let months = duration.num_days() / 30;
        format!("{}mo ago", months)
    } else {
        let years = duration.num_days() / 365;
        format!("{}y ago", years)
    }
}
