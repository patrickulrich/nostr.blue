//! Podcast Soundbites Component
//!
//! Displays soundbite preview clips for podcast episodes with:
//! - Play preview functionality
//! - Share soundbite links
//! - Duration display
//! - Seek to soundbite position

use dioxus::prelude::*;
use crate::utils::podcast::Soundbite;
use crate::utils::safe_duration_millis;
use crate::services::podcast_rss::format_duration;
use crate::stores::music_player;
use crate::components::icons;

// ============================================================================
// Soundbites List Component
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct PodcastSoundbitesProps {
    /// Available soundbites
    pub soundbites: Vec<Soundbite>,
    /// Episode title (for sharing)
    #[props(default)]
    pub episode_title: Option<String>,
    /// Callback when a soundbite is clicked
    #[props(default)]
    pub on_play: Option<EventHandler<Soundbite>>,
    /// Show in compact mode
    #[props(default = false)]
    pub compact: bool,
}

/// Podcast soundbites preview clips component
#[component]
pub fn PodcastSoundbites(props: PodcastSoundbitesProps) -> Element {
    if props.soundbites.is_empty() {
        return rsx! {
            div {
                class: "text-center py-4 text-muted-foreground text-sm",
                "No soundbites available for this episode."
            }
        };
    }

    rsx! {
        div {
            class: "space-y-2",

            // Header
            div {
                class: "flex items-center gap-2 text-sm font-medium text-muted-foreground pb-2",
                span {
                    class: "w-4 h-4",
                    dangerous_inner_html: icons::MUSIC_NOTE
                }
                "Preview Clips ({props.soundbites.len()})"
            }

            // Soundbites list
            div {
                class: "space-y-1",
                for (idx, soundbite) in props.soundbites.iter().enumerate() {
                    SoundbiteCard {
                        key: "{idx}",
                        soundbite: soundbite.clone(),
                        index: idx,
                        episode_title: props.episode_title.clone(),
                        on_play: props.on_play,
                        compact: props.compact
                    }
                }
            }
        }
    }
}

// ============================================================================
// Single Soundbite Card
// ============================================================================

#[derive(Props, Clone, PartialEq)]
struct SoundbiteCardProps {
    soundbite: Soundbite,
    #[props(default = 0)]
    index: usize,
    #[props(default)]
    episode_title: Option<String>,
    on_play: Option<EventHandler<Soundbite>>,
    #[props(default = false)]
    compact: bool,
}

#[component]
fn SoundbiteCard(props: SoundbiteCardProps) -> Element {
    let mut is_playing = use_signal(|| false);
    let soundbite = props.soundbite.clone();

    let title = soundbite.title.clone().unwrap_or_else(|| {
        format!("Clip {}", props.index + 1)
    });

    let start_str = format_duration(soundbite.start_time as u64);
    let duration_str = format!("{}s", soundbite.duration as u32);

    let handle_play = {
        let sb = soundbite.clone();
        let on_play = props.on_play;
        move |_| {
            if let Some(handler) = &on_play {
                handler.call(sb.clone());
            } else {
                // Default: seek to soundbite position
                music_player::seek_to(sb.start_time);
            }
            is_playing.set(true);
            // Reset playing state after duration (using safe conversion to prevent overflow)
            let duration = sb.duration;
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(safe_duration_millis(duration)).await;
                is_playing.set(false);
            });
        }
    };

    let base_class = if *is_playing.read() {
        "flex items-center gap-3 p-3 rounded-lg bg-primary/10 border border-primary/30 transition"
    } else {
        "flex items-center gap-3 p-3 rounded-lg bg-muted/50 hover:bg-muted transition"
    };

    if props.compact {
        // Compact inline view
        rsx! {
            div {
                class: "flex items-center gap-2 text-sm",

                // Play button
                button {
                    class: "h-6 w-6 p-0 inline-flex items-center justify-center rounded-full bg-primary hover:bg-primary/90 text-primary-foreground transition-colors shrink-0",
                    onclick: handle_play.clone(),
                    title: "Play clip",
                    if *is_playing.read() {
                        span { class: "w-3 h-3", dangerous_inner_html: icons::PAUSE }
                    } else {
                        span { class: "w-3 h-3", dangerous_inner_html: icons::PLAY }
                    }
                }

                // Title
                span {
                    class: "truncate flex-1",
                    "{title}"
                }

                // Duration badge
                span {
                    class: "text-xs text-muted-foreground font-mono",
                    "{duration_str}"
                }
            }
        }
    } else {
        // Full card view
        rsx! {
            div {
                class: "{base_class}",

                // Play button
                button {
                    class: "h-10 w-10 p-0 inline-flex items-center justify-center rounded-full bg-primary hover:bg-primary/90 text-primary-foreground transition-colors shrink-0",
                    onclick: handle_play.clone(),
                    title: "Play clip",
                    span {
                        dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY }
                    }
                }

                // Info
                div {
                    class: "flex-1 min-w-0",

                    // Title
                    div {
                        class: "font-medium text-sm truncate",
                        "{title}"
                    }

                    // Timestamp and duration
                    div {
                        class: "flex items-center gap-2 text-xs text-muted-foreground",
                        span {
                            class: "font-mono",
                            "at {start_str}"
                        }
                        span { "•" }
                        span {
                            "{duration_str} clip"
                        }
                    }
                }

                // Playing indicator
                if *is_playing.read() {
                    div {
                        class: "flex gap-0.5 items-end h-4",
                        for i in 0..3 {
                            div {
                                key: "{i}",
                                class: "w-1 bg-primary rounded-full animate-pulse",
                                style: "height: {8 + (i * 4)}px; animation-delay: {i * 100}ms;"
                            }
                        }
                    }
                }

                // Share button
                button {
                    class: "h-8 w-8 p-0 inline-flex items-center justify-center rounded-md hover:bg-accent hover:text-accent-foreground transition-colors shrink-0",
                    title: "Share clip",
                    onclick: {
                        let sb = soundbite.clone();
                        let ep_title = props.episode_title.clone();
                        move |_| {
                            share_soundbite(&sb, ep_title.as_deref());
                        }
                    },
                    dangerous_inner_html: icons::SHARE
                }
            }
        }
    }
}

