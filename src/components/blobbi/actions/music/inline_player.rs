use dioxus::prelude::*;

use crate::components::blobbi::actions::music::audio_playback::AudioPlayer;
use crate::components::blobbi::actions::music::track_catalog;
use crate::components::blobbi::actions::music::floating_notes::FloatingMusicNotes;

#[component]
pub fn InlineMusicPlayer(mut player: AudioPlayer) -> Element {
    let current_id = (*player.current_track_id.read()).clone();

    let Some(ref track_id) = current_id else {
        return rsx! {};
    };

    let track = track_catalog::get_track_by_id(track_id);
    let Some(track) = track else {
        return rsx! {};
    };

    let is_playing = player.is_playing();

    {
        let playing = is_playing;
        use_effect(move || {
            if playing {
                crate::components::blobbi::rooms::reaction_state::set_reaction(
                    crate::components::blobbi::rooms::reaction_state::BlobbiReactionState::Listening,
                );
            } else {
                crate::components::blobbi::rooms::reaction_state::reset_reaction();
            }
        });
    }

    rsx! {
        div { class: "relative mx-4 mb-2 px-3 py-2 rounded-xl bg-card border border-border flex items-center gap-2",
            FloatingMusicNotes { active: is_playing }

            span { class: "text-base", "\u{1F3B5}" }

            div { class: "flex-1 min-w-0",
                p { class: "text-sm font-medium truncate", "{track.title}" }
                p { class: "text-[10px] text-muted-foreground",
                    "{track.artist}"
                }
            }

            button {
                class: "p-1.5 hover:bg-accent rounded-lg transition",
                onclick: move |_| {
                    if is_playing {
                        player.pause();
                    } else {
                        player.play(track.url, track.id);
                    }
                },
                span { class: "text-sm",
                    {if is_playing { "\u{23F8}" } else { "\u{25B6}" }}
                }
            }

            button {
                class: "p-1.5 hover:bg-accent rounded-lg transition text-muted-foreground",
                onclick: move |_| {
                    player.stop();
                },
                span { class: "text-sm", "\u{2715}" }
            }
        }

        super::audio_playback::AudioElement { player: player }
    }
}
