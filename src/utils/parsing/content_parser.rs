use nostr_sdk::prelude::*;
use once_cell::sync::Lazy;
use regex::Regex;
use ::url::Url;
/// Minimum length for npub identifiers to be considered valid for display
const MIN_NPUB_LENGTH: usize = 16;
/// Minimum length for naddr identifiers to be considered valid
const MIN_NADDR_LENGTH: usize = 10;
/// YouTube video IDs are exactly 11 characters
const YOUTUBE_VIDEO_ID_LENGTH: usize = 11;
static URL_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"https?://[^\s]+").expect("Failed to compile URL regex"));
/// Clean trailing punctuation from URLs that may have been captured by regex
fn clean_url_trailing_punctuation(url: &str) -> &str {
    url.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"', '>'])
}
static NOSTR_URI_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)nostr:(npub1|nprofile1|note1|nevent1|naddr1)[a-zA-Z0-9]+")
        .expect("Failed to compile nostr URI regex")
});
static HASHTAG_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"#(\w+)").expect("Failed to compile hashtag regex"));
#[cfg(feature = "cashu")]
static CASHU_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"cashu[AB][A-Za-z0-9_=-]+").expect("Failed to compile cashu regex"));
static ISBN_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"isbn:[\dXx-]{10,17}").expect("Failed to compile ISBN regex"));
static DOI_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"doi:10\.[^\s]+").expect("Failed to compile DOI regex"));
static ISAN_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"isan:[0-9A-Fa-f-]{16,32}").expect("Failed to compile ISAN regex"));
static PODCAST_FEED_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"podcast:guid:[A-Za-z0-9_-]+").expect("Failed to compile podcast feed regex")
});
static PODCAST_EPISODE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"podcast:item:guid:[A-Za-z0-9_-]+")
        .expect("Failed to compile podcast episode regex")
});
static BITCOIN_TX_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"bitcoin:tx:[a-fA-F0-9]{64}").expect("Failed to compile bitcoin tx regex")
});
static BITCOIN_ADDRESS_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"bitcoin:address:(?:bc1[a-zA-HJ-NP-Z0-9]{39,87}|[13][a-km-zA-HJ-NP-Z1-9]{25,34})")
        .expect("Failed to compile bitcoin address regex")
});
static GEOHASH_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"geo:[0-9a-z]{1,12}").expect("Failed to compile geohash regex"));
/// Represents different types of content tokens that can appear in a note
#[derive(Debug, Clone, PartialEq)]
pub enum ContentToken {
    Text(String),
    Link(String),
    Image(String),
    Video(String),
    WavlakeTrack(String),
    WavlakeAlbum(String),
    WavlakeArtist(String),
    WavlakePlaylist(String),
    TwitterTweet(String),
    TwitchStream(String),
    TwitchClip(String),
    TwitchVod(String),
    Mention(String),
    EventMention(String),
    Hashtag(String),
    YouTube(String),
    SpotifyTrack(String),
    SpotifyAlbum(String),
    SpotifyPlaylist(String),
    SpotifyEpisode(String),
    SoundCloud(String),
    AppleMusicAlbum(String),
    AppleMusicPlaylist(String),
    AppleMusicSong(String),
    MixCloud(String, String),
    Rumble(String),
    Tidal(String),
    ZapStream(String),
    ZapCookingRecipe(String),
    #[allow(dead_code)]
    CashuToken(String),
    Isbn(String),
    Doi(String),
    Isan(String),
    PodcastFeed(String),
    PodcastEpisode(String),
    BitcoinTx(String),
    BitcoinAddress(String),
    Geohash(String),
    NostrBlueLiveStream(String),
    NostrBlueVideo(String),
    NostrBluePhoto(String),
    NostrBlueVoice(String),
    NostrBluePodcastShow(String),
    NostrBluePodcastEpisode(String),
    NostrBlueMusicPlaylist(String),
    NostrBlueRadioStation(String),
    NostrBlueRssMusicAlbum(String),
    NostrBlueTrack(String),
    NostrBlueAlbum(String),
    NostrBlueArtist(String),
    NostrBlueArticle(String),
    NostrBlueRecipe(String),
    NostrBlueNote(String),
    NostrBlueProfile(String),
    NostrBlueCalendarEvent(String),
    NostrBlueWiki(String),
    NostrBluePublication(String),
    NostrBluePinboard(String),
    NostrBlueBadge(String),
    NostrBlueProduct(String),
    NostrBlueCodeRepo(String),
    NostrBlueCommunity(String),
    NostrBlueChannel(String),
    NostrBlueRssPodcastEpisode(String, String),
    NostrBlueRssPodcastShow(String),
}
/// Parse note content into structured tokens
pub fn parse_content(content: &str, _tags: &[Tag]) -> Vec<ContentToken> {
    let mut tokens = Vec::new();
    let mut last_end = 0;
    let mut matches: Vec<(usize, usize, ContentToken)> = Vec::new();
    for mat in URL_PATTERN.find_iter(content) {
        let raw_url = mat.as_str();
        let url = clean_url_trailing_punctuation(raw_url).to_string();
        let actual_end = mat.start() + url.len();
        let token = if is_image_url(&url) {
            ContentToken::Image(url)
        } else if let Some(video_id) = extract_youtube_id(&url) {
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
        } else if let Some(naddr) = extract_zapcooking(&url) {
            ContentToken::ZapCookingRecipe(naddr)
        } else if let Some(nostr_blue_token) = extract_nostr_blue(&url) {
            nostr_blue_token
        } else {
            ContentToken::Link(url)
        };
        matches.push((mat.start(), actual_end, token));
    }
    for mat in NOSTR_URI_PATTERN.find_iter(content) {
        let uri = mat.as_str();
        let identifier = if uri.len() > 6 && uri[..6].eq_ignore_ascii_case("nostr:") {
            &uri[6..]
        } else {
            uri
        };
        let identifier_lower = identifier.to_lowercase();
        let content_token = if let Ok(nip19) = Nip19::from_bech32(&identifier_lower) {
            match nip19 {
                Nip19::Pubkey(_) | Nip19::Profile(_) => {
                    Some(ContentToken::Mention(uri.to_string()))
                }
                Nip19::EventId(_) | Nip19::Event(_) | Nip19::Coordinate(_) => {
                    Some(ContentToken::EventMention(uri.to_string()))
                }
                Nip19::Secret(_) | Nip19::EncryptedSecret(_) => None,
            }
        } else if identifier_lower.starts_with("npub1") || identifier_lower.starts_with("nprofile1")
        {
            Some(ContentToken::Mention(uri.to_string()))
        } else if identifier_lower.starts_with("note1")
            || identifier_lower.starts_with("nevent1")
            || identifier_lower.starts_with("naddr1")
        {
            Some(ContentToken::EventMention(uri.to_string()))
        } else {
            None
        };
        if let Some(token) = content_token {
            matches.push((mat.start(), mat.end(), token));
        }
    }
    for mat in HASHTAG_PATTERN.find_iter(content) {
        let hashtag = mat.as_str()[1..].to_string();
        matches.push((mat.start(), mat.end(), ContentToken::Hashtag(hashtag)));
    }
    #[cfg(feature = "cashu")]
    for mat in CASHU_PATTERN.find_iter(content) {
        let token_str = mat.as_str().to_string();
        matches.push((mat.start(), mat.end(), ContentToken::CashuToken(token_str)));
    }
    for mat in ISBN_PATTERN.find_iter(content) {
        let isbn = mat
            .as_str()
            .strip_prefix("isbn:")
            .unwrap_or(mat.as_str())
            .to_string();
        matches.push((mat.start(), mat.end(), ContentToken::Isbn(isbn)));
    }
    for mat in DOI_PATTERN.find_iter(content) {
        let doi = mat
            .as_str()
            .strip_prefix("doi:")
            .unwrap_or(mat.as_str())
            .to_string();
        matches.push((mat.start(), mat.end(), ContentToken::Doi(doi)));
    }
    for mat in ISAN_PATTERN.find_iter(content) {
        let isan = mat
            .as_str()
            .strip_prefix("isan:")
            .unwrap_or(mat.as_str())
            .to_string();
        matches.push((mat.start(), mat.end(), ContentToken::Isan(isan)));
    }
    for mat in PODCAST_FEED_PATTERN.find_iter(content) {
        let guid = mat
            .as_str()
            .strip_prefix("podcast:guid:")
            .unwrap_or(mat.as_str())
            .to_string();
        matches.push((mat.start(), mat.end(), ContentToken::PodcastFeed(guid)));
    }
    for mat in PODCAST_EPISODE_PATTERN.find_iter(content) {
        let guid = mat
            .as_str()
            .strip_prefix("podcast:item:guid:")
            .unwrap_or(mat.as_str())
            .to_string();
        matches.push((mat.start(), mat.end(), ContentToken::PodcastEpisode(guid)));
    }
    for mat in BITCOIN_TX_PATTERN.find_iter(content) {
        let txid = mat
            .as_str()
            .strip_prefix("bitcoin:tx:")
            .unwrap_or(mat.as_str())
            .to_string();
        matches.push((mat.start(), mat.end(), ContentToken::BitcoinTx(txid)));
    }
    for mat in BITCOIN_ADDRESS_PATTERN.find_iter(content) {
        let address = mat
            .as_str()
            .strip_prefix("bitcoin:address:")
            .unwrap_or(mat.as_str())
            .to_string();
        matches.push((
            mat.start(),
            mat.end(),
            ContentToken::BitcoinAddress(address),
        ));
    }
    for mat in GEOHASH_PATTERN.find_iter(content) {
        let geohash = mat
            .as_str()
            .strip_prefix("geo:")
            .unwrap_or(mat.as_str())
            .to_string();
        matches.push((mat.start(), mat.end(), ContentToken::Geohash(geohash)));
    }
    matches.sort_by_key(|m| m.0);
    for (start, end, token) in matches {
        if start < last_end {
            continue;
        }
        if start > last_end {
            let text = content[last_end..start].to_string();
            if !text.is_empty() {
                tokens.push(ContentToken::Text(text));
            }
        }
        tokens.push(token);
        last_end = end;
    }
    if last_end < content.len() {
        let text = content[last_end..].to_string();
        if !text.is_empty() {
            tokens.push(ContentToken::Text(text));
        }
    }
    if tokens.is_empty() {
        tokens.push(ContentToken::Text(content.to_string()));
    }
    tokens
}
/// Check if a URL points to an image
fn is_image_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    let path = lower.split('?').next().unwrap_or(&lower);
    path.ends_with(".jpg")
        || path.ends_with(".jpeg")
        || path.ends_with(".png")
        || path.ends_with(".gif")
        || path.ends_with(".webp")
        || path.ends_with(".svg")
        || path.ends_with(".bmp")
        || lower.contains("/image/")
        || lower.contains("image")
}
/// Extract track ID from Wavlake URLs
/// Supports: https://wavlake.com/track/{id}
fn extract_wavlake_track_id(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    if lower.contains("wavlake.com/track/") {
        if let Some(track_part) = url.split("/track/").nth(1) {
            let track_id = track_part
                .split('?')
                .next()
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
                .split('?')
                .next()
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
                .split('?')
                .next()
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
                .split('?')
                .next()
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
    if lower.contains("youtube.com") || lower.contains("youtu.be") {
        return false;
    }
    let path = lower.split('?').next().unwrap_or(&lower);
    path.ends_with(".mp4")
        || path.ends_with(".webm")
        || path.ends_with(".mov")
        || path.ends_with(".avi")
        || path.ends_with(".mkv")
        || path.ends_with(".ogg")
        || path.ends_with(".3gp")
        || path.ends_with(".3gpp")
}
/// Extract profile name from mention string
#[allow(dead_code)]
pub fn extract_mention_name(mention: &str, _tags: &[Tag]) -> Option<String> {
    if let Some(npub) = mention.strip_prefix("nostr:") {
        if npub.len() > MIN_NPUB_LENGTH {
            return Some(format!("@{}...{}", &npub[0..8], &npub[npub.len() - 4..]));
        }
    }
    None
}
/// Extract tweet ID from Twitter/X URLs
/// Supports: twitter.com/*/status/{id}, x.com/*/status/{id}, mobile.twitter.com/*/status/{id}
fn extract_twitter_tweet_id(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    if (lower.contains("twitter.com/") || lower.contains("x.com/")) && lower.contains("/status/") {
        if let Some(status_part) = url.split("/status/").nth(1) {
            let tweet_id = status_part
                .split('?')
                .next()
                .unwrap_or(status_part)
                .split('/')
                .next()
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
    if lower.contains("twitch.tv/")
        && !lower.contains("/videos/")
        && !lower.contains("/clip/")
        && !lower.contains("clips.twitch.tv")
    {
        if let Some(channel_part) = url.split("twitch.tv/").nth(1) {
            let channel = channel_part
                .split('?')
                .next()
                .unwrap_or(channel_part)
                .split('/')
                .next()
                .unwrap_or(channel_part)
                .trim_end_matches('/')
                .to_string();
            if !channel.is_empty()
                && channel.len() >= 4
                && channel.len() <= 25
                && channel.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
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
    if lower.contains("clips.twitch.tv/") {
        if let Some(clip_part) = url.split("clips.twitch.tv/").nth(1) {
            let clip_slug = clip_part
                .split('?')
                .next()
                .unwrap_or(clip_part)
                .split('/')
                .next()
                .unwrap_or(clip_part)
                .trim_end_matches('/')
                .to_string();
            if !clip_slug.is_empty() {
                return Some(clip_slug);
            }
        }
    }
    if lower.contains("twitch.tv/") && lower.contains("/clip/") {
        if let Some(clip_part) = url.split("/clip/").nth(1) {
            let clip_slug = clip_part
                .split('?')
                .next()
                .unwrap_or(clip_part)
                .split('/')
                .next()
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
                .split('?')
                .next()
                .unwrap_or(vod_part)
                .split('/')
                .next()
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
    if !lower.contains("youtube.com") && !lower.contains("youtu.be") {
        return None;
    }
    let v_pos = url.find("?v=").or_else(|| url.find("&v="));
    if let Some(start) = v_pos {
        let id_start = start + 3;
        let id = &url[id_start..];
        let id = id.split('&').next()?;
        let id = id.split('#').next()?;
        if !id.is_empty() && id.len() == YOUTUBE_VIDEO_ID_LENGTH {
            return Some(id.to_string());
        }
    }
    if lower.contains("youtu.be/") {
        if let Some(start) = url.find("youtu.be/") {
            let id_start = start + 9;
            let id = &url[id_start..];
            let id = id.split('?').next()?;
            let id = id.split('#').next()?;
            if !id.is_empty() && id.len() == YOUTUBE_VIDEO_ID_LENGTH {
                return Some(id.to_string());
            }
        }
    }
    if let Some(start) = lower.find("/shorts/") {
        let id_start = start + 8;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        if !id.is_empty() && id.len() == YOUTUBE_VIDEO_ID_LENGTH {
            return Some(id.to_string());
        }
    }
    if let Some(start) = lower.find("/embed/") {
        let id_start = start + 7;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        if !id.is_empty() && id.len() == YOUTUBE_VIDEO_ID_LENGTH {
            return Some(id.to_string());
        }
    }
    if let Some(start) = lower.find("/live/") {
        let id_start = start + 6;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        if !id.is_empty() && id.len() == YOUTUBE_VIDEO_ID_LENGTH {
            return Some(id.to_string());
        }
    }
    if let Some(start) = lower.find("/v/") {
        let id_start = start + 3;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        if !id.is_empty() && id.len() == YOUTUBE_VIDEO_ID_LENGTH {
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
    if !is_spotify_host(url) {
        return None;
    }
    let lower = url.to_lowercase();
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
            let id = playlist_part
                .split('?')
                .next()?
                .split('/')
                .next()?
                .to_string();
            if !id.is_empty() {
                return Some(ContentToken::SpotifyPlaylist(id));
            }
        }
    } else if lower.contains("/episode/") {
        if let Some(episode_part) = url.split("/episode/").nth(1) {
            let id = episode_part
                .split('?')
                .next()?
                .split('/')
                .next()?
                .to_string();
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
    if !is_soundcloud_host(url) {
        return None;
    }
    let lower = url.to_lowercase();
    if !lower.contains("/live") {
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
    if !is_apple_music_host(url) {
        return None;
    }
    let lower = url.to_lowercase();
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
    if is_tidal_host(url) {
        return Some(url.to_string());
    }
    None
}
/// Check if URL host is a valid zap.stream domain
fn is_zapstream_host(url_str: &str) -> bool {
    Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .map(|h| h == "zap.stream" || h.ends_with(".zap.stream"))
        .unwrap_or(false)
}
/// Extract naddr from zap.stream URL
/// Supports: zap.stream/naddr1...
fn extract_zapstream(url: &str) -> Option<String> {
    if !is_zapstream_host(url) {
        return None;
    }
    if let Some(naddr_start) = url.find("naddr1") {
        let naddr = &url[naddr_start..];
        let naddr = naddr
            .split('?')
            .next()?
            .split('#')
            .next()?
            .split('/')
            .next()?;
        if naddr.starts_with("naddr1") && naddr.len() > MIN_NADDR_LENGTH {
            return Some(naddr.to_string());
        }
    }
    None
}
/// Check if URL host is a valid zap.cooking domain
fn is_zapcooking_host(url_str: &str) -> bool {
    Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .map(|h| h == "zap.cooking" || h.ends_with(".zap.cooking"))
        .unwrap_or(false)
}
/// Extract naddr from zap.cooking recipe URL
/// Supports: zap.cooking/recipe/naddr1...
fn extract_zapcooking(url: &str) -> Option<String> {
    if !is_zapcooking_host(url) {
        return None;
    }
    let lower = url.to_lowercase();
    if !lower.contains("/recipe/") {
        return None;
    }
    if let Some(naddr_start) = url.find("naddr1") {
        let naddr = &url[naddr_start..];
        let naddr = naddr
            .split('?')
            .next()?
            .split('#')
            .next()?
            .split('/')
            .next()?;
        if naddr.starts_with("naddr1") && naddr.len() > MIN_NADDR_LENGTH {
            return Some(naddr.to_string());
        }
    }
    None
}
/// Check if URL host is a valid nostr.blue domain (including localhost for development)
fn is_nostr_blue_host(url_str: &str) -> bool {
    Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .map(|h| {
            h == "nostr.blue" || h.ends_with(".nostr.blue") || h == "localhost" || h == "127.0.0.1"
        })
        .unwrap_or(false)
}
/// Extract content type and identifier from nostr.blue URLs
/// Returns appropriate ContentToken variant based on URL path
fn extract_nostr_blue(url: &str) -> Option<ContentToken> {
    if !is_nostr_blue_host(url) {
        return None;
    }
    let parsed = Url::parse(url).ok()?;
    let path = parsed.path();
    if path.starts_with("/videos/live/") {
        return extract_id_from_path(path, "/videos/live/").map(ContentToken::NostrBlueLiveStream);
    }
    if path.starts_with("/videos/")
        && !path.starts_with("/videos/live")
        && !path.starts_with("/videos/verts")
        && !path.starts_with("/videos/new")
    {
        return extract_id_from_path(path, "/videos/").map(ContentToken::NostrBlueVideo);
    }
    if path.starts_with("/photos/") && !path.starts_with("/photos/new") {
        return extract_id_from_path(path, "/photos/").map(ContentToken::NostrBluePhoto);
    }
    if path.starts_with("/voicemessages/") && !path.starts_with("/voicemessages/new") {
        return extract_id_from_path(path, "/voicemessages/").map(ContentToken::NostrBlueVoice);
    }
    if path.starts_with("/podcast/rss/episode/") {
        if let Some(remainder) = path.strip_prefix("/podcast/rss/episode/") {
            let parts: Vec<&str> = remainder.split('/').collect();
            if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
                let podcast_id = parts[0].to_string();
                let episode_id = parts[1]
                    .split(['?', '#'])
                    .next()
                    .unwrap_or(parts[1])
                    .to_string();
                return Some(ContentToken::NostrBlueRssPodcastEpisode(
                    podcast_id, episode_id,
                ));
            }
        }
    }
    if path.starts_with("/podcast/rss/") && !path.starts_with("/podcast/rss/episode/") {
        return extract_id_from_path(path, "/podcast/rss/")
            .map(ContentToken::NostrBlueRssPodcastShow);
    }
    if path.starts_with("/podcast/nostr/episode/") {
        return extract_id_from_path(path, "/podcast/nostr/episode/")
            .map(ContentToken::NostrBluePodcastEpisode);
    }
    if path.starts_with("/podcast/nostr/") {
        return extract_id_from_path(path, "/podcast/nostr/")
            .map(ContentToken::NostrBluePodcastShow);
    }
    if path.starts_with("/music/rss/album/") {
        return extract_id_from_path(path, "/music/rss/album/")
            .map(ContentToken::NostrBlueRssMusicAlbum);
    }
    if path.starts_with("/music/track/") && !path.starts_with("/music/track/new") {
        return extract_id_from_path(path, "/music/track/").map(ContentToken::NostrBlueTrack);
    }
    if path.starts_with("/music/album/") {
        return extract_id_from_path(path, "/music/album/").map(ContentToken::NostrBlueAlbum);
    }
    if path.starts_with("/music/artist/") {
        return extract_id_from_path(path, "/music/artist/")
            .map(ContentToken::NostrBlueArtist);
    }
    if path.starts_with("/music/playlist/") && !path.starts_with("/music/playlist/new") {
        return extract_id_from_path(path, "/music/playlist/")
            .map(ContentToken::NostrBlueMusicPlaylist);
    }
    if path.starts_with("/radio/") && !path.starts_with("/radio/new") {
        return extract_id_from_path(path, "/radio/").map(ContentToken::NostrBlueRadioStation);
    }
    if path.starts_with("/articles/") && !path.starts_with("/articles/new") {
        return extract_id_from_path(path, "/articles/").map(ContentToken::NostrBlueArticle);
    }
    if path.starts_with("/recipes/")
        && !path.starts_with("/recipes/new")
        && !path.starts_with("/recipes/fork")
        && !path.starts_with("/recipes/all")
        && !path.starts_with("/recipes/tag")
        && !path.starts_with("/recipes/chef")
    {
        return extract_id_from_path(path, "/recipes/").map(ContentToken::NostrBlueRecipe);
    }
    if path.starts_with("/note/") {
        return extract_id_from_path(path, "/note/").map(ContentToken::NostrBlueNote);
    }
    if path.starts_with("/profile/") {
        return extract_id_from_path(path, "/profile/").map(ContentToken::NostrBlueProfile);
    }
    if path.starts_with("/calendar/") && !path.starts_with("/calendar/new") && path != "/calendar" {
        return extract_id_from_path(path, "/calendar/").map(ContentToken::NostrBlueCalendarEvent);
    }
    if path.starts_with("/wiki/")
        && !path.starts_with("/wiki/new")
        && !path.starts_with("/wiki/author")
    {
        let rest = &path["/wiki/".len()..];
        if rest.starts_with("npub1/") {
            let parts: Vec<&str> = rest.splitn(2, '/').collect();
            if parts.len() == 2 {
                return Some(ContentToken::NostrBlueWiki(format!("{}/{}", parts[0], parts[1])));
            }
        }
        return extract_id_from_path(path, "/wiki/").map(ContentToken::NostrBlueWiki);
    }
    if path.starts_with("/publications/")
        && !path.starts_with("/publications/new")
        && !path.starts_with("/publications/search")
    {
        return extract_id_from_path(path, "/publications/")
            .map(ContentToken::NostrBluePublication);
    }
    if path.starts_with("/pinboards/")
        && !path.starts_with("/pinboards/new")
        && !path.starts_with("/pinboards/pin")
        && !path.starts_with("/pinboards/pins")
        && !path.contains("/edit")
    {
        return extract_id_from_path(path, "/pinboards/").map(ContentToken::NostrBluePinboard);
    }
    if path.starts_with("/badges/") && !path.starts_with("/badges/new") {
        return extract_id_from_path(path, "/badges/").map(ContentToken::NostrBlueBadge);
    }
    if path.starts_with("/marketplace/product/")
        && !path.starts_with("/marketplace/product/new")
        && !path.starts_with("/marketplace/product/edit")
    {
        return extract_id_from_path(path, "/marketplace/product/")
            .map(ContentToken::NostrBlueProduct);
    }
    if path.starts_with("/code/repo/") {
        let remainder = path.strip_prefix("/code/repo/")?;
        let naddr = remainder.split('/').next()?;
        if !naddr.is_empty() {
            return Some(ContentToken::NostrBlueCodeRepo(naddr.to_string()));
        }
    }
    if path.starts_with("/community/") {
        return extract_id_from_path(path, "/community/").map(ContentToken::NostrBlueCommunity);
    }
    if path.starts_with("/chats/") && !path.starts_with("/chats/new") {
        return extract_id_from_path(path, "/chats/").and_then(|id| {
            EventId::from_hex(&id)
                .ok()
                .map(|_| ContentToken::NostrBlueChannel(id))
        });
    }
    None
}
/// Helper to extract an identifier from URL path after a given prefix
fn extract_id_from_path(path: &str, prefix: &str) -> Option<String> {
    let remainder = path.strip_prefix(prefix)?;
    let id = remainder.split(['?', '#', '/']).next()?;
    if !id.is_empty() {
        Some(id.to_string())
    } else {
        None
    }
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
            &[],
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
        let image_count = tokens
            .iter()
            .filter(|t| matches!(t, ContentToken::Image(_)))
            .count();
        assert_eq!(image_count, 3);
    }
    #[test]
    #[cfg(feature = "cashu")]
    fn test_parse_cashu_token_v3() {
        let tokens = parse_content("cashuAeyJwYXlsb2FkIjp7fX0=", &[]);
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], ContentToken::CashuToken(t) if t.starts_with("cashuA")),);
    }
    #[test]
    #[cfg(feature = "cashu")]
    fn test_parse_cashu_token_v4() {
        let tokens = parse_content("cashuBeyJwYXlsb2FkIjp7fX0=", &[]);
        assert_eq!(tokens.len(), 1);
        assert!(matches!(&tokens[0], ContentToken::CashuToken(t) if t.starts_with("cashuB")),);
    }
    #[test]
    #[cfg(feature = "cashu")]
    fn test_parse_cashu_token_in_content() {
        let tokens = parse_content(
            "Check this token cashuAeyJwYXlsb2FkIjp7fX0= for payment",
            &[],
        );
        assert_eq!(tokens.len(), 3);
        assert!(matches!(&tokens[0], ContentToken::Text(_)));
        assert!(matches!(&tokens[1], ContentToken::CashuToken(_)));
        assert!(matches!(&tokens[2], ContentToken::Text(_)));
    }
    #[test]
    fn test_parse_isbn() {
        let tokens = parse_content("Check out isbn:9780765382030 for details", &[]);
        assert!(tokens
            .iter()
            .any(|t| { matches!(t, ContentToken::Isbn(isbn) if isbn == "9780765382030") }),);
    }
    #[test]
    fn test_parse_isbn_with_dashes() {
        let tokens = parse_content("isbn:978-0-7653-8203-0", &[]);
        assert!(tokens
            .iter()
            .any(|t| { matches!(t, ContentToken::Isbn(isbn) if isbn == "978-0-7653-8203-0") }),);
    }
    #[test]
    fn test_parse_doi() {
        let tokens = parse_content("Read the paper at doi:10.1000/182", &[]);
        assert!(tokens
            .iter()
            .any(|t| matches!(t, ContentToken::Doi(doi) if doi == "10.1000/182")),);
    }
    #[test]
    fn test_parse_isan() {
        let tokens = parse_content("Watch isan:0000-0000-401A-0000-7", &[]);
        assert!(tokens.iter().any(|t| matches!(t, ContentToken::Isan(_))));
    }
    #[test]
    fn test_parse_podcast_feed() {
        let tokens = parse_content("Listen to podcast:guid:abc123-def456", &[]);
        assert!(tokens.iter().any(|t| {
            matches!(
                t,
                ContentToken::PodcastFeed(guid)
                if guid == "abc123-def456"
            )
        }),);
    }
    #[test]
    fn test_parse_podcast_episode() {
        let tokens = parse_content("podcast:item:guid:ep-001-intro", &[]);
        assert!(tokens.iter().any(|t| {
            matches!(
                t,
                ContentToken::PodcastEpisode(guid)
                if guid == "ep-001-intro"
            )
        }),);
    }
    #[test]
    fn test_parse_bitcoin_tx() {
        let txid = "a1075db55d416d3ca199f55b6084e2115b9345e16c5cf302fc80e9d5fbf5d48d";
        let content = format!("Check tx bitcoin:tx:{}", txid);
        let tokens = parse_content(&content, &[]);
        assert!(tokens
            .iter()
            .any(|t| matches!(t, ContentToken::BitcoinTx(id) if id == txid)),);
    }
    #[test]
    fn test_parse_bitcoin_address_legacy() {
        let tokens = parse_content(
            "Send to bitcoin:address:1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
            &[],
        );
        assert!(tokens
            .iter()
            .any(|t| matches!(t, ContentToken::BitcoinAddress(_))));
    }
    #[test]
    fn test_parse_bitcoin_address_segwit() {
        let tokens = parse_content(
            "bitcoin:address:bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq",
            &[],
        );
        assert!(tokens
            .iter()
            .any(|t| matches!(t, ContentToken::BitcoinAddress(_))));
    }
    #[test]
    fn test_parse_geohash() {
        let tokens = parse_content("Meet at geo:u4pruydqqvj", &[]);
        assert!(tokens
            .iter()
            .any(|t| { matches!(t, ContentToken::Geohash(hash) if hash == "u4pruydqqvj") }),);
    }
    #[test]
    fn test_parse_multiple_nip73() {
        let content = "Book: isbn:9780765382030 Paper: doi:10.1000/182";
        let tokens = parse_content(content, &[]);
        assert!(tokens.iter().any(|t| matches!(t, ContentToken::Isbn(_))));
        assert!(tokens.iter().any(|t| matches!(t, ContentToken::Doi(_))));
    }
}
