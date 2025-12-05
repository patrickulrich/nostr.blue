use nostr_sdk::prelude::*;
use once_cell::sync::Lazy;
use regex::Regex;
use ::url::Url;

// Precompiled regexes for content parsing - compiled once at startup
static URL_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"https?://[^\s]+").expect("Failed to compile URL regex")
});

/// Clean trailing punctuation from URLs that may have been captured by regex
fn clean_url_trailing_punctuation(url: &str) -> &str {
    url.trim_end_matches(|c| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}' | '\'' | '"' | '>'))
}
static NOSTR_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"nostr:(npub1|note1|nevent1|nprofile1|naddr1)[a-zA-Z0-9]+")
        .expect("Failed to compile nostr regex")
});
static HASHTAG_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"#(\w+)").expect("Failed to compile hashtag regex")
});
// Cashu tokens are base64-encoded strings starting with cashuA (V3) or cashuB (V4)
// This regex is intentionally permissive to capture most token formats.
// Invalid tokens are handled gracefully by CashuTokenCard which displays
// a fallback UI when parsing fails. This avoids false negatives from
// strict regex validation while keeping detection simple.
static CASHU_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"cashu[AB][A-Za-z0-9_=-]+").expect("Failed to compile cashu regex")
});

/// Represents different types of content tokens that can appear in a note
#[derive(Debug, Clone, PartialEq)]
pub enum ContentToken {
    Text(String),
    Link(String),
    Image(String),
    Video(String),
    // Wavlake - rendered with in-house player
    WavlakeTrack(String),    // Track ID from wavlake.com/track/{id}
    WavlakeAlbum(String),    // Album ID from wavlake.com/album/{id}
    WavlakeArtist(String),   // Artist ID from wavlake.com/artist/{id}
    WavlakePlaylist(String), // Playlist ID from wavlake.com/playlist/{id}
    // Twitter/X
    TwitterTweet(String),    // Tweet ID from twitter.com/*/status/{id}
    // Twitch
    TwitchStream(String),    // Channel name from twitch.tv/{channel}
    TwitchClip(String),      // Clip slug from clips.twitch.tv/{slug}
    TwitchVod(String),       // Video ID from twitch.tv/videos/{id}
    // Nostr references
    Mention(String),         // npub/nprofile
    EventMention(String),    // note/nevent
    Hashtag(String),
    // YouTube - separate from generic video for iframe embed
    YouTube(String),         // Video ID
    // Spotify
    SpotifyTrack(String),    // Track ID
    SpotifyAlbum(String),    // Album ID
    SpotifyPlaylist(String), // Playlist ID
    SpotifyEpisode(String),  // Podcast episode ID
    // SoundCloud
    SoundCloud(String),      // Full URL for widget
    // Apple Music
    AppleMusicAlbum(String), // Album path (region/album/name/id)
    AppleMusicPlaylist(String), // Playlist path
    AppleMusicSong(String),  // Song with ?i= parameter
    // MixCloud
    MixCloud(String, String), // (username, mix_name)
    // Rumble
    Rumble(String),          // Embed URL
    // Tidal
    Tidal(String),           // Embed URL
    // Zap.stream - Nostr live streaming
    ZapStream(String),       // naddr from zap.stream URL
    // Cashu ecash tokens
    CashuToken(String),      // cashuA.../cashuB... token string
}

