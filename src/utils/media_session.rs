#[cfg(feature = "web")]
pub async fn update_metadata(
    title: &str,
    artist: &str,
    album: &str,
    artwork_url: Option<&str>,
) {
    let artwork_js = match artwork_url {
        Some(url) => {
            let src_json = serde_json::to_string(url).unwrap_or_else(|_| "''".to_string());
            format!("[{{ src: {}, sizes: '512x512', type: 'image/jpeg' }}]", src_json)
        }
        None => "[]".to_string(),
    };
    let title_json = serde_json::to_string(title).unwrap_or_default();
    let artist_json = serde_json::to_string(artist).unwrap_or_default();
    let album_json = serde_json::to_string(album).unwrap_or_default();
    let script = format!(
        r#"if ('mediaSession' in navigator) {{
            navigator.mediaSession.metadata = new MediaMetadata({{
                title: {title_json},
                artist: {artist_json},
                album: {album_json},
                artwork: {artwork_js}
            }});
        }}"#
    );
    let _ = dioxus::document::eval(&script).await;
}

#[cfg(feature = "web")]
pub async fn set_playback_state(playing: bool) {
    let state = if playing { "playing" } else { "paused" };
    let script = format!(
        "if ('mediaSession' in navigator) {{ navigator.mediaSession.playbackState = '{}'; }}",
        state
    );
    let _ = dioxus::document::eval(&script).await;
}

#[cfg(feature = "web")]
#[allow(dead_code)]
pub async fn set_position_state(duration: f64, position: f64, playback_rate: f64) {
    if duration <= 0.0 || duration.is_infinite() {
        return;
    }
    let script = format!(
        "if ('mediaSession' in navigator) {{ navigator.mediaSession.setPositionState({{ duration: {}, playbackRate: {}, position: {} }}); }}",
        duration, playback_rate, position
    );
    let _ = dioxus::document::eval(&script).await;
}

#[cfg(feature = "web")]
pub async fn setup_action_handlers(audio_id: &str) {
    let audio_id_json = serde_json::to_string(audio_id).unwrap_or_else(|_| "''".to_string());
    let script = format!(
        r#"if ('mediaSession' in navigator) {{
            var audio = document.getElementById({audio_id_json});
            if (audio) {{
                navigator.mediaSession.setActionHandler('play', function() {{ audio.play(); }});
                navigator.mediaSession.setActionHandler('pause', function() {{ audio.pause(); }});
                navigator.mediaSession.setActionHandler('seekto', function(details) {{
                    if (details.seekTime !== undefined) {{ audio.currentTime = details.seekTime; }}
                }});
                navigator.mediaSession.setActionHandler('nexttrack', function() {{
                    audio.dispatchEvent(new CustomEvent('mediaaction', {{ detail: 'next' }}));
                }});
                navigator.mediaSession.setActionHandler('previoustrack', function() {{
                    audio.dispatchEvent(new CustomEvent('mediaaction', {{ detail: 'previous' }}));
                }});
            }}
        }}"#
    );
    let _ = dioxus::document::eval(&script).await;
}

#[cfg(not(feature = "web"))]
pub async fn update_metadata(_: &str, _: &str, _: &str, _: Option<&str>) {}
#[cfg(not(feature = "web"))]
pub async fn set_playback_state(_: bool) {}
#[cfg(not(feature = "web"))]
#[allow(dead_code)]
pub async fn set_position_state(_: f64, _: f64, _: f64) {}
#[cfg(not(feature = "web"))]
pub async fn setup_action_handlers(_: &str) {}
