use crate::components::icons;
use crate::routes::Route;
use crate::services::wavlake::WavlakeTrack;
use crate::stores::music_player::{self, MusicPlayerStateStoreExt, MusicTrack};
use dioxus::prelude::*;
#[cfg(feature = "web")]
use dioxus::web::WebEventExt;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;

#[cfg(feature = "web")]
const INTERACTIVE_ELEMENT_SELECTOR: &str =
    "a,button,input,textarea,select,[role='button'],[role='link']";

fn is_event_from_interactive_element(_evt: &KeyboardEvent) -> bool {
    #[cfg(feature = "web")]
    {
        if let Some(target) = _evt.data.as_web_event().target() {
            if let Some(element) = target.dyn_ref::<web_sys::Element>() {
                return element
                    .closest(INTERACTIVE_ELEMENT_SELECTOR)
                    .ok()
                    .flatten()
                    .is_some();
            }
        }
    }

    false
}
#[derive(Props, Clone, PartialEq)]
pub struct TrackCardProps {
    pub track: WavlakeTrack,
    #[props(default = false)]
    pub show_album: bool,
}
/// Track card component for displaying a music track
#[component]
pub fn TrackCard(props: TrackCardProps) -> Element {
    let track = &props.track;
    let track_id = track.id.clone();
    let track_id_for_effect = track_id.clone();
    let mut is_playing = use_signal(|| false);
    use_effect(move || {
        let store = music_player::MUSIC_PLAYER.resolve();
        let current = store.current_track().cloned();
        if let Some(ref cur) = current {
            is_playing.set(cur.id == track_id_for_effect && store.is_playing().cloned());
        } else {
            is_playing.set(false);
        }
    });
    let play_track_on_click = {
        let track = track.clone();
        move || {
            let music_track: MusicTrack = track.clone().into();
            music_player::play_or_toggle_track(music_track, None, None);
        }
    };
    let play_track_on_keydown = {
        let track = track.clone();
        move || {
            let music_track: MusicTrack = track.clone().into();
            music_player::play_or_toggle_track(music_track, None, None);
        }
    };
    let duration_str = {
        let mins = track.duration / 60;
        let secs = track.duration % 60;
        format!("{:02}:{:02}", mins, secs)
    };
    rsx! {
        div {
            class: "flex items-center gap-3 p-3 hover:bg-muted/50 rounded-lg transition group cursor-pointer",
            role: "button",
            tabindex: "0",
            onclick: move |_| play_track_on_click(),
            onkeydown: move |evt| {
                if is_event_from_interactive_element(&evt) {
                    return;
                }
                match evt.key() {
                    Key::Enter => {
                        evt.prevent_default();
                        play_track_on_keydown();
                    }
                    Key::Character(ref c) if c == " " => {
                        evt.prevent_default();
                        play_track_on_keydown();
                    }
                    _ => {}
                }
            },
            div { class: "relative shrink-0",
                img {
                    src: "{track.album_art_url}",
                    alt: "Album art",
                    class: "w-14 h-14 rounded object-cover",
                    loading: "lazy",
                }
                button {
                    class: "absolute inset-0 flex items-center justify-center bg-black/40 opacity-0 group-hover:opacity-100 transition rounded",
                    tabindex: "-1",
                    onkeydown: move |evt| {
                        match evt.key() {
                            Key::Enter => {
                                evt.stop_propagation();
                            }
                            Key::Character(ref c) if c == " " => {
                                evt.stop_propagation();
                            }
                            _ => {}
                        }
                    },
                    dangerous_inner_html: if *is_playing.read() { icons::PAUSE } else { icons::PLAY },
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "font-medium text-sm truncate",
                    if *is_playing.read() {
                        span { class: "text-primary", "{track.title}" }
                    } else {
                        "{track.title}"
                    }
                }
                div {
                    class: "text-xs text-muted-foreground truncate",
                    onkeydown: move |e: Event<KeyboardData>| e.stop_propagation(),
                    Link {
                        to: Route::MusicArtist {
                            artist_id: track.artist_id.clone(),
                        },
                        class: "hover:text-foreground hover:underline",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        "{track.artist}"
                    }
                }
                if props.show_album {
                    div {
                        class: "text-xs text-muted-foreground truncate",
                        onkeydown: move |e: Event<KeyboardData>| e.stop_propagation(),
                        Link {
                            to: Route::MusicAlbum {
                                album_id: track.album_id.clone(),
                            },
                            class: "hover:text-foreground hover:underline",
                            onclick: move |e: Event<MouseData>| e.stop_propagation(),
                            "{track.album_title}"
                        }
                    }
                }
            }
            div { class: "text-xs text-muted-foreground shrink-0", "{duration_str}" }
            div { class: "flex items-center gap-1 shrink-0 opacity-0 group-hover:opacity-100 transition",
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Vote for this track",
                    onkeydown: move |e: Event<KeyboardData>| e.stop_propagation(),
                    onclick: {
                        let vote_track: MusicTrack = track.clone().into();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            let t = vote_track.clone();
                            spawn(async move {
                                if let Err(e) = music_player::vote_for_music(&t).await {
                                    log::error!("Vote failed: {}", e);
                                }
                            });
                        }
                    },
                    dangerous_inner_html: icons::HEART,
                }
                button {
                    class: "p-2 hover:bg-muted rounded-full transition",
                    title: "Zap this artist",
                    onkeydown: move |e: Event<KeyboardData>| e.stop_propagation(),
                    onclick: {
                        let zap_track: MusicTrack = track.clone().into();
                        move |e: Event<MouseData>| {
                            e.stop_propagation();
                            music_player::show_zap_dialog_for_track(Some(zap_track.clone()));
                        }
                    },
                    dangerous_inner_html: icons::ZAP,
                }
            }
        }
    }
}
/// Skeleton loader for track card
#[component]
pub fn TrackCardSkeleton() -> Element {
    rsx! {
        div { class: "flex items-center gap-3 p-3 rounded-lg animate-pulse",
            div { class: "w-14 h-14 bg-muted rounded shrink-0" }
            div { class: "flex-1 min-w-0 space-y-2",
                div { class: "h-4 bg-muted rounded w-3/4" }
                div { class: "h-3 bg-muted rounded w-1/2" }
            }
            div { class: "w-12 h-3 bg-muted rounded shrink-0" }
        }
    }
}