/// Parse note content into structured tokens
pub fn parse_content(content: &str, _tags: &[Tag]) -> Vec<ContentToken> {
    let mut tokens = Vec::new();
    let mut last_end = 0;
    let mut matches: Vec<(usize, usize, ContentToken)> = Vec::new();

    // Find all URLs (using precompiled static regex)
    for mat in URL_PATTERN.find_iter(content) {
        let raw_url = mat.as_str();
        // Clean trailing punctuation that may have been captured
        let url = clean_url_trailing_punctuation(raw_url).to_string();
        // Adjust the end position if we trimmed punctuation
        let actual_end = mat.start() + url.len();
        let token = if is_image_url(&url) {
            ContentToken::Image(url)
        } else if let Some(video_id) = extract_youtube_id(&url) {
            // YouTube before generic video check
            ContentToken::YouTube(video_id)
        } else if is_video_url(&url) {
            ContentToken::Video(url)
        } else if let Some(track_id) = extract_wavlake_track_id(&url) {
            ContentToken::WavlakeTrack(track_id)
        } else if let Some(album_id) = extract_wavlake_album_id(&url) {
            ContentToken::WavlakeAlbum(album_id)
        } else if let Some(artist_id) = extract_wavlake_artist_id(&url) {
            ContentToken::WavlakeArtist(artist_id)
        } else if let Some(playlist_id) = extract_wavlake_playlist_id(&url) {
            ContentToken::WavlakePlaylist(playlist_id)
        } else if let Some(tweet_id) = extract_twitter_tweet_id(&url) {
            ContentToken::TwitterTweet(tweet_id)
        } else if let Some(clip_slug) = extract_twitch_clip(&url) {
            ContentToken::TwitchClip(clip_slug)
        } else if let Some(vod_id) = extract_twitch_vod(&url) {
            ContentToken::TwitchVod(vod_id)
        } else if let Some(channel) = extract_twitch_channel(&url) {
            ContentToken::TwitchStream(channel)
        } else if let Some(spotify_token) = extract_spotify(&url) {
            spotify_token
        } else if let Some(soundcloud_url) = extract_soundcloud(&url) {
            ContentToken::SoundCloud(soundcloud_url)
        } else if let Some(apple_token) = extract_apple_music(&url) {
            apple_token
        } else if let Some((username, mix)) = extract_mixcloud(&url) {
            ContentToken::MixCloud(username, mix)
        } else if let Some(embed_url) = extract_rumble(&url) {
            ContentToken::Rumble(embed_url)
        } else if let Some(embed_url) = extract_tidal(&url) {
            ContentToken::Tidal(embed_url)
        } else if let Some(naddr) = extract_zapstream(&url) {
            ContentToken::ZapStream(naddr)
        } else {
            ContentToken::Link(url)
        };
        matches.push((mat.start(), actual_end, token));
    }

    // Find all nostr: mentions (using precompiled static regex)
    for mat in NOSTR_PATTERN.find_iter(content) {
        let mention = mat.as_str().to_string();
        // Extract the bech32 part after "nostr:"
        let bech32_part = mention.strip_prefix("nostr:").unwrap_or(&mention);

        // Use SDK's NIP-19 parser for type-safe detection
        let token = match nostr_sdk::nips::nip19::Nip19::from_bech32(bech32_part) {
            Ok(nip19) => match nip19 {
                nostr_sdk::nips::nip19::Nip19::Pubkey(_) |
                nostr_sdk::nips::nip19::Nip19::Profile(_) => ContentToken::Mention(mention),
                nostr_sdk::nips::nip19::Nip19::EventId(_) |
                nostr_sdk::nips::nip19::Nip19::Event(_) |
                nostr_sdk::nips::nip19::Nip19::Coordinate(_) => ContentToken::EventMention(mention),
                _ => ContentToken::Mention(mention), // Default for unknown types
            },
            Err(_) => {
                // Fallback to string matching if parsing fails
                if mention.contains("npub1") || mention.contains("nprofile1") {
                    ContentToken::Mention(mention)
                } else {
                    ContentToken::EventMention(mention)
                }
            }
        };
        matches.push((mat.start(), mat.end(), token));
    }

    // Find all hashtags (using precompiled static regex)
    for mat in HASHTAG_PATTERN.find_iter(content) {
        let hashtag = mat.as_str()[1..].to_string(); // Remove the #
        matches.push((mat.start(), mat.end(), ContentToken::Hashtag(hashtag)));
    }

    // Find all cashu tokens (using precompiled static regex)
    for mat in CASHU_PATTERN.find_iter(content) {
        let token_str = mat.as_str().to_string();
        matches.push((mat.start(), mat.end(), ContentToken::CashuToken(token_str)));
    }

    // Sort matches by position
    matches.sort_by_key(|m| m.0);

    // Build token list with text in between, skipping overlapping matches
    for (start, end, token) in matches {
        // Skip if this match overlaps with the previous one
        if start < last_end {
            continue;
        }

        // Add text before this match
        if start > last_end {
            let text = content[last_end..start].to_string();
            if !text.is_empty() {
                tokens.push(ContentToken::Text(text));
            }
        }

        tokens.push(token);
        last_end = end;
    }

    // Add remaining text
    if last_end < content.len() {
        let text = content[last_end..].to_string();
        if !text.is_empty() {
            tokens.push(ContentToken::Text(text));
        }
    }

    // If no tokens were created, return the whole content as text
    if tokens.is_empty() {
        tokens.push(ContentToken::Text(content.to_string()));
    }

    tokens
}

