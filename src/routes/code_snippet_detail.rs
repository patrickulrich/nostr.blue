//! Code Snippet Detail Page
//!
//! View a single NIP-C0 code snippet (Kind 1337).

use dioxus::prelude::*;
use crate::components::icons;
use crate::routes::Route;
use crate::services::git_hosting::fetch_snippet_by_id;
use crate::utils::nip34::DisplaySnippet;
use crate::stores::{profiles::PROFILE_CACHE, nostr_client};

/// Code snippet detail page component
#[component]
pub fn CodeSnippetDetail(note_id: String) -> Element {
    let copied = use_signal(|| false);

    // Snippet state
    let mut snippet_result = use_signal(|| None::<Result<DisplaySnippet, String>>);

    // Clone for effect
    let note_id_for_effect = note_id.clone();

    // Fetch snippet - wait for client initialization
    use_effect(move || {
        let id = note_id_for_effect.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        spawn(async move {
            let result = fetch_snippet_by_id(&id).await;
            snippet_result.set(Some(result));
        });
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "p-4 flex items-center justify-between",
                    div {
                        class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeSnippets {},
                            class: "text-muted-foreground hover:text-foreground",
                            dangerous_inner_html: icons::ARROW_LEFT
                        }
                        h1 {
                            class: "text-xl font-bold flex items-center gap-2",
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
                                polyline { points: "16 18 22 12 16 6" }
                                polyline { points: "8 6 2 12 8 18" }
                            }
                            "Snippet"
                        }
                    }
                }
            }

            // Content
            div {
                class: "p-4",
                match &*snippet_result.read() {
                    Some(Ok(s)) => rsx! {
                        SnippetContent {
                            snippet: s.clone(),
                            copied: copied,
                        }
                    },
                    Some(Err(e)) => rsx! {
                        div {
                            class: "text-center py-12",
                            div {
                                class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
                                svg {
                                    class: "w-8 h-8 text-destructive",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    circle { cx: "12", cy: "12", r: "10" }
                                    line { x1: "12", y1: "8", x2: "12", y2: "12" }
                                    line { x1: "12", y1: "16", x2: "12.01", y2: "16" }
                                }
                            }
                            h3 { class: "font-semibold text-lg mb-2", "Snippet Not Found" }
                            p { class: "text-muted-foreground text-sm mb-4", "{e}" }
                            Link {
                                to: Route::CodeSnippets {},
                                class: "text-primary hover:underline",
                                "← Back to Snippets"
                            }
                        }
                    },
                    None => rsx! {
                        LoadingSkeleton {}
                    },
                }
            }
        }
    }
}

