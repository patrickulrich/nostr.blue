mod embeds;
mod mentions;
mod minicards;
mod nostr_blue_renderers;

use embeds::{
    AppleMusicRenderer, BitcoinAddressRenderer, BitcoinTxRenderer, DoiRenderer, GeohashRenderer,
    IsanRenderer, IsbnRenderer, MixCloudRenderer, PodcastEpisodeRenderer, PodcastFeedRenderer,
    RumbleRenderer, SoundCloudRenderer, SpotifyRenderer, TidalRenderer, TwitchClipRenderer,
    TwitchStreamRenderer, TwitchVodRenderer, TwitterTweetRenderer, WavlakeAlbumRenderer,
    WavlakeArtistRenderer, WavlakePlaylistRenderer, WavlakeTrackRenderer, YouTubeRenderer,
    ZapCookingRecipeRenderer, ZapStreamRenderer,
};
use mentions::{EventMentionRenderer, MentionRenderer};
use nostr_blue_renderers::{
    NostrBlueArticleRenderer, NostrBlueBadgeRenderer, NostrBlueCalendarEventRenderer,
    NostrBlueCodeRepoRenderer, NostrBlueCommunityRenderer, NostrBlueLiveStreamRenderer,
    NostrBlueMusicPlaylistRenderer, NostrBlueNoteRenderer, NostrBluePhotoRenderer,
    NostrBluePinboardRenderer, NostrBluePodcastEpisodeRenderer, NostrBluePodcastShowRenderer,
    NostrBlueProductRenderer, NostrBlueProfileRenderer, NostrBluePublicationRenderer,
    NostrBlueRadioStationRenderer, NostrBlueRecipeRenderer, NostrBlueRssPodcastEpisodeRenderer,
    NostrBlueRssPodcastShowRenderer, NostrBlueVideoRenderer, NostrBlueVoiceRenderer,
    NostrBlueWikiRenderer,
};

