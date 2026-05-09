use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PlaybackState {
    #[default]
    Idle,
    Loading,
    Playing,
    Paused,
    Error,
}

#[derive(Clone, Copy, PartialEq)]
pub struct AudioPlayer {
    pub state: Signal<PlaybackState>,
    pub current_track_id: Signal<Option<String>>,
    pub current_url: Signal<Option<String>>,
    pub volume: Signal<f64>,
}

pub fn use_audio_player() -> AudioPlayer {
    let state = use_signal(PlaybackState::default);
    let current_track_id = use_signal(|| None::<String>);
    let current_url = use_signal(|| None::<String>);
    let volume = use_signal(|| 0.7_f64);

    AudioPlayer {
        state,
        current_track_id,
        current_url,
        volume,
    }
}

impl AudioPlayer {
    pub fn play(&mut self, url: &str, track_id: &str) {
        self.current_track_id.set(Some(track_id.to_string()));
        self.current_url.set(Some(url.to_string()));
        self.state.set(PlaybackState::Loading);
    }

    #[allow(dead_code)]
    pub fn pause(&mut self) {
        self.state.set(PlaybackState::Paused);
    }

    #[allow(dead_code)]
    pub fn stop(&mut self) {
        self.state.set(PlaybackState::Idle);
        self.current_track_id.set(None);
        self.current_url.set(None);
    }

    #[allow(dead_code)]
    pub fn set_volume(&mut self, vol: f64) {
        self.volume.set(vol.clamp(0.0, 1.0));
    }

    pub fn is_playing(&self) -> bool {
        matches!(
            *self.state.read(),
            PlaybackState::Playing | PlaybackState::Loading
        )
    }
}

#[component]
pub fn AudioElement(mut player: AudioPlayer) -> Element {
    let url = (*player.current_url.read()).clone();
    let is_paused = matches!(*player.state.read(), PlaybackState::Paused);
    let vol = *player.volume.read();

    let Some(src) = url else {
        return rsx! {};
    };

    rsx! {
        audio {
            style: "display: none;",
            src: "{src}",
            volume: vol as f32,
            autoplay: !is_paused,
            onloadedmetadata: move |_| {
                player.state.set(PlaybackState::Playing);
            },
            onended: move |_| {
                player.state.set(PlaybackState::Idle);
                player.current_track_id.set(None);
                player.current_url.set(None);
            },
            onerror: move |_| {
                player.state.set(PlaybackState::Error);
            },
        }
    }
}
