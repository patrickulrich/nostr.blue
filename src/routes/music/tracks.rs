use crate::components::{ExploreTrackCard, ExploreTrackCardSkeleton};
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;
use crate::services::music_explore;
use crate::stores::music_player::MusicTrack;
use crate::utils::pagination::safe_cursor_from_timestamps;
use dioxus::prelude::*;
use nostr_sdk::Timestamp;
use std::collections::HashSet;
use std::sync::Arc;

const PAGE_SIZE: usize = 100;

const GENRES: &[&str] = &[
    "all",
    "Rock",
    "Pop",
    "Hip-Hop",
    "Electronic",
    "Folk",
    "Jazz",
    "Classical",
    "Blues",
    "Country",
    "Reggae",
    "Punk",
    "Metal",
];

fn genre_opt(genre: &str) -> Option<&str> {
    if genre == "all" {
        None
    } else {
        Some(genre)
    }
}

/// Full Songs browse page (Wavlake + Nostr + Podcasting 2.0), newest first.
/// Genre chips filter Wavlake + Nostr (RSS is unfiltered). Infinite scroll
/// grows the Nostr slice via a `until` cursor; Wavlake/RSS seed page 1 only.
#[component]
pub fn MusicTracks() -> Element {
    let mut tracks = use_signal(Vec::<MusicTrack>::new);
    let mut loading = use_signal(|| true);
    let mut has_more = use_signal(|| true);
    let mut oldest_ts = use_signal(|| None::<u64>);
    let mut req_id = use_signal(|| 0u32);
    let mut selected_genre = use_signal(|| String::from("all"));

    // Page-1 load (all sources). Re-runs on genre change.
    use_effect(move || {
        let genre = selected_genre.read().clone();
        tracks.set(Vec::new());
        oldest_ts.set(None);
        has_more.set(true);
        loading.set(true);
        let id = {
            let mut r = req_id.write();
            *r += 1;
            *r
        };
        spawn(async move {
            let result = music_explore::fetch_explore_songs(PAGE_SIZE, genre_opt(&genre)).await;
            if *req_id.read() != id {
                return;
            }
            let ts: Vec<u64> = result.iter().filter_map(|t| t.created_at).collect();
            oldest_ts.set(safe_cursor_from_timestamps(&ts));
            has_more.set(result.len() >= PAGE_SIZE);
            tracks.set(result);
            loading.set(false);
        });
    });

    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }
        let genre = selected_genre.read().clone();
        let Some(until_secs) = *oldest_ts.read() else {
            return;
        };
        loading.set(true);
        let id = {
            let mut r = req_id.write();
            *r += 1;
            *r
        };
        spawn(async move {
            let more = music_explore::fetch_more_nostr_tracks(
                PAGE_SIZE,
                genre_opt(&genre),
                Timestamp::from(until_secs),
            )
            .await;
            if *req_id.read() != id {
                return;
            }
            if more.is_empty() {
                has_more.set(false);
            } else {
                let ts: Vec<u64> = more.iter().filter_map(|t| t.created_at).collect();
                if let Some(new_cursor) = safe_cursor_from_timestamps(&ts) {
                    oldest_ts.set(Some(new_cursor));
                }
                has_more.set(more.len() >= PAGE_SIZE);
                let mut current = tracks.read().clone();
                let existing: HashSet<String> = current.iter().map(|t| t.id.clone()).collect();
                for t in more {
                    if !existing.contains(&t.id) {
                        current.push(t);
                    }
                }
                tracks.set(current);
            }
            loading.set(false);
        });
    };

    let sentinel_id = use_infinite_scroll(load_more, has_more, loading);

    rsx! {
        div { class: "max-w-5xl mx-auto p-4 space-y-6",
            Link {
                to: Route::MusicHome {},
                class: "inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground transition",
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    class: "w-4 h-4",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path { d: "M15 19l-7-7 7-7" }
                }
                "Back to Music"
            }
            h1 { class: "text-2xl font-bold", "All Songs" }
            // Genre filter chips.
            div { class: "flex gap-2 overflow-x-auto pb-2 scrollbar-hide",
                for genre in GENRES {
                    {
                        let g = genre.to_string();
                        let is_selected = *selected_genre.read() == g;
                        rsx! {
                            button {
                                key: "{g}",
                                class: if is_selected { "px-3 py-1.5 rounded-full text-xs font-medium bg-primary text-primary-foreground whitespace-nowrap shrink-0" } else { "px-3 py-1.5 rounded-full text-xs font-medium bg-muted hover:bg-muted/80 text-foreground whitespace-nowrap shrink-0" },
                                onclick: move |_| selected_genre.set(g.clone()),
                                "{g}"
                            }
                        }
                    }
                }
            }
            if *loading.read() && tracks.read().is_empty() {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    for i in 0..12 {
                        ExploreTrackCardSkeleton { key: "{i}" }
                    }
                }
            } else if tracks.read().is_empty() {
                div { class: "text-center py-16 text-muted-foreground",
                    "No tracks found right now. Check back later."
                }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    {
                        let playlist = Arc::new(tracks.read().clone());
                        rsx! {
                            for track in playlist.iter() {
                                ExploreTrackCard {
                                    key: "{track.id}",
                                    track: track.clone(),
                                    playlist: Some(playlist.clone()),
                                }
                            }
                        }
                    }
                }
                if *has_more.read() {
                    div { id: "{sentinel_id}", class: "h-20 flex items-center justify-center",
                        div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4 w-full",
                            for i in 0..6 {
                                ExploreTrackCardSkeleton { key: "{i}" }
                            }
                        }
                    }
                } else {
                    div { class: "text-center py-8 text-muted-foreground text-sm", "You've reached the end" }
                }
            }
        }
    }
}
