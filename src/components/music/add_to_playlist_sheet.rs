//! Add-to-playlist modal — the kind-34139 playlist picker used from track
//! cards. Mirrors Amethyst's `AddToMusicPlaylistSheet`: lists the user's own
//! playlists with a contains-checkmark, toggles membership by re-publishing
//! the addressable with the track appended/removed, and offers
//! create-new-playlist-with-track.
use crate::components::modal::Modal;
use crate::stores::music_player::MusicTrack;
use crate::stores::nostr_music::{self, NostrPlaylist, NostrTrack, TrackSource};
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct AddToPlaylistSheetProps {
    pub open: Signal<bool>,
    pub track: MusicTrack,
}

/// Extract the kind-36787 coordinate from a unified track, if it is a Nostr
/// music track (only those can be referenced from a kind-34139 playlist).
pub fn nostr_coordinate(track: &MusicTrack) -> Option<(String, String, String)> {
    match &track.source {
        TrackSource::Nostr {
            coordinate,
            pubkey,
            d_tag,
        } => Some((coordinate.clone(), pubkey.clone(), d_tag.clone())),
        _ => None,
    }
}

/// Toggle a track's membership in one of the user's playlists, then refresh
/// the sheet's playlist list. `signals` are Dioxus `Signal`s (Copy), safe to
/// capture in each row's spawned task.
async fn toggle_membership(
    playlist: NostrPlaylist,
    (coordinate, pubkey, d_tag): (String, String, String),
    title: String,
    mut error_msg: Signal<Option<String>>,
    mut working: Signal<bool>,
    mut playlists: Signal<Vec<NostrPlaylist>>,
) {
    working.set(true);
    let track = NostrTrack {
        event_id: String::new(),
        pubkey,
        d_tag,
        coordinate,
        title,
        url: String::new(),
        image: None,
        gradient: None,
        duration: None,
        genres: Vec::new(),
        ai_generated: false,
        created_at: 0,
    };
    let contains = playlist.track_refs.iter().any(|r| r == &track.coordinate);
    let result = if contains {
        nostr_music::remove_track_from_playlist(&playlist, &track).await
    } else {
        nostr_music::add_track_to_playlist(&playlist, &track).await
    };
    if let Err(e) = result {
        error_msg.set(Some(e));
    }
    if let Ok(list) = nostr_music::fetch_own_playlists().await {
        playlists.set(list);
    }
    working.set(false);
}