/// Check if a URL points to an image
fn is_image_url(url: &str) -> bool {
    let lower = url.to_lowercase();

    // Remove query parameters to check extension
    let path = lower.split('?').next().unwrap_or(&lower);

    path.ends_with(".jpg") ||
    path.ends_with(".jpeg") ||
    path.ends_with(".png") ||
    path.ends_with(".gif") ||
    path.ends_with(".webp") ||
    path.ends_with(".svg") ||
    path.ends_with(".bmp") ||
    lower.contains("/image/") ||
    lower.contains("image")
}

/// Extract track ID from Wavlake URLs
/// Supports: https://wavlake.com/track/{id}
fn extract_wavlake_track_id(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    // Match wavlake.com/track/{id}
    if lower.contains("wavlake.com/track/") {
        if let Some(track_part) = url.split("/track/").nth(1) {
            // Extract just the ID (remove query params or trailing slashes)
            let track_id = track_part
                .split('?').next()
                .unwrap_or(track_part)
                .trim_end_matches('/')
                .to_string();
            if !track_id.is_empty() {
                return Some(track_id);
            }
        }
    }

    None
}

/// Extract album ID from Wavlake URLs
/// Supports: https://wavlake.com/album/{id}
fn extract_wavlake_album_id(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    if lower.contains("wavlake.com/album/") {
        if let Some(album_part) = url.split("/album/").nth(1) {
            let album_id = album_part
                .split('?').next()
                .unwrap_or(album_part)
                .trim_end_matches('/')
                .to_string();
            if !album_id.is_empty() {
                return Some(album_id);
            }
        }
    }

    None
}

/// Extract artist ID from Wavlake URLs
/// Supports: https://wavlake.com/artist/{id}
fn extract_wavlake_artist_id(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    if lower.contains("wavlake.com/artist/") {
        if let Some(artist_part) = url.split("/artist/").nth(1) {
            let artist_id = artist_part
                .split('?').next()
                .unwrap_or(artist_part)
                .trim_end_matches('/')
                .to_string();
            if !artist_id.is_empty() {
                return Some(artist_id);
            }
        }
    }

    None
}

/// Extract playlist ID from Wavlake URLs
/// Supports: https://wavlake.com/playlist/{id}
fn extract_wavlake_playlist_id(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    if lower.contains("wavlake.com/playlist/") {
        if let Some(playlist_part) = url.split("/playlist/").nth(1) {
            let playlist_id = playlist_part
                .split('?').next()
                .unwrap_or(playlist_part)
                .trim_end_matches('/')
                .to_string();
            if !playlist_id.is_empty() {
                return Some(playlist_id);
            }
        }
    }

    None
}

