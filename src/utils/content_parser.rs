use nostr_sdk::prelude::*;
use regex::Regex;

/// Represents different types of content tokens that can appear in a note
#[derive(Debug, Clone, PartialEq)]
pub enum ContentToken {
    Text(String),
    Link(String),
    Image(String),
    Video(String),
    WavlakeTrack(String),    // Track ID from wavlake.com/track/{id}
    WavlakeAlbum(String),    // Album ID from wavlake.com/album/{id}
    WavlakeArtist(String),   // Artist ID from wavlake.com/artist/{id}
    WavlakePlaylist(String), // Playlist ID from wavlake.com/playlist/{id}
    TwitterTweet(String),    // Tweet ID from twitter.com/*/status/{id}
    TwitchStream(String),    // Channel name from twitch.tv/{channel}
    TwitchClip(String),      // Clip slug from clips.twitch.tv/{slug}
    TwitchVod(String),       // Video ID from twitch.tv/videos/{id}
    Mention(String),         // npub/nprofile
    EventMention(String),    // note/nevent
    Hashtag(String),
}

/// Parse note content into structured tokens
pub fn parse_content(content: &str, _tags: &[Tag]) -> Vec<ContentToken> {
    let mut tokens = Vec::new();

    // Regex patterns
    let url_pattern = Regex::new(r"https?://[^\s]+").unwrap();
    let nostr_pattern = Regex::new(r"nostr:(npub1|note1|nevent1|nprofile1|naddr1)[a-zA-Z0-9]+").unwrap();
    let hashtag_pattern = Regex::new(r"#(\w+)").unwrap();

    let mut last_end = 0;
    let mut matches: Vec<(usize, usize, ContentToken)> = Vec::new();

    // Find all URLs
    for mat in url_pattern.find_iter(content) {
        let url = mat.as_str().to_string();
        let token = if is_image_url(&url) {
            ContentToken::Image(url)
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
        } else {
            ContentToken::Link(url)
        };
        matches.push((mat.start(), mat.end(), token));
    }

    // Find all nostr: mentions
    for mat in nostr_pattern.find_iter(content) {
        let mention = mat.as_str().to_string();
        let token = if mention.contains("npub1") || mention.contains("nprofile1") {
            ContentToken::Mention(mention)
        } else {
            ContentToken::EventMention(mention)
        };
        matches.push((mat.start(), mat.end(), token));
    }

    // Find all hashtags
    for mat in hashtag_pattern.find_iter(content) {
        let hashtag = mat.as_str()[1..].to_string(); // Remove the #
        matches.push((mat.start(), mat.end(), ContentToken::Hashtag(hashtag)));
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

/// Check if a URL points to a video
fn is_video_url(url: &str) -> bool {
    let lower = url.to_lowercase();

    // Remove query parameters to check extension
    let path = lower.split('?').next().unwrap_or(&lower);

    path.ends_with(".mp4") ||
    path.ends_with(".webm") ||
    path.ends_with(".mov") ||
    path.ends_with(".avi") ||
    path.ends_with(".mkv") ||
    lower.contains("youtube.com") ||
    lower.contains("youtu.be") ||
    lower.contains("/video/") ||
    lower.contains("video")
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
}
