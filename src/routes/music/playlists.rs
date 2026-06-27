use crate::components::{PlaylistCard, PlaylistCardSkeleton};
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;
use crate::services::music_explore;
use crate::stores::nostr_music::NostrPlaylist;
use crate::utils::pagination::safe_cursor_from_timestamps;
use dioxus::prelude::*;
use nostr_sdk::Timestamp;
use std::collections::HashSet;

const PAGE_SIZE: usize = 24;

/// Full Playlists browse page (Nostr kind 34139) with `until`-cursor infinite
/// scroll.
#[component]
pub fn MusicPlaylists() -> Element {
    let mut playlists = use_signal(Vec::<NostrPlaylist>::new);
    let mut loading = use_signal(|| true);
    let mut has_more = use_signal(|| true);
    let mut oldest_ts = use_signal(|| None::<u64>);
    let mut req_id = use_signal(|| 0u32);

    // Page-1 load.
    use_effect(move || {
        playlists.set(Vec::new());
        oldest_ts.set(None);
        has_more.set(true);
        loading.set(true);
        let id = {
            let mut r = req_id.write();
            *r += 1;
            *r
        };
        spawn(async move {
            let result = music_explore::fetch_explore_playlists(PAGE_SIZE, None).await;
            if *req_id.read() != id {
                return;
            }
            let ts: Vec<u64> = result.iter().map(|p| p.created_at).collect();
            oldest_ts.set(safe_cursor_from_timestamps(&ts));
            has_more.set(result.len() >= PAGE_SIZE);
            playlists.set(result);
            loading.set(false);
        });
    });

    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }
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
            let more =
                music_explore::fetch_explore_playlists(PAGE_SIZE, Some(Timestamp::from(until_secs)))
                    .await;
            if *req_id.read() != id {
                return;
            }
            if more.is_empty() {
                has_more.set(false);
            } else {
                let ts: Vec<u64> = more.iter().map(|p| p.created_at).collect();
                if let Some(new_cursor) = safe_cursor_from_timestamps(&ts) {
                    oldest_ts.set(Some(new_cursor));
                }
                has_more.set(more.len() >= PAGE_SIZE);
                let mut current = playlists.read().clone();
                let existing: HashSet<String> = current.iter().map(|p| p.coordinate.clone()).collect();
                for p in more {
                    if !existing.contains(&p.coordinate) {
                        current.push(p);
                    }
                }
                playlists.set(current);
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
            h1 { class: "text-2xl font-bold", "All Playlists" }
            if *loading.read() && playlists.read().is_empty() {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    for i in 0..12 {
                        PlaylistCardSkeleton { key: "{i}" }
                    }
                }
            } else if playlists.read().is_empty() {
                div { class: "text-center py-16 text-muted-foreground",
                    "No playlists yet. Be the first to create one!"
                }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    for playlist in playlists.read().iter() {
                        PlaylistCard {
                            key: "{playlist.coordinate}",
                            playlist: playlist.clone(),
                        }
                    }
                }
                if *has_more.read() {
                    div { id: "{sentinel_id}", class: "h-20 flex items-center justify-center",
                        div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4 w-full",
                            for i in 0..6 {
                                PlaylistCardSkeleton { key: "{i}" }
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