/// Check if a URL points to a video file (not YouTube - handled separately)
fn is_video_url(url: &str) -> bool {
    let lower = url.to_lowercase();

    // YouTube is handled separately with iframe embed
    if lower.contains("youtube.com") || lower.contains("youtu.be") {
        return false;
    }

    // Remove query parameters to check extension
    let path = lower.split('?').next().unwrap_or(&lower);

    path.ends_with(".mp4") ||
    path.ends_with(".webm") ||
    path.ends_with(".mov") ||
    path.ends_with(".avi") ||
    path.ends_with(".mkv") ||
    path.ends_with(".ogg") ||
    path.ends_with(".3gp") ||
    path.ends_with(".3gpp")
}

/// Extract profile name from mention string
#[allow(dead_code)]
pub fn extract_mention_name(mention: &str, _tags: &[Tag]) -> Option<String> {
    // Try to extract from nostr: URI
    if let Some(npub) = mention.strip_prefix("nostr:") {
        // For now, return shortened version
        if npub.len() > 16 {
            return Some(format!("@{}...{}", &npub[0..8], &npub[npub.len()-4..]));
        }
    }

    // TODO: Look up profile metadata and return actual name
    None
}

/// Extract tweet ID from Twitter/X URLs
/// Supports: twitter.com/*/status/{id}, x.com/*/status/{id}, mobile.twitter.com/*/status/{id}
fn extract_twitter_tweet_id(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    // Match twitter.com, x.com, or mobile.twitter.com with /status/
    if (lower.contains("twitter.com/") || lower.contains("x.com/")) && lower.contains("/status/") {
        if let Some(status_part) = url.split("/status/").nth(1) {
            // Extract just the ID (remove query params, trailing slashes, and additional path segments)
            let tweet_id = status_part
                .split('?').next()
                .unwrap_or(status_part)
                .split('/').next()
                .unwrap_or(status_part)
                .trim_end_matches('/')
                .to_string();
            if !tweet_id.is_empty() && tweet_id.chars().all(|c| c.is_numeric()) {
                return Some(tweet_id);
            }
        }
    }

    None
}

/// Extract channel name from Twitch stream URLs
/// Supports: twitch.tv/{channel}
fn extract_twitch_channel(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    // Match twitch.tv/{channel} but not /videos/, /clip/, or other paths
    if lower.contains("twitch.tv/") &&
       !lower.contains("/videos/") &&
       !lower.contains("/clip/") &&
       !lower.contains("clips.twitch.tv") {
        if let Some(channel_part) = url.split("twitch.tv/").nth(1) {
            let channel = channel_part
                .split('?').next()
                .unwrap_or(channel_part)
                .split('/').next()
                .unwrap_or(channel_part)
                .trim_end_matches('/')
                .to_string();
            // Channel names should be alphanumeric + underscores, 4-25 chars
            if !channel.is_empty() &&
               channel.len() >= 4 &&
               channel.len() <= 25 &&
               channel.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Some(channel);
            }
        }
    }

    None
}

/// Extract clip slug from Twitch clip URLs
/// Supports: clips.twitch.tv/{slug}, twitch.tv/*/clip/{slug}
fn extract_twitch_clip(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    // Match clips.twitch.tv/{slug}
    if lower.contains("clips.twitch.tv/") {
        if let Some(clip_part) = url.split("clips.twitch.tv/").nth(1) {
            let clip_slug = clip_part
                .split('?').next()
                .unwrap_or(clip_part)
                .split('/').next()
                .unwrap_or(clip_part)
                .trim_end_matches('/')
                .to_string();
            if !clip_slug.is_empty() {
                return Some(clip_slug);
            }
        }
    }

    // Match twitch.tv/*/clip/{slug}
    if lower.contains("twitch.tv/") && lower.contains("/clip/") {
        if let Some(clip_part) = url.split("/clip/").nth(1) {
            let clip_slug = clip_part
                .split('?').next()
                .unwrap_or(clip_part)
                .split('/').next()
                .unwrap_or(clip_part)
                .trim_end_matches('/')
                .to_string();
            if !clip_slug.is_empty() {
                return Some(clip_slug);
            }
        }
    }

    None
}

