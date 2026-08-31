use crate::components::{UnifiedTrackCard, UnifiedTrackCardSkeleton};
use crate::services::wavlake::WavlakeAPI;
use crate::stores::auth_store;
use crate::stores::music_library::{self, MusicLibraryItem};
use crate::stores::music_player::MusicTrack;
use crate::stores::nostr_client;
use crate::stores::nostr_music::{self, TrackSource};
use dioxus::prelude::*;
use std::sync::Arc;

/// Library tab — shows the logged-in user's saved tracks (NIP-51 music-library
/// list). Mirrors the podcast Library tab: auth-gate empty state, reactive read
/// of the store, per-item hydration to a playable `MusicTrack`.
#[component]
pub fn MusicLibrarySection() -> Element {
    if !auth_store::is_authenticated() {
        return rsx! {
            div { class: "text-center py-16 space-y-4",
                div { class: "text-4xl", "📚" }
                h3 { class: "text-lg font-semibold", "Sign in to view your library" }
                p { class: "text-muted-foreground text-sm max-w-sm mx-auto",
                    "Save tracks with the + button and access them here."
                }
            }
        };
    }
    let items = use_memo(music_library::get_items);
    let is_loading = music_library::is_loading();
    let is_loaded = music_library::is_loaded();
    // Kick off a fetch on first mount if not yet loaded.
    use_effect(move || {
        if auth_store::is_authenticated() && !is_loaded && !is_loading {
            spawn(async move {
                if let Err(e) = music_library::fetch_music_library().await {
                    log::error!("Failed to fetch music library: {}", e);
                }
            });
        }
    });
    if is_loading && items.read().is_empty() {
        return rsx! {
            div { class: "space-y-1",
                for i in 0..6 {
                    UnifiedTrackCardSkeleton { key: "{i}" }
                }
            }
        };
    }
    let mut show_downloads = use_signal(|| false);
    let downloads_active = *show_downloads.read();
    if items.read().is_empty() && !downloads_active {
        return rsx! {
            div { class: "text-center py-16 space-y-4",
                div { class: "text-4xl", "🎵" }
                h3 { class: "text-lg font-semibold", "Your library is empty" }
                p { class: "text-muted-foreground text-sm max-w-sm mx-auto",
                    "Tap the + button on any track to save it here."
                }
            }
        };
    }
    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center justify-between gap-2 py-2",
                span { class: "text-sm text-muted-foreground",
                    "{items.read().len()} saved track(s)"
                }
                crate::components::downloads::DownloadedFilterChip {
                    active: downloads_active,
                    ontoggle: move |_| show_downloads.toggle(),
                }
            }
            if downloads_active {
                crate::components::downloads::DownloadedMusicList {}
            } else {
                div { class: "divide-y divide-border/50",
                    for item in items.read().iter() {
                        LibraryTrackRow {
                            key: "{item.key()}",
                            item: item.clone(),
                        }
                    }
                }
            }
        }
    }
}

/// A single saved track, hydrated to a playable `MusicTrack` from its source.
#[derive(Props, Clone, PartialEq)]
struct LibraryTrackRowProps {
    item: MusicLibraryItem,
}

#[component]
fn LibraryTrackRow(props: LibraryTrackRowProps) -> Element {
    let item = props.item.clone();
    let item_for_fallback = item.clone();
    let track_resource = use_resource(move || {
        let item = item.clone();
        let client_ready = nostr_client::get_client().is_some();
        async move {
            hydrate_track(&item, client_ready).await
        }
    });
    let result = track_resource.read();
    match result.as_ref().and_then(|opt| opt.as_ref()) {
        Some(track) => {
            let playlist = Arc::new(vec![track.clone()]);
            rsx! {
                UnifiedTrackCard {
                    key: "{track.id}",
                    track: track.clone(),
                    show_album: true,
                    show_sats: true,
                    playlist: Some(playlist),
                }
            }
        }
        None => {
            // Still loading, or hydration unavailable: show a minimal card
            // from the cached display fields so the row isn't blank.
            let title = item_for_fallback
                .title
                .clone()
                .unwrap_or_else(|| "Saved track".to_string());
            rsx! {
                UnifiedTrackCardSkeleton { }
                div { class: "sr-only", "{title}" }
            }
        }
    }
}

/// Resolve a saved library item back into a playable `MusicTrack` from its
/// source. Returns `None` while loading or when the source can't be reached.
async fn hydrate_track(item: &MusicLibraryItem, client_ready: bool) -> Option<MusicTrack> {
    match &item.source {
        TrackSource::Nostr { pubkey, d_tag, .. } => {
            if !client_ready {
                return None;
            }
            nostr_music::fetch_nostr_track_by_coordinate(pubkey, d_tag, Vec::new())
                .await
                .ok()
                .flatten()
                .map(MusicTrack::from)
        }
        TrackSource::Wavlake { .. } => {
            let api = WavlakeAPI::new();
            match api.get_track(&item.track_id).await {
                Ok(wt) => Some(wt.into()),
                Err(_) => fallback_track(item),
            }
        }
        // Sources without a clean single-call hydration: show cached metadata.
        _ => fallback_track(item),
    }
}

/// Build a minimal, possibly non-playable `MusicTrack` from cached display
/// fields (used when the source API can't be reached for an item).
fn fallback_track(item: &MusicLibraryItem) -> Option<MusicTrack> {
    let title = item.title.clone()?;
    Some(MusicTrack {
        id: item.track_id.clone(),
        title,
        artist: item.artist.clone().unwrap_or_default(),
        album: None,
        media_url: String::new(),
        album_art_url: item.album_art_url.clone(),
        artist_art_url: None,
        duration: None,
        artist_id: None,
        album_id: None,
        artist_npub: None,
        source: item.source.clone(),
        msat_total: None,
        created_at: None,
        is_podcast: false,
        is_live_stream: false,
        value_block: None,
        chapters_url: None,
        transcripts: Vec::new(),
    })
}