// ============================================================================
// Soundbite Grid (for discovery)
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct SoundbiteGridProps {
    /// Soundbites to display
    pub soundbites: Vec<SoundbiteWithMeta>,
    /// Number of columns (default 2)
    #[props(default = 2)]
    pub columns: u8,
}

/// Soundbite with additional metadata for grid display
#[derive(Clone, Debug, PartialEq)]
pub struct SoundbiteWithMeta {
    pub soundbite: Soundbite,
    pub episode_title: String,
    pub podcast_title: String,
    pub image_url: Option<String>,
}

/// Grid of soundbites for discovery/browse view
#[component]
pub fn SoundbiteGrid(props: SoundbiteGridProps) -> Element {
    if props.soundbites.is_empty() {
        return rsx! {
            div {
                class: "text-center py-8 text-muted-foreground",
                "No soundbites to display."
            }
        };
    }

    let grid_class = match props.columns {
        1 => "grid grid-cols-1 gap-3",
        3 => "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3",
        _ => "grid grid-cols-1 sm:grid-cols-2 gap-3",
    };

    rsx! {
        div {
            class: "{grid_class}",
            for (idx, item) in props.soundbites.iter().enumerate() {
                SoundbiteGridCard {
                    key: "{idx}",
                    item: item.clone()
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SoundbiteGridCardProps {
    item: SoundbiteWithMeta,
}

#[component]
fn SoundbiteGridCard(props: SoundbiteGridCardProps) -> Element {
    let mut is_playing = use_signal(|| false);
    let item = &props.item;
    let soundbite = &item.soundbite;

    let title = soundbite.title.clone().unwrap_or_else(|| "Clip".to_string());
    let duration_str = format!("{}s", soundbite.duration as u32);

    let handle_play = {
        let sb = soundbite.clone();
        move |_| {
            music_player::seek_to(sb.start_time);
            is_playing.set(true);
            let duration = sb.duration;
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(safe_duration_millis(duration)).await;
                is_playing.set(false);
            });
        }
    };

    rsx! {
        div {
            class: "group relative rounded-lg overflow-hidden bg-muted/50 hover:bg-muted transition cursor-pointer",
            onclick: handle_play,

            // Background image (if available)
            if let Some(ref img_url) = item.image_url {
                div {
                    class: "absolute inset-0",
                    img {
                        src: "{img_url}",
                        alt: "",
                        class: "w-full h-full object-cover opacity-20"
                    }
                }
            }

            // Content
            div {
                class: "relative p-4",

                // Play indicator
                div {
                    class: "flex items-center justify-center mb-3",
                    div {
                        class: "h-12 w-12 rounded-full bg-primary/90 group-hover:bg-primary flex items-center justify-center text-primary-foreground transition shadow-lg",
                        span {
                            dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY }
                        }
                    }
                }

                // Title
                div {
                    class: "font-semibold text-sm truncate mb-1",
                    "{title}"
                }

                // Episode title
                div {
                    class: "text-xs text-muted-foreground truncate mb-1",
                    "{item.episode_title}"
                }

                // Podcast title and duration
                div {
                    class: "flex items-center justify-between text-xs text-muted-foreground",
                    span {
                        class: "truncate",
                        "{item.podcast_title}"
                    }
                    span {
                        class: "font-mono shrink-0",
                        "{duration_str}"
                    }
                }
            }

            // Playing animation overlay
            if *is_playing.read() {
                div {
                    class: "absolute top-2 right-2",
                    div {
                        class: "flex gap-0.5 items-end h-4",
                        for i in 0..3 {
                            div {
                                key: "{i}",
                                class: "w-1 bg-primary rounded-full animate-pulse",
                                style: "height: {6 + (i * 3)}px; animation-delay: {i * 100}ms;"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Share a soundbite (Web Share API or clipboard)
fn share_soundbite(soundbite: &Soundbite, episode_title: Option<&str>) {
    let title = soundbite.title.clone().unwrap_or_else(|| "Podcast clip".to_string());
    let text = if let Some(ep) = episode_title {
        format!("{} - {}", title, ep)
    } else {
        title
    };

    // Log the share action - actual sharing would be handled via clipboard or share modal
    log::info!("Sharing soundbite: {}", text);

    // Copy to clipboard as fallback
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::window;

        if let Some(window) = window() {
            let clipboard = window.navigator().clipboard();
            let _ = clipboard.write_text(&text);
        }
    }
}

// ============================================================================
// Featured Soundbite (Hero)
// ============================================================================

#[derive(Props, Clone, PartialEq)]
pub struct FeaturedSoundbiteProps {
    /// The soundbite to feature
    pub soundbite: Soundbite,
    /// Episode title
    pub episode_title: String,
    /// Podcast title
    pub podcast_title: String,
    /// Cover image URL
    #[props(default)]
    pub image_url: Option<String>,
    /// Callback when played
    #[props(default)]
    pub on_play: Option<EventHandler<Soundbite>>,
}

/// Large featured soundbite card for hero/discovery sections
#[component]
pub fn FeaturedSoundbite(props: FeaturedSoundbiteProps) -> Element {
    let mut is_playing = use_signal(|| false);
    let soundbite = props.soundbite.clone();

    let title = soundbite.title.clone().unwrap_or_else(|| "Featured Clip".to_string());
    let duration_str = format!("{} seconds", soundbite.duration as u32);

    let handle_play = {
        let sb = soundbite.clone();
        let on_play = props.on_play;
        move |_| {
            if let Some(handler) = &on_play {
                handler.call(sb.clone());
            } else {
                music_player::seek_to(sb.start_time);
            }
            is_playing.set(true);
            let duration = sb.duration;
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(safe_duration_millis(duration)).await;
                is_playing.set(false);
            });
        }
    };

    rsx! {
        div {
            class: "relative rounded-xl overflow-hidden bg-gradient-to-br from-primary/20 to-primary/5 border border-primary/20",

            div {
                class: "flex items-center gap-4 p-6",

                // Cover image
                div {
                    class: "relative w-24 h-24 rounded-lg overflow-hidden bg-muted shrink-0 shadow-lg",
                    if let Some(ref img_url) = props.image_url {
                        img {
                            src: "{img_url}",
                            alt: "Cover",
                            class: "w-full h-full object-cover"
                        }
                    }
                    // Play overlay
                    button {
                        class: "absolute inset-0 flex items-center justify-center bg-black/40 hover:bg-black/50 transition",
                        onclick: handle_play,
                        div {
                            class: "h-12 w-12 rounded-full bg-primary flex items-center justify-center text-primary-foreground shadow-lg",
                            span {
                                dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY }
                            }
                        }
                    }
                }

                // Info
                div {
                    class: "flex-1 min-w-0",

                    // Featured badge
                    div {
                        class: "inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-primary/20 text-primary text-xs font-medium mb-2",
                        span { class: "w-3 h-3", dangerous_inner_html: icons::MUSIC_NOTE }
                        "Soundbite"
                    }

                    // Title
                    h3 {
                        class: "font-bold text-lg truncate mb-1",
                        "{title}"
                    }

                    // Episode
                    p {
                        class: "text-sm text-muted-foreground truncate mb-1",
                        "{props.episode_title}"
                    }

                    // Podcast and duration
                    div {
                        class: "flex items-center gap-2 text-xs text-muted-foreground",
                        span { "{props.podcast_title}" }
                        span { "•" }
                        span { "{duration_str}" }
                    }
                }
            }

            // Playing indicator
            if *is_playing.read() {
                div {
                    class: "absolute bottom-0 left-0 right-0 h-1 bg-muted overflow-hidden",
                    div {
                        class: "h-full bg-primary animate-pulse"
                    }
                }
            }
        }
    }
}