/// Extract video ID from Twitch VOD URLs
/// Supports: twitch.tv/videos/{id}
fn extract_twitch_vod(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    if lower.contains("twitch.tv/videos/") {
        if let Some(vod_part) = url.split("/videos/").nth(1) {
            let vod_id = vod_part
                .split('?').next()
                .unwrap_or(vod_part)
                .split('/').next()
                .unwrap_or(vod_part)
                .trim_end_matches('/')
                .to_string();
            if !vod_id.is_empty() && vod_id.chars().all(|c| c.is_numeric()) {
                return Some(vod_id);
            }
        }
    }

    None
}

/// Extract YouTube video ID from various URL formats
/// Supports: youtube.com/watch?v=ID, youtu.be/ID, youtube.com/shorts/ID,
/// youtube.com/embed/ID, youtube.com/live/ID, youtube.com/v/ID
pub fn extract_youtube_id(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    // Must be a YouTube URL
    if !lower.contains("youtube.com") && !lower.contains("youtu.be") {
        return None;
    }

    // Handle youtube.com/watch?v=ID (including with playlist params)
    // Check for ?v= or &v= to avoid matching parameters like av=, rev=, etc.
    let v_pos = url.find("?v=").or_else(|| url.find("&v="));
    if let Some(start) = v_pos {
        let id_start = start + 3; // skip ?v= or &v=
        let id = &url[id_start..];
        let id = id.split('&').next()?;
        let id = id.split('#').next()?;
        // YouTube IDs are exactly 11 characters
        if !id.is_empty() && id.len() == 11 {
            return Some(id.to_string());
        }
    }

    // Handle youtu.be/ID
    if lower.contains("youtu.be/") {
        if let Some(start) = url.find("youtu.be/") {
            let id_start = start + 9;
            let id = &url[id_start..];
            let id = id.split('?').next()?;
            let id = id.split('#').next()?;
            // YouTube IDs are exactly 11 characters
            if !id.is_empty() && id.len() == 11 {
                return Some(id.to_string());
            }
        }
    }

    // Handle youtube.com/shorts/ID
    if let Some(start) = lower.find("/shorts/") {
        let id_start = start + 8;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        // YouTube IDs are exactly 11 characters
        if !id.is_empty() && id.len() == 11 {
            return Some(id.to_string());
        }
    }

    // Handle youtube.com/embed/ID
    if let Some(start) = lower.find("/embed/") {
        let id_start = start + 7;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        // YouTube IDs are exactly 11 characters
        if !id.is_empty() && id.len() == 11 {
            return Some(id.to_string());
        }
    }

    // Handle youtube.com/live/ID
    if let Some(start) = lower.find("/live/") {
        let id_start = start + 6;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        // YouTube IDs are exactly 11 characters
        if !id.is_empty() && id.len() == 11 {
            return Some(id.to_string());
        }
    }

    // Handle youtube.com/v/ID (older embed format)
    if let Some(start) = lower.find("/v/") {
        let id_start = start + 3;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        // YouTube IDs are exactly 11 characters
        if !id.is_empty() && id.len() == 11 {
            return Some(id.to_string());
        }
    }

    None
}

/// Check if URL host is a valid Spotify domain
fn is_spotify_host(url_str: &str) -> bool {
    Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .map(|h| h == "open.spotify.com" || h == "spotify.com" || h.ends_with(".spotify.com"))
        .unwrap_or(false)
}

/// Check if URL host is a valid Tidal domain
fn is_tidal_host(url_str: &str) -> bool {
    Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .map(|h| h == "tidal.com" || h == "embed.tidal.com" || h.ends_with(".tidal.com"))
        .unwrap_or(false)
}