#[component]
fn SnippetContent(snippet: DisplaySnippet, copied: Signal<bool>) -> Element {
    // Get author profile from cache
    let author_profile = PROFILE_CACHE.read().peek(&snippet.pubkey).cloned();
    let display_name = author_profile
        .as_ref()
        .and_then(|p| p.display_name.clone().or_else(|| p.name.clone()))
        .unwrap_or_else(|| snippet.pubkey_display());

    #[allow(unused_variables)]
    let code_for_copy = snippet.code.clone();
    let handle_copy = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let window = web_sys::window().unwrap();
            let navigator = window.navigator();
            let clipboard = navigator.clipboard();
            let code_to_copy = code_for_copy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = wasm_bindgen_futures::JsFuture::from(
                    clipboard.write_text(&code_to_copy)
                ).await;
            });
        }
        copied.set(true);
        // Reset after 2 seconds
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            {
                gloo_timers::future::TimeoutFuture::new(2000).await;
            }
            copied.set(false);
        });
    };

    rsx! {
        div {
            class: "space-y-6",

            // Header with author and metadata
            div {
                class: "flex items-start justify-between",
                div {
                    class: "flex items-center gap-3",
                    // Author avatar
                    Link {
                        to: Route::Profile { pubkey: snippet.pubkey.clone() },
                        class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center overflow-hidden",
                        if let Some(picture) = author_profile.as_ref().and_then(|p| p.picture.as_ref()) {
                            img {
                                class: "w-full h-full object-cover",
                                src: "{picture}",
                                alt: "Author"
                            }
                        } else {
                            span { class: "text-lg", "{display_name.chars().next().unwrap_or('?')}" }
                        }
                    }
                    div {
                        Link {
                            to: Route::Profile { pubkey: snippet.pubkey.clone() },
                            class: "font-medium hover:underline",
                            "{display_name}"
                        }
                        div {
                            class: "text-sm text-muted-foreground",
                            "{format_timestamp(snippet.created_at)}"
                        }
                    }
                }

                // Kind badge
                div {
                    class: "px-2 py-1 text-xs rounded-full bg-green-500/10 text-green-500 border border-green-500/20",
                    "Kind 1337"
                }
            }

            // Snippet name and description
            if let Some(name) = &snippet.name {
                h2 {
                    class: "text-xl font-semibold",
                    "{name}"
                }
            }

            if let Some(description) = &snippet.description {
                p {
                    class: "text-muted-foreground",
                    "{description}"
                }
            }

            // Metadata badges
            div {
                class: "flex flex-wrap gap-2",

                // Language badge
                if let Some(lang) = snippet.display_language() {
                    span {
                        class: "px-2 py-1 text-xs rounded-full bg-blue-500/10 text-blue-500 border border-blue-500/20",
                        "{lang}"
                    }
                }

                // Extension badge
                if let Some(ext) = &snippet.extension {
                    span {
                        class: "px-2 py-1 text-xs rounded-full bg-muted text-muted-foreground",
                        ".{ext}"
                    }
                }

                // License badge
                if let Some(license) = &snippet.license {
                    span {
                        class: "px-2 py-1 text-xs rounded-full bg-muted text-muted-foreground",
                        "{license}"
                    }
                }
            }

            // Code block
            div {
                class: "border border-border rounded-lg overflow-hidden",

                // Header with language and copy button
                div {
                    class: "px-4 py-2 bg-muted/50 border-b border-border flex items-center justify-between",
                    span {
                        class: "text-sm text-muted-foreground",
                        if let Some(name) = &snippet.name {
                            "{name}"
                        } else if let Some(lang) = snippet.display_language() {
                            "{lang}"
                        } else {
                            "code"
                        }
                    }
                    button {
                        class: "px-3 py-1 text-xs rounded bg-accent hover:bg-accent/80 transition flex items-center gap-1",
                        onclick: handle_copy,
                        if *copied.read() {
                            svg {
                                class: "w-3 h-3 text-green-500",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                polyline { points: "20 6 9 17 4 12" }
                            }
                            "Copied!"
                        } else {
                            svg {
                                class: "w-3 h-3",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                rect { x: "9", y: "9", width: "13", height: "13", rx: "2", ry: "2" }
                                path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
                            }
                            "Copy"
                        }
                    }
                }

                // Code content
                pre {
                    class: "p-4 overflow-x-auto text-sm font-mono bg-background",
                    code {
                        "{snippet.code}"
                    }
                }
            }

            // Dependencies
            if !snippet.dependencies.is_empty() {
                div {
                    h3 {
                        class: "font-medium mb-2 text-sm",
                        "Dependencies"
                    }
                    div {
                        class: "flex flex-wrap gap-2",
                        for dep in snippet.dependencies.iter() {
                            span {
                                key: "{dep}",
                                class: "px-2 py-1 text-xs rounded bg-muted",
                                "{dep}"
                            }
                        }
                    }
                }
            }

            // Repository link
            if let Some(repo) = &snippet.repo {
                div {
                    a {
                        href: "{repo}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        class: "inline-flex items-center gap-2 text-sm text-primary hover:underline",
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
                            path { d: "M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.28 1.15-.28 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4" }
                            path { d: "M9 18c-4.51 2-5-2-7-2" }
                        }
                        "View Repository"
                    }
                }
            }

            // Share section
            div {
                class: "pt-4 border-t border-border",
                h3 {
                    class: "font-medium mb-2 text-sm",
                    "Share this snippet"
                }
                div {
                    class: "flex items-center gap-2",
                    code {
                        class: "flex-1 px-3 py-2 bg-muted rounded text-xs font-mono overflow-x-auto",
                        "nostr:note1{snippet.event_id.chars().take(8).collect::<String>()}..."
                    }
                }
            }
        }
    }
}

#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div {
            class: "space-y-6 animate-pulse",

            // Header skeleton
            div {
                class: "flex items-start justify-between",
                div {
                    class: "flex items-center gap-3",
                    div { class: "w-10 h-10 rounded-full bg-muted" }
                    div {
                        div { class: "h-4 bg-muted rounded w-24 mb-2" }
                        div { class: "h-3 bg-muted rounded w-16" }
                    }
                }
                div { class: "h-6 bg-muted rounded w-16" }
            }

            // Title skeleton
            div { class: "h-6 bg-muted rounded w-1/3" }
            div { class: "h-4 bg-muted rounded w-2/3" }

            // Badges skeleton
            div {
                class: "flex gap-2",
                div { class: "h-6 bg-muted rounded w-16" }
                div { class: "h-6 bg-muted rounded w-12" }
            }

            // Code block skeleton
            div {
                class: "border border-border rounded-lg overflow-hidden",
                div {
                    class: "px-4 py-2 bg-muted/50 border-b border-border",
                    div { class: "h-4 bg-muted rounded w-24" }
                }
                div {
                    class: "p-4 space-y-2",
                    div { class: "h-3 bg-muted rounded w-full" }
                    div { class: "h-3 bg-muted rounded w-5/6" }
                    div { class: "h-3 bg-muted rounded w-4/6" }
                    div { class: "h-3 bg-muted rounded w-full" }
                    div { class: "h-3 bg-muted rounded w-3/4" }
                    div { class: "h-3 bg-muted rounded w-1/2" }
                }
            }
        }
    }
}

/// Format a Unix timestamp as a relative or absolute time
fn format_timestamp(timestamp: u64) -> String {
    // Use js_sys::Date for WASM compatibility
    let now = (js_sys::Date::now() / 1000.0) as u64;

    let diff = now.saturating_sub(timestamp);

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        let minutes = diff / 60;
        format!("{}m ago", minutes)
    } else if diff < 86400 {
        let hours = diff / 3600;
        format!("{}h ago", hours)
    } else if diff < 604800 {
        let days = diff / 86400;
        format!("{}d ago", days)
    } else {
        // Format as date
        let date = chrono::DateTime::from_timestamp(timestamp as i64, 0)
            .unwrap_or_default();
        date.format("%b %d, %Y").to_string()
    }
}
