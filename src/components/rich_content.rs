use crate::components::citation::card::get_citation_style;
use crate::components::icons::{self, NostrBlueMiniLogo};
use crate::components::live::stream_card::LiveStreamCard;
use crate::components::{
    CashuTokenCard, PhotoCard, PollCard, VideoCard, VoiceMessageCard,
};
use crate::components::{EventCardCompact, P2POrderCard};
use crate::hooks::{use_fetch_event_by_coordinate_with_message, use_fetch_event_by_id};
use crate::routes::Route;
use crate::services::podcast_index;
use crate::services::wavlake::WavlakeAPI;
use crate::stores::calendar_store::UnifiedEvent;
use crate::stores::music_player::{self, MusicTrack};
use crate::stores::nostr_client;
use crate::stores::nostr_music::TrackSource;
use crate::stores::nostr_music::{
    parse_playlist_event, parse_track_event, NostrPlaylist, NostrTrack,
};
use crate::stores::pin_boards_store::{parse_pinboard_event, Pinboard};
use crate::stores::profiles;
use crate::stores::publication_store::{parse_publication_index, PublicationIndex};
use crate::utils::content_parser::{parse_content, ContentToken};
use crate::utils::markdown::sanitize_html;
use crate::utils::nip34::{Issue, PullRequest, Repository};
use crate::utils::nip52::parse_calendar_event;
use crate::utils::nip53::{
    parse_meeting_room_event, parse_meeting_space, LiveActivityEvent,
};
use crate::utils::nip54::{parse_wiki_article, WikiArticle};
use crate::utils::nip58::{parse_badge_definition, BadgeDefinition};
use crate::utils::nip69::parse_p2p_order;
use crate::utils::nip99::{
    parse_collection, parse_product, parse_review, Product, ProductCollection,
    ProductReview,
};
use crate::utils::nkbip03::{parse_citation, Citation};
use crate::utils::podcast::parse_podcast_episode;
use crate::utils::recipe::{
    extract_metadata as extract_recipe_metadata, is_recipe_event, RecipeMetadata,
};
use dioxus::prelude::*;
use dioxus_primitives::hover_card::{HoverCard, HoverCardContent, HoverCardTrigger};
use dioxus_primitives::ContentSide;
use nostr_sdk::nips::nip01::Coordinate;
use nostr_sdk::nips::nip19::Nip19;
use nostr_sdk::{
    Event, EventId, Filter, FromBech32, Kind, Metadata, PublicKey, Tag, ToBech32,
};
use crate::components::article_card::ArticleCard;
use crate::components::code::repo_card::CodeRepoCardCompact;
use crate::components::pin_board_card::PinBoardCardCompact;
use crate::components::podcast_episode_card::{DisplayEpisode, PodcastEpisodeCard};
use crate::components::podcast_show_card::{PodcastShow, PodcastShowCard};
use crate::components::publication_card::PublicationCardCompact;
use crate::components::radio_card::RadioCard;
use crate::components::recipe_card::RecipeCard;
use crate::components::wiki_card::WikiCardCompact;
use crate::utils::podcast::parse_podcast_metadata;
use crate::utils::radio::RadioStation;
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
                            class: "px-4 py-1.5 text-sm font-medium text-primary border border-border rounded-md bg-background hover:bg-accent transition-colors",
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
            rsx! {
                a {
                    href: "{url}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    "{url}"
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
                    class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 font-medium hover:underline",
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
#[component]
fn MentionRenderer(mention: String) -> Element {
    let identifier = mention.strip_prefix("nostr:").unwrap_or(&mention);
    let pubkey_result: Option<PublicKey> = Nip19::from_bech32(identifier)
        .ok()
        .and_then(|nip19| match nip19 {
            Nip19::Pubkey(pk) => Some(pk),
            Nip19::Profile(profile) => Some(profile.public_key),
            _ => None,
        });
    let cached_metadata = pubkey_result
        .as_ref()
        .and_then(|pk| profiles::get_profile(&pk.to_hex()));
    let mut metadata = use_signal(move || cached_metadata);
    use_effect(move || {
        if metadata.read().is_some() {
            return;
        }
        if let Some(pubkey) = pubkey_result {
            let pubkey_hex = pubkey.to_hex();
            spawn(async move {
                match profiles::fetch_profile(pubkey_hex).await {
                    Ok(profile) => {
                        let mut meta = Metadata::new();
                        if let Some(name) = profile.name {
                            meta = meta.name(&name);
                        }
                        if let Some(display_name) = profile.display_name {
                            meta = meta.display_name(&display_name);
                        }
                        metadata.set(Some(meta));
                    }
                    Err(e) => {
                        log::debug!("Failed to fetch profile for mention: {}", e);
                    }
                }
            });
        }
    });
    if let Some(pubkey) = pubkey_result {
        let pubkey_str = pubkey.to_hex();
        let display = if let Some(meta) = metadata.read().as_ref() {
            if let Some(display_name) = &meta.display_name {
                format!("@{}", display_name)
            } else if let Some(name) = &meta.name {
                format!("@{}", name)
            } else if pubkey_str.len() > 16 {
                format!(
                    "@{}...{}",
                    &pubkey_str[..8],
                    &pubkey_str[pubkey_str.len() - 4..],
                )
            } else {
                format!("@{}", pubkey_str)
            }
        } else if pubkey_str.len() > 16 {
            format!(
                "@{}...{}",
                &pubkey_str[..8],
                &pubkey_str[pubkey_str.len() - 4..],
            )
        } else {
            format!("@{}", pubkey_str)
        };
        rsx! {
            Link {
                to: Route::Profile {
                    pubkey: pubkey.to_hex(),
                },
                class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 font-medium hover:underline",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                "{display}"
            }
        }
    } else {
        rsx! {
            span { class: "text-blue-500 dark:text-blue-400 font-medium", "{mention}" }
        }
    }
}
/// Try to extract event ID from a nevent string even when SDK parsing fails
/// This handles cases where the nevent has invalid relay URLs (e.g., empty strings)
/// by using lower-level bech32 decoding and manually parsing the TLV data
fn try_extract_event_id_from_nevent(identifier: &str) -> Option<EventId> {
    use bech32::Hrp;
    if !identifier.starts_with("nevent1") {
        return None;
    }
    let (hrp, data) = bech32::decode(identifier).ok()?;
    if hrp != Hrp::parse("nevent").ok()? {
        return None;
    }
    let mut pos = 0;
    while pos + 2 <= data.len() {
        let tlv_type = data[pos];
        let tlv_len = data[pos + 1] as usize;
        if pos + 2 + tlv_len > data.len() {
            break;
        }
        if tlv_type == 0 && tlv_len == 32 {
            let event_id_bytes: [u8; 32] = data[pos + 2..pos + 2 + 32].try_into().ok()?;
            return EventId::from_byte_array(event_id_bytes).into();
        }
        pos += 2 + tlv_len;
    }
    None
}
#[component]
fn EventMentionRenderer(mention: String) -> Element {
    let identifier = mention.strip_prefix("nostr:").unwrap_or(&mention);
    let nip19_result = Nip19::from_bech32(identifier).ok();
    if matches!(&nip19_result, Some(Nip19::Coordinate(_))) {
        return rsx! {
            NaddrMentionRenderer { mention: mention.clone() }
        };
    }
    let parsed_event: Option<(EventId, Vec<String>)> = nip19_result
        .and_then(|nip19| match nip19 {
            Nip19::Event(nevent) => {
                let relays: Vec<String> = nevent
                    .relays
                    .iter()
                    .map(|r| r.to_string())
                    .collect();
                Some((nevent.event_id, relays))
            }
            Nip19::EventId(id) => Some((id, Vec::new())),
            _ => None,
        });
    let (event_id_result, relay_hints) = if let Some((id, relays)) = parsed_event {
        (Some(id), relays)
    } else if let Some(id) = try_extract_event_id_from_nevent(identifier) {
        (Some(id), Vec::new())
    } else {
        (None, Vec::new())
    };
    let mut embedded_event = use_signal(|| None::<Event>);
    let mut author_metadata = use_signal(|| None::<Metadata>);
    use_effect(move || {
        if let Some(event_id) = event_id_result {
            let relay_hints_clone = relay_hints.clone();
            spawn(async move {
                let event_filter = Filter::new().id(event_id).limit(1);
                let fetch_result = if !relay_hints_clone.is_empty() {
                    if let Some(client) = nostr_client::get_client() {
                        let relay_urls: Vec<nostr_sdk::Url> = relay_hints_clone
                            .iter()
                            .filter_map(|r| nostr_sdk::Url::parse(r).ok())
                            .collect();
                        if !relay_urls.is_empty() {
                            nostr_client::ensure_relays_ready(&client).await;
                            client
                                .fetch_events_from(
                                    relay_urls,
                                    event_filter.clone(),
                                    std::time::Duration::from_secs(5),
                                )
                                .await
                                .map(|events| events.into_iter().collect::<Vec<_>>())
                                .ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                let events = match fetch_result {
                    Some(events) if !events.is_empty() => events,
                    _ => {
                        nostr_client::fetch_events_aggregated(
                                event_filter,
                                std::time::Duration::from_secs(5),
                            )
                            .await
                            .unwrap_or_default()
                    }
                };
                if let Some(event) = events.into_iter().next() {
                    let author_pubkey = event.pubkey;
                    embedded_event.set(Some(event));
                    let metadata_filter = Filter::new()
                        .author(author_pubkey)
                        .kind(Kind::Metadata)
                        .limit(1);
                    if let Ok(metadata_events) = nostr_client::fetch_events_aggregated_outbox(
                            metadata_filter,
                            std::time::Duration::from_secs(5),
                        )
                        .await
                    {
                        if let Some(metadata_event) = metadata_events.into_iter().next()
                        {
                            if let Ok(meta) = serde_json::from_str::<
                                Metadata,
                            >(&metadata_event.content) {
                                author_metadata.set(Some(meta));
                            }
                        }
                    }
                }
            });
        }
    });
    if let Some(event_id) = event_id_result {
        let has_event = embedded_event.read().is_some();
        let event_clone = embedded_event.read().clone();
        let metadata_clone = author_metadata.read().clone();
        if has_event {
            let event = event_clone.unwrap();
            let event_kind = event.kind.as_u16();
            match event_kind {
                20 => {
                    rsx! {
                        PhotoCard { event }
                    }
                }
                21 | 22 => {
                    rsx! {
                        VideoCard { event }
                    }
                }
                1040 => {
                    rsx! {
                        VoiceMessageCard { event }
                    }
                }
                1068 => {
                    rsx! {
                        div { onclick: move |e: MouseEvent| e.stop_propagation(),
                            PollCard { event }
                        }
                    }
                }
                1621 => {
                    if let Some(issue) = Issue::from_event(&event) {
                        rsx! {
                            {render_issue_minicard(&issue)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_note(&event, metadata_clone.as_ref())}
                        }
                    }
                }
                1622 => {
                    if let Some(pr) = PullRequest::from_event(&event) {
                        rsx! {
                            {render_pr_minicard(&pr)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_note(&event, metadata_clone.as_ref())}
                        }
                    }
                }
                6 => {
                    rsx! {
                        {render_repost_minicard(&event)}
                    }
                }
                1111 => {
                    rsx! {
                        {render_comment_minicard(&event, metadata_clone.as_ref())}
                    }
                }
                30..=33 => {
                    if let Ok(citation) = parse_citation(&event) {
                        rsx! {
                            {render_citation_minicard(&citation)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_note(&event, metadata_clone.as_ref())}
                        }
                    }
                }
                _ => {
                    rsx! {
                        {render_embedded_note(&event, metadata_clone.as_ref())}
                    }
                }
            }
        } else {
            let event_str = event_id.to_hex();
            let short = if event_str.len() > 16 {
                format!(
                    "note:{}...{}",
                    &event_str[..8],
                    &event_str[event_str.len() - 4..],
                )
            } else {
                format!("note:{}", event_str)
            };
            rsx! {
                Link {
                    to: Route::Note {
                        note_id: event_id.to_hex(),
                        from_voice: None,
                    },
                    class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 font-medium hover:underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    "{short}"
                }
            }
        }
    } else {
        rsx! {
            span { class: "text-blue-500 dark:text-blue-400 font-medium", "{mention}" }
        }
    }
}
fn render_embedded_note(event: &Event, metadata: Option<&Metadata>) -> Element {
    let event_id = event.id.to_hex();
    let content = &event.content;
    let pubkey = event.pubkey;
    let pubkey_str = pubkey.to_hex();
    let display_content = {
        let char_count = content.chars().count();
        if char_count > 280 {
            let truncated: String = content.chars().take(280).collect();
            format!("{}...", truncated)
        } else {
            content.clone()
        }
    };
    let display_name = if let Some(meta) = metadata {
        meta.display_name
            .clone()
            .or_else(|| meta.name.clone())
            .unwrap_or_else(|| {
                format!("{}...{}", &pubkey_str[..8], &pubkey_str[pubkey_str.len() - 4..])
            })
    } else {
        format!("{}...{}", &pubkey_str[..8], &pubkey_str[pubkey_str.len() - 4..])
    };
    rsx! {
        Link {
            to: Route::Note {
                note_id: event_id.clone(),
                from_voice: None,
            },
            class: "block my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "border border-border rounded-lg p-3 hover:bg-accent/10 transition cursor-pointer",
                div { class: "flex items-center gap-2 mb-2",
                    if let Some(meta) = metadata {
                        if let Some(picture) = &meta.picture {
                            img {
                                class: "w-8 h-8 rounded-full",
                                src: "{picture}",
                                alt: "Avatar",
                            }
                        } else {
                            div { class: "w-8 h-8 rounded-full bg-blue-500 flex items-center justify-center text-white text-xs font-bold",
                                "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                            }
                        }
                    } else {
                        div { class: "w-8 h-8 rounded-full bg-gray-400 flex items-center justify-center text-white text-xs",
                            "?"
                        }
                    }
                    span { class: "font-semibold text-sm", "{display_name}" }
                }
                div { class: "text-sm text-muted-foreground whitespace-pre-wrap break-words",
                    "{display_content}"
                }
            }
        }
    }
}
#[component]
fn TwitterTweetRenderer(tweet_id: String) -> Element {
    let tweet_url = format!("https://twitter.com/x/status/{}", tweet_id);
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden border border-border bg-card p-4",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            "data-tweet-id": "{tweet_id}",
            blockquote {
                class: "twitter-tweet",
                "data-theme": "dark",
                "data-dnt": "true",
                p { "Loading tweet..." }
                a { href: "{tweet_url}", "View tweet" }
            }
        }
    }
}
#[component]
fn TwitchStreamRenderer(channel: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let parent_domain = if cfg!(debug_assertions) { "localhost" } else { "nostr.blue" };
    let embed_url = format!(
        "https://player.twitch.tv/?channel={}&parent={}",
        channel,
        parent_domain,
    );
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden border border-border",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            "data-twitch-visible": "{is_visible}",
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    class: "w-full aspect-video",
                    allowfullscreen: true,
                }
            } else {
                div {
                    class: "w-full aspect-video bg-card flex items-center justify-center cursor-pointer",
                    onclick: move |_| is_visible.set(true),
                    div { class: "text-center",
                        div { class: "text-purple-500 text-4xl mb-2", "▶" }
                        div { class: "text-lg font-medium", "Watch {channel} on Twitch" }
                        div { class: "text-sm text-muted-foreground mt-1", "Click to load stream" }
                    }
                }
            }
        }
    }
}
#[component]
fn TwitchClipRenderer(clip_slug: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let parent_domain = if cfg!(debug_assertions) { "localhost" } else { "nostr.blue" };
    let embed_url = format!(
        "https://clips.twitch.tv/embed?clip={}&parent={}",
        clip_slug,
        parent_domain,
    );
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden border border-border",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            "data-twitch-visible": "{is_visible}",
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    class: "w-full aspect-video",
                    allowfullscreen: true,
                }
            } else {
                div {
                    class: "w-full aspect-video bg-card flex items-center justify-center cursor-pointer",
                    onclick: move |_| is_visible.set(true),
                    div { class: "text-center",
                        div { class: "text-purple-500 text-4xl mb-2", "▶" }
                        div { class: "text-lg font-medium", "Watch Twitch Clip" }
                        div { class: "text-sm text-muted-foreground mt-1", "Click to load clip" }
                    }
                }
            }
        }
    }
}
#[component]
fn TwitchVodRenderer(vod_id: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let parent_domain = if cfg!(debug_assertions) { "localhost" } else { "nostr.blue" };
    let embed_url = format!(
        "https://player.twitch.tv/?video={}&parent={}",
        vod_id,
        parent_domain,
    );
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden border border-border",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            "data-twitch-visible": "{is_visible}",
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    class: "w-full aspect-video",
                    allowfullscreen: true,
                }
            } else {
                div {
                    class: "w-full aspect-video bg-card flex items-center justify-center cursor-pointer",
                    onclick: move |_| is_visible.set(true),
                    div { class: "text-center",
                        div { class: "text-purple-500 text-4xl mb-2", "▶" }
                        div { class: "text-lg font-medium", "Watch Twitch VOD" }
                        div { class: "text-sm text-muted-foreground mt-1", "Click to load video" }
                    }
                }
            }
        }
    }
}
#[component]
fn NaddrMentionRenderer(mention: String) -> Element {
    let identifier = mention.strip_prefix("nostr:").unwrap_or(&mention);
    let coord_data = nostr_sdk::nips::nip19::Nip19Coordinate::from_bech32(identifier)
        .ok()
        .map(|coord| {
            let relay_hints: Vec<String> = coord
                .relays
                .iter()
                .map(|r| r.to_string())
                .collect();
            (
                coord.public_key.to_hex(),
                coord.identifier.clone(),
                coord.kind.as_u16(),
                relay_hints,
            )
        });
    let mut article_event = use_signal(|| None::<Event>);
    let mut author_metadata = use_signal(|| None::<Metadata>);
    let mut loading = use_signal(|| true);
    let coord_data_for_effect = coord_data.clone();
    use_effect(move || {
        if let Some((ref pubkey, ref ident, kind, ref relays)) = coord_data_for_effect {
            let pubkey = pubkey.clone();
            let ident = ident.clone();
            let relay_hints = relays.clone();
            spawn(async move {
                loading.set(true);
                match crate::stores::nostr_client::fetch_event_by_coordinate_with_relays(
                        kind,
                        pubkey.clone(),
                        ident,
                        relay_hints,
                    )
                    .await
                {
                    Ok(Some(event)) => {
                        let author_pubkey = event.pubkey;
                        article_event.set(Some(event));
                        let metadata_filter = Filter::new()
                            .author(author_pubkey)
                            .kind(Kind::Metadata)
                            .limit(1);
                        if let Ok(metadata_events) = nostr_client::fetch_events_aggregated_outbox(
                                metadata_filter,
                                std::time::Duration::from_secs(5),
                            )
                            .await
                        {
                            if let Some(metadata_event) = metadata_events
                                .into_iter()
                                .next()
                            {
                                if let Ok(meta) = serde_json::from_str::<
                                    Metadata,
                                >(&metadata_event.content) {
                                    author_metadata.set(Some(meta));
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        log::warn!("Article not found for coordinate");
                    }
                    Err(e) => {
                        log::error!("Failed to fetch article: {}", e);
                    }
                }
                loading.set(false);
            });
        }
    });
    if let Some((_pubkey, _ident, kind, _relays)) = coord_data {
        let naddr_for_link = identifier.to_string();
        let has_event = article_event.read().is_some();
        let event_clone = article_event.read().clone();
        let metadata_clone = author_metadata.read().clone();
        if has_event {
            let event = event_clone.unwrap();
            const LIVE_EVENT: u16 = 30311;
            const ARTICLE: u16 = 30023;
            const GIT_REPO: u16 = 30617;
            const P2P_ORDER: u16 = 38383;
            const DATE_CALENDAR: u16 = 31922;
            const TIME_CALENDAR: u16 = 31923;
            const MEETING_SPACE: u16 = 30312;
            const MEETING_ROOM: u16 = 30313;
            const PODCAST_EPISODE: u16 = 30054;
            const WIKI_ARTICLE: u16 = 30818;
            const PUBLICATION_INDEX: u16 = 30040;
            const PINBOARD: u16 = 30067;
            const BADGE_DEFINITION: u16 = 30009;
            const PRODUCT: u16 = 30402;
            const COLLECTION: u16 = 30405;
            const REVIEW: u16 = 31555;
            const MUSIC_TRACK: u16 = 36787;
            const PLAYLIST: u16 = 34139;
            match kind {
                LIVE_EVENT => {
                    rsx! {
                        div { onclick: move |e: MouseEvent| e.stop_propagation(),
                            LiveStreamCard { event }
                        }
                    }
                }
                ARTICLE => {
                    if is_recipe_event(&event) {
                        let recipe_meta = extract_recipe_metadata(&event);
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_recipe_minicard(&recipe_meta, &naddr_clone, &event)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                DATE_CALENDAR | TIME_CALENDAR => {
                    if let Ok(cal_event) = parse_calendar_event(&event) {
                        let unified = UnifiedEvent::Calendar(cal_event);
                        rsx! {
                            div { onclick: move |e: MouseEvent| e.stop_propagation(),
                                EventCardCompact { event: unified }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                MEETING_SPACE => {
                    if let Ok(space) = parse_meeting_space(&event) {
                        let unified = UnifiedEvent::Live(
                            LiveActivityEvent::Space(space),
                        );
                        rsx! {
                            div { onclick: move |e: MouseEvent| e.stop_propagation(),
                                EventCardCompact { event: unified }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                MEETING_ROOM => {
                    if let Ok(room) = parse_meeting_room_event(&event) {
                        let unified = UnifiedEvent::Live(
                            LiveActivityEvent::Meeting(room),
                        );
                        rsx! {
                            div { onclick: move |e: MouseEvent| e.stop_propagation(),
                                EventCardCompact { event: unified }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                GIT_REPO => {
                    if let Some(repo) = Repository::from_event(&event) {
                        rsx! {
                            div { onclick: move |e: MouseEvent| e.stop_propagation(),
                                CodeRepoCardCompact { repo }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                PODCAST_EPISODE => {
                    if let Ok(episode) = parse_podcast_episode(&event) {
                        let episode_title = episode.title.clone();
                        rsx! {
                            Link {
                                to: Route::PodcastNostrDetail {
                                    naddr: naddr_for_link.clone(),
                                },
                                class: "flex items-center gap-2 p-3 rounded-lg border border-border hover:bg-accent/50 transition",
                                onclick: move |e: MouseEvent| e.stop_propagation(),
                                svg {
                                    class: "w-8 h-8 text-purple-500 shrink-0",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M19 11a7 7 0 01-7 7m0 0a7 7 0 01-7-7m7 7v4m0 0H8m4 0h4m-4-8a3 3 0 01-3-3V5a3 3 0 116 0v6a3 3 0 01-3 3z",
                                    }
                                }
                                div { class: "flex-1 min-w-0",
                                    p { class: "font-medium truncate", "{episode_title}" }
                                    p { class: "text-xs text-muted-foreground", "Podcast Episode" }
                                }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                P2P_ORDER => {
                    if let Ok(order) = parse_p2p_order(&event) {
                        rsx! {
                            div { onclick: move |e: MouseEvent| e.stop_propagation(),
                                P2POrderCard { order }
                            }
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                WIKI_ARTICLE => {
                    if let Ok(wiki) = parse_wiki_article(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_wiki_minicard(&wiki, &naddr_clone, &event)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                PRODUCT => {
                    if let Ok(product) = parse_product(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_product_minicard(&product, &naddr_clone, &event)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                BADGE_DEFINITION => {
                    if let Ok(badge) = parse_badge_definition(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_badge_minicard(&badge, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                MUSIC_TRACK => {
                    if let Ok(track) = parse_track_event(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_track_minicard(&track, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                PLAYLIST => {
                    if let Ok(playlist) = parse_playlist_event(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_playlist_minicard(&playlist, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                PUBLICATION_INDEX => {
                    if let Some(pub_index) = parse_publication_index(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_publication_minicard(&pub_index, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                PINBOARD => {
                    if let Some(board) = parse_pinboard_event(&event, None) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_pinboard_minicard(&board, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                COLLECTION => {
                    if let Ok(collection) = parse_collection(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_collection_minicard(&collection, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                REVIEW => {
                    if let Ok(review) = parse_review(&event) {
                        let naddr_clone = naddr_for_link.clone();
                        rsx! {
                            {render_review_minicard(&review, &naddr_clone)}
                        }
                    } else {
                        rsx! {
                            {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                        }
                    }
                }
                _ => {
                    rsx! {
                        {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                    }
                }
            }
        } else if *loading.read() {
            rsx! {
                div { class: "my-2 p-3 border border-border rounded-lg bg-accent/5 animate-pulse",
                    div { class: "h-4 bg-muted rounded w-3/4 mb-2" }
                    div { class: "h-3 bg-muted rounded w-1/2" }
                }
            }
        } else {
            rsx! {
                Link {
                    to: Route::ArticleDetail {
                        naddr: naddr_for_link.clone(),
                    },
                    class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 font-medium hover:underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    "📄 Article"
                }
            }
        }
    } else {
        rsx! {
            span { class: "text-blue-500 dark:text-blue-400 font-medium", "{mention}" }
        }
    }
}
fn render_embedded_article(
    event: &Event,
    metadata: Option<&Metadata>,
    naddr: &str,
) -> Element {
    use crate::utils::article_meta::{get_image, get_summary, get_title};
    let title = get_title(event);
    let summary = get_summary(event);
    let image_url = get_image(event);
    let pubkey_str = event.pubkey.to_hex();
    let display_name = if let Some(meta) = metadata {
        meta.display_name
            .clone()
            .or_else(|| meta.name.clone())
            .unwrap_or_else(|| {
                format!("{}...{}", &pubkey_str[..8], &pubkey_str[pubkey_str.len() - 4..])
            })
    } else {
        format!("{}...{}", &pubkey_str[..8], &pubkey_str[pubkey_str.len() - 4..])
    };
    let display_summary = if let Some(sum) = summary {
        let char_count = sum.chars().count();
        if char_count > 200 {
            let truncated: String = sum.chars().take(200).collect();
            format!("{}...", truncated)
        } else {
            sum
        }
    } else {
        String::new()
    };
    rsx! {
        Link {
            to: Route::ArticleDetail {
                naddr: naddr.to_string(),
            },
            class: "block my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition cursor-pointer",
                if let Some(img_url) = image_url {
                    div { class: "aspect-video w-full bg-muted overflow-hidden",
                        img {
                            src: "{img_url}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    }
                }
                div { class: "p-3",
                    h4 { class: "font-bold text-base mb-1 line-clamp-2", "{title}" }
                    if !display_summary.is_empty() {
                        p { class: "text-sm text-muted-foreground mb-2 line-clamp-2",
                            "{display_summary}"
                        }
                    }
                    div { class: "flex items-center gap-2",
                        if let Some(meta) = metadata {
                            if let Some(picture) = &meta.picture {
                                img {
                                    class: "w-6 h-6 rounded-full",
                                    src: "{picture}",
                                    alt: "Avatar",
                                }
                            } else {
                                div { class: "w-6 h-6 rounded-full bg-blue-500 flex items-center justify-center text-white text-xs font-bold",
                                    "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                                }
                            }
                        } else {
                            div { class: "w-6 h-6 rounded-full bg-gray-400 flex items-center justify-center text-white text-xs",
                                "?"
                            }
                        }
                        span { class: "text-xs text-muted-foreground", "{display_name}" }
                        span { class: "text-xs text-muted-foreground", "• Article" }
                    }
                }
            }
        }
    }
}
/// Render a wiki article minicard with HoverCard preview
fn render_wiki_minicard(wiki: &WikiArticle, _naddr: &str, _event: &Event) -> Element {
    let title = wiki.title.clone();
    let identifier = wiki.identifier.clone();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::WikiDetail {
                    identifier: identifier.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-8 h-8 rounded bg-purple-500/10 flex items-center justify-center shrink-0",
                    icons::BookOpenIcon { class: "w-4 h-4 text-purple-500".to_string() }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{title}" }
                    p { class: "text-xs text-muted-foreground", "Wiki Article" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-80 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        h4 { class: "font-bold mb-2", "{title}" }
                        if let Some(summary) = &wiki.summary {
                            p { class: "text-sm text-muted-foreground mb-2 line-clamp-3",
                                "{summary}"
                            }
                        }
                        p { class: "text-xs text-muted-foreground", "Click to view full article" }
                    }
                }
            }
        }
    }
}
/// Render a product minicard with HoverCard preview
fn render_product_minicard(product: &Product, naddr: &str, _event: &Event) -> Element {
    let title = product.title.clone();
    let price_display = if product.price.is_sats() {
        Some(format!("{}", product.price.amount as u64))
    } else {
        None
    };
    let image_url = product.images.first().map(|i| i.url.clone());
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::ShopProductDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-10 h-10 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image_url {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center",
                            icons::ShoppingBagIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{title}" }
                    if let Some(ref price) = price_display {
                        p { class: "text-xs text-primary font-semibold", "⚡ {price} sats" }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-80 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = image_url {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-square object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold mb-1", "{title}" }
                        if let Some(ref price) = price_display {
                            p { class: "text-lg text-primary font-semibold", "⚡ {price} sats" }
                        }
                        if let Some(summary) = &product.summary {
                            p { class: "text-sm text-muted-foreground mt-2 line-clamp-2",
                                "{summary}"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render a badge definition minicard with HoverCard preview
fn render_badge_minicard(badge: &BadgeDefinition, naddr: &str) -> Element {
    let name = badge.name.clone().unwrap_or_else(|| "Badge".to_string());
    let image_url = badge.image.clone();
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::BadgeDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-8 h-8 rounded-full bg-amber-500/10 shrink-0 overflow-hidden flex items-center justify-center",
                    if let Some(ref img) = image_url {
                        img {
                            src: "{img}",
                            alt: "{name}",
                            class: "w-full h-full object-cover",
                        }
                    } else {
                        icons::DiscIcon { class: "w-4 h-4 text-amber-500".to_string() }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{name}" }
                    p { class: "text-xs text-muted-foreground", "Badge" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        div { class: "flex items-center gap-3 mb-2",
                            if let Some(ref img) = image_url {
                                img {
                                    src: "{img}",
                                    alt: "{name}",
                                    class: "w-12 h-12 rounded-full",
                                }
                            }
                            h4 { class: "font-bold", "{name}" }
                        }
                        if let Some(desc) = &badge.description {
                            p { class: "text-sm text-muted-foreground line-clamp-3",
                                "{desc}"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render a music track minicard with HoverCard preview
fn render_track_minicard(track: &NostrTrack, _naddr: &str) -> Element {
    let title = track.title.clone();
    let image = track.image.clone();
    let duration = track
        .duration
        .map(|d| {
            let mins = d / 60;
            let secs = d % 60;
            format!("{}:{:02}", mins, secs)
        });
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                div { class: "w-10 h-10 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center",
                            icons::MusicIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "🎵 {title}" }
                    if let Some(ref dur) = duration {
                        p { class: "text-xs text-muted-foreground", "{dur}" }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = image {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-square object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold", "{title}" }
                        if let Some(ref dur) = duration {
                            p { class: "text-sm text-muted-foreground", "Duration: {dur}" }
                        }
                        if !track.genres.is_empty() {
                            p { class: "text-xs text-muted-foreground mt-1",
                                "Genres: {track.genres.join(\", \")}"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render a playlist minicard with HoverCard preview
fn render_playlist_minicard(playlist: &NostrPlaylist, naddr: &str) -> Element {
    let title = playlist.title.clone();
    let track_count = playlist.track_refs.len();
    let image = playlist.image.clone();
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::MusicPlaylistDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-10 h-10 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-purple-500/20 to-pink-500/20",
                            icons::MusicIcon { class: "w-5 h-5 text-purple-500".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{title}" }
                    p { class: "text-xs text-muted-foreground", "{track_count} tracks" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = image {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-square object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold", "{title}" }
                        p { class: "text-sm text-muted-foreground", "{track_count} tracks" }
                        if let Some(desc) = &playlist.description {
                            p { class: "text-xs text-muted-foreground mt-1 line-clamp-2",
                                "{desc}"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render a recipe minicard with HoverCard preview
fn render_recipe_minicard(
    meta: &RecipeMetadata,
    naddr: &str,
    _event: &Event,
) -> Element {
    let title = meta.title.clone();
    let image_url = meta.primary_image().cloned();
    let summary = meta.summary.clone();
    let tags = meta.tags.clone();
    let naddr_owned = naddr.to_string();
    let displayed_tags: Vec<String> = tags.iter().take(2).cloned().collect();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::RecipeDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-12 h-12 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image_url {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-orange-500/20 to-amber-500/20",
                            span { class: "text-lg", "🍳" }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "🍽️ {title}" }
                    if !displayed_tags.is_empty() {
                        p { class: "text-xs text-muted-foreground truncate",
                            "{displayed_tags.join(\", \")}"
                        }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = image_url {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-video object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold mb-1", "{title}" }
                        if let Some(ref sum) = summary {
                            p { class: "text-sm text-muted-foreground line-clamp-2",
                                "{sum}"
                            }
                        }
                        if !tags.is_empty() {
                            div { class: "flex flex-wrap gap-1 mt-2",
                                for tag in tags.iter().take(4) {
                                    span { class: "px-2 py-0.5 text-xs bg-primary/10 text-primary rounded-full",
                                        "{tag}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render a publication minicard with HoverCard preview
fn render_publication_minicard(pub_index: &PublicationIndex, naddr: &str) -> Element {
    let title = pub_index.title.clone();
    let summary = pub_index.summary.clone();
    let cover_image = pub_index.cover_image.clone();
    let section_count = pub_index.section_addresses.len();
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::PublicationDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-12 h-16 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = cover_image {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-blue-500/20 to-purple-500/20",
                            icons::BookOpenIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "📚 {title}" }
                    p { class: "text-xs text-muted-foreground", "{section_count} sections" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = cover_image {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-[2/1] object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold mb-1", "{title}" }
                        if let Some(ref sum) = summary {
                            p { class: "text-sm text-muted-foreground line-clamp-2",
                                "{sum}"
                            }
                        }
                        p { class: "text-xs text-muted-foreground mt-1", "{section_count} sections" }
                    }
                }
            }
        }
    }
}
/// Render a pinboard minicard with HoverCard preview
fn render_pinboard_minicard(board: &Pinboard, naddr: &str) -> Element {
    let title = board.title.clone();
    let description = board.description.clone();
    let image = board.image.clone();
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::PinBoardDetail {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-10 h-10 rounded bg-muted shrink-0 overflow-hidden",
                    if let Some(ref img) = image {
                        img {
                            src: "{img}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-pink-500/20 to-red-500/20",
                            icons::GridIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "📌 {title}" }
                    if !board.tags.is_empty() {
                        p { class: "text-xs text-muted-foreground truncate",
                            "{board.tags.join(\", \")}"
                        }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        if let Some(ref img) = image {
                            img {
                                src: "{img}",
                                alt: "{title}",
                                class: "w-full aspect-square object-cover rounded mb-2",
                            }
                        }
                        h4 { class: "font-bold", "{title}" }
                        if let Some(ref desc) = description {
                            p { class: "text-sm text-muted-foreground line-clamp-2",
                                "{desc}"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render a product collection minicard with HoverCard preview
fn render_collection_minicard(collection: &ProductCollection, naddr: &str) -> Element {
    let title = collection.title.clone();
    let description = collection.description.clone();
    let product_count = collection.products.len();
    let naddr_owned = naddr.to_string();
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::ShopCollection {
                    naddr: naddr_owned.clone(),
                },
                class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                div { class: "w-10 h-10 rounded bg-muted shrink-0 flex items-center justify-center bg-gradient-to-br from-green-500/20 to-emerald-500/20",
                    icons::ShoppingBagIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "🛍️ {title}" }
                    p { class: "text-xs text-muted-foreground", "{product_count} products" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        h4 { class: "font-bold mb-1", "{title}" }
                        p { class: "text-sm text-primary font-medium", "{product_count} products" }
                        if let Some(ref desc) = description {
                            p { class: "text-sm text-muted-foreground mt-1 line-clamp-2",
                                "{desc}"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render a product review minicard with HoverCard preview
fn render_review_minicard(review: &ProductReview, _naddr: &str) -> Element {
    let content = review.content.clone();
    let rating = review.thumb_rating;
    let rating_display = if rating >= 0.5 { "👍" } else { "👎" };
    let quality_str = review.quality_rating.map(|q| format!("{:.1}", q));
    let value_str = review.value_rating.map(|v| format!("{:.1}", v));
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                div { class: "w-8 h-8 rounded bg-muted shrink-0 flex items-center justify-center",
                    span { class: "text-lg", "{rating_display}" }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm", "Product Review" }
                    p { class: "text-xs text-muted-foreground truncate", "{content}" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-64 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        div { class: "flex items-center gap-2 mb-2",
                            span { class: "text-2xl", "{rating_display}" }
                            span { class: "font-bold", "Product Review" }
                        }
                        if !content.is_empty() {
                            p { class: "text-sm text-muted-foreground line-clamp-4",
                                "{content}"
                            }
                        }
                        if let Some(ref q) = quality_str {
                            p { class: "text-xs text-muted-foreground mt-1", "Quality: {q}/5" }
                        }
                        if let Some(ref v) = value_str {
                            p { class: "text-xs text-muted-foreground", "Value: {v}/5" }
                        }
                    }
                }
            }
        }
    }
}
/// Render a Git Issue minicard with HoverCard preview
fn render_issue_minicard(issue: &Issue) -> Element {
    let title = issue.display_title();
    let status = issue.status;
    let status_class = match status {
        crate::utils::nip34::IssueStatus::Open => "bg-green-500/20 text-green-500",
        crate::utils::nip34::IssueStatus::Closed => "bg-red-500/20 text-red-500",
        crate::utils::nip34::IssueStatus::Applied => "bg-purple-500/20 text-purple-500",
        crate::utils::nip34::IssueStatus::Draft => "bg-yellow-500/20 text-yellow-500",
    };
    let status_text = format!("{:?}", status);
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                div { class: "w-8 h-8 rounded bg-muted shrink-0 flex items-center justify-center",
                    icons::CommentIcon { class: "w-4 h-4 text-green-500".to_string() }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "🔧 {title}" }
                    div { class: "flex items-center gap-2",
                        span { class: "px-1.5 py-0.5 text-xs rounded {status_class}",
                            "{status_text}"
                        }
                        if !issue.labels.is_empty() {
                            span { class: "text-xs text-muted-foreground truncate",
                                "{issue.labels.join(\", \")}"
                            }
                        }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        h4 { class: "font-bold mb-1", "{title}" }
                        span { class: "px-2 py-0.5 text-xs rounded {status_class}", "{status_text}" }
                        if !issue.labels.is_empty() {
                            div { class: "flex flex-wrap gap-1 mt-2",
                                for label in issue.labels.iter().take(4) {
                                    span { class: "px-2 py-0.5 text-xs bg-muted text-muted-foreground rounded-full",
                                        "{label}"
                                    }
                                }
                            }
                        }
                        p { class: "text-xs text-muted-foreground mt-2", "Git Issue" }
                    }
                }
            }
        }
    }
}
/// Render a Git PR minicard with HoverCard preview
fn render_pr_minicard(pr: &PullRequest) -> Element {
    let title = if pr.is_cover_letter {
        pr.content.lines().next().unwrap_or("Pull Request").to_string()
    } else {
        format!(
            "Patch: {}",
            pr.commit.as_deref().unwrap_or("").chars().take(8).collect::<String>(),
        )
    };
    let status = pr.status;
    let status_class = match status {
        crate::utils::nip34::IssueStatus::Open => "bg-green-500/20 text-green-500",
        crate::utils::nip34::IssueStatus::Closed => "bg-red-500/20 text-red-500",
        crate::utils::nip34::IssueStatus::Applied => "bg-purple-500/20 text-purple-500",
        crate::utils::nip34::IssueStatus::Draft => "bg-yellow-500/20 text-yellow-500",
    };
    let status_text = format!("{:?}", status);
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                div { class: "w-8 h-8 rounded bg-muted shrink-0 flex items-center justify-center",
                    icons::GitMergeIcon { class: "w-4 h-4 text-purple-500".to_string() }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "🔀 {title}" }
                    span { class: "px-1.5 py-0.5 text-xs rounded {status_class}", "{status_text}" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        h4 { class: "font-bold mb-1", "{title}" }
                        span { class: "px-2 py-0.5 text-xs rounded {status_class}", "{status_text}" }
                        if let Some(ref commit) = pr.commit {
                            p { class: "text-xs text-muted-foreground mt-2", "Commit: {commit}" }
                        }
                        p { class: "text-xs text-muted-foreground", "Git Patch/PR" }
                    }
                }
            }
        }
    }
}
/// Render a repost minicard
fn render_repost_minicard(event: &Event) -> Element {
    let reposted_id = event
        .tags
        .iter()
        .find_map(|t| {
            if t.kind() == nostr_sdk::TagKind::e() {
                t.content().map(|s| s.to_string())
            } else {
                None
            }
        })
        .or_else(|| {
            if event.content.starts_with("nostr:") {
                Some(
                    event
                        .content
                        .strip_prefix("nostr:")
                        .unwrap_or(&event.content)
                        .to_string(),
                )
            } else {
                None
            }
        });
    let short_id = reposted_id
        .as_ref()
        .map(|id| {
            if id.len() > 16 {
                format!("{}...{}", &id[..8], &id[id.len() - 4..])
            } else {
                id.clone()
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                icons::Repeat2Icon { class: "w-4 h-4 text-green-500".to_string() }
                span { class: "text-sm text-muted-foreground", "Repost of " }
                span { class: "text-sm font-medium text-primary", "{short_id}" }
            }
        }
    }
}
/// Render a comment minicard (NIP-22)
fn render_comment_minicard(event: &Event, metadata: Option<&Metadata>) -> Element {
    let content = &event.content;
    let display_content = if content.chars().count() > 100 {
        format!("{}...", content.chars().take(100).collect::<String>())
    } else {
        content.clone()
    };
    let author_name = metadata
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            let pk = event.pubkey.to_hex();
            format!("{}...{}", &pk[..8], &pk[pk.len() - 4..])
        });
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-start gap-2 p-2 border border-border rounded-lg bg-card",
                icons::MessageCircleIcon { class: "w-4 h-4 text-blue-500 shrink-0 mt-0.5".to_string() }
                div { class: "flex-1 min-w-0",
                    p { class: "text-xs text-muted-foreground", "Comment by {author_name}" }
                    p { class: "text-sm line-clamp-2", "{display_content}" }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        p { class: "text-xs text-muted-foreground mb-1", "Comment by {author_name}" }
                        p { class: "text-sm", "{content}" }
                    }
                }
            }
        }
    }
}
/// Render a citation minicard with HoverCard preview
fn render_citation_minicard(citation: &Citation) -> Element {
    let base = citation.base();
    let title = base.title.clone();
    let author = base.author.clone();
    let citation_type = citation.citation_type();
    let style = get_citation_style(&citation_type);
    let type_icon = style.emoji;
    let type_text = style.label;
    let type_color = style.text_class;
    rsx! {
        div {
            class: "relative my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div { class: "flex items-center gap-2 p-2 border border-border rounded-lg bg-card",
                div { class: "w-8 h-8 rounded bg-muted shrink-0 flex items-center justify-center",
                    span { class: "text-sm", "{type_icon}" }
                }
                div { class: "flex-1 min-w-0",
                    p { class: "font-medium text-sm truncate", "{title}" }
                    if !author.is_empty() {
                        p { class: "text-xs text-muted-foreground truncate", "by {author}" }
                    }
                }
            }
            div { class: "absolute bottom-1 right-1",
                HoverCard { open: Signal::new(None),
                    HoverCardTrigger { NostrBlueMiniLogo {} }
                    HoverCardContent {
                        side: ContentSide::Top,
                        class: "w-72 p-4 bg-popover border border-border rounded-lg shadow-lg",
                        div { class: "flex items-center gap-2 mb-2",
                            span { class: "text-lg", "{type_icon}" }
                            span { class: "text-xs font-medium {type_color}", "{type_text}" }
                        }
                        h4 { class: "font-bold mb-1", "{title}" }
                        if !author.is_empty() {
                            p { class: "text-sm text-muted-foreground", "by {author}" }
                        }
                        if let Some(ref summary) = base.summary {
                            p { class: "text-xs text-muted-foreground mt-2 line-clamp-3",
                                "{summary}"
                            }
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn WavlakeTrackRenderer(track_id: String) -> Element {
    let track_resource = use_resource(move || {
        let id = track_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_track(&id).await
        }
    });
    match track_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-3",
                        div { class: "w-16 h-16 bg-muted rounded" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-4 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/2" }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            rsx! {
                div {
                    class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-2 text-red-500 text-sm",
                        icons::MusicIcon { class: "w-4 h-4" }
                        span { "Unable to load track: {e}" }
                    }
                }
            }
        }
        Some(Ok(track)) => {
            let track_clone = track.clone();
            let handle_play = move |_: MouseEvent| {
                let music_track: MusicTrack = track_clone.clone().into();
                music_player::play_track(music_track, None, None);
            };
            rsx! {
                div {
                    class: "my-2 border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition bg-card",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-4 p-4",
                        div { class: "relative w-16 h-16 shrink-0 rounded overflow-hidden bg-muted group",
                            img {
                                src: "{track.album_art_url}",
                                alt: "Album art",
                                class: "w-full h-full object-cover",
                            }
                            button {
                                class: "absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition",
                                onclick: handle_play,
                                dangerous_inner_html: icons::PLAY,
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "font-semibold text-sm truncate", "{track.title}" }
                            div { class: "text-xs text-muted-foreground truncate",
                                Link {
                                    to: Route::MusicArtist {
                                        artist_id: track.artist_id.clone(),
                                    },
                                    class: "hover:text-foreground hover:underline",
                                    onclick: move |e: dioxus::prelude::Event<MouseData>| e.stop_propagation(),
                                    "{track.artist}"
                                }
                            }
                            div { class: "text-xs text-muted-foreground/80 truncate mt-1",
                                Link {
                                    to: Route::MusicAlbum {
                                        album_id: track.album_id.clone(),
                                    },
                                    class: "hover:text-foreground hover:underline",
                                    onclick: move |e: dioxus::prelude::Event<MouseData>| e.stop_propagation(),
                                    "{track.album_title}"
                                }
                            }
                        }
                        div { class: "flex flex-col items-end gap-1 shrink-0",
                            div { class: "text-xs text-muted-foreground",
                                {
                                    let mins = track.duration / 60;
                                    let secs = track.duration % 60;
                                    format!("{:02}:{:02}", mins, secs)
                                }
                            }
                            div { class: "flex items-center gap-1 text-xs text-purple-400",
                                icons::MusicIcon { class: "w-3 h-3" }
                                "Wavlake"
                            }
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn WavlakeAlbumRenderer(album_id: String) -> Element {
    let album_resource = use_resource(move || {
        let id = album_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_album(&id).await
        }
    });
    match album_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex gap-4",
                        div { class: "w-32 h-32 bg-muted rounded" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-5 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/2" }
                            div { class: "h-3 bg-muted rounded w-1/3" }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            rsx! {
                div {
                    class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-2 text-red-500 text-sm",
                        icons::DiscIcon { class: "w-4 h-4" }
                        span { "Unable to load album: {e}" }
                    }
                }
            }
        }
        Some(Ok(album)) => {
            let tracks: Vec<MusicTrack> = album
                .tracks
                .iter()
                .map(|track| track.clone().into())
                .collect();
            rsx! {
                div {
                    class: "my-2 border border-border rounded-lg overflow-hidden bg-card",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex gap-4 p-4 border-b border-border",
                        if let Some(art_url) = &album.album_art_url {
                            img {
                                src: "{art_url}",
                                alt: "Album art",
                                class: "w-32 h-32 rounded object-cover shrink-0",
                            }
                        } else {
                            div { class: "w-32 h-32 rounded bg-muted flex items-center justify-center shrink-0",
                                icons::DiscIcon { class: "w-16 h-16 text-muted-foreground" }
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "text-xs text-muted-foreground mb-1", "ALBUM" }
                            div { class: "font-bold text-lg truncate mb-1", "{album.title}" }
                            div { class: "text-sm text-muted-foreground truncate mb-2",
                                a {
                                    href: if let Some(first_track) = album.tracks.first() { format!("/music/artist/{}", first_track.artist_id) } else { "#".to_string() },
                                    class: "hover:text-foreground hover:underline",
                                    onclick: move |e| e.stop_propagation(),
                                    "{album.artist}"
                                }
                            }
                            div { class: "flex items-center gap-3 text-xs text-muted-foreground",
                                span {
                                    {
                                        album
                                            .release_date
                                            .split('T')
                                            .next()
                                            .unwrap_or("Unknown")
                                            .split('-')
                                            .next()
                                            .unwrap_or("Unknown")
                                    }
                                }
                                span { "•" }
                                span {
                                    "{album.tracks.len()} "
                                    {if album.tracks.len() == 1 { "track" } else { "tracks" }}
                                }
                                span { "•" }
                                span { class: "flex items-center gap-1 text-purple-400",
                                    icons::MusicIcon { class: "w-3 h-3" }
                                    "Wavlake"
                                }
                            }
                        }
                    }
                    div { class: "divide-y divide-border",
                        for (index , track_data) in album.tracks.iter().enumerate() {
                            {
                                let track_clone = tracks[index].clone();
                                let playlist = tracks.clone();
                                let track_title = track_data.title.clone();
                                let track_artist = track_data.artist.clone();
                                let track_duration = track_data.duration;
                                rsx! {
                                    div {
                                        key: "{track_data.id}",
                                        class: "flex items-center gap-3 p-3 hover:bg-accent/10 transition cursor-pointer group",
                                        onclick: move |_| {
                                            music_player::play_track(
                                                track_clone.clone(),
                                                Some(playlist.clone()),
                                                Some(index),
                                            );
                                        },
                                        div { class: "w-8 text-center text-sm text-muted-foreground shrink-0",
                                            span { class: "group-hover:hidden", "{index + 1}" }
                                            div {
                                                class: "hidden group-hover:flex items-center justify-center",
                                                dangerous_inner_html: icons::PLAY,
                                            }
                                        }
                                        div { class: "flex-1 min-w-0",
                                            div { class: "font-medium text-sm truncate", "{track_title}" }
                                            div { class: "text-xs text-muted-foreground truncate", "{track_artist}" }
                                        }
                                        div { class: "text-xs text-muted-foreground shrink-0",
                                            {
                                                let mins = track_duration / 60;
                                                let secs = track_duration % 60;
                                                format!("{:02}:{:02}", mins, secs)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn WavlakeArtistRenderer(artist_id: String) -> Element {
    let artist_resource = use_resource(move || {
        let id = artist_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_artist(&id).await
        }
    });
    let nav = use_navigator();
    match artist_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-4",
                        div { class: "w-20 h-20 bg-muted rounded-full" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-5 bg-muted rounded w-1/2" }
                            div { class: "h-3 bg-muted rounded w-1/3" }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            rsx! {
                div {
                    class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-2 text-red-500 text-sm",
                        icons::UserIcon { class: "w-4 h-4" }
                        span { "Unable to load artist: {e}" }
                    }
                }
            }
        }
        Some(Ok(artist)) => {
            rsx! {
                div {
                    class: "my-2 border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition bg-card cursor-pointer",
                    onclick: {
                        let artist_id_nav = artist.id.clone();
                        let navigator = nav;
                        move |e: MouseEvent| {
                            e.stop_propagation();
                            navigator
                                .push(Route::MusicArtist {
                                    artist_id: artist_id_nav.clone(),
                                });
                        }
                    },
                    div { class: "flex items-center gap-4 p-4",
                        if let Some(art_url) = &artist.artist_art_url {
                            if !art_url.is_empty() {
                                img {
                                    src: "{art_url}",
                                    alt: "Artist",
                                    class: "w-20 h-20 rounded-full object-cover shrink-0",
                                }
                            } else {
                                div { class: "w-20 h-20 rounded-full bg-muted flex items-center justify-center shrink-0",
                                    icons::UserIcon { class: "w-10 h-10 text-muted-foreground" }
                                }
                            }
                        } else {
                            div { class: "w-20 h-20 rounded-full bg-muted flex items-center justify-center shrink-0",
                                icons::UserIcon { class: "w-10 h-10 text-muted-foreground" }
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "text-xs text-muted-foreground mb-1", "ARTIST" }
                            div { class: "font-bold text-lg truncate mb-1", "{artist.name}" }
                            div { class: "flex items-center gap-2 text-xs text-muted-foreground",
                                span {
                                    "{artist.albums.len()} "
                                    {if artist.albums.len() == 1 { "album" } else { "albums" }}
                                }
                                span { "•" }
                                span { class: "flex items-center gap-1 text-purple-400",
                                    icons::MusicIcon { class: "w-3 h-3" }
                                    "Wavlake"
                                }
                            }
                        }
                        div {
                            class: "shrink-0 text-muted-foreground",
                            dangerous_inner_html: r#"<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5"><path stroke-linecap="round" stroke-linejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" /></svg>"#,
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn WavlakePlaylistRenderer(playlist_id: String) -> Element {
    let playlist_resource = use_resource(move || {
        let id = playlist_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_playlist(&id).await
        }
    });
    match playlist_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex gap-4",
                        div { class: "w-32 h-32 bg-muted rounded" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-5 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/2" }
                        }
                    }
                }
            }
        }
        Some(Err(e)) => {
            rsx! {
                div {
                    class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-2 text-red-500 text-sm",
                        icons::MusicIcon { class: "w-4 h-4" }
                        span { "Unable to load playlist: {e}" }
                    }
                }
            }
        }
        Some(Ok(playlist)) => {
            let tracks: Vec<MusicTrack> = playlist
                .tracks
                .iter()
                .map(|track| track.clone().into())
                .collect();
            rsx! {
                div {
                    class: "my-2 border border-border rounded-lg overflow-hidden bg-card",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex gap-4 p-4 border-b border-border",
                        if let Some(first_track) = playlist.tracks.first() {
                            img {
                                src: "{first_track.album_art_url}",
                                alt: "Playlist cover",
                                class: "w-32 h-32 rounded object-cover shrink-0",
                            }
                        } else {
                            div { class: "w-32 h-32 rounded bg-muted flex items-center justify-center shrink-0",
                                icons::MusicIcon { class: "w-16 h-16 text-muted-foreground" }
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "text-xs text-muted-foreground mb-1", "PLAYLIST" }
                            div { class: "font-bold text-lg truncate mb-1", "{playlist.title}" }
                            div { class: "flex items-center gap-3 text-xs text-muted-foreground",
                                span {
                                    "{playlist.tracks.len()} "
                                    {if playlist.tracks.len() == 1 { "track" } else { "tracks" }}
                                }
                                span { "•" }
                                span { class: "flex items-center gap-1 text-purple-400",
                                    icons::MusicIcon { class: "w-3 h-3" }
                                    "Wavlake"
                                }
                            }
                        }
                    }
                    div { class: "divide-y divide-border max-h-96 overflow-y-auto",
                        for (index , track_data) in playlist.tracks.iter().enumerate() {
                            {
                                let track_clone = tracks[index].clone();
                                let playlist_clone = tracks.clone();
                                let track_title = track_data.title.clone();
                                let track_artist = track_data.artist.clone();
                                let track_duration = track_data.duration;
                                let track_album_art = track_data.album_art_url.clone();
                                rsx! {
                                    div {
                                        key: "{track_data.id}",
                                        class: "flex items-center gap-3 p-3 hover:bg-accent/10 transition cursor-pointer group",
                                        onclick: move |_| {
                                            music_player::play_track(
                                                track_clone.clone(),
                                                Some(playlist_clone.clone()),
                                                Some(index),
                                            );
                                        },
                                        div { class: "relative w-10 h-10 shrink-0 rounded overflow-hidden bg-muted group-hover:opacity-80",
                                            img {
                                                src: "{track_album_art}",
                                                alt: "Album art",
                                                class: "w-full h-full object-cover",
                                            }
                                            div {
                                                class: "absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition",
                                                dangerous_inner_html: icons::PLAY_SMALL,
                                            }
                                        }
                                        div { class: "flex-1 min-w-0",
                                            div { class: "font-medium text-sm truncate", "{track_title}" }
                                            div { class: "text-xs text-muted-foreground truncate", "{track_artist}" }
                                        }
                                        div { class: "text-xs text-muted-foreground shrink-0",
                                            {
                                                let mins = track_duration / 60;
                                                let secs = track_duration % 60;
                                                format!("{:02}:{:02}", mins, secs)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Renders a YouTube embed with click-to-load for privacy/performance
#[component]
fn YouTubeRenderer(video_id: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let mut tried_fallback = use_signal(|| false);
    let video_id_for_fallback = video_id.clone();
    let thumbnail_url = format!(
        "https://img.youtube.com/vi/{}/maxresdefault.jpg",
        video_id,
    );
    let fallback_url = format!("https://img.youtube.com/vi/{}/hqdefault.jpg", video_id);
    let embed_url = format!("https://www.youtube.com/embed/{}?autoplay=1", video_id);
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden bg-black aspect-video max-w-full",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    class: "w-full aspect-video",
                    allow: "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture",
                    allowfullscreen: true,
                    frame_border: "0",
                }
            } else {
                div {
                    class: "relative w-full aspect-video cursor-pointer group",
                    onclick: move |_| is_visible.set(true),
                    img {
                        src: if *tried_fallback.read() { "{fallback_url}" } else { "{thumbnail_url}" },
                        alt: "YouTube video thumbnail",
                        class: "w-full h-full object-cover",
                        onerror: move |_| {
                            if !*tried_fallback.peek() {
                                log::debug!(
                                    "YouTube maxresdefault.jpg failed for {}, trying hqdefault.jpg",
                                    video_id_for_fallback
                                );
                                tried_fallback.set(true);
                            }
                        },
                    }
                    div { class: "absolute inset-0 flex items-center justify-center bg-black/30 group-hover:bg-black/40 transition",
                        div { class: "w-16 h-16 bg-red-600 rounded-full flex items-center justify-center shadow-lg group-hover:scale-110 transition",
                            svg {
                                class: "w-8 h-8 text-white ml-1",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path { d: "M8 5v14l11-7z" }
                            }
                        }
                    }
                    div { class: "absolute bottom-2 right-2 px-2 py-1 bg-black/70 rounded text-white text-xs font-medium",
                        "YouTube"
                    }
                }
            }
        }
    }
}
/// Renders a Spotify embed
#[component]
fn SpotifyRenderer(content_type: String, content_id: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let embed_url = format!(
        "https://open.spotify.com/embed/{}/{}?utm_source=generator&theme=0",
        content_type,
        content_id,
    );
    let height = match content_type.as_str() {
        "track" => "152",
        _ => "352",
    };
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    width: "100%",
                    height: "{height}",
                    frame_border: "0",
                    allow: "autoplay; clipboard-write; encrypted-media; fullscreen; picture-in-picture",
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#1DB954]/10 border border-[#1DB954]/30 rounded-lg cursor-pointer hover:bg-[#1DB954]/20 transition",
                    onclick: move |_| is_visible.set(true),
                    div { class: "w-12 h-12 bg-[#1DB954] rounded-full flex items-center justify-center shrink-0",
                        svg {
                            class: "w-7 h-7 text-black",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path { d: "M12 0C5.4 0 0 5.4 0 12s5.4 12 12 12 12-5.4 12-12S18.66 0 12 0zm5.521 17.34c-.24.359-.66.48-1.021.24-2.82-1.74-6.36-2.101-10.561-1.141-.418.122-.779-.179-.899-.539-.12-.421.18-.78.54-.9 4.56-1.021 8.52-.6 11.64 1.32.42.18.479.659.301 1.02zm1.44-3.3c-.301.42-.841.6-1.262.3-3.239-1.98-8.159-2.58-11.939-1.38-.479.12-1.02-.12-1.14-.6-.12-.48.12-1.021.6-1.141C9.6 9.9 15 10.561 18.72 12.84c.361.181.54.78.241 1.2zm.12-3.36C15.24 8.4 8.82 8.16 5.16 9.301c-.6.179-1.2-.181-1.38-.721-.18-.601.18-1.2.72-1.381 4.26-1.26 11.28-1.02 15.721 1.621.539.3.719 1.02.419 1.56-.299.421-1.02.599-1.559.3z" }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm text-[#1DB954]", "Spotify {content_type}" }
                        div { class: "text-xs text-muted-foreground", "Click to load player" }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL,
                    }
                }
            }
        }
    }
}
/// Renders a SoundCloud embed
#[component]
fn SoundCloudRenderer(url: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let encoded_url = urlencoding::encode(&url);
    let embed_url = format!(
        "https://w.soundcloud.com/player/?url={}&color=%23ff5500&auto_play=false&hide_related=false&show_comments=true&show_user=true&show_reposts=false&show_teaser=true&visual=true",
        encoded_url,
    );
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    width: "100%",
                    height: "166",
                    frame_border: "0",
                    allow: "autoplay",
                    scrolling: "no",
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#ff5500]/10 border border-[#ff5500]/30 rounded-lg cursor-pointer hover:bg-[#ff5500]/20 transition",
                    onclick: move |_| is_visible.set(true),
                    div { class: "w-12 h-12 bg-[#ff5500] rounded-full flex items-center justify-center shrink-0",
                        svg {
                            class: "w-7 h-7 text-white",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                d: "M1.175 12.225c-.051 0-.094.046-.101.1l-.233 2.154.233 2.105c.007.058.05.098.101.098.05 0 .09-.04.099-.098l.255-2.105-.27-2.154c-.009-.06-.052-.1-.102-.1m-.899.828c-.06 0-.091.037-.104.094L0 14.479l.165 1.308c.014.057.045.094.09.094s.089-.037.099-.094l.19-1.308-.19-1.332c-.01-.057-.045-.094-.09-.094m1.83-1.229c-.061 0-.12.045-.12.104l-.21 2.563.225 2.458c0 .06.045.104.106.104.061 0 .12-.044.12-.104l.24-2.458-.24-2.563c0-.06-.059-.104-.12-.104m.945-.089c-.075 0-.135.06-.15.135l-.193 2.64.21 2.544c.016.077.075.138.149.138.075 0 .135-.061.15-.138l.24-2.544-.24-2.64c-.015-.075-.074-.135-.15-.135m.93-.104c-.09 0-.165.075-.18.165l-.178 2.73.195 2.61c.015.09.089.164.179.164.09 0 .164-.074.18-.164l.21-2.61-.21-2.73c-.015-.09-.09-.165-.18-.165m.964-.03c-.105 0-.195.09-.21.195l-.165 2.76.18 2.64c.015.105.105.18.21.18s.195-.075.21-.18l.195-2.64-.195-2.76c-.015-.105-.105-.195-.21-.195m1.005.15c-.12 0-.225.105-.225.225l-.15 2.595.165 2.655c0 .12.105.225.225.225s.225-.105.225-.225l.18-2.655-.18-2.595c0-.12-.105-.225-.225-.225m1.02-.135c-.135 0-.255.12-.255.255l-.135 2.58.15 2.685c0 .135.12.24.255.24s.255-.105.255-.24l.165-2.685-.165-2.58c0-.135-.12-.255-.255-.255m2.04.165c-.15 0-.285.135-.285.285l-.12 2.4.135 2.67c0 .15.135.285.285.285s.285-.135.285-.285l.15-2.67-.15-2.4c0-.15-.135-.285-.285-.285m-1.02-.15c-.15 0-.27.12-.27.27l-.135 2.55.135 2.67c0 .135.12.255.27.255.135 0 .255-.12.27-.255l.15-2.67-.15-2.55c-.015-.15-.135-.27-.27-.27m2.04-.105c-.165 0-.3.135-.315.3l-.105 2.415.12 2.685c.015.165.15.3.315.3.15 0 .285-.135.3-.3l.135-2.685-.135-2.415c-.015-.165-.15-.3-.3-.3m1.02.105c-.18 0-.33.15-.33.33l-.105 2.295.105 2.685c0 .165.15.315.33.315.165 0 .315-.15.33-.315l.12-2.685-.12-2.295c-.015-.18-.165-.33-.33-.33m1.02-.255c-.195 0-.345.15-.36.345l-.09 2.535.105 2.7c.015.18.165.33.345.33.195 0 .345-.15.36-.33l.12-2.7-.12-2.535c-.015-.195-.165-.345-.36-.345m1.034.035c-.21 0-.375.165-.375.375l-.09 2.52.09 2.685c0 .21.165.375.375.375.195 0 .36-.165.375-.375l.105-2.685-.105-2.52c-.015-.21-.18-.375-.375-.375m1.035-.18c-.225 0-.405.18-.405.405l-.075 2.295.075 2.685c0 .225.18.405.405.405.21 0 .39-.18.405-.405l.09-2.685-.09-2.295c-.015-.225-.195-.405-.405-.405m1.02-.24c-.225 0-.42.195-.42.42l-.06 2.13.075 2.685c0 .225.195.405.42.405.225 0 .405-.18.42-.405l.09-2.685-.09-2.13c-.015-.225-.195-.42-.42-.42m1.034-.09c-.24 0-.435.195-.435.435l-.06 1.83.06 2.685c0 .24.195.435.435.435.24 0 .435-.195.435-.435l.075-2.685-.075-1.83c0-.24-.195-.435-.435-.435m1.05.075c-.255 0-.465.21-.465.465l-.045 1.35.06 2.67c0 .255.21.465.465.465.24 0 .45-.21.465-.465l.06-2.67-.06-1.35c-.015-.255-.225-.465-.465-.465m1.035-.42c-.27 0-.495.225-.495.495l-.03.96.045 2.67c0 .27.225.495.495.495s.48-.225.495-.495l.06-2.67-.06-.96c-.015-.27-.225-.495-.495-.495m1.05.27c-.285 0-.51.24-.51.525l-.03.66.045 2.685c0 .285.225.51.51.51.27 0 .495-.225.51-.51l.06-2.685-.06-.66c-.015-.285-.24-.525-.51-.525m1.065.435c-.3 0-.54.24-.54.54v.195l.03 2.7c0 .285.24.525.54.525.285 0 .525-.24.54-.54l.045-2.685-.045-.195c-.015-.3-.255-.54-.54-.54m2.28 1.29c-.135 0-.27.015-.39.045-.105-.885-.87-1.575-1.8-1.575-.255 0-.51.06-.72.165-.09.045-.12.09-.12.18v5.295c0 .09.06.165.15.18.015 0 2.88 0 2.88 0 .945 0 1.71-.765 1.71-1.71s-.765-1.71-1.71-1.71",
                            }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm text-[#ff5500]", "SoundCloud" }
                        div { class: "text-xs text-muted-foreground", "Click to load player" }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL,
                    }
                }
            }
        }
    }
}
/// Renders an Apple Music embed
#[component]
fn AppleMusicRenderer(embed_url: String, is_song: bool) -> Element {
    let mut is_visible = use_signal(|| false);
    let final_embed_url = if embed_url.contains("embed.music.apple.com") {
        embed_url.clone()
    } else {
        embed_url.replace("music.apple.com", "embed.music.apple.com")
    };
    let height = if is_song { "175" } else { "450" };
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{final_embed_url}",
                    width: "100%",
                    height: "{height}",
                    frame_border: "0",
                    allow: "autoplay *; encrypted-media *; fullscreen *; clipboard-write",
                    style: "border-radius: 10px;",
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-gradient-to-r from-[#fc3c44]/10 to-[#fa57c1]/10 border border-[#fc3c44]/30 rounded-lg cursor-pointer hover:from-[#fc3c44]/20 hover:to-[#fa57c1]/20 transition",
                    onclick: move |_| is_visible.set(true),
                    div { class: "w-12 h-12 bg-gradient-to-br from-[#fc3c44] to-[#fa57c1] rounded-xl flex items-center justify-center shrink-0",
                        svg {
                            class: "w-7 h-7 text-white",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                d: "M23.994 6.124a9.23 9.23 0 00-.24-2.19c-.317-1.31-1.062-2.31-2.18-3.043a5.022 5.022 0 00-1.877-.726 10.496 10.496 0 00-1.564-.15c-.04-.003-.083-.01-.124-.013H5.99c-.042.003-.083.01-.124.013-.5.032-.999.09-1.486.191a5.023 5.023 0 00-1.815.74c-1.113.737-1.857 1.736-2.177 3.038-.2.808-.255 1.634-.254 2.465.002.05.007.1.01.15v11.28c-.003.05-.008.1-.01.15.001.83.057 1.658.255 2.465.32 1.303 1.064 2.302 2.177 3.039a5.023 5.023 0 001.815.74c.487.1.986.159 1.486.19.041.004.082.01.124.013h12.02c.042-.003.083-.01.124-.013.5-.031.999-.09 1.486-.19a5.023 5.023 0 001.815-.74c1.113-.738 1.857-1.737 2.177-3.04.2-.807.255-1.634.254-2.464-.002-.05-.007-.1-.01-.15V6.274c.003-.05.008-.1.01-.15zM17.5 17.5c0 .397-.063.79-.187 1.163a2.5 2.5 0 01-2.658 1.682c-.674-.099-1.261-.437-1.655-.97-.394-.534-.56-1.202-.46-1.877.098-.674.437-1.261.97-1.656.534-.394 1.202-.56 1.877-.46.28.04.544.126.784.251V9.13c0-.277.175-.524.437-.616l4.5-1.5a.642.642 0 01.842.608v1.5a.643.643 0 01-.437.608l-3.563 1.188a.643.643 0 00-.437.608v6.474z",
                            }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm bg-gradient-to-r from-[#fc3c44] to-[#fa57c1] bg-clip-text text-transparent",
                            "Apple Music"
                        }
                        div { class: "text-xs text-muted-foreground", "Click to load player" }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL,
                    }
                }
            }
        }
    }
}
/// Renders a MixCloud embed
#[component]
fn MixCloudRenderer(username: String, mix_name: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let path = format!("/{}/{}/", username, mix_name);
    let encoded_path = urlencoding::encode(&path);
    let embed_url = format!(
        "https://www.mixcloud.com/widget/iframe/?hide_cover=1&feed={}",
        encoded_path,
    );
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    width: "100%",
                    height: "120",
                    frame_border: "0",
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#5000ff]/10 border border-[#5000ff]/30 rounded-lg cursor-pointer hover:bg-[#5000ff]/20 transition",
                    onclick: move |_| is_visible.set(true),
                    div { class: "w-12 h-12 bg-[#5000ff] rounded-full flex items-center justify-center shrink-0",
                        svg {
                            class: "w-7 h-7 text-white",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path { d: "M19.943 6.667c0-1.534-1.632-2.667-3.432-2.667-1.963 0-3.768 1.12-4.511 2.793-.743-1.673-2.548-2.793-4.511-2.793-1.8 0-3.432 1.133-3.432 2.667S1.98 10 1.98 13.333c0 .867.327 1.667.843 2.267.517.6 1.237 1.067 2.047 1.333.81.267 1.69.4 2.62.4h8.52c.93 0 1.81-.133 2.62-.4.81-.266 1.53-.733 2.047-1.333.516-.6.843-1.4.843-2.267 0-3.333-2.077-6.666-2.077-6.666z" }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm text-[#5000ff]", "MixCloud" }
                        div { class: "text-xs text-muted-foreground truncate", "{username}/{mix_name}" }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL,
                    }
                }
            }
        }
    }
}
/// Renders a Rumble embed
#[component]
fn RumbleRenderer(embed_url: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let final_embed_url = if embed_url.contains("/embed/") {
        embed_url.clone()
    } else if let Some(start) = embed_url.find("/v") {
        let after_v = &embed_url[start + 1..];
        let video_id: String = after_v
            .chars()
            .take_while(|c| *c != '-' && *c != '.' && *c != '/')
            .collect();
        if !video_id.is_empty() {
            format!("https://rumble.com/embed/{}/", video_id)
        } else {
            embed_url.clone()
        }
    } else {
        embed_url.clone()
    };
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden bg-black aspect-video max-w-full",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{final_embed_url}",
                    class: "w-full aspect-video",
                    frame_border: "0",
                    allowfullscreen: true,
                }
            } else {
                div {
                    class: "relative w-full aspect-video cursor-pointer group bg-[#85c742]/10",
                    onclick: move |_| is_visible.set(true),
                    div { class: "absolute inset-0 flex flex-col items-center justify-center gap-4",
                        div { class: "w-20 h-20 bg-[#85c742] rounded-full flex items-center justify-center shadow-lg group-hover:scale-110 transition",
                            svg {
                                class: "w-10 h-10 text-white ml-1",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path { d: "M8 5v14l11-7z" }
                            }
                        }
                        div { class: "px-3 py-1.5 bg-[#85c742] rounded text-white text-sm font-bold",
                            "Rumble"
                        }
                    }
                }
            }
        }
    }
}
/// Renders a Tidal embed
#[component]
fn TidalRenderer(embed_url: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let final_embed_url = if embed_url.contains("embed.tidal.com") {
        embed_url.clone()
    } else if embed_url.contains("tidal.com/browse/track/") {
        let track_id = embed_url
            .split("/track/")
            .nth(1)
            .and_then(|s| s.split(&['?', '#', '/'][..]).next())
            .unwrap_or("");
        format!("https://embed.tidal.com/tracks/{}?layout=gridify", track_id)
    } else {
        embed_url.clone()
    };
    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            if *is_visible.read() {
                iframe {
                    src: "{final_embed_url}",
                    width: "100%",
                    height: "96",
                    frame_border: "0",
                    allow: "encrypted-media",
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#000000]/10 border border-[#000000]/30 dark:bg-white/10 dark:border-white/30 rounded-lg cursor-pointer hover:bg-[#000000]/20 dark:hover:bg-white/20 transition",
                    onclick: move |_| is_visible.set(true),
                    div { class: "w-12 h-12 bg-black dark:bg-white rounded-full flex items-center justify-center shrink-0",
                        svg {
                            class: "w-6 h-6 text-white dark:text-black",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path { d: "M12.012 3.992L8.008 7.996 4.004 3.992 0 7.996l4.004 4.004 4.004-4.004 4.004 4.004-4.004 4.004-4.004-4.004-4.004 4.004L0 20.008l4.004-4.004 4.004 4.004 4.004-4.004 4.004 4.004 4.004-4.004-4.004-4.004 4.004-4.004 4.004 4.004 4.004-4.004-4.004-4.004 4.004-4.004L20.02 0l-4.004 4.004L12.012 0 8.008 4.004l4.004 3.988z" }
                        }
                    }
                    div { class: "flex-1 min-w-0",
                        div { class: "font-medium text-sm", "Tidal" }
                        div { class: "text-xs text-muted-foreground", "Click to load player" }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL,
                    }
                }
            }
        }
    }
}
/// Renders a zap.stream live event card
#[component]
fn ZapStreamRenderer(naddr: String) -> Element {
    let mut event = use_signal(|| None::<Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        let naddr_clone = naddr.clone();
        spawn(async move {
            match Nip19::from_bech32(&naddr_clone) {
                Ok(Nip19::Coordinate(coord)) => {
                    let relay_hints: Vec<String> = coord
                        .relays
                        .iter()
                        .map(|r| r.to_string())
                        .collect();
                    match nostr_client::fetch_event_by_coordinate_with_relays(
                            coord.kind.as_u16(),
                            coord.public_key.to_hex(),
                            coord.identifier.clone(),
                            relay_hints,
                        )
                        .await
                    {
                        Ok(Some(e)) => {
                            event.set(Some(e));
                        }
                        Ok(None) => {
                            error.set(Some("Live event not found".to_string()));
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                    loading.set(false);
                }
                Ok(_) => {
                    error.set(Some("Invalid naddr format".to_string()));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(format!("Failed to parse naddr: {}", e)));
                    loading.set(false);
                }
            }
        });
    });
    rsx! {
        div { class: "my-2",
            if *loading.read() {
                div { class: "flex items-center gap-3 p-4 bg-purple-500/10 border border-purple-500/30 rounded-lg animate-pulse",
                    div { class: "w-12 h-12 bg-purple-500/30 rounded-full" }
                    div { class: "flex-1",
                        div { class: "h-4 bg-purple-500/30 rounded w-32 mb-2" }
                        div { class: "h-3 bg-purple-500/20 rounded w-24" }
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                div { class: "p-4 bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-200 rounded-lg text-sm",
                    "{err}"
                }
            } else if let Some(ev) = event.read().as_ref() {
                div { onclick: move |e: MouseEvent| e.stop_propagation(),
                    LiveStreamCard { event: ev.clone() }
                }
            }
        }
    }
}
/// Renders a zap.cooking recipe as a recipe minicard
#[component]
fn ZapCookingRecipeRenderer(naddr: String) -> Element {
    let mut event = use_signal(|| None::<Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let naddr_for_effect = naddr.clone();
    let naddr_for_render = naddr.clone();
    use_effect(move || {
        let naddr_clone = naddr_for_effect.clone();
        spawn(async move {
            match Nip19::from_bech32(&naddr_clone) {
                Ok(Nip19::Coordinate(coord)) => {
                    let relay_hints: Vec<String> = coord
                        .relays
                        .iter()
                        .map(|r| r.to_string())
                        .collect();
                    match nostr_client::fetch_event_by_coordinate_with_relays(
                            coord.kind.as_u16(),
                            coord.public_key.to_hex(),
                            coord.identifier.clone(),
                            relay_hints,
                        )
                        .await
                    {
                        Ok(Some(e)) => {
                            event.set(Some(e));
                        }
                        Ok(None) => {
                            error.set(Some("Recipe not found".to_string()));
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                    loading.set(false);
                }
                Ok(_) => {
                    error.set(Some("Invalid recipe address format".to_string()));
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(format!("Failed to parse recipe address: {}", e)));
                    loading.set(false);
                }
            }
        });
    });
    rsx! {
        div { class: "my-2",
            if *loading.read() {
                div { class: "flex items-center gap-2 p-2 border border-border rounded-lg animate-pulse",
                    div { class: "w-12 h-12 bg-muted rounded shrink-0" }
                    div { class: "flex-1",
                        div { class: "h-4 bg-muted rounded w-32 mb-2" }
                        div { class: "h-3 bg-muted rounded w-24" }
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                Link {
                    to: Route::RecipeDetail {
                        naddr: naddr_for_render.clone(),
                    },
                    class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "w-12 h-12 rounded bg-gradient-to-br from-orange-500/20 to-amber-500/20 flex items-center justify-center shrink-0",
                        span { class: "text-lg", "🍳" }
                    }
                    div { class: "flex-1 min-w-0",
                        p { class: "font-medium text-sm", "🍽️ View Recipe" }
                        p { class: "text-xs text-muted-foreground truncate", "{err}" }
                    }
                }
            } else if let Some(ev) = event.read().as_ref() {
                div { onclick: move |e: MouseEvent| e.stop_propagation(),
                    if is_recipe_event(ev) {
                        {
                            let recipe_meta = extract_recipe_metadata(ev);
                            render_recipe_minicard(&recipe_meta, &naddr_for_render, ev)
                        }
                    } else {
                        Link {
                            to: Route::RecipeDetail {
                                naddr: naddr_for_render.clone(),
                            },
                            class: "flex items-center gap-2 p-2 border border-border rounded-lg hover:bg-accent/50 transition",
                            div { class: "w-12 h-12 rounded bg-gradient-to-br from-orange-500/20 to-amber-500/20 flex items-center justify-center shrink-0",
                                span { class: "text-lg", "🍳" }
                            }
                            div { class: "flex-1 min-w-0",
                                p { class: "font-medium text-sm", "🍽️ View Recipe" }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render ISBN book reference with OpenLibrary cover
#[component]
fn IsbnRenderer(isbn: String) -> Element {
    use crate::services::openlibrary::CoverSize;
    let clean_isbn = crate::services::openlibrary::clean_isbn(&isbn);
    let cover_url = crate::services::openlibrary::get_cover_url(
        &clean_isbn,
        CoverSize::Small,
    );
    let openlibrary_url = format!("https://openlibrary.org/isbn/{}", clean_isbn);
    rsx! {
        a {
            href: "{openlibrary_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-2 px-2 py-1 bg-amber-100 dark:bg-amber-900/30 text-amber-800 dark:text-amber-200 rounded hover:bg-amber-200 dark:hover:bg-amber-800/40 transition text-sm",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            img {
                src: "{cover_url}",
                alt: "Book cover",
                class: "w-5 h-7 object-cover rounded-xs",
                onerror: move |_| {
                    log::debug!("Failed to load book cover for ISBN: {}", isbn);
                },
            }
            span { "ISBN: {clean_isbn}" }
        }
    }
}
/// Render DOI paper reference
#[component]
fn DoiRenderer(doi: String) -> Element {
    let doi_url = format!("https://doi.org/{}", doi);
    rsx! {
        a {
            href: "{doi_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2 py-1 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { class: "font-mono text-xs", "DOI" }
            span { "{doi}" }
        }
    }
}
/// Render ISAN movie reference
#[component]
fn IsanRenderer(isan: String) -> Element {
    let isan_url = format!("https://web.isan.org/public/en/search?isan={}", isan);
    rsx! {
        a {
            href: "{isan_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2 py-1 bg-purple-100 dark:bg-purple-900/30 text-purple-800 dark:text-purple-200 rounded hover:bg-purple-200 dark:hover:bg-purple-800/40 transition text-sm",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { class: "font-mono text-xs", "ISAN" }
            span { "{isan}" }
        }
    }
}
/// Render podcast feed GUID with playable card
#[component]
fn PodcastFeedRenderer(guid: String) -> Element {
    let guid_for_resource = guid.clone();
    let podcast_resource = use_resource(move || {
        let g = guid_for_resource.clone();
        async move { podcast_index::get_podcast_by_guid(&g).await }
    });
    match podcast_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-3",
                        div { class: "w-16 h-16 bg-muted rounded" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-4 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/2" }
                        }
                    }
                }
            }
        }
        Some(Err(_)) => {
            let podcast_index_url = format!("https://podcastindex.org/podcast/{}", guid);
            rsx! {
                a {
                    href: "{podcast_index_url}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    class: "inline-flex items-center gap-1.5 px-2 py-1 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-200 rounded hover:bg-green-200 dark:hover:bg-green-800/40 transition text-sm",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    span { "Podcast: " }
                    span { class: "font-mono text-xs truncate max-w-32", "{guid}" }
                }
            }
        }
        Some(Ok(podcast)) => {
            let image = podcast
                .get_image()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    format!(
                        "https://api.dicebear.com/7.x/shapes/svg?seed={}",
                        podcast.title,
                    )
                });
            let podcast_id = podcast.id;
            rsx! {
                Link {
                    to: Route::PodcastRssFeedDetail {
                        podcast_id: podcast_id.to_string(),
                    },
                    class: "my-2 border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition bg-card block",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-4 p-4",
                        div { class: "relative w-16 h-16 shrink-0 rounded overflow-hidden bg-muted",
                            img {
                                src: "{image}",
                                alt: "Podcast cover",
                                class: "w-full h-full object-cover",
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "font-semibold text-sm truncate", "{podcast.title}" }
                            if let Some(ref author) = podcast.author {
                                div { class: "text-xs text-muted-foreground truncate",
                                    "{author}"
                                }
                            }
                            if let Some(count) = podcast.episode_count {
                                div { class: "text-xs text-muted-foreground/80 mt-1",
                                    "{count} episodes"
                                }
                            }
                        }
                        div { class: "flex flex-col items-end gap-1 shrink-0",
                            div {
                                class: "flex items-center gap-1 text-xs text-green-500",
                                dangerous_inner_html: icons::PODCAST,
                                "Podcast"
                            }
                            if podcast.has_v4v() {
                                div { class: "text-xs text-amber-500", "V4V" }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render podcast episode GUID with playable card
#[component]
fn PodcastEpisodeRenderer(guid: String) -> Element {
    let guid_for_resource = guid.clone();
    let episode_resource = use_resource(move || {
        let g = guid_for_resource.clone();
        async move { podcast_index::get_episode_by_guid(&g, None).await }
    });
    match episode_resource.read_unchecked().as_ref() {
        None => {
            rsx! {
                div {
                    class: "my-2 p-4 border border-border rounded-lg bg-accent/5 animate-pulse",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-3",
                        div { class: "w-16 h-16 bg-muted rounded" }
                        div { class: "flex-1 space-y-2",
                            div { class: "h-4 bg-muted rounded w-3/4" }
                            div { class: "h-3 bg-muted rounded w-1/2" }
                        }
                    }
                }
            }
        }
        Some(Err(_)) => {
            let podcast_index_url = format!(
                "https://podcastindex.org/search?q={}",
                guid,
            );
            rsx! {
                a {
                    href: "{podcast_index_url}",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    class: "inline-flex items-center gap-1.5 px-2 py-1 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-200 rounded hover:bg-green-200 dark:hover:bg-green-800/40 transition text-sm",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    span { "Episode: " }
                    span { class: "font-mono text-xs truncate max-w-32", "{guid}" }
                }
            }
        }
        Some(Ok((episode, podcast))) => {
            let image = episode
                .get_image()
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    format!(
                        "https://api.dicebear.com/7.x/shapes/svg?seed={}",
                        episode.title,
                    )
                });
            let episode_clone = episode.clone();
            let podcast_clone = podcast.clone();
            let handle_play = move |e: MouseEvent| {
                e.stop_propagation();
                let ep = episode_clone.clone();
                let pod = podcast_clone.clone();
                let media_url = match &ep.enclosure_url {
                    Some(url) if !url.trim().is_empty() => url.clone(),
                    _ => {
                        log::warn!(
                            "Cannot play episode '{}': missing or empty enclosure URL",
                            ep.title
                        );
                        return;
                    }
                };
                let duration = ep
                    .duration
                    .map(|d| { if d > u32::MAX as u64 { u32::MAX } else { d as u32 } });
                let track = MusicTrack {
                    id: format!("pi-ep-{}", ep.id),
                    title: ep.title.clone(),
                    artist: ep
                        .feed_title
                        .clone()
                        .unwrap_or_else(|| {
                            pod.as_ref().map(|p| p.title.clone()).unwrap_or_default()
                        }),
                    artist_npub: None,
                    artist_id: None,
                    artist_art_url: None,
                    album: ep.feed_title.clone(),
                    album_id: ep.feed_id.map(|id| id.to_string()),
                    album_art_url: ep.get_image().map(|s| s.to_string()),
                    duration,
                    media_url,
                    source: TrackSource::RssPodcast {
                        feed_url: ep.feed_url.clone().unwrap_or_default(),
                        podcast_id: ep.feed_id,
                        episode_guid: guid.clone(),
                        podcast_title: ep.feed_title.clone().unwrap_or_default(),
                    },
                    msat_total: None,
                    created_at: None,
                    is_podcast: true,
                    is_live_stream: false,
                    value_block: None,
                    chapters_url: ep.chapters_url.clone(),
                    transcripts: Vec::new(),
                };
                music_player::play_track(track, None, None);
            };
            let has_v4v = episode.value.is_some();
            let duration_str = episode
                .duration
                .map(|d| {
                    let mins = d / 60;
                    let secs = d % 60;
                    format!("{:02}:{:02}", mins, secs)
                });
            let safe_desc = episode.description.as_ref().map(|d| sanitize_html(d));
            rsx! {
                div {
                    class: "my-2 border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition bg-card",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { class: "flex items-center gap-4 p-4",
                        div { class: "relative w-16 h-16 shrink-0 rounded overflow-hidden bg-muted group",
                            img {
                                src: "{image}",
                                alt: "Episode art",
                                class: "w-full h-full object-cover",
                            }
                            button {
                                class: "absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition",
                                onclick: handle_play,
                                dangerous_inner_html: icons::PLAY,
                            }
                        }
                        div { class: "flex-1 min-w-0",
                            div { class: "font-semibold text-sm truncate", "{episode.title}" }
                            if let Some(ref feed_title) = episode.feed_title {
                                div { class: "text-xs text-muted-foreground truncate",
                                    "{feed_title}"
                                }
                            }
                            if let Some(ref desc) = safe_desc {
                                div {
                                    class: "text-xs text-muted-foreground/80 truncate mt-1",
                                    dangerous_inner_html: "{desc}",
                                }
                            }
                        }
                        div { class: "flex flex-col items-end gap-1 shrink-0",
                            if let Some(ref dur) = duration_str {
                                div { class: "text-xs text-muted-foreground", "{dur}" }
                            }
                            div {
                                class: "flex items-center gap-1 text-xs text-green-500",
                                dangerous_inner_html: icons::PODCAST,
                                "Episode"
                            }
                            if has_v4v {
                                div { class: "text-xs text-amber-500", "V4V" }
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Render Bitcoin transaction reference
#[component]
fn BitcoinTxRenderer(txid: String) -> Element {
    let mempool_endpoint = crate::stores::settings_store::get_mempool_endpoint();
    let base_url = mempool_endpoint.trim_end_matches("/api").trim_end_matches('/');
    let tx_url = format!("{}/tx/{}", base_url, txid);
    let truncated = crate::services::mempool::truncate_bitcoin_id(&txid);
    rsx! {
        a {
            href: "{tx_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2 py-1 bg-orange-100 dark:bg-orange-900/30 text-orange-800 dark:text-orange-200 rounded hover:bg-orange-200 dark:hover:bg-orange-800/40 transition text-sm font-mono",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { "TX: {truncated}" }
        }
    }
}
/// Render Bitcoin address reference
#[component]
fn BitcoinAddressRenderer(address: String) -> Element {
    let mempool_endpoint = crate::stores::settings_store::get_mempool_endpoint();
    let base_url = mempool_endpoint.trim_end_matches("/api").trim_end_matches('/');
    let addr_url = format!("{}/address/{}", base_url, address);
    let truncated = crate::services::mempool::truncate_bitcoin_id(&address);
    rsx! {
        a {
            href: "{addr_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2 py-1 bg-orange-100 dark:bg-orange-900/30 text-orange-800 dark:text-orange-200 rounded hover:bg-orange-200 dark:hover:bg-orange-800/40 transition text-sm font-mono",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { "Addr: {truncated}" }
        }
    }
}
/// Render geohash location reference
#[component]
fn GeohashRenderer(hash: String) -> Element {
    let geohash_url = format!("https://geohash.org/{}", hash);
    rsx! {
        a {
            href: "{geohash_url}",
            target: "_blank",
            rel: "noopener noreferrer",
            class: "inline-flex items-center gap-1.5 px-2 py-1 bg-teal-100 dark:bg-teal-900/30 text-teal-800 dark:text-teal-200 rounded hover:bg-teal-200 dark:hover:bg-teal-800/40 transition text-sm",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            span { "Location: {hash}" }
        }
    }
}
/// Generic loading skeleton for nostr.blue content cards
fn nostr_blue_loading_skeleton() -> Element {
    rsx! {
        div { class: "flex items-center gap-3 p-4 bg-blue-500/10 border border-blue-500/30 rounded-lg animate-pulse",
            div { class: "w-12 h-12 bg-blue-500/30 rounded-lg shrink-0" }
            div { class: "flex-1 min-w-0",
                div { class: "h-4 bg-blue-500/30 rounded w-3/4 mb-2" }
                div { class: "h-3 bg-blue-500/20 rounded w-1/2" }
            }
        }
    }
}
/// Generic error display for nostr.blue content
fn nostr_blue_error(message: &str) -> Element {
    rsx! {
        div { class: "p-4 bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-200 rounded-lg text-sm",
            "{message}"
        }
    }
}
/// Renders a nostr.blue livestream link as a LiveStreamCard
#[component]
fn NostrBlueLiveStreamRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Livestream not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                LiveStreamCard { event: ev.clone() }
            }
        }
    }
}
/// Renders a nostr.blue video link
#[component]
fn NostrBlueVideoRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_id(id, &[21, 22], "Video not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                VideoCard { event: ev.clone() }
            }
        }
    }
}
/// Renders a nostr.blue photo link
#[component]
fn NostrBluePhotoRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_id(id, &[20], "Photo not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                PhotoCard { event: ev.clone() }
            }
        }
    }
}
/// Renders a nostr.blue voice message link
#[component]
fn NostrBlueVoiceRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_id(id, &[1040], "Voice message not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                VoiceMessageCard { event: ev.clone() }
            }
        }
    }
}
/// Renders a nostr.blue podcast show link
#[component]
fn NostrBluePodcastShowRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Podcast not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_podcast_show_card(ev, &id_for_link)}
            }
        }
    }
}
fn render_podcast_show_card(event: &Event, naddr: &str) -> Element {
    match parse_podcast_metadata(event) {
        Ok(metadata) => {
            let show = PodcastShow::from_nostr_metadata(&metadata);
            rsx! {
                PodcastShowCard { show, compact: true }
            }
        }
        Err(_) => {
            rsx! {
                Link {
                    to: Route::PodcastNostrDetail {
                        naddr: naddr.to_string(),
                    },
                    class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                    icons::MusicIcon { class: "w-4 h-4" }
                    "View Podcast"
                }
            }
        }
    }
}
/// Renders a nostr.blue podcast episode link
#[component]
fn NostrBluePodcastEpisodeRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Episode not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_podcast_episode_card(ev, &id_for_link)}
            }
        }
    }
}
fn render_podcast_episode_card(event: &Event, naddr: &str) -> Element {
    match parse_podcast_episode(event) {
        Ok(episode) => {
            let display_episode = DisplayEpisode::from_nostr_episode(
                &episode,
                "Podcast Episode",
                None,
            );
            rsx! {
                PodcastEpisodeCard { episode: display_episode, show_description: false }
            }
        }
        Err(_) => {
            rsx! {
                Link {
                    to: Route::PodcastNostrEpisodeDetail {
                        naddr: naddr.to_string(),
                    },
                    class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                    icons::MusicIcon { class: "w-4 h-4" }
                    "View Episode"
                }
            }
        }
    }
}
/// Renders a nostr.blue RSS podcast episode link with playback
#[component]
fn NostrBlueRssPodcastEpisodeRenderer(
    podcast_id: String,
    episode_id: String,
) -> Element {
    let podcast_id_for_link = podcast_id.clone();
    let episode_id_for_link = episode_id.clone();
    let resource: Resource<Result<DisplayEpisode, String>> = use_resource(move || {
        let podcast_id = podcast_id.clone();
        let episode_id = episode_id.clone();
        async move {
            let decoded_episode_id = urlencoding::decode(&episode_id)
                .map(|s| s.into_owned())
                .unwrap_or(episode_id);
            let feed_id = podcast_id
                .parse::<u64>()
                .map_err(|_| "Invalid podcast ID format".to_string())?;
            let create_minimal_feed = |
                ep: &podcast_index::Episode,
                feed_id: u64,
            | -> podcast_index::PodcastFeed {
                podcast_index::PodcastFeed {
                    id: feed_id,
                    title: ep.feed_title.clone().unwrap_or_default(),
                    url: ep.feed_url.clone().unwrap_or_default(),
                    original_url: None,
                    link: None,
                    description: None,
                    author: None,
                    owner_name: None,
                    image: ep.feed_image.clone(),
                    artwork: None,
                    language: None,
                    itunes_id: None,
                    podcast_guid: ep.podcast_guid.clone(),
                    categories: None,
                    episode_count: None,
                    trending_score: None,
                    value: None,
                }
            };
            if let Ok(ep_id) = decoded_episode_id.parse::<u64>() {
                if let Ok(ep) = podcast_index::get_episode_by_id(ep_id).await {
                    let feed = podcast_index::get_podcast_by_id(feed_id)
                        .await
                        .unwrap_or_else(|e| {
                            log::warn!("Feed fetch failed but episode found: {}", e);
                            create_minimal_feed(&ep, feed_id)
                        });
                    return Ok(DisplayEpisode::from_podcast_index_episode(&ep, &feed));
                }
                log::debug!("Direct episode fetch failed, falling back to search");
            } else if let Ok((ep, feed_opt)) = podcast_index::get_episode_by_guid(
                    &decoded_episode_id,
                    None,
                )
                .await
            {
                if ep.feed_id == Some(feed_id) {
                    let feed = match feed_opt {
                        Some(f) => f,
                        None => {
                            podcast_index::get_podcast_by_id(feed_id)
                                .await
                                .unwrap_or_else(|_| create_minimal_feed(&ep, feed_id))
                        }
                    };
                    return Ok(
                        DisplayEpisode::from_podcast_index_episode(&ep, &feed),
                    );
                } else {
                    log::debug!(
                        "GUID lookup returned episode from different feed, falling back to search"
                    );
                }
            } else {
                log::debug!(
                    "GUID-based episode fetch failed, falling back to search"
                );
            }
            let feed = podcast_index::get_podcast_by_id(feed_id)
                .await
                .map_err(|e| format!("Failed to fetch podcast: {}", e))?;
            const MAX_PAGES: u32 = 5;
            const PAGE_SIZE: u32 = 100;
            for page in 0..MAX_PAGES {
                let fetch_count = PAGE_SIZE * (page + 1);
                let episodes = podcast_index::get_episodes_by_feed_id(
                        feed_id,
                        Some(fetch_count),
                    )
                    .await
                    .map_err(|e| format!("Failed to fetch episodes: {}", e))?;
                let start_idx = if page == 0 { 0 } else { (PAGE_SIZE * page) as usize };
                let episodes_to_check = if start_idx < episodes.len() {
                    &episodes[start_idx..]
                } else {
                    break;
                };
                if let Some(ep) = episodes_to_check
                    .iter()
                    .find(|e| e.id.to_string() == decoded_episode_id)
                {
                    return Ok(DisplayEpisode::from_podcast_index_episode(ep, &feed));
                }
                if episodes.len() < fetch_count as usize {
                    break;
                }
            }
            Err(
                format!(
                    "Episode not found (searched {} episodes)",
                    MAX_PAGES * PAGE_SIZE,
                ),
            )
        }
    });
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            match resource.read_unchecked().as_ref() {
                None => nostr_blue_loading_skeleton(),
                Some(Err(err)) => rsx! {
                    div { class: "p-3 border border-border rounded-lg bg-card",
                        p { class: "text-sm text-muted-foreground mb-2", "{err}" }
                        Link {
                            to: Route::PodcastRssEpisodeDetail {
                                podcast_id: podcast_id_for_link.clone(),
                                episode_id: episode_id_for_link.clone(),
                            },
                            class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                            icons::MusicIcon { class: "w-4 h-4" }
                            "View Episode"
                        }
                    }
                },
                Some(Ok(display)) => rsx! {
                    PodcastEpisodeCard { episode: display.clone(), show_description: false }
                },
            }
        }
    }
}
/// Renders a nostr.blue RSS podcast show link
#[component]
fn NostrBlueRssPodcastShowRenderer(podcast_id: String) -> Element {
    let podcast_id_for_link = podcast_id.clone();
    let resource: Resource<Result<PodcastShow, String>> = use_resource(move || {
        let podcast_id = podcast_id.clone();
        async move {
            let feed_id = podcast_id
                .parse::<u64>()
                .map_err(|_| "Invalid podcast ID format".to_string())?;
            let feed = podcast_index::get_podcast_by_id(feed_id)
                .await
                .map_err(|e| e.to_string())?;
            let value = feed
                .value
                .as_ref()
                .and_then(|v| {
                    let model = v.model.as_ref()?;
                    Some(crate::utils::podcast::ValueBlock {
                        value_type: model
                            .model_type
                            .clone()
                            .unwrap_or_else(|| "lightning".to_string()),
                        method: model
                            .method
                            .clone()
                            .unwrap_or_else(|| "keysend".to_string()),
                        suggested: model.suggested.as_ref().and_then(|s| s.parse().ok()),
                        recipients: v
                            .destinations
                            .iter()
                            .filter_map(|d| {
                                Some(crate::utils::podcast::ValueRecipient {
                                    name: d.name.clone(),
                                    custom_key: None,
                                    custom_value: None,
                                    recipient_type: d
                                        .dest_type
                                        .clone()
                                        .unwrap_or_else(|| "node".to_string()),
                                    address: d.address.clone()?,
                                    split: d.split.unwrap_or(0),
                                    fee: None,
                                })
                            })
                            .collect(),
                    })
                });
            Ok(PodcastShow {
                id: feed.id.to_string(),
                title: feed.title.clone(),
                author: feed.author.clone().or(feed.owner_name.clone()),
                description: feed.description.clone(),
                image: feed.get_image().map(|s| s.to_string()),
                episode_count: feed.episode_count.map(|c| c as usize),
                source: crate::utils::podcast::PodcastSource::Rss {
                    feed_url: feed.url.clone(),
                    guid: feed.podcast_guid.clone().unwrap_or_default(),
                    podcast_id: Some(feed.id),
                },
                value,
                categories: feed
                    .categories
                    .as_ref()
                    .map(|c| c.values().cloned().collect())
                    .unwrap_or_default(),
                explicit: false,
            })
        }
    });
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            match resource.read_unchecked().as_ref() {
                None => nostr_blue_loading_skeleton(),
                Some(Err(_)) => rsx! {
                    Link {
                        to: Route::PodcastRssFeedDetail {
                            podcast_id: podcast_id_for_link.clone(),
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        icons::MusicIcon { class: "w-4 h-4" }
                        "View Podcast"
                    }
                },
                Some(Ok(show)) => rsx! {
                    PodcastShowCard { show: show.clone(), compact: true }
                },
            }
        }
    }
}
/// Renders a nostr.blue music playlist link
#[component]
fn NostrBlueMusicPlaylistRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Playlist not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                if let Ok(playlist) = parse_playlist_event(ev) {
                    {render_playlist_minicard(&playlist, &id_for_link)}
                } else {
                    Link {
                        to: Route::MusicPlaylistDetail {
                            naddr: id_for_link.clone(),
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        icons::MusicIcon { class: "w-4 h-4" }
                        "View Playlist"
                    }
                }
            }
        }
    }
}
/// Renders a nostr.blue radio station link
#[component]
fn NostrBlueRadioStationRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(
        id,
        "Radio station not found",
    );
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_radio_station_card(ev, &id_for_link)}
            }
        }
    }
}
fn render_radio_station_card(event: &Event, naddr: &str) -> Element {
    match RadioStation::from_event(event) {
        Ok(station) => {
            rsx! {
                RadioCard { station }
            }
        }
        Err(_) => {
            rsx! {
                Link {
                    to: Route::RadioStation {
                        naddr: naddr.to_string(),
                    },
                    class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                    icons::RssIcon { class: "w-4 h-4" }
                    "View Radio Station"
                }
            }
        }
    }
}
/// Renders a nostr.blue article link
#[component]
fn NostrBlueArticleRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Article not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                ArticleCard { event: ev.clone() }
            }
        }
    }
}
/// Renders a nostr.blue recipe link
#[component]
fn NostrBlueRecipeRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Recipe not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_recipe_from_event(ev, &id_for_link)}
            }
        }
    }
}
fn render_recipe_from_event(event: &Event, naddr: &str) -> Element {
    use crate::stores::recipe_store::CachedRecipe;
    let metadata = extract_recipe_metadata(event);
    let identifier = metadata
        .identifier
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            log::debug!(
                "Recipe event {} has empty identifier, using event ID as fallback", event
                .id
            );
            event.id.to_hex()
        });
    let a_tag = format!("30023:{}:{}", event.pubkey.to_hex(), identifier);
    let cached = CachedRecipe {
        event: event.clone(),
        metadata,
        parsed: None,
        naddr: naddr.to_string(),
        a_tag,
    };
    rsx! {
        RecipeCard { recipe: cached }
    }
}
/// Renders a nostr.blue note link
#[component]
fn NostrBlueNoteRenderer(id: String) -> Element {
    let fetch = use_fetch_event_by_id(id, &[1, 6, 16], "Note not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_note_minicard(ev, &ev.id.to_hex())}
            }
        }
    }
}
fn render_note_minicard(event: &Event, note_id: &str) -> Element {
    let content_preview = {
        let char_count = event.content.chars().count();
        if char_count > 200 {
            let truncated: String = event.content.chars().take(200).collect();
            format!("{}...", truncated)
        } else {
            event.content.clone()
        }
    };
    rsx! {
        Link {
            to: Route::Note {
                note_id: note_id.to_string(),
                from_voice: None,
            },
            class: "block p-3 border border-border rounded-lg hover:bg-accent/50 transition",
            div { class: "text-sm text-foreground line-clamp-3 whitespace-pre-wrap",
                "{content_preview}"
            }
        }
    }
}
/// Renders a nostr.blue profile link
#[component]
fn NostrBlueProfileRenderer(id: String) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut loading = use_signal(|| true);
    let mut valid_pubkey_hex = use_signal(|| None::<String>);
    use_effect(move || {
        let id_clone = id.clone();
        spawn(async move {
            if let Ok(pubkey) = PublicKey::parse(&id_clone) {
                let hex = pubkey.to_hex();
                valid_pubkey_hex.set(Some(hex.clone()));
                if let Ok(fetched) = profiles::fetch_profile(hex).await {
                    profile.set(Some(fetched));
                }
            }
            loading.set(false);
        });
    });
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if *loading.read() {
                {nostr_blue_loading_skeleton()}
            } else {
                {
                    render_profile_minicard(
                        profile.read().as_ref(),
                        valid_pubkey_hex.read().as_deref(),
                    )
                }
            }
        }
    }
}
fn render_profile_minicard(
    profile: Option<&profiles::Profile>,
    valid_pubkey: Option<&str>,
) -> Element {
    let display_name = profile
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| "Unknown".to_string());
    let picture = profile.and_then(|p| p.picture.clone());
    let about = profile.and_then(|p| p.about.clone());
    let avatar_initial = display_name.chars().next().unwrap_or('?');
    if let Some(pubkey) = valid_pubkey {
        rsx! {
            Link {
                to: Route::Profile {
                    pubkey: pubkey.to_string(),
                },
                class: "flex items-center gap-3 p-3 border border-border rounded-lg hover:bg-accent/50 transition",
                if let Some(ref pic) = picture {
                    img {
                        src: "{pic}",
                        class: "w-12 h-12 rounded-full object-cover shrink-0",
                    }
                } else {
                    div { class: "w-12 h-12 rounded-full bg-muted flex items-center justify-center shrink-0 text-lg font-medium",
                        "{avatar_initial}"
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-medium text-foreground truncate", "{display_name}" }
                    if let Some(ref bio) = about {
                        div { class: "text-sm text-muted-foreground line-clamp-1", "{bio}" }
                    }
                }
            }
        }
    } else {
        rsx! {
            div { class: "flex items-center gap-3 p-3 border border-border rounded-lg bg-muted/50",
                if let Some(ref pic) = picture {
                    img {
                        src: "{pic}",
                        class: "w-12 h-12 rounded-full object-cover shrink-0",
                    }
                } else {
                    div { class: "w-12 h-12 rounded-full bg-muted flex items-center justify-center shrink-0 text-lg font-medium",
                        "{avatar_initial}"
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-medium text-foreground truncate", "{display_name}" }
                    if let Some(ref bio) = about {
                        div { class: "text-sm text-muted-foreground line-clamp-1", "{bio}" }
                    }
                }
            }
        }
    }
}
/// Renders a nostr.blue calendar event link
#[component]
fn NostrBlueCalendarEventRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Event not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                if let Ok(cal_event) = parse_calendar_event(ev) {
                    EventCardCompact { event: UnifiedEvent::Calendar(cal_event), from: None }
                } else {
                    Link {
                        to: Route::CalendarEventDetail {
                            naddr: id_for_link.clone(),
                            from: None,
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        "View Event"
                    }
                }
            }
        }
    }
}
/// Renders a nostr.blue wiki link - dispatches to appropriate renderer based on id type
#[component]
fn NostrBlueWikiRenderer(id: String) -> Element {
    if id.starts_with("naddr1") {
        rsx! {
            NostrBlueWikiNaddrRenderer { id }
        }
    } else {
        rsx! {
            NostrBlueWikiTopicRenderer { id }
        }
    }
}
/// Renders wiki topic links (simple d-tag identifier) - no hooks needed
#[component]
fn NostrBlueWikiTopicRenderer(id: String) -> Element {
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::WikiDetail {
                    identifier: id.clone(),
                },
                class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                "Wiki: {id}"
            }
        }
    }
}
/// Renders wiki naddr links with event fetching
#[component]
fn NostrBlueWikiNaddrRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Wiki page not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_wiki_card(ev, &id_for_link)}
            }
        }
    }
}
fn render_wiki_card(event: &Event, identifier: &str) -> Element {
    use crate::stores::wiki_store::CachedWikiPage;
    if let Ok(article) = parse_wiki_article(event) {
        let coord = Coordinate::new(Kind::from(30818), event.pubkey)
            .identifier(&article.identifier);
        let naddr = coord.to_bech32().unwrap_or_else(|_| identifier.to_string());
        let a_tag = format!("30818:{}:{}", event.pubkey.to_hex(), article.identifier);
        let cached = CachedWikiPage {
            event: event.clone(),
            article: article.clone(),
            naddr,
            a_tag,
            forward_links: article.forward_links.clone(),
            backward_links: vec![],
            mime_type: None,
        };
        rsx! {
            WikiCardCompact { page: cached }
        }
    } else {
        rsx! {
            Link {
                to: Route::WikiDetail {
                    identifier: identifier.to_string(),
                },
                class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                "View Wiki Page"
            }
        }
    }
}
/// Renders a nostr.blue publication link
#[component]
fn NostrBluePublicationRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Publication not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                if let Some(pub_index) = parse_publication_index(ev) {
                    PublicationCardCompact { publication: pub_index }
                } else {
                    Link {
                        to: Route::PublicationDetail {
                            naddr: id_for_link.clone(),
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        "View Publication"
                    }
                }
            }
        }
    }
}
/// Renders a nostr.blue pinboard link
#[component]
fn NostrBluePinboardRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Pinboard not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                if let Some(pinboard) = parse_pinboard_event(ev, None) {
                    PinBoardCardCompact { board: pinboard }
                } else {
                    Link {
                        to: Route::PinBoardDetail {
                            naddr: id_for_link.clone(),
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        "View Pinboard"
                    }
                }
            }
        }
    }
}
/// Renders a nostr.blue badge link
#[component]
fn NostrBlueBadgeRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Badge not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_badge_card(ev, &id_for_link)}
            }
        }
    }
}
fn render_badge_card(event: &Event, naddr: &str) -> Element {
    if let Ok(badge) = parse_badge_definition(event) {
        let name = badge.name.clone().unwrap_or_else(|| "Badge".to_string());
        let desc = badge.description.clone();
        let image = badge.image.clone();
        let thumb = badge.thumb.clone();
        rsx! {
            Link {
                to: Route::BadgeDetail {
                    naddr: naddr.to_string(),
                },
                class: "flex items-center gap-3 p-3 border border-border rounded-lg hover:bg-accent/50 transition",
                if let Some(img_url) = image {
                    img {
                        src: "{img_url}",
                        class: "w-12 h-12 rounded-lg object-cover shrink-0",
                    }
                } else if let Some(thumb_url) = thumb {
                    img {
                        src: "{thumb_url}",
                        class: "w-12 h-12 rounded-lg object-cover shrink-0",
                    }
                } else {
                    div { class: "w-12 h-12 rounded-lg bg-muted flex items-center justify-center shrink-0",
                        "🏆"
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-medium text-foreground truncate", "{name}" }
                    if let Some(description) = desc {
                        div { class: "text-sm text-muted-foreground line-clamp-1", "{description}" }
                    }
                }
            }
        }
    } else {
        rsx! {
            Link {
                to: Route::BadgeDetail {
                    naddr: naddr.to_string(),
                },
                class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                "View Badge"
            }
        }
    }
}
/// Renders a nostr.blue product link
#[component]
fn NostrBlueProductRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Product not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                {render_product_card(ev, &id_for_link)}
            }
        }
    }
}
fn render_product_card(event: &Event, naddr: &str) -> Element {
    if let Ok(product) = parse_product(event) {
        let title = product.title.clone();
        let image_url = product.images.first().map(|img| img.url.clone());
        let price_display = format!(
            "{} {}",
            product.price.amount,
            product.price.currency,
        );
        rsx! {
            Link {
                to: Route::ShopProductDetail {
                    naddr: naddr.to_string(),
                },
                class: "flex items-center gap-3 p-3 border border-border rounded-lg hover:bg-accent/50 transition",
                if let Some(img_url) = image_url {
                    img {
                        src: "{img_url}",
                        class: "w-16 h-16 rounded-lg object-cover shrink-0",
                    }
                } else {
                    div { class: "w-16 h-16 rounded-lg bg-muted flex items-center justify-center shrink-0",
                        "🛍️"
                    }
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-medium text-foreground truncate", "{title}" }
                    div { class: "text-sm font-medium text-green-600 dark:text-green-400",
                        "{price_display}"
                    }
                }
            }
        }
    } else {
        rsx! {
            Link {
                to: Route::ShopProductDetail {
                    naddr: naddr.to_string(),
                },
                class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                "View Product"
            }
        }
    }
}
/// Renders a nostr.blue code repo link
#[component]
fn NostrBlueCodeRepoRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let fetch = use_fetch_event_by_coordinate_with_message(id, "Repository not found");
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if fetch.is_loading() {
                {nostr_blue_loading_skeleton()}
            } else if let Some(err) = fetch.error().as_ref() {
                {nostr_blue_error(err)}
            } else if let Some(ev) = fetch.event().as_ref() {
                if let Some(repo) = Repository::from_event(ev) {
                    CodeRepoCardCompact { repo }
                } else {
                    Link {
                        to: Route::CodeRepo {
                            naddr: id_for_link.clone(),
                        },
                        class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                        "View Repository"
                    }
                }
            }
        }
    }
}
/// Renders a nostr.blue community link
#[component]
fn NostrBlueCommunityRenderer(id: String) -> Element {
    let id_for_link = id.clone();
    let parts: Vec<&str> = id.splitn(3, ':').collect();
    let is_valid = parts.len() == 3 && parts[0].parse::<u32>().is_ok()
        && PublicKey::from_hex(parts[1]).is_ok() && !parts[2].is_empty();
    rsx! {
        div { class: "my-2", onclick: move |e: MouseEvent| e.stop_propagation(),
            if is_valid {
                Link {
                    to: Route::CommunityPage {
                        a_tag: id_for_link.clone(),
                    },
                    class: "inline-flex items-center gap-2 px-3 py-2 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-200 rounded-lg hover:bg-blue-200 dark:hover:bg-blue-800/40 transition text-sm",
                    icons::UsersIcon { class: "w-4 h-4" }
                    "View Community"
                }
            } else {
                span { class: "inline-flex items-center gap-2 px-3 py-2 bg-muted text-muted-foreground rounded-lg text-sm",
                    icons::UsersIcon { class: "w-4 h-4" }
                    "Invalid Community"
                }
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
