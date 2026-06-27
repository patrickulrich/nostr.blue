use crate::components::{
    ExploreAlbumCard, ExploreAlbumCardSkeleton, ExploreArtistCard, ExploreArtistCardSkeleton,
    ExploreTrackCard, ExploreTrackCardSkeleton, PlaylistCard,
};
use crate::routes::Route;
use crate::services::music_explore::{self, ExploreOverview};
use crate::stores::music_player::{self, MusicTrack};
use crate::stores::nostr_music;
use crate::stores::profiles;
use dioxus::prelude::*;
use std::sync::Arc;

/// Number of cards shown per row in the Explore preview grid.
const PREVIEW_COUNT: usize = 6;

/// Explore tab: nostria-style stacked discovery rows (Songs / Albums /
/// Playlists / Artists), merging Wavlake + Nostr + Podcasting 2.0. Each row
/// previews a handful of cards and links to a full-list sub-route.
#[component]
pub fn MusicExplore() -> Element {
    let mut overview = use_signal(|| None::<ExploreOverview>);
    let mut loading = use_signal(|| true);
    use_effect(move || {
        spawn(async move {
            let data = music_explore::fetch_explore_overview(PREVIEW_COUNT * 2).await;
            overview.set(Some(data));
            loading.set(false);
        });
    });
    let data = overview.read().clone();
    rsx! {
        div { class: "space-y-10",
            SongsRow { loading: *loading.read(), songs: data.as_ref().map(|d| d.songs.clone()).unwrap_or_default() }
            AlbumsRow { loading: *loading.read(), albums: data.as_ref().map(|d| d.albums.clone()).unwrap_or_default() }
            PlaylistsRow { loading: *loading.read(), playlists: data.as_ref().map(|d| d.playlists.clone()).unwrap_or_default() }
            ArtistsRow { loading: *loading.read(), artists: data.as_ref().map(|d| d.artists.clone()).unwrap_or_default() }
            ListeningRow { loading: *loading.read(), entries: data.as_ref().map(|d| d.listening.clone()).unwrap_or_default() }
        }
    }
}

