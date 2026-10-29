use crate::components::{UnifiedTrackCard, UnifiedTrackCardSkeleton};
use crate::routes::Route;
use crate::stores::{auth_store, music_player, nostr_music, profiles};
use dioxus::prelude::*;

/// Fetch a playlist (any supported kind) by naddr, then render it.
#[component]
pub fn PlaylistViewer(naddr: String) -> Element {
    let mut playlist = use_signal(|| None::<nostr_music::NostrPlaylist>);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    use_effect(use_reactive!(|naddr| {
        let naddr_clone = naddr.clone();
        loading.set(true);
        error_msg.set(None);
        spawn(async move {
            let parsed = match crate::utils::nip19::parse_naddr(&naddr_clone) {
                Ok(result) => result,
                Err(e) => {
                    error_msg.set(Some(e));
                    loading.set(false);
                    return;
                }
            };
            let fetched = nostr_music::fetch_playlist_by_coordinate_with_kind(
                parsed.kind,
                &parsed.pubkey,
                &parsed.identifier,
                parsed.relay_hints,
            )
            .await;
            match fetched {
                Ok(Some(pl)) => {
                    playlist.set(Some(pl));
                }
                Ok(None) => {
                    error_msg.set(Some("Playlist not found".to_string()));
                }
                Err(e) => {
                    error_msg.set(Some(e));
                }
            }
            loading.set(false);
        });
    }));
    rsx! {
        div { class: "max-w-4xl mx-auto p-4",
            Link {
                to: Route::MusicHome {},
                class: "inline-flex items-center gap-2 text-muted-foreground hover:text-foreground mb-6 transition",
                svg {
                    xmlns: "http://www.w3.org/2000/svg",
                    class: "w-4 h-4",
                    fill: "none",
                    view_box: "0 0 24 24",
                    stroke: "currentColor",
                    stroke_width: "2",
                    path {
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        d: "M10 19l-7-7m0 0l7-7m-7 7h18",
                    }
                }
                "Back to Music"
            }
            if *loading.read() {
                div { class: "space-y-6",
                    div { class: "flex items-start gap-6 animate-pulse",
                        div { class: "w-48 h-48 bg-muted rounded-lg shrink-0" }
                        div { class: "flex-1 space-y-4",
                            div { class: "h-8 bg-muted rounded w-3/4" }
                            div { class: "h-4 bg-muted rounded w-1/2" }
                            div { class: "h-4 bg-muted rounded w-1/3" }
                        }
                    }
                    div { class: "space-y-2 mt-8",
                        for i in 0..5 {
                            UnifiedTrackCardSkeleton { key: "{i}" }
                        }
                    }
                }
            } else if let Some(err) = error_msg.read().clone() {
                div { class: "text-center py-16",
                    div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            class: "w-8 h-8 text-destructive",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            stroke_width: "2",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                d: "M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z",
                            }
                        }
                    }
                    p { class: "text-muted-foreground font-medium", "{err}" }
                    Link {
                        to: Route::MusicHome {},
                        class: "inline-flex items-center gap-2 mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition text-sm font-medium",
                        "Back to Music"
                    }
                }
            } else if let Some(pl) = playlist.read().clone() {
                PlaylistBody { playlist: pl }
            }
        }
    }
}

fn ErrorBanner(err: String) -> Element {
    rsx! {
        div { class: "p-3 bg-destructive/10 border border-destructive rounded-lg text-destructive text-sm",
            "{err}"
        }
    }
}

