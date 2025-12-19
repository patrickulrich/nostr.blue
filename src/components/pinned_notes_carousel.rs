use dioxus::prelude::*;
use nostr_sdk::Event as NostrEvent;
use crate::routes::Route;
use crate::stores::profiles;
use crate::utils::time::format_relative_time;
use crate::components::icons::PinIcon;

/// Props for the PinnedNotesCarousel component
#[derive(Props, Clone, PartialEq)]
pub struct PinnedNotesCarouselProps {
    /// Pinned note events to display
    pub events: Vec<NostrEvent>,
    /// Whether this is the current user's profile
    #[props(default = false)]
    pub is_own_profile: bool,
}

/// Horizontal carousel displaying pinned notes
#[component]
pub fn PinnedNotesCarousel(props: PinnedNotesCarouselProps) -> Element {
    if props.events.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "flex gap-3 overflow-x-auto pb-2 px-4 scrollbar-styled snap-x snap-mandatory",
            for event in props.events.iter() {
                PinnedNoteCard {
                    key: "{event.id.to_hex()}",
                    event: event.clone()
                }
            }
        }
    }
}

/// Props for the PinnedNoteCard component
#[derive(Props, Clone, PartialEq)]
pub struct PinnedNoteCardProps {
    /// The pinned note event
    pub event: NostrEvent,
}

/// Compact card for a pinned note
#[component]
pub fn PinnedNoteCard(props: PinnedNoteCardProps) -> Element {
    let event = props.event;
    let event_id = event.id.to_hex();
    let author_pubkey = event.pubkey.to_hex();
    let content = event.content.clone();
    let created_at = event.created_at;

    // Fetch author metadata
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // Fetch metadata on mount (get_profile is synchronous, reads from cache)
    use_effect(use_reactive(&author_pubkey, move |pubkey| {
        if let Some(metadata) = profiles::get_profile(&pubkey) {
            author_metadata.set(Some(metadata));
        }
    }));

    // Get display info from metadata
    let author_avatar = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone())
        .unwrap_or_default();

    let author_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| format!("{}...", &author_pubkey[..8]));

    // Truncate content for preview (first 150 chars, UTF-8 safe)
    let preview_content = if content.chars().count() > 150 {
        format!("{}...", content.chars().take(150).collect::<String>())
    } else {
        content.clone()
    };

    rsx! {
        Link {
            to: Route::Note { note_id: event_id.clone(), from_voice: None },
            class: "block min-w-72 max-w-72 bg-card border border-border rounded-lg p-3 hover:bg-accent/50 transition snap-start flex-shrink-0",
            onclick: |e: MouseEvent| e.stop_propagation(),

            // Header: Avatar + Name + Pin icon
            div {
                class: "flex items-center gap-2 mb-2",

                // Small avatar
                if !author_avatar.is_empty() {
                    img {
                        class: "w-6 h-6 rounded-full object-cover",
                        src: "{author_avatar}",
                        alt: "avatar"
                    }
                } else {
                    div {
                        class: "w-6 h-6 rounded-full bg-primary/20 flex items-center justify-center text-xs font-medium text-primary",
                        "{author_name.chars().next().unwrap_or('?').to_uppercase()}"
                    }
                }

                // Author name
                span {
                    class: "text-sm font-medium truncate flex-1 text-foreground",
                    "{author_name}"
                }

                // Pin icon indicator
                PinIcon {
                    class: "w-4 h-4 text-primary flex-shrink-0".to_string(),
                    filled: true
                }
            }

            // Content preview (truncated plain text)
            p {
                class: "text-sm line-clamp-3 text-foreground/80 mb-2 break-words",
                "{preview_content}"
            }

            // Footer: Timestamp
            div {
                class: "text-xs text-muted-foreground",
                "{format_relative_time(created_at)}"
            }
        }
    }
}

/// Loading skeleton for the pinned notes carousel
#[component]
pub fn PinnedNotesCarouselSkeleton() -> Element {
    rsx! {
        div {
            class: "flex gap-3 overflow-x-auto pb-2 px-4",

            // Show 2-3 skeleton cards
            for _ in 0..3 {
                div {
                    class: "min-w-72 max-w-72 bg-card border border-border rounded-lg p-3 animate-pulse flex-shrink-0",

                    // Header skeleton
                    div {
                        class: "flex items-center gap-2 mb-2",
                        div { class: "w-6 h-6 rounded-full bg-muted" }
                        div { class: "h-4 w-24 bg-muted rounded" }
                    }

                    // Content skeleton
                    div { class: "space-y-2 mb-2",
                        div { class: "h-3 w-full bg-muted rounded" }
                        div { class: "h-3 w-full bg-muted rounded" }
                        div { class: "h-3 w-2/3 bg-muted rounded" }
                    }

                    // Timestamp skeleton
                    div { class: "h-3 w-12 bg-muted rounded" }
                }
            }
        }
    }
}
