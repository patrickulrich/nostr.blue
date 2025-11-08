use dioxus::prelude::*;
use nostr_sdk::EventId;

/// Voice message playback state
#[derive(Clone, Debug, PartialEq)]
pub struct VoicePlaybackState {
    /// Currently playing voice message event ID
    pub currently_playing: Option<EventId>,
    /// Current playback time in seconds
    pub current_time: f64,
    /// Duration in seconds
    pub duration: f64,
    /// Volume (0.0 to 1.0)
    pub volume: f64,
    /// Is muted
    pub is_muted: bool,
}

impl Default for VoicePlaybackState {
    fn default() -> Self {
        Self {
            currently_playing: None,
            current_time: 0.0,
            duration: 0.0,
            volume: 0.8,
            is_muted: false,
        }
    }
}

/// Voice recording state
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum RecordingState {
    Idle,
    Recording { started_at: f64, duration: f64 },
    Paused { duration: f64 },
    Completed { blob_url: String, duration: f64, waveform: Vec<u8> },
}

impl Default for RecordingState {
    fn default() -> Self {
        Self::Idle
    }
}

/// Global playback state signal
pub static VOICE_PLAYBACK: GlobalSignal<VoicePlaybackState> = Signal::global(VoicePlaybackState::default);

/// Global recording state signal
#[allow(dead_code)]
pub static RECORDING_STATE: GlobalSignal<RecordingState> = Signal::global(|| RecordingState::default());

/// Play a voice message (pauses any currently playing)
#[allow(dead_code)]
pub fn play_voice_message(event_id: EventId) {
    let mut state = VOICE_PLAYBACK.write();
    state.currently_playing = Some(event_id);
    state.current_time = 0.0;
}

/// Pause the currently playing voice message
pub fn pause_voice_message() {
    let mut state = VOICE_PLAYBACK.write();
    state.currently_playing = None;
}

/// Toggle play/pause for a specific voice message
pub fn toggle_voice_message(event_id: EventId) {
    let mut state = VOICE_PLAYBACK.write();
    if state.currently_playing == Some(event_id) {
        state.currently_playing = None;
    } else {
        state.currently_playing = Some(event_id);
        state.current_time = 0.0;
    }
}

/// Check if a specific voice message is currently playing
#[allow(dead_code)]
pub fn is_playing(event_id: &EventId) -> bool {
    VOICE_PLAYBACK.read().currently_playing.as_ref() == Some(event_id)
}

/// Update current playback time
pub fn set_current_time(time: f64) {
    VOICE_PLAYBACK.write().current_time = time;
}

/// Update duration
pub fn set_duration(duration: f64) {
    VOICE_PLAYBACK.write().duration = duration;
}

/// Set volume
#[allow(dead_code)]
pub fn set_volume(volume: f64) {
    let mut state = VOICE_PLAYBACK.write();
    state.volume = volume.clamp(0.0, 1.0);
    state.is_muted = false;
}

/// Toggle mute
#[allow(dead_code)]
pub fn toggle_mute() {
    let mut state = VOICE_PLAYBACK.write();
    state.is_muted = !state.is_muted;
}

/// Start recording
#[allow(dead_code)]
pub fn start_recording() {
    let now = js_sys::Date::now() / 1000.0;
    RECORDING_STATE.write().clone_from(&RecordingState::Recording {
        started_at: now,
        duration: 0.0,
    });
}

/// Stop recording
#[allow(dead_code)]
pub fn stop_recording(blob_url: String, duration: f64, waveform: Vec<u8>) {
    RECORDING_STATE.write().clone_from(&RecordingState::Completed {
        blob_url,
        duration,
        waveform,
    });
}

/// Cancel recording
#[allow(dead_code)]
pub fn cancel_recording() {
    RECORDING_STATE.write().clone_from(&RecordingState::Idle);
}

/// Update recording duration
#[allow(dead_code)]
pub fn update_recording_duration(duration: f64) {
    let mut state = RECORDING_STATE.write();
    if let RecordingState::Recording { started_at, .. } = *state {
        *state = RecordingState::Recording {
            started_at,
            duration,
        };
    }
}

/// Generate waveform data from audio samples
/// Returns a vector of amplitude values (0-100) suitable for NIP-92 imeta tag
#[allow(dead_code)]
pub fn generate_waveform(samples: &[f32], target_points: usize) -> Vec<u8> {
    if samples.is_empty() {
        return vec![0; target_points];
    }

    let chunk_size = samples.len() / target_points;
    let mut waveform = Vec::with_capacity(target_points);

    for i in 0..target_points {
        let start = i * chunk_size;
        let end = ((i + 1) * chunk_size).min(samples.len());

        if start >= samples.len() {
            waveform.push(0);
            continue;
        }

        // Calculate RMS (root mean square) for this chunk
        let chunk = &samples[start..end];
        let sum_squares: f32 = chunk.iter().map(|&s| s * s).sum();
        let rms = (sum_squares / chunk.len() as f32).sqrt();

        // Convert to 0-100 scale
        let amplitude = (rms * 100.0).min(100.0) as u8;
        waveform.push(amplitude);
    }

    waveform
}

/// Format time as M:SS
pub fn format_time(seconds: f64) -> String {
    if seconds.is_nan() || seconds < 0.0 {
        return "0:00".to_string();
    }
    let mins = (seconds / 60.0).floor() as u32;
    let secs = (seconds % 60.0).floor() as u32;
    format!("{}:{:02}", mins, secs)
}