/// The playlist header + track list shared by [PlaylistViewer] and
/// [crate::components::viewers::bookmark_set_viewer::BookmarkSetViewer]
/// (playlist-shaped NIP-51 bookmark sets).
///
/// For the signed-in owner, an inline editor handles metadata edits, track
/// reordering/removal, and NIP-09 deletion; every mutation re-publishes the
/// addressable through the publish queue (see `nostr_music`).
#[component]
pub fn PlaylistBody(playlist: nostr_music::NostrPlaylist) -> Element {
    let initial = playlist.clone();
    let mut current = use_signal(|| initial.clone());
    let mut tracks = use_signal(Vec::<music_player::MusicTrack>::new);
    let mut creator_name = use_signal(|| String::from("Unknown"));
    let mut loading_tracks = use_signal(|| true);
    let mut action_error = use_signal(|| None::<String>);
    let mut working = use_signal(|| false);
    let mut editing = use_signal(|| false);
    let mut share_copied = use_signal(|| false);

    let navigator = navigator();
    let is_own = auth_store::get_pubkey().as_deref() == Some(initial.pubkey.as_str());

    // Editor state (seeded when entering edit mode).
    let mut ed_title = use_signal(String::new);
    let mut ed_description = use_signal(String::new);
    let mut ed_image = use_signal(String::new);
    let mut ed_public = use_signal(|| true);
    let mut ed_collaborative = use_signal(|| false);
    let mut ed_tracks = use_signal(Vec::<String>::new);

    let refresh_tracks = move |pl: nostr_music::NostrPlaylist| {
        spawn(async move {
            loading_tracks.set(true);
            match nostr_music::resolve_playlist_tracks(&pl).await {
                Ok(nostr_tracks) => {
                    let music_tracks: Vec<music_player::MusicTrack> =
                        nostr_tracks.into_iter().map(|t| t.into()).collect();
                    tracks.set(music_tracks);
                }
                Err(e) => {
                    log::warn!("Failed to resolve playlist tracks: {}", e);
                    tracks.set(Vec::new());
                }
            }
            loading_tracks.set(false);
        });
    };

    use_effect(use_reactive!(|initial| {
        current.set(initial.clone());
        let author_pk = initial.pubkey.clone();
        spawn(async move {
            if let Ok(profile) = profiles::fetch_profile(author_pk).await {
                creator_name.set(profile.get_display_name());
            }
        });
        refresh_tracks(initial.clone());
    }));

    let refetch_playlist = move || {
        let pl = current.read().clone();
        spawn(async move {
            if let Ok(Some(fresh)) =
                nostr_music::fetch_playlist_by_coordinate_with_kind(
                    pl.kind,
                    &pl.pubkey,
                    &pl.d_tag,
                    Vec::new(),
                )
                .await
            {
                current.set(fresh.clone());
                refresh_tracks(fresh);
            }
        });
    };

    let play_playlist = move |_| {
        let playlist_tracks = tracks.read().clone();
        if let Some(first_track) = playlist_tracks.first().cloned() {
            music_player::play_track(first_track, Some(playlist_tracks), Some(0));
        }
    };
    let play_track = move |track: music_player::MusicTrack, index: usize| {
        let playlist_tracks = tracks.read().clone();
        music_player::play_track(track, Some(playlist_tracks), Some(index));
    };

    let share = move |_| {
        let pl = current.read().clone();
        if let Some(naddr) = pl.naddr.clone() {
            spawn(async move {
                let web_url = format!("https://nostr.blue/address/{}", naddr);
                let text = format!("{}\n{}", pl.title, web_url);
                if crate::platform::clipboard::copy_to_clipboard(&text).await.is_ok() {
                    share_copied.set(true);
                }
            });
        }
    };

    let start_editing = move |_| {
        let pl = current.read().clone();
        ed_title.set(pl.title.clone());
        ed_description.set(pl.description.clone().unwrap_or_default());
        ed_image.set(pl.image.clone().unwrap_or_default());
        ed_public.set(pl.is_public);
        ed_collaborative.set(pl.is_collaborative);
        ed_tracks.set(pl.track_refs.clone());
        action_error.set(None);
        editing.set(true);
    };

    let save_edit = move |_| {
        let pl = current.read().clone();
        let title = ed_title.read().trim().to_string();
        if title.is_empty() || *working.read() {
            return;
        }
        working.set(true);
        action_error.set(None);
        let description = {
            let d = ed_description.read().trim().to_string();
            if d.is_empty() { None } else { Some(d) }
        };
        let image = {
            let i = ed_image.read().trim().to_string();
            if i.is_empty() { None } else { Some(i) }
        };
        let track_refs = ed_tracks.read().clone();
        let is_public = *ed_public.read();
        let is_collaborative = *ed_collaborative.read();
        spawn(async move {
            match nostr_music::update_playlist(
                &pl,
                title,
                description,
                image,
                track_refs,
                is_public,
                is_collaborative,
            )
            .await
            {
                Ok(_) => {
                    editing.set(false);
                    refetch_playlist();
                }
                Err(e) => action_error.set(Some(e)),
            }
            working.set(false);
        });
    };

    let delete_playlist = move |_| {
        let pl = current.read().clone();
        if *working.read() {
            return;
        }
        working.set(true);
        action_error.set(None);
        spawn(async move {
            match nostr_music::delete_playlist(&pl).await {
                Ok(_) => {
                    navigator.replace(Route::MusicHome {});
                }
                Err(e) => action_error.set(Some(e)),
            }
            working.set(false);
        });
    };

    let mut remove_track = move |index: usize| {
        let pl = current.read().clone();
        let tracks_snapshot = tracks.read().clone();
        let Some(track) = tracks_snapshot.get(index) else {
            return;
        };
        let Some(nostr_music::TrackSource::Nostr { coordinate, pubkey, d_tag }) =
            Some(track.source.clone())
        else {
            return;
        };
        let nostr_track = nostr_music::NostrTrack {
            event_id: String::new(),
            pubkey,
            d_tag,
            coordinate,
            title: track.title.clone(),
            url: String::new(),
            image: None,
            gradient: None,
            duration: None,
            genres: Vec::new(),
            ai_generated: false,
            created_at: 0,
        };
        if *working.read() {
            return;
        }
        working.set(true);
        action_error.set(None);
        spawn(async move {
            match nostr_music::remove_track_from_playlist(&pl, &nostr_track).await {
                Ok(_) => refetch_playlist(),
                Err(e) => action_error.set(Some(e)),
            }
            working.set(false);
        });
    };

    rsx! {
        div { class: "space-y-8",
            if *editing.read() {
                div { class: "p-6 bg-card rounded-xl border border-border space-y-5",
                    h2 { class: "text-xl font-bold", "Edit Playlist" }
                    if let Some(err) = action_error.read().clone() {
                        {ErrorBanner(err)}
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Title *" }
                        input {
                            r#type: "text",
                            class: "w-full px-4 py-3 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                            value: "{ed_title}",
                            oninput: move |e| ed_title.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Description" }
                        textarea {
                            rows: "2",
                            class: "w-full px-4 py-3 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary resize-none",
                            value: "{ed_description}",
                            oninput: move |e| ed_description.set(e.value()),
                        }
                    }
                    div {
                        label { class: "block text-sm font-medium mb-2", "Cover Image URL" }
                        input {
                            r#type: "url",
                            class: "w-full px-4 py-3 border border-border rounded-lg bg-background focus:outline-hidden focus:ring-2 focus:ring-primary",
                            value: "{ed_image}",
                            oninput: move |e| ed_image.set(e.value()),
                        }
                    }
                    div { class: "flex flex-wrap gap-6",
                        label { class: "flex items-center gap-2 text-sm",
                            input {
                                r#type: "checkbox",
                                class: "w-4 h-4 rounded border-border",
                                checked: *ed_public.read(),
                                onchange: move |e| ed_public.set(e.checked()),
                            }
                            "Public playlist"
                        }
                        label { class: "flex items-center gap-2 text-sm",
                            input {
                                r#type: "checkbox",
                                class: "w-4 h-4 rounded border-border",
                                checked: *ed_collaborative.read(),
                                onchange: move |e| ed_collaborative.set(e.checked()),
                            }
                            "Collaborative"
                        }
                    }
                    div { class: "space-y-1",
                        h3 { class: "text-sm font-medium mb-3", "Tracks" }
                        if ed_tracks.read().is_empty() {
                            p { class: "text-sm text-muted-foreground", "No tracks." }
                        } else {
                            div { class: "divide-y divide-border/50",
                                for (index , _coord) in ed_tracks.read().iter().enumerate() {
                                    {
                                        let idx = index;
                                        let count = ed_tracks.read().len();
                                        let track_title = tracks
                                            .read()
                                            .get(index)
                                            .map(|t| t.title.clone())
                                            .unwrap_or_else(|| "Track".to_string());
                                        rsx! {
                                            div {
                                                key: "{idx}",
                                                class: "flex items-center gap-3 py-2",
                                                span { class: "text-xs text-muted-foreground w-6", "{idx + 1}" }
                                                span { class: "flex-1 text-sm truncate", "{track_title}" }
                                                button {
                                                    class: "p-1.5 hover:bg-muted rounded-full transition disabled:opacity-30",
                                                    disabled: idx == 0,
                                                    onclick: move |_| {
                                                        let mut t = ed_tracks.write();
                                                        if idx > 0 {
                                                            t.swap(idx - 1, idx);
                                                        }
                                                    },
                                                    "↑"
                                                }
                                                button {
                                                    class: "p-1.5 hover:bg-muted rounded-full transition disabled:opacity-30",
                                                    disabled: idx + 1 >= count,
                                                    onclick: move |_| {
                                                        let mut t = ed_tracks.write();
                                                        if idx + 1 < t.len() {
                                                            t.swap(idx, idx + 1);
                                                        }
                                                    },
                                                    "↓"
                                                }
                                                button {
                                                    class: "p-1.5 hover:bg-destructive/10 text-destructive rounded-full transition",
                                                    onclick: move |_| {
                                                        ed_tracks.write().remove(idx);
                                                    },
                                                    "✕"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex items-center gap-3 pt-2",
                        button {
                            class: "px-5 py-2.5 bg-primary text-primary-foreground rounded-lg font-medium hover:bg-primary/90 transition disabled:opacity-50",
                            disabled: *working.read(),
                            onclick: save_edit,
                            if *working.read() { "Saving…" } else { "Save" }
                        }
                        button {
                            class: "px-5 py-2.5 border border-border rounded-lg hover:bg-muted transition",
                            onclick: move |_| editing.set(false),
                            "Cancel"
                        }
                        div { class: "flex-1" }
                        button {
                            class: "px-5 py-2.5 border border-destructive text-destructive rounded-lg hover:bg-destructive/10 transition disabled:opacity-50",
                            disabled: *working.read(),
                            onclick: delete_playlist,
                            "Delete Playlist"
                        }
                    }
                }
            } else {
                div { class: "flex flex-col sm:flex-row items-start gap-6 p-6 bg-card rounded-xl border border-border",
                    div { class: "w-48 h-48 rounded-lg overflow-hidden bg-muted shrink-0",
                        if let Some(ref image) = current.read().image {
                            img {
                                src: "{image}",
                                alt: "Playlist cover",
                                class: "w-full h-full object-cover",
                            }
                        } else {
                            div { class: "w-full h-full bg-gradient-to-br from-purple-500/30 to-blue-500/30 flex items-center justify-center",
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-16 h-16 text-muted-foreground/50",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "1.5",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3",
                                    }
                                }
                            }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        span { class: "text-xs font-medium text-muted-foreground uppercase tracking-wide",
                            if current.read().kind == 34139 { "Playlist" } else { "Playlist (Bookmark Set)" }
                        }
                        h1 { class: "text-3xl font-bold mt-1", "{current.read().title}" }
                        if let Some(ref desc) = current.read().description {
                            p { class: "text-muted-foreground mt-2", "{desc}" }
                        }
                        div { class: "flex flex-wrap items-center gap-x-4 gap-y-1 mt-4 text-sm text-muted-foreground",
                            span { "by {creator_name}" }
                            span { "·" }
                            span { "{current.read().track_refs.len()} tracks" }
                            if current.read().is_collaborative {
                                span { "·" }
                                span { class: "text-primary", "Collaborative" }
                            }
                            if !current.read().is_public {
                                span { "·" }
                                span {
                                    title: "Private playlist",
                                    "🔒 Private"
                                }
                            }
                        }
                        if !current.read().categories.is_empty() {
                            div { class: "flex flex-wrap gap-2 mt-4",
                                for cat in current.read().categories.iter() {
                                    span {
                                        key: "{cat}",
                                        class: "px-2 py-1 bg-muted rounded-full text-xs",
                                        "{cat}"
                                    }
                                }
                            }
                        }
                        if let Some(err) = action_error.read().clone() {
                            div { class: "mt-4", {ErrorBanner(err)} }
                        }
                        div { class: "flex flex-wrap items-center gap-3 mt-6",
                            button {
                                class: "px-6 py-3 bg-primary text-primary-foreground rounded-full font-medium hover:bg-primary/90 transition flex items-center gap-2 disabled:opacity-50",
                                disabled: tracks.read().is_empty(),
                                onclick: play_playlist,
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "w-5 h-5",
                                    fill: "currentColor",
                                    view_box: "0 0 24 24",
                                    path { d: "M8 5v14l11-7z" }
                                }
                                "Play"
                            }
                            button {
                                class: "px-4 py-3 border border-border rounded-full text-sm hover:bg-muted transition",
                                onclick: share,
                                if *share_copied.read() { "Copied!" } else { "Share" }
                            }
                            if is_own && nostr_client_has_signer() {
                                button {
                                    class: "px-4 py-3 border border-border rounded-full text-sm hover:bg-muted transition",
                                    onclick: start_editing,
                                    "Edit"
                                }
                            }
                        }
                    }
                }
                div { class: "space-y-1",
                    h2 { class: "text-lg font-semibold mb-4", "Tracks" }
                    if *loading_tracks.read() {
                        div { class: "space-y-2",
                            for i in 0..3 {
                                UnifiedTrackCardSkeleton { key: "{i}" }
                            }
                        }
                    } else if tracks.read().is_empty() {
                        div { class: "text-center py-12 text-muted-foreground",
                            p { "No tracks in this playlist yet." }
                        }
                    } else {
                        div { class: "divide-y divide-border/50",
                            for (index , track) in tracks.read().iter().enumerate() {
                                {
                                    let track_clone = track.clone();
                                    let show_remove = is_own && nostr_client_has_signer();
                                    let remove_index = index;
                                    rsx! {
                                        div {
                                            key: "{track.id}",
                                            class: "flex items-center gap-1",
                                            div {
                                                class: "flex-1 min-w-0 cursor-pointer",
                                                onclick: move |_| play_track(track_clone.clone(), index),
                                                UnifiedTrackCard { track: track.clone(), show_album: false, show_sats: true }
                                            }
                                            if show_remove {
                                                button {
                                                    class: "p-2 text-muted-foreground hover:text-destructive hover:bg-destructive/10 rounded-full transition shrink-0 disabled:opacity-50",
                                                    title: "Remove from playlist",
                                                    disabled: *working.read(),
                                                    onclick: move |_| remove_track(remove_index),
                                                    "✕"
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

fn nostr_client_has_signer() -> bool {
    *crate::stores::nostr_client::HAS_SIGNER.read()
}