/// Extract Spotify content from URL
/// Supports: open.spotify.com/track/ID, /album/ID, /playlist/ID, /episode/ID
fn extract_spotify(url: &str) -> Option<ContentToken> {
    // Use proper URL parsing to check host (prevents false positives like notspotify.com)
    if !is_spotify_host(url) {
        return None;
    }

    let lower = url.to_lowercase();

    // Extract the path type and ID
    if lower.contains("/track/") {
        if let Some(track_part) = url.split("/track/").nth(1) {
            let id = track_part.split('?').next()?.split('/').next()?.to_string();
            if !id.is_empty() {
                return Some(ContentToken::SpotifyTrack(id));
            }
        }
    } else if lower.contains("/album/") {
        if let Some(album_part) = url.split("/album/").nth(1) {
            let id = album_part.split('?').next()?.split('/').next()?.to_string();
            if !id.is_empty() {
                return Some(ContentToken::SpotifyAlbum(id));
            }
        }
    } else if lower.contains("/playlist/") {
        if let Some(playlist_part) = url.split("/playlist/").nth(1) {
            let id = playlist_part.split('?').next()?.split('/').next()?.to_string();
            if !id.is_empty() {
                return Some(ContentToken::SpotifyPlaylist(id));
            }
        }
    } else if lower.contains("/episode/") {
        if let Some(episode_part) = url.split("/episode/").nth(1) {
            let id = episode_part.split('?').next()?.split('/').next()?.to_string();
            if !id.is_empty() {
                return Some(ContentToken::SpotifyEpisode(id));
            }
        }
    }

    None
}

/// Check if URL host is a valid SoundCloud domain
fn is_soundcloud_host(url_str: &str) -> bool {
    Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .map(|h| h == "soundcloud.com" || h.ends_with(".soundcloud.com"))
        .unwrap_or(false)
}

/// Extract SoundCloud URL for widget embed
/// Supports: soundcloud.com/{user}/{track}
fn extract_soundcloud(url: &str) -> Option<String> {
    // Validate host to prevent substring spoofing
    if !is_soundcloud_host(url) {
        return None;
    }

    let lower = url.to_lowercase();
    if !lower.contains("/live") {
        // Return the full URL for the widget
        return Some(url.to_string());
    }

    None
}

/// Check if URL host is a valid Apple Music domain
fn is_apple_music_host(url_str: &str) -> bool {
    Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .map(|h| h == "music.apple.com")
        .unwrap_or(false)
}

/// Extract Apple Music content from URL
/// Supports: music.apple.com/{region}/album/{name}/{id}, /playlist/{name}/{id}
fn extract_apple_music(url: &str) -> Option<ContentToken> {
    // Validate host to prevent substring spoofing
    if !is_apple_music_host(url) {
        return None;
    }

    let lower = url.to_lowercase();

    // Check if it's a song (has ?i= parameter)
    if lower.contains("?i=") {
        return Some(ContentToken::AppleMusicSong(url.to_string()));
    }

    if lower.contains("/album/") {
        return Some(ContentToken::AppleMusicAlbum(url.to_string()));
    }

    if lower.contains("/playlist/") {
        return Some(ContentToken::AppleMusicPlaylist(url.to_string()));
    }

    None
}

/// Extract MixCloud username and mix name from URL
/// Supports: mixcloud.com/{username}/{mix-name}
fn extract_mixcloud(url: &str) -> Option<(String, String)> {
    let lower = url.to_lowercase();

    if !lower.contains("mixcloud.com/") || lower.contains("/live") {
        return None;
    }

    // Extract path after mixcloud.com/
    if let Some(path_part) = url.split("mixcloud.com/").nth(1) {
        let parts: Vec<&str> = path_part.trim_end_matches('/').split('/').collect();
        if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }

    None
}

/// Extract Rumble embed URL
/// Only supports: rumble.com/embed/{id}
/// Non-embed URLs are not supported as the renderer cannot convert them.
fn extract_rumble(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    if lower.contains("rumble.com/embed/") {
        return Some(url.to_string());
    }

    None
}

