//! Podcast Chapters Component
//!
//! Displays JSON chapters for a podcast episode with:
//! - Chapter list with timestamps
//! - Chapter images (if available)
//! - Click-to-seek functionality
//! - Current chapter highlighting

use crate::components::icons;
use crate::services::podcast_index::fetch_chapters_proxied;
use crate::services::podcast_rss::format_duration;
use crate::stores::music_player;
use crate::utils::podcast::Chapter;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PodcastChaptersProps {
    /// URL to the JSON chapters file
    pub chapters_url: String,
    /// Current playback position in seconds (for highlighting)
    #[props(default = 0.0)]
    pub current_time: f64,
    /// Callback when a chapter is clicked (seeks to that time)
    #[props(default)]
    pub on_seek: Option<EventHandler<f64>>,
    /// Compact display mode
    #[props(default = false)]
    pub compact: bool,
}

/// Podcast chapters display component
#[component]
pub fn PodcastChapters(props: PodcastChaptersProps) -> Element {
    let chapters_url = props.chapters_url.clone();

    // Fetch chapters through proxy to avoid CORS issues
    let chapters = use_resource(move || {
        let url = chapters_url.clone();
        async move { fetch_chapters_proxied(&url).await }
    });

    let chapters_read = chapters.read();
    match &*chapters_read {
        Some(Ok(file)) => {
            let chapter_list = file.chapters.clone();
            drop(chapters_read);
            rsx! {
                ChapterList {
                    chapters: chapter_list,
                    current_time: props.current_time,
                    on_seek: props.on_seek,
                    compact: props.compact
                }
            }
        }
        Some(Err(e)) => {
            let err_msg = e.clone();
            drop(chapters_read);
            rsx! {
                div {
                    class: "text-xs text-muted-foreground p-2",
                    "Failed to load chapters: {err_msg}"
                }
            }
        }
        None => {
            drop(chapters_read);
            rsx! {
                ChapterListSkeleton {}
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct ChapterListProps {
    /// List of chapters
    pub chapters: Vec<Chapter>,
    /// Current playback position in seconds
    #[props(default = 0.0)]
    pub current_time: f64,
    /// Callback when a chapter is clicked
    #[props(default)]
    pub on_seek: Option<EventHandler<f64>>,
    /// Compact display mode
    #[props(default = false)]
    pub compact: bool,
}

/// Chapter list component (for when chapters are already loaded)
#[component]
pub fn ChapterList(props: ChapterListProps) -> Element {
    // Filter chapters that should appear in TOC
    let visible_chapters: Vec<_> = props
        .chapters
        .iter()
        .filter(|ch| ch.toc.unwrap_or(true))
        .collect();

    // Find current chapter index
    let current_chapter_idx = find_current_chapter(&visible_chapters, props.current_time);

    rsx! {
        div {
            class: "space-y-1",

            for (idx, chapter) in visible_chapters.iter().enumerate() {
                ChapterItem {
                    key: "{idx}",
                    chapter: (*chapter).clone(),
                    is_current: Some(idx) == current_chapter_idx,
                    on_click: props.on_seek,
                    compact: props.compact
                }
            }
        }
    }
}

/// Find the index of the currently playing chapter
fn find_current_chapter(chapters: &[&Chapter], current_time: f64) -> Option<usize> {
    // Find the chapter with the greatest start_time that is <= current_time
    // This handles unsorted chapter lists defensively (per Podlove spec recommendation)
    let mut best_idx = None;
    let mut best_start_time = -1.0;

    for (idx, chapter) in chapters.iter().enumerate() {
        if chapter.start_time <= current_time && chapter.start_time > best_start_time {
            best_idx = Some(idx);
            best_start_time = chapter.start_time;
        }
    }

    best_idx
}

#[derive(Props, Clone, PartialEq)]
struct ChapterItemProps {
    chapter: Chapter,
    #[props(default = false)]
    is_current: bool,
    on_click: Option<EventHandler<f64>>,
    #[props(default = false)]
    compact: bool,
}

/// Individual chapter item
#[component]
fn ChapterItem(props: ChapterItemProps) -> Element {
    let chapter = &props.chapter;

    let handle_click = {
        let start_time = chapter.start_time;
        let on_click = props.on_click;
        move |_| {
            if let Some(handler) = &on_click {
                handler.call(start_time);
            } else {
                // Default: seek using music player
                music_player::seek_to(start_time);
            }
        }
    };

    let base_class = if props.is_current {
        "flex items-center gap-3 p-2 rounded-lg bg-primary/10 border border-primary/20 cursor-pointer hover:bg-primary/15 transition"
    } else {
        "flex items-center gap-3 p-2 rounded-lg hover:bg-muted/50 cursor-pointer transition"
    };

    let timestamp = format_duration(chapter.start_time as u64);

    rsx! {
        div {
            class: "{base_class}",
            onclick: handle_click,

            // Chapter image (if not compact and has image)
            if !props.compact {
                if let Some(ref img_url) = chapter.img {
                    img {
                        src: "{img_url}",
                        alt: chapter.title.as_deref().unwrap_or("Chapter"),
                        class: "w-12 h-12 rounded object-cover shrink-0"
                    }
                }
            }

            // Chapter info
            div {
                class: "flex-1 min-w-0",

                // Title
                div {
                    class: if props.is_current { "font-medium text-sm text-primary" } else { "font-medium text-sm" },
                    {chapter.title.as_deref().unwrap_or("Untitled Chapter")}
                }

                // URL link (if present, not compact)
                if !props.compact {
                    if let Some(ref url) = chapter.url {
                        a {
                            href: "{url}",
                            target: "_blank",
                            class: "text-xs text-muted-foreground hover:text-primary hover:underline truncate block",
                            onclick: |e| e.stop_propagation(),
                            "{url}"
                        }
                    }
                }
            }

            // Timestamp
            div {
                class: "text-xs text-muted-foreground shrink-0 font-mono",
                "{timestamp}"
            }

            // Play indicator for current chapter
            if props.is_current {
                div {
                    class: "text-primary",
                    dangerous_inner_html: icons::PLAY
                }
            }
        }
    }
}

/// Skeleton loader for chapter list
#[component]
pub fn ChapterListSkeleton() -> Element {
    rsx! {
        div {
            class: "space-y-1 animate-pulse",

            for i in 0..5 {
                div {
                    key: "{i}",
                    class: "flex items-center gap-3 p-2 rounded-lg",
                    div { class: "w-12 h-12 bg-muted rounded shrink-0" }
                    div {
                        class: "flex-1 space-y-1",
                        div { class: "h-4 bg-muted rounded w-3/4" }
                        div { class: "h-3 bg-muted rounded w-1/2" }
                    }
                    div { class: "w-12 h-4 bg-muted rounded shrink-0" }
                }
            }
        }
    }
}

/// Inline chapter progress indicator for the player
#[component]
pub fn ChapterProgressIndicator(
    chapters: Vec<Chapter>,
    current_time: f64,
    total_duration: f64,
) -> Element {
    // Filter visible chapters
    let visible_chapters: Vec<_> = chapters
        .iter()
        .filter(|ch| ch.toc.unwrap_or(true))
        .collect();

    if visible_chapters.is_empty() {
        return rsx! {};
    }

    // Find current chapter
    let current_chapter_idx = find_current_chapter(&visible_chapters, current_time);

    rsx! {
        div {
            class: "relative w-full h-1 bg-muted rounded-full overflow-hidden",

            // Chapter markers
            for (idx, chapter) in visible_chapters.iter().enumerate() {
                {
                    // Guard against division by zero
                    let position = if total_duration > 0.0 {
                        (chapter.start_time / total_duration * 100.0).min(100.0)
                    } else {
                        0.0
                    };
                    let is_current = Some(idx) == current_chapter_idx;

                    rsx! {
                        div {
                            key: "{idx}",
                            class: if is_current { "absolute w-1 h-2 bg-primary -top-0.5 transform -translate-x-1/2" } else { "absolute w-0.5 h-1.5 bg-muted-foreground/50 -top-0.25 transform -translate-x-1/2" },
                            style: "left: {position}%",
                            title: "{chapter.title.as_deref().unwrap_or(\"Chapter\")}"
                        }
                    }
                }
            }

            // Playback progress (guard against division by zero)
            {
                let progress_width = if total_duration > 0.0 {
                    (current_time / total_duration * 100.0).min(100.0)
                } else {
                    0.0
                };
                rsx! {
                    div {
                        class: "absolute left-0 top-0 h-full bg-primary rounded-full",
                        style: "width: {progress_width}%"
                    }
                }
            }
        }
    }
}

/// Mini chapter display for the player (shows current chapter name)
#[component]
pub fn CurrentChapterDisplay(chapters: Vec<Chapter>, current_time: f64) -> Element {
    // Filter visible chapters
    let visible_chapters: Vec<_> = chapters
        .iter()
        .filter(|ch| ch.toc.unwrap_or(true))
        .collect();

    // Find current chapter
    if let Some(idx) = find_current_chapter(&visible_chapters, current_time) {
        if let Some(chapter) = visible_chapters.get(idx) {
            let title = chapter.title.as_deref().unwrap_or("Untitled");
            return rsx! {
                div {
                    class: "flex items-center gap-2 text-xs text-muted-foreground truncate",
                    span {
                        class: "text-primary",
                        dangerous_inner_html: icons::MUSIC_NOTE
                    }
                    span {
                        class: "truncate",
                        "{title}"
                    }
                }
            };
        }
    }

    rsx! {}
}
