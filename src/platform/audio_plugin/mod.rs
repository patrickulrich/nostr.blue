#![allow(non_snake_case)]

use crate::stores::music_player::MusicTrack;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AndroidPlaybackSnapshot {
    #[serde(default)]
    pub queue_len: usize,
    #[serde(default)]
    pub current_index: usize,
    #[serde(default)]
    pub is_playing: bool,
    #[serde(default)]
    pub is_buffering: bool,
    #[serde(default)]
    pub current_time: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub playback_error: Option<String>,
}

static PLUGIN: OnceLock<Result<AudioPlugin, String>> = OnceLock::new();

fn get_plugin() -> Result<&'static AudioPlugin, String> {
    PLUGIN
        .get_or_init(AudioPlugin::new)
        .as_ref()
        .map_err(|e| e.clone())
}

fn expect_ok(result: String) -> Result<(), String> {
    if let Some(error) = result.strip_prefix("error:") {
        Err(error.to_string())
    } else {
        Ok(())
    }
}

pub fn set_queue(
    queue: &[MusicTrack],
    current_index: usize,
    play_when_ready: bool,
) -> Result<(), String> {
    let plugin = get_plugin()?;
    let queue_json = serde_json::to_string(queue).map_err(|e| e.to_string())?;
    let result = ffi::setQueue(plugin, queue_json, current_index as i32, play_when_ready)?;
    expect_ok(result)
}

pub fn play() -> Result<(), String> {
    let plugin = get_plugin()?;
    expect_ok(ffi::play(plugin)?)
}

pub fn pause() -> Result<(), String> {
    let plugin = get_plugin()?;
    expect_ok(ffi::pause(plugin)?)
}

pub fn next_track() -> Result<(), String> {
    let plugin = get_plugin()?;
    expect_ok(ffi::skipNext(plugin)?)
}

pub fn previous_track() -> Result<(), String> {
    let plugin = get_plugin()?;
    expect_ok(ffi::skipPrevious(plugin)?)
}

pub fn stop() -> Result<(), String> {
    let plugin = get_plugin()?;
    expect_ok(ffi::stop(plugin)?)
}

pub fn clear_queue() -> Result<(), String> {
    let plugin = get_plugin()?;
    expect_ok(ffi::clearQueue(plugin)?)
}

pub fn set_volume(volume: f64) -> Result<(), String> {
    let plugin = get_plugin()?;
    expect_ok(ffi::setVolume(plugin, volume as f32)?)
}

pub fn set_playback_speed(speed: f64) -> Result<(), String> {
    let plugin = get_plugin()?;
    expect_ok(ffi::setPlaybackSpeed(plugin, speed as f32)?)
}

pub fn seek_to(position_seconds: f64) -> Result<(), String> {
    let plugin = get_plugin()?;
    expect_ok(ffi::seekTo(
        plugin,
        (position_seconds.max(0.0) * 1000.0) as i64,
    )?)
}

pub fn snapshot() -> Result<AndroidPlaybackSnapshot, String> {
    let plugin = get_plugin()?;
    let json = ffi::getSnapshot(plugin)?;
    if let Some(error) = json.strip_prefix("error:") {
        return Err(error.to_string());
    }
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
mod ffi {
    #[manganis::ffi("src/platform/audio_plugin/android")]
    extern "Kotlin" {
        pub type AudioPlugin;
        pub fn setQueue(
            this: &AudioPlugin,
            queueJson: String,
            startIndex: i32,
            playWhenReady: bool,
        ) -> String;
        pub fn play(this: &AudioPlugin) -> String;
        pub fn pause(this: &AudioPlugin) -> String;
        pub fn skipNext(this: &AudioPlugin) -> String;
        pub fn skipPrevious(this: &AudioPlugin) -> String;
        pub fn seekTo(this: &AudioPlugin, positionMs: i64) -> String;
        pub fn setPlaybackSpeed(this: &AudioPlugin, speed: f32) -> String;
        pub fn setVolume(this: &AudioPlugin, volume: f32) -> String;
        pub fn stop(this: &AudioPlugin) -> String;
        pub fn clearQueue(this: &AudioPlugin) -> String;
        pub fn getSnapshot(this: &AudioPlugin) -> String;
    }
}

#[cfg(not(target_os = "android"))]
mod ffi {
    pub struct AudioPlugin;

    impl AudioPlugin {
        pub fn new() -> Result<Self, String> {
            Err("AudioPlugin is only available on Android".to_string())
        }
    }

    pub fn setQueue(
        _this: &AudioPlugin,
        _queue_json: String,
        _start_index: i32,
        _play_when_ready: bool,
    ) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
    pub fn play(_this: &AudioPlugin) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
    pub fn pause(_this: &AudioPlugin) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
    pub fn skipNext(_this: &AudioPlugin) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
    pub fn skipPrevious(_this: &AudioPlugin) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
    pub fn seekTo(_this: &AudioPlugin, _position_ms: i64) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
    pub fn setPlaybackSpeed(_this: &AudioPlugin, _speed: f32) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
    pub fn setVolume(_this: &AudioPlugin, _volume: f32) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
    pub fn stop(_this: &AudioPlugin) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
    pub fn clearQueue(_this: &AudioPlugin) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
    pub fn getSnapshot(_this: &AudioPlugin) -> Result<String, String> {
        Err("AudioPlugin not available".to_string())
    }
}

pub use ffi::AudioPlugin;