/// Extract Tidal embed URL
/// Supports: embed.tidal.com/{type}/{id}, tidal.com/browse/{type}/{id}
fn extract_tidal(url: &str) -> Option<String> {
    // Use proper URL parsing to check host (prevents false positives like faketidal.com)
    if is_tidal_host(url) {
        return Some(url.to_string());
    }

    None
}

/// Extract naddr from zap.stream URL
/// Supports: zap.stream/naddr1...
fn extract_zapstream(url: &str) -> Option<String> {
    let lower = url.to_lowercase();

    if !lower.contains("zap.stream") {
        return None;
    }

    // Extract naddr from URL
    if let Some(naddr_start) = url.find("naddr1") {
        let naddr = &url[naddr_start..];
        // Extract just the naddr (stop at query params or hash)
        let naddr = naddr.split('?').next()?.split('#').next()?.split('/').next()?;
        // Validate the extracted string still starts with naddr1 and has reasonable length
        if naddr.starts_with("naddr1") && naddr.len() > 10 {
            return Some(naddr.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plain_text() {
        let tokens = parse_content("Hello, world!", &[]);
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0], ContentToken::Text(_)));
    }

    #[test]
    fn test_parse_with_url() {
        let tokens = parse_content("Check out https://example.com for more info", &[]);
        assert_eq!(tokens.len(), 3);
        assert!(matches!(tokens[0], ContentToken::Text(_)));
        assert!(matches!(tokens[1], ContentToken::Link(_)));
        assert!(matches!(tokens[2], ContentToken::Text(_)));
    }

    #[test]
    fn test_parse_with_image() {
        let tokens = parse_content("Look at this https://example.com/image.jpg", &[]);
        assert!(tokens.iter().any(|t| matches!(t, ContentToken::Image(_))));
    }

    #[test]
    fn test_parse_with_hashtag() {
        let tokens = parse_content("This is #nostr content", &[]);
        assert!(tokens.iter().any(|t| matches!(t, ContentToken::Hashtag(_))));
    }

    #[test]
    fn test_parse_image_with_query_params() {
        let tokens = parse_content(
            "Check out https://example.com/photo.jpeg?timestamp=123456",
            &[]
        );
        assert!(tokens.iter().any(|t| matches!(t, ContentToken::Image(_))));
    }

    #[test]
    fn test_parse_multiple_images() {
        let content = "Look at these cats!\n\
            https://example.com/cat1.jpeg?1234\n\
            https://example.com/cat2.jpg?5678\n\
            https://example.com/cat3.png?9012";
        let tokens = parse_content(content, &[]);
        let image_count = tokens.iter().filter(|t| matches!(t, ContentToken::Image(_))).count();
        assert_eq!(image_count, 3);
    }

    #[test]
    fn test_parse_cashu_token_v3() {
        // V3 tokens start with cashuA
        let tokens = parse_content("cashuAeyJwYXlsb2FkIjp7fX0=", &[]);
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], ContentToken::CashuToken(t) if t.starts_with("cashuA")));
    }

    #[test]
    fn test_parse_cashu_token_v4() {
        // V4 tokens start with cashuB
        let tokens = parse_content("cashuBeyJwYXlsb2FkIjp7fX0=", &[]);
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], ContentToken::CashuToken(t) if t.starts_with("cashuB")));
    }

    #[test]
    fn test_parse_cashu_token_in_content() {
        let tokens = parse_content("Check this token cashuAeyJwYXlsb2FkIjp7fX0= for payment", &[]);
        assert_eq!(tokens.len(), 3); // Text, CashuToken, Text
        assert!(matches!(&tokens[0], ContentToken::Text(_)));
        assert!(matches!(&tokens[1], ContentToken::CashuToken(_)));
        assert!(matches!(&tokens[2], ContentToken::Text(_)));
    }
}