/// Modal sheet listing the signed-in user's own playlists with membership
/// toggles for the given track.
#[component]
pub fn AddToPlaylistSheet(props: AddToPlaylistSheetProps) -> Element {
    let open = props.open;
    let mut playlists = use_signal(Vec::<NostrPlaylist>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let working = use_signal(|| false);
    let mut new_title = use_signal(String::new);
    let mut creating = use_signal(|| false);

    let coordinate = nostr_coordinate(&props.track);
    let track_title = props.track.title.clone();

    use_effect(use_reactive(
        (&*open.read(), &*nostr_client::HAS_SIGNER.read()),
        move |(is_open, has_signer)| {
            if !is_open || !has_signer {
                return;
            }
            loading.set(true);
            error_msg.set(None);
            spawn(async move {
                match nostr_music::fetch_own_playlists().await {
                    Ok(list) => playlists.set(list),
                    Err(e) => error_msg.set(Some(e)),
                }
                loading.set(false);
            });
        },
    ));

    if coordinate.is_none() {
        return rsx! {
            Modal { open: open,
                div { class: "flex items-center justify-between p-4 border-b border-border",
                    h2 { class: "text-lg font-bold", "Add to Playlist" }
                }
                div { class: "p-4 text-sm text-muted-foreground",
                    "Only Nostr music tracks (kind 36787) can be added to playlists."
                }
            }
        };
    }

    rsx! {
        Modal { open: open,
            div { class: "flex items-center justify-between p-4 border-b border-border",
                h2 { class: "text-lg font-bold", "Add to Playlist" }
            }
            div { class: "p-4 space-y-4 max-h-[60vh] overflow-y-auto",
                if let Some(err) = error_msg.read().clone() {
                    div { class: "p-3 bg-destructive/10 border border-destructive rounded-lg text-destructive text-sm",
                        "{err}"
                    }
                }
                if !auth_store::is_authenticated() {
                    p { class: "text-sm text-muted-foreground",
                        "Sign in to manage your playlists."
                    }
                } else if *loading.read() {
                    div { class: "space-y-2",
                        for i in 0..3 {
                            div { key: "{i}", class: "h-12 bg-muted rounded-lg animate-pulse" }
                        }
                    }
                } else if playlists.read().is_empty() {
                    p { class: "text-sm text-muted-foreground",
                        "You don't have any playlists yet — create one below."
                    }
                } else {
                    div { class: "space-y-1",
                        for playlist in playlists.read().iter() {
                            {
                                let pl = playlist.clone();
                                let row_coord = coordinate.clone();
                                let row_title = track_title.clone();
                                let is_working = *working.read();
                                let contains = pl
                                    .track_refs
                                    .iter()
                                    .any(|r| Some(r) == coordinate.as_ref().map(|(c, _, _)| c));
                                rsx! {
                                    button {
                                        key: "{pl.coordinate}",
                                        class: "w-full flex items-center gap-3 p-3 hover:bg-muted rounded-lg transition text-left disabled:opacity-50",
                                        disabled: is_working,
                                        onclick: move |_| {
                                            let pl = pl.clone();
                                            let row_coord = row_coord.clone();
                                            let row_title = row_title.clone();
                                            spawn(async move {
                                                toggle_membership(
                                                    pl,
                                                    row_coord.expect("checked above"),
                                                    row_title,
                                                    error_msg,
                                                    working,
                                                    playlists,
                                                )
                                                .await;
                                            });
                                        },
                                        if contains {
                                            span { class: "text-primary font-bold shrink-0", "✓" }
                                        } else {
                                            span { class: "text-muted-foreground shrink-0 w-4", "" }
                                        }
                                        div { class: "flex-1 min-w-0",
                                            p { class: "font-medium text-sm truncate", "{pl.title}" }
                                            p { class: "text-xs text-muted-foreground",
                                                "{pl.track_refs.len()} tracks"
                                            }
                                        }
                                        if !pl.is_public {
                                            span {
                                                class: "text-xs text-muted-foreground shrink-0",
                                                title: "Private playlist",
                                                "🔒"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if auth_store::is_authenticated() {
                    div { class: "pt-3 border-t border-border",
                        div { class: "flex gap-2",
                            input {
                                r#type: "text",
                                placeholder: "New playlist name…",
                                class: "flex-1 px-3 py-2 border border-border rounded-lg bg-background text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                value: "{new_title}",
                                oninput: move |e| new_title.set(e.value()),
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:bg-primary/90 transition disabled:opacity-50",
                                disabled: *creating.read() || new_title.read().trim().is_empty(),
                                onclick: move |_| {
                                    let title = new_title.read().trim().to_string();
                                    let Some((coord, _, _)) = coordinate.clone() else {
                                        return;
                                    };
                                    if title.is_empty() || *creating.peek() {
                                        return;
                                    }
                                    creating.set(true);
                                    error_msg.set(None);
                                    spawn(async move {
                                        let d_tag_value = format!(
                                            "{}-{}",
                                            crate::utils::slugify(&title),
                                            chrono::Utc::now().timestamp()
                                        );
                                        let result = nostr_music::publish_playlist(
                                            d_tag_value,
                                            title.clone(),
                                            None,
                                            None,
                                            vec![coord.clone()],
                                            Vec::new(),
                                            true,
                                            false,
                                        )
                                        .await;
                                        match result {
                                            Ok(_) => {
                                                new_title.set(String::new());
                                                if let Ok(list) =
                                                    nostr_music::fetch_own_playlists().await
                                                {
                                                    playlists.set(list);
                                                }
                                            }
                                            Err(e) => error_msg.set(Some(format!(
                                                "Failed to create playlist: {}",
                                                e
                                            ))),
                                        }
                                        creating.set(false);
                                    });
                                },
                                if *creating.read() { "Creating…" } else { "Create" }
                            }
                        }
                        p { class: "text-xs text-muted-foreground mt-2",
                            "Creates a public playlist containing this track."
                        }
                    }
                }
            }
        }
    }
}