/// Section header with a title, count badge and a "Show all" link.
#[derive(Props, Clone, PartialEq)]
struct RowHeaderProps {
    title: &'static str,
    count: usize,
    show_all: Option<Route>,
}
#[component]
fn RowHeader(props: RowHeaderProps) -> Element {
    rsx! {
        div { class: "flex items-center justify-between mb-3",
            h2 { class: "text-lg font-semibold flex items-center gap-2",
                span { "{props.title}" }
                if props.count > 0 {
                    span { class: "text-xs font-medium px-2 py-0.5 rounded-full bg-primary/10 text-primary",
                        "{props.count}"
                    }
                }
            }
            if let Some(route) = props.show_all {
                Link {
                    to: route,
                    class: "text-sm text-primary hover:underline flex items-center gap-1",
                    "Show all"
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "w-4 h-4",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        stroke_width: "2",
                        path { d: "M9 5l7 7-7 7" }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SongsRowProps {
    loading: bool,
    songs: Vec<MusicTrack>,
}
#[component]
fn SongsRow(props: SongsRowProps) -> Element {
    rsx! {
        section {
            RowHeader {
                title: "Songs",
                count: props.songs.len(),
                show_all: Some(Route::MusicTracks {}),
            }
            if props.loading {
                TrackGridSkeleton { count: PREVIEW_COUNT }
            } else if props.songs.is_empty() {
                EmptyRow { message: "No tracks found yet. Check back later." }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    {
                        let playlist = Arc::new(props.songs.clone());
                        let preview: Vec<&MusicTrack> =
                            props.songs.iter().take(PREVIEW_COUNT).collect();
                        rsx! {
                            for track in preview {
                                ExploreTrackCard {
                                    key: "{track.id}",
                                    track: track.clone(),
                                    playlist: Some(playlist.clone()),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct AlbumsRowProps {
    loading: bool,
    albums: Vec<music_explore::ExploreAlbum>,
}
#[component]
fn AlbumsRow(props: AlbumsRowProps) -> Element {
    rsx! {
        section {
            RowHeader {
                title: "Albums",
                count: props.albums.len(),
                show_all: Some(Route::MusicAlbums {}),
            }
            if props.loading {
                CardGridSkeleton { count: PREVIEW_COUNT }
            } else if props.albums.is_empty() {
                EmptyRow { message: "No albums found yet." }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    for album in props.albums.iter().take(PREVIEW_COUNT) {
                        ExploreAlbumCard {
                            key: "{album_key(album)}",
                            album: album.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct PlaylistsRowProps {
    loading: bool,
    playlists: Vec<crate::stores::nostr_music::NostrPlaylist>,
}
#[component]
fn PlaylistsRow(props: PlaylistsRowProps) -> Element {
    rsx! {
        section {
            RowHeader {
                title: "Playlists",
                count: props.playlists.len(),
                show_all: Some(Route::MusicPlaylists {}),
            }
            if props.loading {
                CardGridSkeleton { count: PREVIEW_COUNT }
            } else if props.playlists.is_empty() {
                EmptyRow { message: "No playlists yet. Be the first to create one!" }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    for playlist in props.playlists.iter().take(PREVIEW_COUNT) {
                        PlaylistCard {
                            key: "{playlist.coordinate}",
                            playlist: playlist.clone(),
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ArtistsRowProps {
    loading: bool,
    artists: Vec<music_explore::ExploreArtist>,
}
#[component]
fn ArtistsRow(props: ArtistsRowProps) -> Element {
    rsx! {
        section {
            RowHeader {
                title: "Artists",
                count: props.artists.len(),
                show_all: Some(Route::MusicArtists {}),
            }
            if props.loading {
                ArtistGridSkeleton { count: PREVIEW_COUNT }
            } else if props.artists.is_empty() {
                EmptyRow { message: "No artists found yet." }
            } else {
                div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
                    for artist in props.artists.iter().take(PREVIEW_COUNT) {
                        ExploreArtistCard {
                            key: "{artist_key(artist)}",
                            artist: artist.clone(),
                        }
                    }
                }
            }
        }
    }
}

fn album_key(album: &music_explore::ExploreAlbum) -> String {
    match album {
        music_explore::ExploreAlbum::Wavlake { id, .. } => format!("wl-{id}"),
        music_explore::ExploreAlbum::Rss { feed_id, .. } => format!("rss-{feed_id}"),
    }
}

fn artist_key(artist: &music_explore::ExploreArtist) -> String {
    match artist {
        music_explore::ExploreArtist::Wavlake { id, .. } => format!("wl-{id}"),
        music_explore::ExploreArtist::Nostr { pubkey } => format!("nostr-{pubkey}"),
        music_explore::ExploreArtist::Rss { name } => format!("rss-{name}"),
    }
}

#[derive(Props, Clone, PartialEq)]
struct EmptyRowProps {
    message: &'static str,
}
#[component]
fn EmptyRow(props: EmptyRowProps) -> Element {
    rsx! {
        div { class: "text-center py-10 text-sm text-muted-foreground", "{props.message}" }
    }
}

#[component]
fn TrackGridSkeleton(count: usize) -> Element {
    rsx! {
        div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
            for i in 0..count {
                ExploreTrackCardSkeleton { key: "{i}" }
            }
        }
    }
}

#[component]
fn CardGridSkeleton(count: usize) -> Element {
    rsx! {
        div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
            for i in 0..count {
                ExploreAlbumCardSkeleton { key: "{i}" }
            }
        }
    }
}

#[component]
fn ArtistGridSkeleton(count: usize) -> Element {
    rsx! {
        div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4",
            for i in 0..count {
                ExploreArtistCardSkeleton { key: "{i}" }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ListeningRowProps {
    loading: bool,
    entries: Vec<music_explore::ListeningEntry>,
}
fn ListeningRow(props: ListeningRowProps) -> Element {
    rsx! {
        section {
            RowHeader {
                title: "Listening",
                count: props.entries.len(),
                show_all: None,
            }
            if props.loading {
                div { class: "text-sm text-muted-foreground py-6", "Loading listening activity…" }
            } else if props.entries.is_empty() {
                div { class: "text-sm text-muted-foreground py-6",
                    "Nobody is broadcasting right now. Play a track to share what you're listening to."
                }
            } else {
                div { class: "flex gap-3 overflow-x-auto pb-2 scrollbar-hide",
                    for entry in props.entries.iter() {
                        ListeningCard { key: "{entry.pubkey}-{entry.created_at}", entry: entry.clone() }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct ListeningCardProps {
    entry: music_explore::ListeningEntry,
}
#[component]
fn ListeningCard(props: ListeningCardProps) -> Element {
    let pubkey = props.entry.pubkey.clone();
    let display = use_resource(move || {
        let pk = pubkey.clone();
        async move { profiles::fetch_profile(pk).await.ok() }
    });
    let display_guard = display.read();
    let (name, picture) = match display_guard.as_ref().and_then(|opt| opt.as_ref()) {
        Some(p) => (
            p.get_display_name(),
            p.picture.clone().filter(|s| !s.is_empty()),
        ),
        None => (
            crate::utils::format::truncate_pubkey(&props.entry.pubkey),
            None,
        ),
    };
    let coordinate = props.entry.coordinate.clone();
    let handle_play = move |_| {
        let Some(coord) = coordinate.clone() else { return };
        spawn(async move {
            let parts: Vec<&str> = coord.splitn(3, ':').collect();
            if parts.len() < 3 {
                return;
            }
            let pubkey = parts[1];
            let d_tag = parts[2];
            if let Ok(Some(track)) =
                nostr_music::fetch_nostr_track_by_coordinate(pubkey, d_tag, Vec::new()).await
            {
                let mt: MusicTrack = track.into();
                music_player::play_or_toggle_track(mt, None, None);
            }
        });
    };
    rsx! {
        div { class: "shrink-0 w-56 rounded-lg border border-border bg-card p-3 hover:border-primary/40 transition",
            div { class: "flex items-center gap-2 mb-2",
                div { class: "w-8 h-8 rounded-full bg-muted overflow-hidden shrink-0",
                    if let Some(ref url) = picture {
                        img {
                            src: "{url}",
                            alt: "{name}",
                            class: "w-full h-full object-cover",
                            referrerpolicy: "no-referrer",
                        }
                    }
                }
                span { class: "text-xs font-medium truncate", "{name}" }
            }
            div { class: "text-xs text-muted-foreground mb-2", "Listening to" }
            div { class: "flex items-center gap-2",
                if props.entry.coordinate.is_some() {
                    button {
                        class: "w-8 h-8 shrink-0 rounded-full bg-primary text-primary-foreground flex items-center justify-center hover:bg-primary/90 transition",
                        title: "Play this track",
                        onclick: handle_play,
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-4 h-4 ml-0.5",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path { d: "M8 5v14l11-7z" }
                        }
                    }
                }
                span { class: "text-sm font-medium truncate", "{props.entry.content}" }
            }
        }
    }
}