use crate::components::CashuTokenCard;
use crate::routes::Route;
use crate::utils::content_parser::{parse_content, ContentToken};
use dioxus::prelude::*;
use nostr_sdk::Tag;
#[component]
pub fn RichContent(
    content: String,
    tags: Vec<Tag>,
    #[props(default = false)]
    collapsible: bool,
) -> Element {
    let tokens = parse_content(&content, &tags);
    let mut is_expanded = use_signal(|| false);
    let is_long_content = if collapsible {
        let char_count = content.chars().count();
        let media_count = tokens
            .iter()
            .filter(|t| {
                matches!(
                    t,
                    ContentToken::Image(_)
                    | ContentToken::Video(_)
                    | ContentToken::WavlakeTrack(_)
                    | ContentToken::WavlakeAlbum(_)
                    | ContentToken::TwitterTweet(_)
                    | ContentToken::TwitchStream(_)
                    | ContentToken::TwitchClip(_)
                    | ContentToken::TwitchVod(_)
                    | ContentToken::EventMention(_)
                    | ContentToken::CashuToken(_)
                    | ContentToken::NostrBlueLiveStream(_)
                    | ContentToken::NostrBlueVideo(_)
                    | ContentToken::NostrBluePhoto(_)
                    | ContentToken::NostrBluePodcastShow(_)
                    | ContentToken::NostrBluePodcastEpisode(_)
                    | ContentToken::NostrBlueArticle(_)
                    | ContentToken::NostrBlueRecipe(_)
                    | ContentToken::NostrBlueWiki(_)
                    | ContentToken::NostrBluePublication(_)
                    | ContentToken::NostrBluePinboard(_)
                    | ContentToken::NostrBlueProduct(_)
                    | ContentToken::NostrBlueCodeRepo(_)
                    | ContentToken::NostrBlueVoice(_)
                    | ContentToken::NostrBlueMusicPlaylist(_)
                    | ContentToken::NostrBlueRadioStation(_)
                    | ContentToken::NostrBlueNote(_)
                    | ContentToken::NostrBlueProfile(_)
                    | ContentToken::NostrBlueCalendarEvent(_)
                    | ContentToken::NostrBlueBadge(_)
                    | ContentToken::NostrBlueRssPodcastEpisode(_, _)
                    | ContentToken::NostrBlueRssPodcastShow(_)
                )
            })
            .count();
        char_count > 800 || (media_count > 0 && char_count > 200)
    } else {
        false
    };
    let groups = group_tokens(&tokens);
    if collapsible && is_long_content {
        rsx! {
            div { class: "relative",
                div { class: if *is_expanded.read() { "whitespace-pre-wrap break-words" } else { "whitespace-pre-wrap break-words max-h-[24em] overflow-hidden" },
                    for group in groups.iter() {
                        match group {
                            TokenGroup::Inline(items) => rsx! {
                                span { key: "inline-{items[0].0}",
                                    for (_idx , token) in items.iter() {
                                        {render_token(token)}
                                    }
                                }
                            },
                            TokenGroup::Block(idx, token) => rsx! {
                                div { key: "{token_key(token, *idx)}", {render_token(token)} }
                            },
                        }
                    }
                }
                if !*is_expanded.read() {
                    div { class: "absolute bottom-0 left-0 right-0 h-12 bg-gradient-to-t from-background via-background/95 to-transparent flex items-end justify-center pb-1",
                        button {
                            class: "px-4 py-1.5 text-sm font-medium text-primary border border-border rounded-md bg-background hover:bg-accent transition",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                is_expanded.set(true);
                            },
                            "Show More"
                        }
                    }
                }
            }
        }
    } else {
        rsx! {
            div { class: "whitespace-pre-wrap break-words",
                for group in groups.iter() {
                    match group {
                        TokenGroup::Inline(items) => rsx! {
                            span { key: "inline-{items[0].0}",
                                for (_idx , token) in items.iter() {
                                    {render_token(token)}
                                }
                            }
                        },
                        TokenGroup::Block(idx, token) => rsx! {
                            div { key: "{token_key(token, *idx)}", {render_token(token)} }
                        },
                    }
                }
            }
        }
    }
}
/// Simple hash function for generating stable keys (avoids external dependencies)
fn hash_str(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
/// Check if a token should be rendered inline (flows with text)
/// vs block-level (renders on its own line with spacing)
fn is_inline_token(token: &ContentToken) -> bool {
    matches!(
        token,
        ContentToken::Text(_)
        | ContentToken::Link(_)
        | ContentToken::Mention(_)
        | ContentToken::Hashtag(_)
    )
}
/// Represents a group of tokens for rendering purposes
enum TokenGroup<'a> {
    /// Consecutive inline tokens that should flow together
    Inline(Vec<(usize, &'a ContentToken)>),
    /// A single block-level token that needs its own line
    Block(usize, &'a ContentToken),
}
/// Group consecutive inline tokens together for proper text flow
fn group_tokens(tokens: &[ContentToken]) -> Vec<TokenGroup<'_>> {
    let mut groups = Vec::new();
    let mut inline_group: Vec<(usize, &ContentToken)> = Vec::new();
    for (idx, token) in tokens.iter().enumerate() {
        if is_inline_token(token) {
            inline_group.push((idx, token));
        } else {
            if !inline_group.is_empty() {
                groups.push(TokenGroup::Inline(std::mem::take(&mut inline_group)));
            }
            groups.push(TokenGroup::Block(idx, token));
        }
    }
    if !inline_group.is_empty() {
        groups.push(TokenGroup::Inline(inline_group));
    }
    groups
}
/// Generate a stable key for a ContentToken to avoid DOM reuse bugs from index-based keys.
/// Combines variant name with content identifiers for uniqueness.
fn token_key(token: &ContentToken, idx: usize) -> String {
    match token {
        ContentToken::Text(text) => {
            let preview: String = text.chars().take(32).collect();
            format!("text-{}-{:x}", idx, hash_str(&preview))
        }
        ContentToken::Link(url) => format!("link-{}-{:x}", idx, hash_str(url)),
        ContentToken::Image(url) => format!("img-{}-{:x}", idx, hash_str(url)),
        ContentToken::Video(url) => format!("vid-{}-{:x}", idx, hash_str(url)),
        ContentToken::Mention(m) => format!("mention-{}-{:x}", idx, hash_str(m)),
        ContentToken::EventMention(m) => format!("event-{}-{:x}", idx, hash_str(m)),
        ContentToken::Hashtag(tag) => format!("tag-{}-{}", idx, tag),
        ContentToken::WavlakeTrack(id) => format!("wavlake-track-{}-{}", idx, id),
        ContentToken::WavlakeAlbum(id) => format!("wavlake-album-{}-{}", idx, id),
        ContentToken::WavlakeArtist(id) => format!("wavlake-artist-{}-{}", idx, id),
        ContentToken::WavlakePlaylist(id) => format!("wavlake-playlist-{}-{}", idx, id),
        ContentToken::TwitterTweet(id) => format!("tweet-{}-{}", idx, id),
        ContentToken::TwitchStream(ch) => format!("twitch-stream-{}-{}", idx, ch),
        ContentToken::TwitchClip(slug) => format!("twitch-clip-{}-{}", idx, slug),
        ContentToken::TwitchVod(id) => format!("twitch-vod-{}-{}", idx, id),
        ContentToken::YouTube(id) => format!("yt-{}-{}", idx, id),
        ContentToken::SpotifyTrack(id) => format!("spotify-track-{}-{}", idx, id),
        ContentToken::SpotifyAlbum(id) => format!("spotify-album-{}-{}", idx, id),
        ContentToken::SpotifyPlaylist(id) => format!("spotify-playlist-{}-{}", idx, id),
        ContentToken::SpotifyEpisode(id) => format!("spotify-ep-{}-{}", idx, id),
        ContentToken::SoundCloud(url) => {
            format!("soundcloud-{}-{:x}", idx, hash_str(url))
        }
        ContentToken::AppleMusicAlbum(url) => {
            format!("apple-album-{}-{:x}", idx, hash_str(url))
        }
        ContentToken::AppleMusicPlaylist(url) => {
            format!("apple-playlist-{}-{:x}", idx, hash_str(url))
        }
        ContentToken::AppleMusicSong(url) => {
            format!("apple-song-{}-{:x}", idx, hash_str(url))
        }
        ContentToken::MixCloud(user, mix) => format!("mixcloud-{}-{}-{}", idx, user, mix),
        ContentToken::Rumble(url) => format!("rumble-{}-{:x}", idx, hash_str(url)),
        ContentToken::Tidal(url) => format!("tidal-{}-{:x}", idx, hash_str(url)),
        ContentToken::ZapStream(naddr) => {
            format!("zapstream-{}-{:x}", idx, hash_str(naddr))
        }
        ContentToken::ZapCookingRecipe(naddr) => {
            format!("zapcooking-{}-{:x}", idx, hash_str(naddr))
        }
        ContentToken::CashuToken(token) => format!("cashu-{}-{:x}", idx, hash_str(token)),
        ContentToken::Isbn(isbn) => format!("isbn-{}-{}", idx, isbn),
        ContentToken::Doi(doi) => format!("doi-{}-{:x}", idx, hash_str(doi)),
        ContentToken::Isan(isan) => format!("isan-{}-{}", idx, isan),
        ContentToken::PodcastFeed(guid) => {
            format!("podcast-feed-{}-{:x}", idx, hash_str(guid))
        }
        ContentToken::PodcastEpisode(guid) => {
            format!("podcast-ep-{}-{:x}", idx, hash_str(guid))
        }
        ContentToken::BitcoinTx(txid) => format!("btc-tx-{}-{}", idx, txid),
        ContentToken::BitcoinAddress(addr) => format!("btc-addr-{}-{}", idx, addr),
        ContentToken::Geohash(hash) => format!("geo-{}-{}", idx, hash),
        ContentToken::NostrBlueLiveStream(id) => {
            format!("nb-live-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueVideo(id) => {
            format!("nb-video-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBluePhoto(id) => {
            format!("nb-photo-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueVoice(id) => {
            format!("nb-voice-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBluePodcastShow(id) => {
            format!("nb-podcast-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBluePodcastEpisode(id) => {
            format!("nb-podcast-ep-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueMusicPlaylist(id) => {
            format!("nb-playlist-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueRadioStation(id) => {
            format!("nb-radio-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueArticle(id) => {
            format!("nb-article-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueRecipe(id) => {
            format!("nb-recipe-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueNote(id) => format!("nb-note-{}-{:x}", idx, hash_str(id)),
        ContentToken::NostrBlueProfile(id) => {
            format!("nb-profile-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueCalendarEvent(id) => {
            format!("nb-calendar-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueWiki(id) => format!("nb-wiki-{}-{:x}", idx, hash_str(id)),
        ContentToken::NostrBluePublication(id) => {
            format!("nb-pub-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBluePinboard(id) => {
            format!("nb-pinboard-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueBadge(id) => {
            format!("nb-badge-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueProduct(id) => {
            format!("nb-product-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueCodeRepo(id) => {
            format!("nb-repo-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueCommunity(id) => {
            format!("nb-community-{}-{:x}", idx, hash_str(id))
        }
        ContentToken::NostrBlueRssPodcastEpisode(pid, eid) => {
            format!("nb-rss-ep-{}-{:x}-{:x}", idx, hash_str(pid), hash_str(eid))
        }
        ContentToken::NostrBlueRssPodcastShow(id) => {
            format!("nb-rss-show-{}-{:x}", idx, hash_str(id))
        }
    }
}
fn render_token(token: &ContentToken) -> Element {
    match token {
        ContentToken::Text(text) => {
            rsx! {
                span { "{text}" }
            }
        }
        ContentToken::Link(url) => {
            let is_safe = url.starts_with("http://") || url.starts_with("https://") || url.starts_with("nostr:");
            if is_safe {
                rsx! {
                    a {
                        href: "{url}",
                        target: "_blank",
                        rel: "noopener noreferrer",
                        class: "text-foreground hover:text-muted-foreground underline",
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        "{url}"
                    }
                }
            } else {
                rsx! {
                    span { class: "text-muted-foreground break-all", "{url}" }
                }
            }
        }
        ContentToken::Image(url) => {
            let url_for_error = url.clone();
            rsx! {
                div {
                    class: "my-2 rounded-lg overflow-hidden border border-border",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    img {
                        src: "{url}",
                        alt: "Image",
                        class: "max-w-full h-auto",
                        loading: "lazy",
                        onerror: move |_| {
                            log::warn!("Failed to load image: {}", url_for_error);
                        },
                    }
                }
            }
        }
        ContentToken::Video(url) => {
            rsx! {
                div {
                    class: "my-2 rounded-lg overflow-hidden border border-border",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    video {
                        src: "{url}",
                        controls: true,
                        class: "max-w-full h-auto",
                        "Your browser does not support the video tag."
                    }
                }
            }
        }
        ContentToken::Mention(mention) => {
            rsx! {
                MentionRenderer { mention: mention.clone() }
            }
        }
        ContentToken::EventMention(mention) => {
            rsx! {
                EventMentionRenderer { mention: mention.clone() }
            }
        }
        ContentToken::Hashtag(tag) => {
            rsx! {
                Link {
                    to: Route::Hashtag { tag: tag.clone() },
                    class: "text-foreground hover:text-muted-foreground font-medium hover:underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    "#{tag}"
                }
            }
        }
        ContentToken::WavlakeTrack(track_id) => {
            rsx! {
                WavlakeTrackRenderer { track_id: track_id.clone() }
            }
        }
        ContentToken::WavlakeAlbum(album_id) => {
            rsx! {
                WavlakeAlbumRenderer { album_id: album_id.clone() }
            }
        }
        ContentToken::WavlakeArtist(artist_id) => {
            rsx! {
                WavlakeArtistRenderer { artist_id: artist_id.clone() }
            }
        }
        ContentToken::WavlakePlaylist(playlist_id) => {
            rsx! {
                WavlakePlaylistRenderer { playlist_id: playlist_id.clone() }
            }
        }
        ContentToken::TwitterTweet(tweet_id) => {
            rsx! {
                TwitterTweetRenderer { tweet_id: tweet_id.clone() }
            }
        }
        ContentToken::TwitchStream(channel) => {
            rsx! {
                TwitchStreamRenderer { channel: channel.clone() }
            }
        }
        ContentToken::TwitchClip(clip_slug) => {
            rsx! {
                TwitchClipRenderer { clip_slug: clip_slug.clone() }
            }
        }
        ContentToken::TwitchVod(vod_id) => {
            rsx! {
                TwitchVodRenderer { vod_id: vod_id.clone() }
            }
        }
        ContentToken::YouTube(video_id) => {
            rsx! {
                YouTubeRenderer { video_id: video_id.clone() }
            }
        }
        ContentToken::SpotifyTrack(track_id) => {
            rsx! {
                SpotifyRenderer {
                    content_type: "track".to_string(),
                    content_id: track_id.clone(),
                }
            }
        }
        ContentToken::SpotifyAlbum(album_id) => {
            rsx! {
                SpotifyRenderer {
                    content_type: "album".to_string(),
                    content_id: album_id.clone(),
                }
            }
        }
        ContentToken::SpotifyPlaylist(playlist_id) => {
            rsx! {
                SpotifyRenderer {
                    content_type: "playlist".to_string(),
                    content_id: playlist_id.clone(),
                }
            }
        }
        ContentToken::SpotifyEpisode(episode_id) => {
            rsx! {
                SpotifyRenderer {
                    content_type: "episode".to_string(),
                    content_id: episode_id.clone(),
                }
            }
        }
        ContentToken::SoundCloud(url) => {
            rsx! {
                SoundCloudRenderer { url: url.clone() }
            }
        }
        ContentToken::AppleMusicAlbum(url) | ContentToken::AppleMusicPlaylist(url) => {
            rsx! {
                AppleMusicRenderer { embed_url: url.clone(), is_song: false }
            }
        }
        ContentToken::AppleMusicSong(url) => {
            rsx! {
                AppleMusicRenderer { embed_url: url.clone(), is_song: true }
            }
        }
        ContentToken::MixCloud(username, mix_name) => {
            rsx! {
                MixCloudRenderer { username: username.clone(), mix_name: mix_name.clone() }
            }
        }
        ContentToken::Rumble(embed_url) => {
            rsx! {
                RumbleRenderer { embed_url: embed_url.clone() }
            }
        }
        ContentToken::Tidal(embed_url) => {
            rsx! {
                TidalRenderer { embed_url: embed_url.clone() }
            }
        }
        ContentToken::ZapStream(naddr) => {
            rsx! {
                ZapStreamRenderer { naddr: naddr.clone() }
            }
        }
        ContentToken::ZapCookingRecipe(naddr) => {
            rsx! {
                ZapCookingRecipeRenderer { naddr: naddr.clone() }
            }
        }
        ContentToken::CashuToken(token) => {
            rsx! {
                CashuTokenCard { token: token.clone() }
            }
        }
        ContentToken::Isbn(isbn) => {
            rsx! {
                IsbnRenderer { isbn: isbn.clone() }
            }
        }
        ContentToken::Doi(doi) => {
            rsx! {
                DoiRenderer { doi: doi.clone() }
            }
        }
        ContentToken::Isan(isan) => {
            rsx! {
                IsanRenderer { isan: isan.clone() }
            }
        }
        ContentToken::PodcastFeed(guid) => {
            rsx! {
                PodcastFeedRenderer { guid: guid.clone() }
            }
        }
        ContentToken::PodcastEpisode(guid) => {
            rsx! {
                PodcastEpisodeRenderer { guid: guid.clone() }
            }
        }
        ContentToken::BitcoinTx(txid) => {
            rsx! {
                BitcoinTxRenderer { txid: txid.clone() }
            }
        }
        ContentToken::BitcoinAddress(address) => {
            rsx! {
                BitcoinAddressRenderer { address: address.clone() }
            }
        }
        ContentToken::Geohash(hash) => {
            rsx! {
                GeohashRenderer { hash: hash.clone() }
            }
        }
        ContentToken::NostrBlueLiveStream(id) => {
            rsx! {
                NostrBlueLiveStreamRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueVideo(id) => {
            rsx! {
                NostrBlueVideoRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBluePhoto(id) => {
            rsx! {
                NostrBluePhotoRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueVoice(id) => {
            rsx! {
                NostrBlueVoiceRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBluePodcastShow(id) => {
            rsx! {
                NostrBluePodcastShowRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBluePodcastEpisode(id) => {
            rsx! {
                NostrBluePodcastEpisodeRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueMusicPlaylist(id) => {
            rsx! {
                NostrBlueMusicPlaylistRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueRadioStation(id) => {
            rsx! {
                NostrBlueRadioStationRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueArticle(id) => {
            rsx! {
                NostrBlueArticleRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueRecipe(id) => {
            rsx! {
                NostrBlueRecipeRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueNote(id) => {
            rsx! {
                NostrBlueNoteRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueProfile(id) => {
            rsx! {
                NostrBlueProfileRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueCalendarEvent(id) => {
            rsx! {
                NostrBlueCalendarEventRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueWiki(id) => {
            rsx! {
                NostrBlueWikiRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBluePublication(id) => {
            rsx! {
                NostrBluePublicationRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBluePinboard(id) => {
            rsx! {
                NostrBluePinboardRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueBadge(id) => {
            rsx! {
                NostrBlueBadgeRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueProduct(id) => {
            rsx! {
                NostrBlueProductRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueCodeRepo(id) => {
            rsx! {
                NostrBlueCodeRepoRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueCommunity(id) => {
            rsx! {
                NostrBlueCommunityRenderer { id: id.clone() }
            }
        }
        ContentToken::NostrBlueRssPodcastEpisode(podcast_id, episode_id) => {
            rsx! {
                NostrBlueRssPodcastEpisodeRenderer {
                    podcast_id: podcast_id.clone(),
                    episode_id: episode_id.clone(),
                }
            }
        }
        ContentToken::NostrBlueRssPodcastShow(podcast_id) => {
            rsx! {
                NostrBlueRssPodcastShowRenderer { podcast_id: podcast_id.clone() }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::content_parser::ContentToken;
    /// Test that all ContentToken variants produce non-empty keys via token_key()
    /// This ensures parser↔renderer parity - if a new variant is added to ContentToken,
    /// adding it to this test will catch any missing token_key() handling at compile time.
    #[test]
    fn test_token_key_handles_all_variants() {
        let test_cases: Vec<ContentToken> = vec![
            ContentToken::Text("test".to_string()),
            ContentToken::Link("https://example.com".to_string()),
            ContentToken::Image("https://example.com/img.jpg".to_string()),
            ContentToken::Video("https://example.com/vid.mp4".to_string()),
            ContentToken::WavlakeTrack("abc123".to_string()),
            ContentToken::WavlakeAlbum("def456".to_string()),
            ContentToken::WavlakeArtist("ghi789".to_string()),
            ContentToken::WavlakePlaylist("jkl012".to_string()),
            ContentToken::TwitterTweet("123456789".to_string()),
            ContentToken::TwitchStream("channel".to_string()),
            ContentToken::TwitchClip("slug".to_string()),
            ContentToken::TwitchVod("12345".to_string()),
            ContentToken::Mention("npub1test".to_string()),
            ContentToken::EventMention("note1test".to_string()),
            ContentToken::Hashtag("nostr".to_string()),
            ContentToken::YouTube("dQw4w9WgXcQ".to_string()),
            ContentToken::SpotifyTrack("track123".to_string()),
            ContentToken::SpotifyAlbum("album123".to_string()),
            ContentToken::SpotifyPlaylist("playlist123".to_string()),
            ContentToken::SpotifyEpisode("ep123".to_string()),
            ContentToken::SoundCloud("https://soundcloud.com/test".to_string()),
            ContentToken::AppleMusicAlbum("us/album/test/123".to_string()),
            ContentToken::AppleMusicPlaylist("us/playlist/test/123".to_string()),
            ContentToken::AppleMusicSong("us/album/test/123?i=456".to_string()),
            ContentToken::MixCloud("user".to_string(), "mix".to_string()),
            ContentToken::Rumble("https://rumble.com/embed/123".to_string()),
            ContentToken::Tidal("https://embed.tidal.com/track/123".to_string()),
            ContentToken::ZapStream("naddr1test".to_string()),
            ContentToken::ZapCookingRecipe("naddr1test".to_string()),
            ContentToken::CashuToken("cashuAtest".to_string()),
            ContentToken::Isbn("9780765382030".to_string()),
            ContentToken::Doi("10.1000/182".to_string()),
            ContentToken::Isan("0000-0000-401A-0000-7".to_string()),
            ContentToken::PodcastFeed("guid123".to_string()),
            ContentToken::PodcastEpisode("ep-guid".to_string()),
            ContentToken::BitcoinTx(
                "a1075db55d416d3ca199f55b6084e2115b9345e16c5cf302fc80e9d5fbf5d48d"
                    .to_string(),
            ),
            ContentToken::BitcoinAddress("bc1qtest".to_string()),
            ContentToken::Geohash("u4pruydqqvj".to_string()),
            ContentToken::NostrBlueLiveStream("naddr1test".to_string()),
            ContentToken::NostrBlueVideo("note1test".to_string()),
            ContentToken::NostrBluePhoto("note1test".to_string()),
            ContentToken::NostrBlueVoice("note1test".to_string()),
            ContentToken::NostrBluePodcastShow("naddr1test".to_string()),
            ContentToken::NostrBluePodcastEpisode("naddr1test".to_string()),
            ContentToken::NostrBlueMusicPlaylist("naddr1test".to_string()),
            ContentToken::NostrBlueRadioStation("naddr1test".to_string()),
            ContentToken::NostrBlueArticle("naddr1test".to_string()),
            ContentToken::NostrBlueRecipe("naddr1test".to_string()),
            ContentToken::NostrBlueNote("note1test".to_string()),
            ContentToken::NostrBlueProfile("npub1test".to_string()),
            ContentToken::NostrBlueCalendarEvent("naddr1test".to_string()),
            ContentToken::NostrBlueWiki("article-title".to_string()),
            ContentToken::NostrBluePublication("naddr1test".to_string()),
            ContentToken::NostrBluePinboard("naddr1test".to_string()),
            ContentToken::NostrBlueBadge("naddr1test".to_string()),
            ContentToken::NostrBlueProduct("naddr1test".to_string()),
            ContentToken::NostrBlueCodeRepo("naddr1test".to_string()),
            ContentToken::NostrBlueCommunity("34550:pubkey:community-name".to_string()),
            ContentToken::NostrBlueRssPodcastEpisode(
                "podcast123".to_string(),
                "ep456".to_string(),
            ),
            ContentToken::NostrBlueRssPodcastShow("podcast123".to_string()),
        ];
        assert_eq!(
            test_cases.len(),
            60,
            "Test cases should cover all ContentToken variants. If you added a new variant, add it to this test.",
        );
        for (idx, token) in test_cases.iter().enumerate() {
            let key = token_key(token, idx);
            assert!(
                !key.is_empty(),
                "token_key should return non-empty string for {:?}",
                token,
            );
        }
    }
    /// Test that duplicate URLs at different positions get unique keys (Issue 10 fix verification)
    #[test]
    fn test_token_key_uniqueness_for_duplicates() {
        let url = "https://example.com/test.jpg";
        let token1 = ContentToken::Image(url.to_string());
        let token2 = ContentToken::Image(url.to_string());
        let key1 = token_key(&token1, 0);
        let key2 = token_key(&token2, 1);
        assert_ne!(
            key1,
            key2,
            "Same Image URL at different positions should have unique keys",
        );
        let hashtag = "nostr";
        let token3 = ContentToken::Hashtag(hashtag.to_string());
        let token4 = ContentToken::Hashtag(hashtag.to_string());
        let key3 = token_key(&token3, 0);
        let key4 = token_key(&token4, 1);
        assert_ne!(
            key3,
            key4,
            "Same Hashtag at different positions should have unique keys",
        );
        let youtube_url = "https://youtube.com/watch?v=abc123";
        let token5 = ContentToken::YouTube(youtube_url.to_string());
        let token6 = ContentToken::YouTube(youtube_url.to_string());
        let key5 = token_key(&token5, 0);
        let key6 = token_key(&token6, 1);
        assert_ne!(
            key5,
            key6,
            "Same YouTube URL at different positions should have unique keys",
        );
        let note_id = "note1abc123def456";
        let token7 = ContentToken::NostrBlueNote(note_id.to_string());
        let token8 = ContentToken::NostrBlueNote(note_id.to_string());
        let key7 = token_key(&token7, 0);
        let key8 = token_key(&token8, 1);
        assert_ne!(
            key7,
            key8,
            "Same NostrBlueNote at different positions should have unique keys",
        );
        let feed_url = "https://example.com/feed.xml";
        let guid = "episode-123";
        let token9 = ContentToken::NostrBlueRssPodcastEpisode(
            feed_url.to_string(),
            guid.to_string(),
        );
        let token10 = ContentToken::NostrBlueRssPodcastEpisode(
            feed_url.to_string(),
            guid.to_string(),
        );
        let key9 = token_key(&token9, 0);
        let key10 = token_key(&token10, 1);
        assert_ne!(
            key9,
            key10,
            "Same NostrBlueRssPodcastEpisode at different positions should have unique keys",
        );
    }
}
