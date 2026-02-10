use crate::components::icons;
use crate::components::live::stream_card::LiveStreamCard;
use crate::routes::Route;
use crate::services::podcast_index;
use crate::services::wavlake::WavlakeAPI;
use crate::stores::music_player::{self, MusicTrack};
use crate::stores::nostr_client;
use crate::stores::nostr_music::TrackSource;
use crate::utils::markdown::sanitize_html;
use crate::utils::recipe::{extract_metadata as extract_recipe_metadata, is_recipe_event};
use dioxus::prelude::*;
use nostr_sdk::nips::nip19::Nip19;
use nostr_sdk::{Event, FromBech32};
use std::rc::Rc;
use url::Url;

use super::minicards::render_recipe_minicard;

#[component]
pub(super) fn TwitterTweetRenderer(tweet_id: String) -> Element {
    let tweet_url = format!("https://twitter.com/i/status/{}", tweet_id);
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden border border-border bg-card p-4",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            "data-tweet-id": "{tweet_id}",
            blockquote {
                class: "twitter-tweet",
                "data-theme": "dark",
                "data-dnt": "true",
                p { "Loading tweet..." }
                a { href: "{tweet_url}", "View tweet" }
            }
        }
    }
}

#[component]
pub(super) fn TwitchStreamRenderer(channel: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let parent_domain = web_sys::window()
        .and_then(|w| w.location().hostname().ok())
        .unwrap_or_else(|| "localhost".to_string());
    let embed_url = format!(
        "https://player.twitch.tv/?channel={}&parent={}",
        channel,
        parent_domain,
    );
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden border border-border",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            "data-twitch-visible": "{is_visible}",
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    class: "w-full aspect-video",
                    allowfullscreen: true,
                }
            } else {
                div {
                    class: "w-full aspect-video bg-card flex items-center justify-center cursor-pointer",
                    role: "button",
                    tabindex: "0",
                    onclick: move |_| is_visible.set(true),
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if activate {
                            evt.prevent_default();
                            is_visible.set(true);
                        }
                    },
                    div { class: "text-center",
                        div { class: "text-purple-500 text-4xl mb-2", "▶" }
                        div { class: "text-lg font-medium", "Watch {channel} on Twitch" }
                        div { class: "text-sm text-muted-foreground mt-1", "Click to load stream" }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn TwitchClipRenderer(clip_slug: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let parent_domain = web_sys::window()
        .and_then(|w| w.location().hostname().ok())
        .unwrap_or_else(|| "localhost".to_string());
    let embed_url = format!(
        "https://clips.twitch.tv/embed?clip={}&parent={}",
        clip_slug,
        parent_domain,
    );
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden border border-border",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            "data-twitch-visible": "{is_visible}",
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    class: "w-full aspect-video",
                    allowfullscreen: true,
                }
            } else {
                div {
                    class: "w-full aspect-video bg-card flex items-center justify-center cursor-pointer",
                    role: "button",
                    tabindex: "0",
                    onclick: move |_| is_visible.set(true),
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if activate {
                            evt.prevent_default();
                            is_visible.set(true);
                        }
                    },
                    div { class: "text-center",
                        div { class: "text-purple-500 text-4xl mb-2", "▶" }
                        div { class: "text-lg font-medium", "Watch Twitch Clip" }
                        div { class: "text-sm text-muted-foreground mt-1", "Click to load clip" }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn TwitchVodRenderer(vod_id: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let parent_domain = web_sys::window()
        .and_then(|w| w.location().hostname().ok())
        .unwrap_or_else(|| "localhost".to_string());
    let embed_url = format!(
        "https://player.twitch.tv/?video={}&parent={}",
        vod_id,
        parent_domain,
    );
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden border border-border",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            "data-twitch-visible": "{is_visible}",
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    class: "w-full aspect-video",
                    allowfullscreen: true,
                }
            } else {
                div {
                    class: "w-full aspect-video bg-card flex items-center justify-center cursor-pointer",
                    role: "button",
                    tabindex: "0",
                    onclick: move |_| is_visible.set(true),
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if activate {
                            evt.prevent_default();
                            is_visible.set(true);
                        }
                    },
                    div { class: "text-center",
                        div { class: "text-purple-500 text-4xl mb-2", "▶" }
                        div { class: "text-lg font-medium", "Watch Twitch VOD" }
                        div { class: "text-sm text-muted-foreground mt-1", "Click to load video" }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn WavlakeTrackRenderer(track_id: String) -> Element {
    let track_resource = use_resource(move || {
        let id = track_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_track(&id).await
        }
    });
    match track_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-3",
                        div { class: "w-16 h-16 bg-muted rounded" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-4 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/2" }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            rsx! {
                div {
                    class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-2 text-red-500 text-sm",
                        icons::MusicIcon { class: "w-4 h-4" }
                        span { "Unable to load track: {e}" }
                    }
                }
            }
        }
        Some(Ok(track)) => {
            let track_clone = track.clone();
            let handle_play = move |_: MouseEvent| {
                let music_track: MusicTrack = track_clone.clone().into();
                music_player::play_track(music_track, None, None);
            };
            rsx! {
                div {
                    class: "my-2 border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition bg-card",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-4 p-4",
                        div { class: "relative w-16 h-16 shrink-0 rounded overflow-hidden bg-muted group",
                            img {
                                src: "{track.album_art_url}",
                                alt: "Album art",
                                class: "w-full h-full object-cover",
                            }
                            button {
                                class: "absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition",
                                onclick: handle_play,
                                dangerous_inner_html: icons::PLAY,
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "font-semibold text-sm truncate", "{track.title}" }
                            div { class: "text-xs text-muted-foreground truncate",
                                Link {
                                    to: Route::MusicArtist {
                                        artist_id: track.artist_id.clone(),
                                    },
                                    class: "hover:text-foreground hover:underline",
                                    onclick: move |e: dioxus::prelude::Event<MouseData>| e.stop_propagation(),
                                    "{track.artist}"
                                }
                            }
                            div { class: "text-xs text-muted-foreground/80 truncate mt-1",
                                Link {
                                    to: Route::MusicAlbum {
                                        album_id: track.album_id.clone(),
                                    },
                                    class: "hover:text-foreground hover:underline",
                                    onclick: move |e: dioxus::prelude::Event<MouseData>| e.stop_propagation(),
                                    "{track.album_title}"
                                }
                            }
                        }
                        div { class: "flex flex-col items-end gap-1 shrink-0",
                            div { class: "text-xs text-muted-foreground",
                                {
                                    let mins = track.duration / 60;
                                    let secs = track.duration % 60;
                                    format!("{:02}:{:02}", mins, secs)
                                }
                            }
                            div { class: "flex items-center gap-1 text-xs text-purple-400",
                                icons::MusicIcon { class: "w-3 h-3" }
                                "Wavlake"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn WavlakeAlbumRenderer(album_id: String) -> Element {
    let album_resource = use_resource(move || {
        let id = album_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_album(&id).await
        }
    });
    match album_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex gap-4",
                        div { class: "w-32 h-32 bg-muted rounded" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-5 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/2" }
                            div { class: "h-3 bg-muted rounded w-1/3" }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            rsx! {
                div {
                    class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-2 text-red-500 text-sm",
                        icons::DiscIcon { class: "w-4 h-4" }
                        span { "Unable to load album: {e}" }
                    }
                }
            }
        }
        Some(Ok(album)) => {
            let tracks: Vec<MusicTrack> = album
                .tracks
                .iter()
                .map(|track| track.clone().into())
                .collect();
            rsx! {
                div {
                    class: "my-2 border border-border rounded-lg overflow-hidden bg-card",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex gap-4 p-4 border-b border-border",
                        if let Some(art_url) = &album.album_art_url {
                            img {
                                src: "{art_url}",
                                alt: "Album art",
                                class: "w-32 h-32 rounded object-cover shrink-0",
                            }
                        } else {
                            div { class: "w-32 h-32 rounded bg-muted flex items-center justify-center shrink-0",
                                icons::DiscIcon { class: "w-16 h-16 text-muted-foreground" }
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "text-xs text-muted-foreground mb-1", "ALBUM" }
                            div { class: "font-bold text-lg truncate mb-1", "{album.title}" }
                            div { class: "text-sm text-muted-foreground truncate mb-2",
                                if let Some(first_track) = album.tracks.first() {
                                    Link {
                                        to: Route::MusicArtist { artist_id: first_track.artist_id.clone() },
                                        class: "hover:text-foreground hover:underline",
                                        onclick: move |e: MouseEvent| e.stop_propagation(),
                                        "{album.artist}"
                                    }
                                } else {
                                    span { class: "text-muted-foreground", "{album.artist}" }
                                }
                            }
                            div { class: "flex items-center gap-3 text-xs text-muted-foreground",
                                span {
                                    {
                                        album
                                            .release_date
                                            .split('T')
                                            .next()
                                            .unwrap_or("Unknown")
                                            .split('-')
                                            .next()
                                            .unwrap_or("Unknown")
                                    }
                                }
                                span { "•" }
                                span {
                                    "{album.tracks.len()} "
                                    {if album.tracks.len() == 1 { "track" } else { "tracks" }}
                                }
                                span { "•" }
                                span { class: "flex items-center gap-1 text-purple-400",
                                    icons::MusicIcon { class: "w-3 h-3" }
                                    "Wavlake"
                                }
                            }
                        }
                    }
                    {
                        let tracks_rc = Rc::new(tracks);
                        rsx! {
                    div { class: "divide-y divide-border",
                        for (index , track_data) in album.tracks.iter().enumerate() {
                            {
                                let track_clone = tracks_rc[index].clone();
                                let playlist_rc = Rc::clone(&tracks_rc);
                                let track_clone_kb = tracks_rc[index].clone();
                                let playlist_rc_kb = Rc::clone(&tracks_rc);
                                let track_title = track_data.title.clone();
                                let track_artist = track_data.artist.clone();
                                let track_duration = track_data.duration;
                                rsx! {
                                    div {
                                        key: "{track_data.id}",
                                        class: "flex items-center gap-3 p-3 hover:bg-accent/10 transition cursor-pointer group",
                                        role: "button",
                                        tabindex: "0",
                                        onclick: move |_| {
                                            music_player::play_track(
                                                track_clone.clone(),
                                                Some((*playlist_rc).clone()),
                                                Some(index),
                                            );
                                        },
                                        onkeydown: move |evt: KeyboardEvent| {
                                            let activate = matches!(evt.key(), Key::Enter)
                                                || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                                            if activate {
                                                evt.prevent_default();
                                                music_player::play_track(
                                                    track_clone_kb.clone(),
                                                    Some((*playlist_rc_kb).clone()),
                                                    Some(index),
                                                );
                                            }
                                        },
                                        div { class: "w-8 text-center text-sm text-muted-foreground shrink-0",
                                            span { class: "group-hover:hidden", "{index + 1}" }
                                            div {
                                                class: "hidden group-hover:flex items-center justify-center",
                                                dangerous_inner_html: icons::PLAY,
                                            }
                                        }
                                        div { class: "flex-1 min-w-0",
                                            div { class: "font-medium text-sm truncate", "{track_title}" }
                                            div { class: "text-xs text-muted-foreground truncate", "{track_artist}" }
                                        }
                                        div { class: "text-xs text-muted-foreground shrink-0",
                                            {
                                                let mins = track_duration / 60;
                                                let secs = track_duration % 60;
                                                format!("{:02}:{:02}", mins, secs)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn WavlakeArtistRenderer(artist_id: String) -> Element {
    let artist_resource = use_resource(move || {
        let id = artist_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_artist(&id).await
        }
    });
    let nav = use_navigator();
    match artist_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-4",
                        div { class: "w-20 h-20 bg-muted rounded-full" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-5 bg-muted rounded w-1/2" }
                            div { class: "h-3 bg-muted rounded w-1/3" }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            rsx! {
                div {
                    class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-2 text-red-500 text-sm",
                        icons::UserIcon { class: "w-4 h-4" }
                        span { "Unable to load artist: {e}" }
                    }
                }
            }
        }
        Some(Ok(artist)) => {
            rsx! {
                div {
                    class: "my-2 border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition bg-card cursor-pointer",
                    onclick: {
                        let artist_id_nav = artist.id.clone();
                        let navigator = nav;
                        move |e: MouseEvent| {
                            e.stop_propagation();
                            navigator
                                .push(Route::MusicArtist {
                                    artist_id: artist_id_nav.clone(),
                                });
                        }
                    },
                    div { class: "flex items-center gap-4 p-4",
                        if let Some(art_url) = &artist.artist_art_url {
                            if !art_url.is_empty() {
                                img {
                                    src: "{art_url}",
                                    alt: "Artist",
                                    class: "w-20 h-20 rounded-full object-cover shrink-0",
                                }
                            } else {
                                div { class: "w-20 h-20 rounded-full bg-muted flex items-center justify-center shrink-0",
                                    icons::UserIcon { class: "w-10 h-10 text-muted-foreground" }
                                }
                            }
                        } else {
                            div { class: "w-20 h-20 rounded-full bg-muted flex items-center justify-center shrink-0",
                                icons::UserIcon { class: "w-10 h-10 text-muted-foreground" }
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "text-xs text-muted-foreground mb-1", "ARTIST" }
                            div { class: "font-bold text-lg truncate mb-1", "{artist.name}" }
                            div { class: "flex items-center gap-2 text-xs text-muted-foreground",
                                span {
                                    "{artist.albums.len()} "
                                    {if artist.albums.len() == 1 { "album" } else { "albums" }}
                                }
                                span { "•" }
                                span { class: "flex items-center gap-1 text-purple-400",
                                    icons::MusicIcon { class: "w-3 h-3" }
                                    "Wavlake"
                                }
                            }
                        }
                        div {
                            class: "shrink-0 text-muted-foreground",
                            dangerous_inner_html: r#"<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5"><path stroke-linecap="round" stroke-linejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" /></svg>"#,
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(super) fn WavlakePlaylistRenderer(playlist_id: String) -> Element {
    let playlist_resource = use_resource(move || {
        let id = playlist_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_playlist(&id).await
        }
    });
    match playlist_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex gap-4",
                        div { class: "w-32 h-32 bg-muted rounded" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-5 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/2" }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            rsx! {
                div {
                    class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-2 text-red-500 text-sm",
                        icons::MusicIcon { class: "w-4 h-4" }
                        span { "Unable to load playlist: {e}" }
                    }
                }
            }
        }
        Some(Ok(playlist)) => {
            let tracks: Vec<MusicTrack> = playlist
                .tracks
                .iter()
                .map(|track| track.clone().into())
                .collect();
            rsx! {
                div {
                    class: "my-2 border border-border rounded-lg overflow-hidden bg-card",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex gap-4 p-4 border-b border-border",
                        if let Some(first_track) = playlist.tracks.first() {
                            img {
                                src: "{first_track.album_art_url}",
                                alt: "Playlist cover",
                                class: "w-32 h-32 rounded object-cover shrink-0",
                            }
                        } else {
                            div { class: "w-32 h-32 rounded bg-muted flex items-center justify-center shrink-0",
                                icons::MusicIcon { class: "w-16 h-16 text-muted-foreground" }
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "text-xs text-muted-foreground mb-1", "PLAYLIST" }
                            div { class: "font-bold text-lg truncate mb-1", "{playlist.title}" }
                            div { class: "flex items-center gap-3 text-xs text-muted-foreground",
                                span {
                                    "{playlist.tracks.len()} "
                                    {if playlist.tracks.len() == 1 { "track" } else { "tracks" }}
                                }
                                span { "•" }
                                span { class: "flex items-center gap-1 text-purple-400",
                                    icons::MusicIcon { class: "w-3 h-3" }
                                    "Wavlake"
                                }
                            }
                        }
                    }
                    {
                        let tracks_rc = Rc::new(tracks);
                        rsx! {
                    div { class: "divide-y divide-border max-h-96 overflow-y-auto",
                        for (index , track_data) in playlist.tracks.iter().enumerate() {
                            {
                                let track_clone = tracks_rc[index].clone();
                                let playlist_rc = Rc::clone(&tracks_rc);
                                let track_clone_kb = tracks_rc[index].clone();
                                let playlist_rc_kb = Rc::clone(&tracks_rc);
                                let track_title = track_data.title.clone();
                                let track_artist = track_data.artist.clone();
                                let track_duration = track_data.duration;
                                let track_album_art = track_data.album_art_url.clone();
                                rsx! {
                                    div {
                                        key: "{track_data.id}",
                                        class: "flex items-center gap-3 p-3 hover:bg-accent/10 transition cursor-pointer group",
                                        role: "button",
                                        tabindex: "0",
                                        onclick: move |_| {
                                            music_player::play_track(
                                                track_clone.clone(),
                                                Some((*playlist_rc).clone()),
                                                Some(index),
                                            );
                                        },
                                        onkeydown: move |evt: KeyboardEvent| {
                                            let activate = matches!(evt.key(), Key::Enter)
                                                || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                                            if activate {
                                                evt.prevent_default();
                                                music_player::play_track(
                                                    track_clone_kb.clone(),
                                                    Some((*playlist_rc_kb).clone()),
                                                    Some(index),
                                                );
                                            }
                                        },
                                        div { class: "relative w-10 h-10 shrink-0 rounded overflow-hidden bg-muted group-hover:opacity-80",
                                            img {
                                                src: "{track_album_art}",
                                                alt: "Album art",
                                                class: "w-full h-full object-cover",
                                            }
                                            div {
                                                class: "absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition",
                                                dangerous_inner_html: icons::PLAY_SMALL,
                                            }
                                        }
                                        div { class: "flex-1 min-w-0",
                                            div { class: "font-medium text-sm truncate", "{track_title}" }
                                            div { class: "text-xs text-muted-foreground truncate", "{track_artist}" }
                                        }
                                        div { class: "text-xs text-muted-foreground shrink-0",
                                            {
                                                let mins = track_duration / 60;
                                                let secs = track_duration % 60;
                                                format!("{:02}:{:02}", mins, secs)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                        }
                    }
                }
            }
        }
    }
}

/// Renders a YouTube embed with click-to-load for privacy/performance
#[component]
pub(super) fn YouTubeRenderer(video_id: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let mut tried_fallback = use_signal(|| false);
    let video_id_for_fallback = video_id.clone();
    let thumbnail_url = format!(
        "https://img.youtube.com/vi/{}/maxresdefault.jpg",
        video_id,
    );
    let fallback_url = format!("https://img.youtube.com/vi/{}/hqdefault.jpg", video_id);
    let embed_url = format!("https://www.youtube.com/embed/{}?autoplay=1", video_id);
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden bg-black aspect-video max-w-full",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    class: "w-full aspect-video",
                    allow: "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture",
                    allowfullscreen: true,
                    frame_border: "0",
                }
            } else {
                div {
                    class: "relative w-full aspect-video cursor-pointer group",
                    role: "button",
                    tabindex: "0",
                    onclick: move |_| is_visible.set(true),
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if activate {
                            evt.prevent_default();
                            is_visible.set(true);
                        }
                    },
                    img {
                        src: if *tried_fallback.read() { "{fallback_url}" } else { "{thumbnail_url}" },
                        alt: "YouTube video thumbnail",
                        class: "w-full h-full object-cover",
                        onerror: move |_| {
                            if !*tried_fallback.peek() {
                                log::debug!(
                                    "YouTube maxresdefault.jpg failed for {}, trying hqdefault.jpg",
                                    video_id_for_fallback
                                );
                                tried_fallback.set(true);
                            }
                        },
                    }
                    div { class: "absolute inset-0 flex items-center justify-center bg-black/30 group-hover:bg-black/40 transition",
                        div { class: "w-16 h-16 bg-red-600 rounded-full flex items-center justify-center shadow-lg group-hover:scale-110 transition",
                            svg {
                                class: "w-8 h-8 text-white ml-1",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path { d: "M8 5v14l11-7z" }
                            }
                        }
                    }
                    div { class: "absolute bottom-2 right-2 px-2 py-1 bg-black/70 rounded text-white text-xs font-medium",
                        "YouTube"
                    }
                }
            }
        }
    }
}

/// Renders a Spotify embed
#[component]
pub(super) fn SpotifyRenderer(content_type: String, content_id: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let embed_url = format!(
        "https://open.spotify.com/embed/{}/{}?utm_source=generator&theme=0",
        content_type,
        content_id,
    );
    let height = match content_type.as_str() {
        "track" => "152",
        _ => "352",
    };
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    width: "100%",
                    height: "{height}",
                    frame_border: "0",
                    allow: "autoplay; clipboard-write; encrypted-media; fullscreen; picture-in-picture",
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#1DB954]/10 border border-[#1DB954]/30 rounded-lg cursor-pointer hover:bg-[#1DB954]/20 transition",
                    role: "button",
                    tabindex: "0",
                    onclick: move |_| is_visible.set(true),
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if activate {
                            evt.prevent_default();
                            is_visible.set(true);
                        }
                    },
                    div { class: "w-12 h-12 bg-[#1DB954] rounded-full flex items-center justify-center shrink-0",
                        svg {
                            class: "w-7 h-7 text-black",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path { d: "M12 0C5.4 0 0 5.4 0 12s5.4 12 12 12 12-5.4 12-12S18.66 0 12 0zm5.521 17.34c-.24.359-.66.48-1.021.24-2.82-1.74-6.36-2.101-10.561-1.141-.418.122-.779-.179-.899-.539-.12-.421.18-.78.54-.9 4.56-1.021 8.52-.6 11.64 1.32.42.18.479.659.301 1.02zm1.44-3.3c-.301.42-.841.6-1.262.3-3.239-1.98-8.159-2.58-11.939-1.38-.479.12-1.02-.12-1.14-.6-.12-.48.12-1.021.6-1.141C9.6 9.9 15 10.561 18.72 12.84c.361.181.54.78.241 1.2zm.12-3.36C15.24 8.4 8.82 8.16 5.16 9.301c-.6.179-1.2-.181-1.38-.721-.18-.601.18-1.2.72-1.381 4.26-1.26 11.28-1.02 15.721 1.621.539.3.719 1.02.419 1.56-.299.421-1.02.599-1.559.3z" }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm text-[#1DB954]", "Spotify {content_type}" }
                        div { class: "text-xs text-muted-foreground", "Click to load player" }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL,
                    }
                }
            }
        }
    }
}

/// Renders a SoundCloud embed
#[component]
pub(super) fn SoundCloudRenderer(url: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let encoded_url = urlencoding::encode(&url);
    let embed_url = format!(
        "https://w.soundcloud.com/player/?url={}&color=%23ff5500&auto_play=false&hide_related=false&show_comments=true&show_user=true&show_reposts=false&show_teaser=true&visual=true",
        encoded_url,
    );
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    width: "100%",
                    height: "166",
                    frame_border: "0",
                    allow: "autoplay",
                    scrolling: "no",
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#ff5500]/10 border border-[#ff5500]/30 rounded-lg cursor-pointer hover:bg-[#ff5500]/20 transition",
                    role: "button",
                    tabindex: "0",
                    onclick: move |_| is_visible.set(true),
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if activate {
                            evt.prevent_default();
                            is_visible.set(true);
                        }
                    },
                    div { class: "w-12 h-12 bg-[#ff5500] rounded-full flex items-center justify-center shrink-0",
                        svg {
                            class: "w-7 h-7 text-white",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                d: "M1.175 12.225c-.051 0-.094.046-.101.1l-.233 2.154.233 2.105c.007.058.05.098.101.098.05 0 .09-.04.099-.098l.255-2.105-.27-2.154c-.009-.06-.052-.1-.102-.1m-.899.828c-.06 0-.091.037-.104.094L0 14.479l.165 1.308c.014.057.045.094.09.094s.089-.037.099-.094l.19-1.308-.19-1.332c-.01-.057-.045-.094-.09-.094m1.83-1.229c-.061 0-.12.045-.12.104l-.21 2.563.225 2.458c0 .06.045.104.106.104.061 0 .12-.044.12-.104l.24-2.458-.24-2.563c0-.06-.059-.104-.12-.104m.945-.089c-.075 0-.135.06-.15.135l-.193 2.64.21 2.544c.016.077.075.138.149.138.075 0 .135-.061.15-.138l.24-2.544-.24-2.64c-.015-.075-.074-.135-.15-.135m.93-.104c-.09 0-.165.075-.18.165l-.178 2.73.195 2.61c.015.09.089.164.179.164.09 0 .164-.074.18-.164l.21-2.61-.21-2.73c-.015-.09-.09-.165-.18-.165m.964-.03c-.105 0-.195.09-.21.195l-.165 2.76.18 2.64c.015.105.105.18.21.18s.195-.075.21-.18l.195-2.64-.195-2.76c-.015-.105-.105-.195-.21-.195m1.005.15c-.12 0-.225.105-.225.225l-.15 2.595.165 2.655c0 .12.105.225.225.225s.225-.105.225-.225l.18-2.655-.18-2.595c0-.12-.105-.225-.225-.225m1.02-.135c-.135 0-.255.12-.255.255l-.135 2.58.15 2.685c0 .135.12.24.255.24s.255-.105.255-.24l.165-2.685-.165-2.58c0-.135-.12-.255-.255-.255m2.04.165c-.15 0-.285.135-.285.285l-.12 2.4.135 2.67c0 .15.135.285.285.285s.285-.135.285-.285l.15-2.67-.15-2.4c0-.15-.135-.285-.285-.285m-1.02-.15c-.15 0-.27.12-.27.27l-.135 2.55.135 2.67c0 .135.12.255.27.255.135 0 .255-.12.27-.255l.15-2.67-.15-2.55c-.015-.15-.135-.27-.27-.27m2.04-.105c-.165 0-.3.135-.315.3l-.105 2.415.12 2.685c.015.165.15.3.315.3.15 0 .285-.135.3-.3l.135-2.685-.135-2.415c-.015-.165-.15-.3-.3-.3m1.02.105c-.18 0-.33.15-.33.33l-.105 2.295.105 2.685c0 .165.15.315.33.315.165 0 .315-.15.33-.315l.12-2.685-.12-2.295c-.015-.18-.165-.33-.33-.33m1.02-.255c-.195 0-.345.15-.36.345l-.09 2.535.105 2.7c.015.18.165.33.345.33.195 0 .345-.15.36-.33l.12-2.7-.12-2.535c-.015-.195-.165-.345-.36-.345m1.034.035c-.21 0-.375.165-.375.375l-.09 2.52.09 2.685c0 .21.165.375.375.375.195 0 .36-.165.375-.375l.105-2.685-.105-2.52c-.015-.21-.18-.375-.375-.375m1.035-.18c-.225 0-.405.18-.405.405l-.075 2.295.075 2.685c0 .225.18.405.405.405.21 0 .39-.18.405-.405l.09-2.685-.09-2.295c-.015-.225-.195-.405-.405-.405m1.02-.24c-.225 0-.42.195-.42.42l-.06 2.13.075 2.685c0 .225.195.405.42.405.225 0 .405-.18.42-.405l.09-2.685-.09-2.13c-.015-.225-.195-.42-.42-.42m1.034-.09c-.24 0-.435.195-.435.435l-.06 1.83.06 2.685c0 .24.195.435.435.435.24 0 .435-.195.435-.435l.075-2.685-.075-1.83c0-.24-.195-.435-.435-.435m1.05.075c-.255 0-.465.21-.465.465l-.045 1.35.06 2.67c0 .255.21.465.465.465.24 0 .45-.21.465-.465l.06-2.67-.06-1.35c-.015-.255-.225-.465-.465-.465m1.035-.42c-.27 0-.495.225-.495.495l-.03.96.045 2.67c0 .27.225.495.495.495s.48-.225.495-.495l.06-2.67-.06-.96c-.015-.27-.225-.495-.495-.495m1.05.27c-.285 0-.51.24-.51.525l-.03.66.045 2.685c0 .285.225.51.51.51.27 0 .495-.225.51-.51l.06-2.685-.06-.66c-.015-.285-.24-.525-.51-.525m1.065.435c-.3 0-.54.24-.54.54v.195l.03 2.7c0 .285.24.525.54.525.285 0 .525-.24.54-.54l.045-2.685-.045-.195c-.015-.3-.255-.54-.54-.54m2.28 1.29c-.135 0-.27.015-.39.045-.105-.885-.87-1.575-1.8-1.575-.255 0-.51.06-.72.165-.09.045-.12.09-.12.18v5.295c0 .09.06.165.15.18.015 0 2.88 0 2.88 0 .945 0 1.71-.765 1.71-1.71s-.765-1.71-1.71-1.71",
                            }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm text-[#ff5500]", "SoundCloud" }
                        div { class: "text-xs text-muted-foreground", "Click to load player" }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL,
                    }
                }
            }
        }
    }
}

/// Renders an Apple Music embed
#[component]
pub(super) fn AppleMusicRenderer(embed_url: String, is_song: bool) -> Element {
    let mut is_visible = use_signal(|| false);
    let final_embed_url = if embed_url.contains("embed.music.apple.com") {
        embed_url.clone()
    } else {
        embed_url.replace("music.apple.com", "embed.music.apple.com")
    };
    let url_valid = Url::parse(&final_embed_url)
        .map(|u| u.scheme() == "https" && u.host_str() == Some("embed.music.apple.com"))
        .unwrap_or(false);
    let height = if is_song { "175" } else { "450" };
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() && url_valid {
                iframe {
                    src: "{final_embed_url}",
                    width: "100%",
                    height: "{height}",
                    frame_border: "0",
                    allow: "autoplay *; encrypted-media *; fullscreen *; clipboard-write",
                    style: "border-radius: 10px;",
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-gradient-to-r from-[#fc3c44]/10 to-[#fa57c1]/10 border border-[#fc3c44]/30 rounded-lg cursor-pointer hover:from-[#fc3c44]/20 hover:to-[#fa57c1]/20 transition",
                    role: "button",
                    tabindex: "0",
                    onclick: move |_| is_visible.set(true),
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if activate {
                            evt.prevent_default();
                            is_visible.set(true);
                        }
                    },
                    div { class: "w-12 h-12 bg-gradient-to-br from-[#fc3c44] to-[#fa57c1] rounded-xl flex items-center justify-center shrink-0",
                        svg {
                            class: "w-7 h-7 text-white",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                d: "M23.994 6.124a9.23 9.23 0 00-.24-2.19c-.317-1.31-1.062-2.31-2.18-3.043a5.022 5.022 0 00-1.877-.726 10.496 10.496 0 00-1.564-.15c-.04-.003-.083-.01-.124-.013H5.99c-.042.003-.083.01-.124.013-.5.032-.999.09-1.486.191a5.023 5.023 0 00-1.815.74c-1.113.737-1.857 1.736-2.177 3.038-.2.808-.255 1.634-.254 2.465.002.05.007.1.01.15v11.28c-.003.05-.008.1-.01.15.001.83.057 1.658.255 2.465.32 1.303 1.064 2.302 2.177 3.039a5.023 5.023 0 001.815.74c.487.1.986.159 1.486.19.041.004.082.01.124.013h12.02c.042-.003.083-.01.124-.013.5-.031.999-.09 1.486-.19a5.023 5.023 0 001.815-.74c1.113-.738 1.857-1.737 2.177-3.04.2-.807.255-1.634.254-2.464-.002-.05-.007-.1-.01-.15V6.274c.003-.05.008-.1.01-.15zM17.5 17.5c0 .397-.063.79-.187 1.163a2.5 2.5 0 01-2.658 1.682c-.674-.099-1.261-.437-1.655-.97-.394-.534-.56-1.202-.46-1.877.098-.674.437-1.261.97-1.656.534-.394 1.202-.56 1.877-.46.28.04.544.126.784.251V9.13c0-.277.175-.524.437-.616l4.5-1.5a.642.642 0 01.842.608v1.5a.643.643 0 01-.437.608l-3.563 1.188a.643.643 0 00-.437.608v6.474z",
                            }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm bg-gradient-to-r from-[#fc3c44] to-[#fa57c1] bg-clip-text text-transparent",
                            "Apple Music"
                        }
                        div { class: "text-xs text-muted-foreground", "Click to load player" }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL,
                    }
                }
            }
        }
    }
}

/// Renders a MixCloud embed
#[component]
pub(super) fn MixCloudRenderer(username: String, mix_name: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let path = format!("/{}/{}/", username, mix_name);
    let encoded_path = urlencoding::encode(&path);
    let embed_url = format!(
        "https://www.mixcloud.com/widget/iframe/?hide_cover=1&feed={}",
        encoded_path,
    );
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    width: "100%",
                    height: "120",
                    frame_border: "0",
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#5000ff]/10 border border-[#5000ff]/30 rounded-lg cursor-pointer hover:bg-[#5000ff]/20 transition",
                    role: "button",
                    tabindex: "0",
                    onclick: move |_| is_visible.set(true),
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if activate {
                            evt.prevent_default();
                            is_visible.set(true);
                        }
                    },
                    div { class: "w-12 h-12 bg-[#5000ff] rounded-full flex items-center justify-center shrink-0",
                        svg {
                            class: "w-7 h-7 text-white",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path { d: "M19.943 6.667c0-1.534-1.632-2.667-3.432-2.667-1.963 0-3.768 1.12-4.511 2.793-.743-1.673-2.548-2.793-4.511-2.793-1.8 0-3.432 1.133-3.432 2.667S1.98 10 1.98 13.333c0 .867.327 1.667.843 2.267.517.6 1.237 1.067 2.047 1.333.81.267 1.69.4 2.62.4h8.52c.93 0 1.81-.133 2.62-.4.81-.266 1.53-.733 2.047-1.333.516-.6.843-1.4.843-2.267 0-3.333-2.077-6.666-2.077-6.666z" }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm text-[#5000ff]", "MixCloud" }
                        div { class: "text-xs text-muted-foreground truncate", "{username}/{mix_name}" }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL,
                    }
                }
            }
        }
    }
}

/// Renders a Rumble embed
#[component]
pub(super) fn RumbleRenderer(embed_url: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let final_embed_url = if embed_url.contains("/embed/") {
        embed_url.clone()
    } else if let Some(start) = embed_url.find("/v") {
        let after_v = &embed_url[start + 1..];
        let video_id: String = after_v
            .chars()
            .take_while(|c| *c != '-' && *c != '.' && *c != '/')
            .collect();
        if !video_id.is_empty() {
            format!("https://rumble.com/embed/{}/", video_id)
        } else {
            embed_url.clone()
        }
    } else {
        embed_url.clone()
    };
    let url_valid = Url::parse(&final_embed_url)
        .map(|u| u.scheme() == "https" && u.host_str() == Some("rumble.com"))
        .unwrap_or(false);
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden bg-black aspect-video max-w-full",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() && url_valid {
                iframe {
                    src: "{final_embed_url}",
                    class: "w-full aspect-video",
                    frame_border: "0",
                    allowfullscreen: true,
                }
            } else {
                div {
                    class: "relative w-full aspect-video cursor-pointer group bg-[#85c742]/10",
                    role: "button",
                    tabindex: "0",
                    onclick: move |_| is_visible.set(true),
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if activate {
                            evt.prevent_default();
                            is_visible.set(true);
                        }
                    },
                    div { class: "absolute inset-0 flex flex-col items-center justify-center gap-4",
                        div { class: "w-20 h-20 bg-[#85c742] rounded-full flex items-center justify-center shadow-lg group-hover:scale-110 transition",
                            svg {
                                class: "w-10 h-10 text-white ml-1",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path { d: "M8 5v14l11-7z" }
                            }
                        }
                        div { class: "px-3 py-1.5 bg-[#85c742] rounded text-white text-sm font-bold",
                            "Rumble"
                        }
                    }
                }
            }
        }
    }
}

/// Renders a Tidal embed
#[component]
pub(super) fn TidalRenderer(embed_url: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let final_embed_url = if embed_url.contains("embed.tidal.com") {
        embed_url.clone()
    } else if embed_url.contains("tidal.com/browse/track/") {
        let track_id = embed_url
            .split("/track/")
            .nth(1)
            .and_then(|s| s.split(&['?', '#', '/'][..]).next())
            .unwrap_or("");
        format!("https://embed.tidal.com/tracks/{}?layout=gridify", track_id)
    } else {
        embed_url.clone()
    };
    let url_valid = Url::parse(&final_embed_url)
        .map(|u| u.scheme() == "https" && u.host_str() == Some("embed.tidal.com"))
        .unwrap_or(false);
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() && url_valid {
                iframe {
                    src: "{final_embed_url}",
                    width: "100%",
                    height: "96",
                    frame_border: "0",
                    allow: "encrypted-media",
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#000000]/10 border border-[#000000]/30 dark:bg-white/10 dark:border-white/30 rounded-lg cursor-pointer hover:bg-[#000000]/20 dark:hover:bg-white/20 transition",
                    role: "button",
                    tabindex: "0",
                    onclick: move |_| is_visible.set(true),
                    onkeydown: move |evt: KeyboardEvent| {
                        let activate = matches!(evt.key(), Key::Enter)
                            || matches!(evt.key(), Key::Character(ref ch) if ch == " ");
                        if activate {
                            evt.prevent_default();
                            is_visible.set(true);
                        }
                    },
                    div { class: "w-12 h-12 bg-black dark:bg-white rounded-full flex items-center justify-center shrink-0",
                        svg {
                            class: "w-6 h-6 text-white dark:text-black",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path { d: "M12.012 3.992L8.008 7.996 4.004 3.992 0 7.996l4.004 4.004 4.004-4.004 4.004 4.004-4.004 4.004-4.004-4.004-4.004 4.004L0 20.008l4.004-4.004 4.004 4.004 4.004-4.004 4.004 4.004 4.004-4.004-4.004-4.004 4.004-4.004 4.004 4.004 4.004-4.004-4.004-4.004 4.004-4.004L20.02 0l-4.004 4.004L12.012 0 8.008 4.004l4.004 3.988z" }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm", "Tidal" }
                        div { class: "text-xs text-muted-foreground", "Click to load player" }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL,
                    }
                }
            }
        }
    }
}

/// Renders a zap.stream live event card
#[component]
pub(super) fn ZapStreamRenderer(naddr: String) -> Element {
    let mut event = use_signal(|| None::<Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        let naddr_clone = naddr.clone();
        spawn(async move {
            match Nip19::from_bech32(&naddr_clone) {
                Ok(Nip19::Coordinate(coord)) => {
                    let relay_hints: Vec<String> = coord
                        .relays
                        .iter()
                        .map(|r| r.to_string())
                        .collect();
                    match nostr_client::fetch_event_by_coordinate_with_relays(
                            coord.kind.as_u16(),
                            coord.public_key.to_hex(),
                            coord.identifier.clone(),
                            relay_hints,
                        )
                        .await
                    {
                        Ok(Some(e)) => {
                            event.set(Some(e));
                        }
                        Ok(None) => {
                            error.set(Some("Live event not found".to_string()));
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                    loading.set(false);
                }
                Ok(_) => {
                    error.set(Some("Invalid naddr format".to_string()));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(format!("Failed to parse naddr: {}", e)));
                    loading.set(false);
                }
            }
        });
    });
    rsx! {
        div { class: "my-2",
            if *loading.read() {
                div { class: "flex items-center gap-3 p-4 bg-purple-500/10 border border-purple-500/30 rounded-lg animate-pulse",
                    div { class: "w-12 h-12 bg-purple-500/30 rounded-full" }
                    div { class: "flex-1",
                        div { class: "h-4 bg-purple-500/30 rounded w-32 mb-2" }
                        div { class: "h-3 bg-purple-500/20 rounded w-24" }
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                div { class: "p-4 bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-200 rounded-lg text-sm",
                    "{err}"
                }
            } else if let Some(ev) = event.read().as_ref() {
                div { onclick: move |e: MouseEvent| e.stop_propagation(),
                    LiveStreamCard { event: ev.clone() }
                }
            }
        }
    }
}

/// Renders a zap.cooking recipe as a recipe minicard
#[component]
pub(super) fn ZapCookingRecipeRenderer(naddr: String) -> Element {
    let mut event = use_signal(|| None::<Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let naddr_for_effect = naddr.clone();
    let naddr_for_render = naddr.clone();
    use_effect(move || {
        let naddr_clone = naddr_for_effect.clone();
        spawn(async move {
            match Nip19::from_bech32(&naddr_clone) {
                Ok(Nip19::Coordinate(coord)) => {
                    let relay_hints: Vec<String> = coord
                        .relays
                        .iter()
                        .map(|r| r.to_string())
                        .collect();
                    match nostr_client::fetch_event_by_coordinate_with_relays(
                            coord.kind.as_u16(),
                            coord.public_key.to_hex(),
                            coord.identifier.clone(),
                            relay_hints,
                        )
                        .await
                    {
                        Ok(Some(e)) => {
                            event.set(Some(e));
                        }
                        Ok(None) => {
                            error.set(Some("Recipe not found".to_string()));
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                    loading.set(false);
                }
                Ok(_) => {
                    error.set(Some("Invalid recipe address format".to_string()));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(format!("Failed to parse recipe address: {}", e)));
                    loading.set(false);
                }
            }
        });
    });
    rsx! {
        div { class: "my-2",
            if *loading.read() {
                div { class: "flex items-center gap-2 p-2 border border-border rounded-lg animate-pulse",
                    div { class: "w-12 h-12 bg-muted rounded shrink-0" }
                    div { class: "flex-1",
                        div { class: "h-4 bg-muted rounded w-32 mb-2" }
                        div { class: "h-3 bg-muted rounded w-24" }
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                Link {
                    to: Route::RecipeDetail {
                        naddr: naddr_for_render.clone(),
                    },
                    class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "w-12 h-12 rounded bg-gradient-to-br from-orange-500/20 to-amber-500/20 flex items-center justify-center shrink-0",
                        span { class: "text-lg", "🍳" }
                    }
                    div { class: "flex-1 min-w-0",
                        p { class: "font-medium text-sm", "🍽️ View Recipe" }
                        p { class: "text-xs text-muted-foreground truncate", "{err}" }
                    }
                }
            } else if let Some(ev) = event.read().as_ref() {
                div { onclick: move |e: MouseEvent| e.stop_propagation(),
                    if is_recipe_event(ev) {
                        {
                            let recipe_meta = extract_recipe_metadata(ev);
                            render_recipe_minicard(&recipe_meta, &naddr_for_render, ev)
                        }
                    } else {
                        Link {
                            to: Route::RecipeDetail {
                                naddr: naddr_for_render.clone(),
                            },
                            class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                            div { class: "w-12 h-12 rounded bg-gradient-to-br from-orange-500/20 to-amber-500/20 flex items-center justify-center shrink-0",
                                span { class: "text-lg", "🍳" }
                            }
                            div { class: "flex-1 min-w-0",
                                p { class: "font-medium text-sm", "🍽️ View Recipe" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render ISBN book reference with OpenLibrary cover
#[component]
pub(super) fn IsbnRenderer(isbn: String) -> Element {
    use crate::services::openlibrary::CoverSize;
    let clean_isbn = crate::services::openlibrary::clean_isbn(&isbn);
    let cover_url = crate::services::openlibrary::get_cover_url(
        &clean_isbn,
        CoverSize::Small,
    );
    let openlibrary_url = format!("https://openlibrary.org/isbn/{}", clean_isbn);
    rsx! {
        a {
            href: "{openlibrary_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-2 px-2 py-1 bg-amber-100 dark:bg-amber-900/30 text-amber-800 dark:text-amber-200 rounded hover:bg-amber-200 dark:hover:bg-amber-800/40 transition text-sm",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            img {
                src: "{cover_url}",
                alt: "Book cover",
                class: "w-5 h-7 object-cover rounded-xs",
                onerror: move |_| {
                    log::debug!("Failed to load book cover for ISBN: {}", isbn);
                },
            }
            span { "ISBN: {clean_isbn}" }
        }
    }
}

/// Render DOI paper reference
#[component]
pub(super) fn DoiRenderer(doi: String) -> Element {
    let doi_url = format!("https://doi.org/{}", doi);
    rsx! {
        a {
            href: "{doi_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2 py-1 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { class: "font-mono text-xs", "DOI" }
            span { "{doi}" }
        }
    }
}

/// Render ISAN movie reference
#[component]
pub(super) fn IsanRenderer(isan: String) -> Element {
    let isan_url = format!("https://web.isan.org/public/en/search?isan={}", isan);
    rsx! {
        a {
            href: "{isan_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2 py-1 bg-purple-100 dark:bg-purple-900/30 text-purple-800 dark:text-purple-200 rounded hover:bg-purple-200 dark:hover:bg-purple-800/40 transition text-sm",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { class: "font-mono text-xs", "ISAN" }
            span { "{isan}" }
        }
    }
}

/// Render podcast feed GUID with playable card
#[component]
pub(super) fn PodcastFeedRenderer(guid: String) -> Element {
    let guid_for_resource = guid.clone();
    let podcast_resource = use_resource(move || {
        let g = guid_for_resource.clone();
        async move { podcast_index::get_podcast_by_guid(&g).await }
    });
    match podcast_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-3",
                        div { class: "w-16 h-16 bg-muted rounded" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-4 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/2" }
                        }
                    }
                }
            }
        }
        Some(Err(_)) => {
            let podcast_index_url = format!("https://podcastindex.org/podcast/{}", urlencoding::encode(&guid));
            rsx! {
                a {
                    href: "{podcast_index_url}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    class: "inline-flex items-center gap-1.5 px-2 py-1 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-200 rounded hover:bg-green-200 dark:hover:bg-green-800/40 transition text-sm",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    span { "Podcast: " }
                    span { class: "font-mono text-xs truncate max-w-32", "{guid}" }
                }
            }
        }
        Some(Ok(podcast)) => {
            let image = podcast
                .get_image()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    format!(
                        "https://api.dicebear.com/7.x/shapes/svg?seed={}",
                        podcast.title,
                    )
                });
            let podcast_id = podcast.id;
            rsx! {
                Link {
                    to: Route::PodcastRssFeedDetail {
                        podcast_id: podcast_id.to_string(),
                    },
                    class: "my-2 border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition bg-card block",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-4 p-4",
                        div { class: "relative w-16 h-16 shrink-0 rounded overflow-hidden bg-muted",
                            img {
                                src: "{image}",
                                alt: "Podcast cover",
                                class: "w-full h-full object-cover",
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "font-semibold text-sm truncate", "{podcast.title}" }
                            if let Some(ref author) = podcast.author {
                                div { class: "text-xs text-muted-foreground truncate",
                                    "{author}"
                                }
                            }
                            if let Some(count) = podcast.episode_count {
                                div { class: "text-xs text-muted-foreground/80 mt-1",
                                    "{count} episodes"
                                }
                            }
                        }
                        div { class: "flex flex-col items-end gap-1 shrink-0",
                            div {
                                class: "flex items-center gap-1 text-xs text-green-500",
                                dangerous_inner_html: icons::PODCAST,
                                "Podcast"
                            }
                            if podcast.has_v4v() {
                                div { class: "text-xs text-amber-500", "V4V" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render podcast episode GUID with playable card
#[component]
pub(super) fn PodcastEpisodeRenderer(guid: String) -> Element {
    let guid_for_resource = guid.clone();
    let episode_resource = use_resource(move || {
        let g = guid_for_resource.clone();
        async move { podcast_index::get_episode_by_guid(&g, None).await }
    });
    match episode_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-3",
                        div { class: "w-16 h-16 bg-muted rounded" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-4 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/2" }
                        }
                    }
                }
            }
        }
        Some(Err(_)) => {
            let podcast_index_url = format!(
                "https://podcastindex.org/search?q={}",
                urlencoding::encode(&guid),
            );
            rsx! {
                a {
                    href: "{podcast_index_url}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    class: "inline-flex items-center gap-1.5 px-2 py-1 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-200 rounded hover:bg-green-200 dark:hover:bg-green-800/40 transition text-sm",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    span { "Episode: " }
                    span { class: "font-mono text-xs truncate max-w-32", "{guid}" }
                }
            }
        }
        Some(Ok((episode, podcast))) => {
            let image = episode
                .get_image()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    format!(
                        "https://api.dicebear.com/7.x/shapes/svg?seed={}",
                        episode.title,
                    )
                });
            let episode_clone = episode.clone();
            let podcast_clone = podcast.clone();
            let handle_play = move |e: MouseEvent| {
                e.stop_propagation();
                let ep = episode_clone.clone();
                let pod = podcast_clone.clone();
                let media_url = match &ep.enclosure_url {
                    Some(url) if !url.trim().is_empty() => url.clone(),
                    _ => {
                        log::warn!(
                            "Cannot play episode '{}': missing or empty enclosure URL",
                            ep.title
                        );
                        return;
                    }
                };
                let duration = ep
                    .duration
                    .map(|d| { if d > u32::MAX as u64 { u32::MAX } else { d as u32 } });
                let track = MusicTrack {
                    id: format!("pi-ep-{}", ep.id),
                    title: ep.title.clone(),
                    artist: ep
                        .feed_title
                        .clone()
                        .unwrap_or_else(|| {
                            pod.as_ref().map(|p| p.title.clone()).unwrap_or_default()
                        }),
                    artist_npub: None,
                    artist_id: None,
                    artist_art_url: None,
                    album: ep.feed_title.clone(),
                    album_id: ep.feed_id.map(|id| id.to_string()),
                    album_art_url: ep.get_image().map(|s| s.to_string()),
                    duration,
                    media_url,
                    source: TrackSource::RssPodcast {
                        feed_url: ep.feed_url.clone().unwrap_or_default(),
                        podcast_id: ep.feed_id,
                        episode_guid: guid.clone(),
                        podcast_title: ep.feed_title.clone().unwrap_or_default(),
                    },
                    msat_total: None,
                    created_at: None,
                    is_podcast: true,
                    is_live_stream: false,
                    value_block: None,
                    chapters_url: ep.chapters_url.clone(),
                    transcripts: Vec::new(),
                };
                music_player::play_track(track, None, None);
            };
            let has_v4v = episode.value.is_some();
            let duration_str = episode
                .duration
                .map(|d| {
                    let mins = d / 60;
                    let secs = d % 60;
                    format!("{:02}:{:02}", mins, secs)
                });
            let safe_desc = episode.description.as_ref().map(|d| sanitize_html(d));
            rsx! {
                div {
                    class: "my-2 border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition bg-card",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-4 p-4",
                        div { class: "relative w-16 h-16 shrink-0 rounded overflow-hidden bg-muted group",
                            img {
                                src: "{image}",
                                alt: "Episode art",
                                class: "w-full h-full object-cover",
                            }
                            button {
                                class: "absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition",
                                onclick: handle_play,
                                dangerous_inner_html: icons::PLAY,
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "font-semibold text-sm truncate", "{episode.title}" }
                            if let Some(ref feed_title) = episode.feed_title {
                                div { class: "text-xs text-muted-foreground truncate",
                                    "{feed_title}"
                                }
                            }
                            if let Some(ref desc) = safe_desc {
                                div {
                                    class: "text-xs text-muted-foreground/80 truncate mt-1",
                                    dangerous_inner_html: "{desc}",
                                }
                            }
                        }
                        div { class: "flex flex-col items-end gap-1 shrink-0",
                            if let Some(ref dur) = duration_str {
                                div { class: "text-xs text-muted-foreground", "{dur}" }
                            }
                            div {
                                class: "flex items-center gap-1 text-xs text-green-500",
                                dangerous_inner_html: icons::PODCAST,
                                "Episode"
                            }
                            if has_v4v {
                                div { class: "text-xs text-amber-500", "V4V" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render Bitcoin transaction reference
#[component]
pub(super) fn BitcoinTxRenderer(txid: String) -> Element {
    let mempool_endpoint = crate::stores::settings_store::get_mempool_endpoint();
    let base_url = mempool_endpoint.trim_end_matches("/api").trim_end_matches('/');
    let tx_url = format!("{}/tx/{}", base_url, txid);
    let truncated = crate::services::mempool::truncate_bitcoin_id(&txid);
    rsx! {
        a {
            href: "{tx_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2 py-1 bg-orange-100 dark:bg-orange-900/30 text-orange-800 dark:text-orange-200 rounded hover:bg-orange-200 dark:hover:bg-orange-800/40 transition text-sm font-mono",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { "TX: {truncated}" }
        }
    }
}

/// Render Bitcoin address reference
#[component]
pub(super) fn BitcoinAddressRenderer(address: String) -> Element {
    let mempool_endpoint = crate::stores::settings_store::get_mempool_endpoint();
    let base_url = mempool_endpoint.trim_end_matches("/api").trim_end_matches('/');
    let addr_url = format!("{}/address/{}", base_url, address);
    let truncated = crate::services::mempool::truncate_bitcoin_id(&address);
    rsx! {
        a {
            href: "{addr_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2 py-1 bg-orange-100 dark:bg-orange-900/30 text-orange-800 dark:text-orange-200 rounded hover:bg-orange-200 dark:hover:bg-orange-800/40 transition text-sm font-mono",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { "Addr: {truncated}" }
        }
    }
}

/// Render geohash location reference
#[component]
pub(super) fn GeohashRenderer(hash: String) -> Element {
    let geohash_url = format!("https://geohash.org/{}", hash);
    rsx! {
        a {
            href: "{geohash_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2 py-1 bg-teal-100 dark:bg-teal-900/30 text-teal-800 dark:text-teal-200 rounded hover:bg-teal-200 dark:hover:bg-teal-800/40 transition text-sm",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { "Location: {hash}" }
        }
    }
}
