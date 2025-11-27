use dioxus::prelude::*;
use crate::utils::content_parser::{parse_content, ContentToken, extract_youtube_id};
use crate::routes::Route;
use nostr_sdk::{Tag, FromBech32, Metadata, PublicKey, Filter, Kind, Event, EventId};
use nostr_sdk::nips::nip19::Nip19;
use crate::stores::nostr_client;
use crate::services::wavlake::WavlakeAPI;
use crate::stores::music_player::{self, MusicTrack};
use crate::components::icons;
use crate::components::{PhotoCard, VideoCard, VoiceMessageCard, PollCard};
use crate::components::live_stream_card::LiveStreamCard;

#[component]
pub fn RichContent(
    content: String,
    tags: Vec<Tag>,
    #[props(default = false)] collapsible: bool,
) -> Element {
    let tokens = parse_content(&content, &tags);
    let mut is_expanded = use_signal(|| false);

    // Estimate if content is long enough to need collapsing
    // Count characters and media items to estimate content height
    let is_long_content = if collapsible {
        let char_count = content.chars().count();
        let media_count = tokens.iter().filter(|t| {
            matches!(t, ContentToken::Image(_) | ContentToken::Video(_) |
                     ContentToken::WavlakeTrack(_) | ContentToken::WavlakeAlbum(_) |
                     ContentToken::TwitterTweet(_) | ContentToken::TwitchStream(_) |
                     ContentToken::TwitchClip(_) | ContentToken::TwitchVod(_) |
                     ContentToken::EventMention(_))
        }).count();

        // Heuristic: >800 chars (roughly 16 lines at ~50 chars/line)
        // OR has media AND enough text that it would overflow with media (~200 chars + media)
        char_count > 800 || (media_count > 0 && char_count > 200)
    } else {
        false
    };

    if collapsible && is_long_content {
        rsx! {
            div {
                class: "relative",
                div {
                    class: if *is_expanded.read() {
                        "whitespace-pre-wrap break-words space-y-2"
                    } else {
                        "whitespace-pre-wrap break-words space-y-2 max-h-[24em] overflow-hidden"
                    },
                    for token in tokens.iter() {
                        {render_token(token)}
                    }
                }
                // Show More button - only visible when collapsed
                if !*is_expanded.read() {
                    div {
                        class: "absolute bottom-0 left-0 right-0 h-12 bg-gradient-to-t from-background via-background/95 to-transparent flex items-end justify-center pb-1",
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
            div {
                class: "whitespace-pre-wrap break-words space-y-2",
                for token in tokens.iter() {
                    {render_token(token)}
                }
            }
        }
    }
}

fn render_token(token: &ContentToken) -> Element {
    match token {
        ContentToken::Text(text) => rsx! {
            span { "{text}" }
        },

        ContentToken::Link(url) => rsx! {
            a {
                href: "{url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 underline",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                "{url}"
            }
        },

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
                        }
                    }
                }
            }
        },

        ContentToken::Video(url) => rsx! {
            div {
                class: "my-2 rounded-lg overflow-hidden border border-border",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                if url.contains("youtube.com") || url.contains("youtu.be") {
                    // YouTube embed
                    {render_youtube_embed(url)}
                } else {
                    // Regular video
                    video {
                        src: "{url}",
                        controls: true,
                        class: "max-w-full h-auto",
                        "Your browser does not support the video tag."
                    }
                }
            }
        },

        ContentToken::Mention(mention) => rsx! {
            MentionRenderer { mention: mention.clone() }
        },

        ContentToken::EventMention(mention) => rsx! {
            EventMentionRenderer { mention: mention.clone() }
        },

        ContentToken::Hashtag(tag) => {
            rsx! {
                Link {
                    to: Route::Hashtag { tag: tag.clone() },
                    class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 font-medium hover:underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    "#{tag}"
                }
            }
        },

        ContentToken::WavlakeTrack(track_id) => rsx! {
            WavlakeTrackRenderer { track_id: track_id.clone() }
        },

        ContentToken::WavlakeAlbum(album_id) => rsx! {
            WavlakeAlbumRenderer { album_id: album_id.clone() }
        },

        ContentToken::WavlakeArtist(artist_id) => rsx! {
            WavlakeArtistRenderer { artist_id: artist_id.clone() }
        },

        ContentToken::WavlakePlaylist(playlist_id) => rsx! {
            WavlakePlaylistRenderer { playlist_id: playlist_id.clone() }
        },

        ContentToken::TwitterTweet(tweet_id) => rsx! {
            TwitterTweetRenderer { tweet_id: tweet_id.clone() }
        },

        ContentToken::TwitchStream(channel) => rsx! {
            TwitchStreamRenderer { channel: channel.clone() }
        },

        ContentToken::TwitchClip(clip_slug) => rsx! {
            TwitchClipRenderer { clip_slug: clip_slug.clone() }
        },

        ContentToken::TwitchVod(vod_id) => rsx! {
            TwitchVodRenderer { vod_id: vod_id.clone() }
        },

        // YouTube iframe embed
        ContentToken::YouTube(video_id) => rsx! {
            YouTubeRenderer { video_id: video_id.clone() }
        },

        // Spotify embeds
        ContentToken::SpotifyTrack(track_id) => rsx! {
            SpotifyRenderer { content_type: "track".to_string(), content_id: track_id.clone() }
        },
        ContentToken::SpotifyAlbum(album_id) => rsx! {
            SpotifyRenderer { content_type: "album".to_string(), content_id: album_id.clone() }
        },
        ContentToken::SpotifyPlaylist(playlist_id) => rsx! {
            SpotifyRenderer { content_type: "playlist".to_string(), content_id: playlist_id.clone() }
        },
        ContentToken::SpotifyEpisode(episode_id) => rsx! {
            SpotifyRenderer { content_type: "episode".to_string(), content_id: episode_id.clone() }
        },

        // SoundCloud embed
        ContentToken::SoundCloud(url) => rsx! {
            SoundCloudRenderer { url: url.clone() }
        },

        // Apple Music embeds
        ContentToken::AppleMusicAlbum(url) | ContentToken::AppleMusicPlaylist(url) => rsx! {
            AppleMusicRenderer { embed_url: url.clone(), is_song: false }
        },
        ContentToken::AppleMusicSong(url) => rsx! {
            AppleMusicRenderer { embed_url: url.clone(), is_song: true }
        },

        // MixCloud embed
        ContentToken::MixCloud(username, mix_name) => rsx! {
            MixCloudRenderer { username: username.clone(), mix_name: mix_name.clone() }
        },

        // Rumble embed
        ContentToken::Rumble(embed_url) => rsx! {
            RumbleRenderer { embed_url: embed_url.clone() }
        },

        // Tidal embed
        ContentToken::Tidal(embed_url) => rsx! {
            TidalRenderer { embed_url: embed_url.clone() }
        },

        // Zap.stream - Nostr live streaming
        ContentToken::ZapStream(naddr) => rsx! {
            ZapStreamRenderer { naddr: naddr.clone() }
        },
    }
}

#[component]
fn MentionRenderer(mention: String) -> Element {
    // Extract the identifier from "nostr:npub..." or just "npub..."
    let identifier = mention.strip_prefix("nostr:").unwrap_or(&mention);

    // Parse pubkey from either nprofile or npub
    let pubkey_result: Option<PublicKey> = if identifier.starts_with("nprofile1") {
        nostr_sdk::nips::nip19::Nip19Profile::from_bech32(identifier)
            .ok()
            .map(|nip19| nip19.public_key)
    } else {
        nostr_sdk::PublicKey::from_bech32(identifier).ok()
    };

    // Always call hooks unconditionally
    let mut metadata = use_signal(|| None::<Metadata>);

    // Fetch profile metadata
    use_effect(move || {
        if let Some(pubkey) = pubkey_result {
            spawn(async move {
                let metadata_filter = Filter::new()
                    .author(pubkey)
                    .kind(Kind::Metadata)
                    .limit(1);

                if let Ok(metadata_events) = nostr_client::fetch_events_aggregated_outbox(
                    metadata_filter,
                    std::time::Duration::from_secs(5)
                ).await {
                    if let Some(metadata_event) = metadata_events.into_iter().next() {
                        if let Ok(meta) = serde_json::from_str::<Metadata>(&metadata_event.content) {
                            metadata.set(Some(meta));
                        }
                    }
                }
            });
        }
    });

    if let Some(pubkey) = pubkey_result {
        let pubkey_str = pubkey.to_hex();

        // Display name logic
        let display = if let Some(meta) = metadata.read().as_ref() {
            if let Some(display_name) = &meta.display_name {
                format!("@{}", display_name)
            } else if let Some(name) = &meta.name {
                format!("@{}", name)
            } else {
                // Fallback to truncated hex
                if pubkey_str.len() > 16 {
                    format!("@{}...{}", &pubkey_str[..8], &pubkey_str[pubkey_str.len()-4..])
                } else {
                    format!("@{}", pubkey_str)
                }
            }
        } else {
            // Loading state - show truncated hex
            if pubkey_str.len() > 16 {
                format!("@{}...{}", &pubkey_str[..8], &pubkey_str[pubkey_str.len()-4..])
            } else {
                format!("@{}", pubkey_str)
            }
        };

        rsx! {
            Link {
                to: Route::Profile { pubkey: pubkey.to_hex() },
                class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 font-medium hover:underline",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                "{display}"
            }
        }
    } else {
        // Fallback if parsing fails
        rsx! {
            span {
                class: "text-blue-500 dark:text-blue-400 font-medium",
                "{mention}"
            }
        }
    }
}

#[component]
fn EventMentionRenderer(mention: String) -> Element {
    // Extract the identifier from "nostr:note..." or just "note..."
    let identifier = mention.strip_prefix("nostr:").unwrap_or(&mention);

    // Parse event ID and relay hints from either nevent or note
    let parsed_event: Option<(EventId, Vec<String>)> = if identifier.starts_with("nevent1") {
        nostr_sdk::nips::nip19::Nip19Event::from_bech32(identifier)
            .ok()
            .map(|nip19| {
                let relays: Vec<String> = nip19.relays.iter()
                    .map(|r| r.to_string())
                    .collect();
                (nip19.event_id, relays)
            })
    } else if identifier.starts_with("note1") {
        nostr_sdk::EventId::from_bech32(identifier).ok().map(|id| (id, Vec::new()))
    } else {
        None
    };

    let event_id_result = parsed_event.as_ref().map(|(id, _)| *id);
    let relay_hints = parsed_event.map(|(_, relays)| relays).unwrap_or_default();

    // Handle naddr (parameterized replaceable event coordinate) - typically articles
    if identifier.starts_with("naddr1") {
        return rsx! {
            ArticleMentionRenderer { mention: mention.clone() }
        };
    }

    // Always call hooks unconditionally
    let mut embedded_event = use_signal(|| None::<Event>);
    let mut author_metadata = use_signal(|| None::<Metadata>);

    // Fetch the referenced event
    use_effect(move || {
        if let Some(event_id) = event_id_result {
            let relay_hints_clone = relay_hints.clone();
            spawn(async move {
                let event_filter = Filter::new()
                    .id(event_id)
                    .limit(1);

                // Try relay hints first if available, then fall back to aggregated fetch
                let fetch_result = if !relay_hints_clone.is_empty() {
                    // Use relay hints from nevent
                    if let Some(client) = nostr_client::get_client() {
                        let relay_urls: Vec<nostr_sdk::Url> = relay_hints_clone.iter()
                            .filter_map(|r| nostr_sdk::Url::parse(r).ok())
                            .collect();

                        if !relay_urls.is_empty() {
                            nostr_client::ensure_relays_ready(&client).await;
                            client.fetch_events_from(relay_urls, event_filter.clone(), std::time::Duration::from_secs(5)).await
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

                // Fall back to aggregated fetch if relay hints didn't work
                let events = match fetch_result {
                    Some(events) if !events.is_empty() => events,
                    _ => {
                        nostr_client::fetch_events_aggregated(
                            event_filter,
                            std::time::Duration::from_secs(5)
                        ).await.unwrap_or_default()
                    }
                };

                if let Some(event) = events.into_iter().next() {
                    let author_pubkey = event.pubkey;
                    embedded_event.set(Some(event));

                    // Fetch author metadata using Outbox
                    let metadata_filter = Filter::new()
                        .author(author_pubkey)
                        .kind(Kind::Metadata)
                        .limit(1);

                    if let Ok(metadata_events) = nostr_client::fetch_events_aggregated_outbox(
                        metadata_filter,
                        std::time::Duration::from_secs(5)
                    ).await {
                        if let Some(metadata_event) = metadata_events.into_iter().next() {
                            if let Ok(meta) = serde_json::from_str::<Metadata>(&metadata_event.content) {
                                author_metadata.set(Some(meta));
                            }
                        }
                    }
                }
            });
        }
    });

    if let Some(event_id) = event_id_result {
        // Render embedded note card
        let has_event = embedded_event.read().is_some();
        let event_clone = embedded_event.read().clone();
        let metadata_clone = author_metadata.read().clone();

        if has_event {
            let event = event_clone.unwrap();
            let event_kind = event.kind.as_u16();

            // Route to appropriate card based on event kind
            match event_kind {
                20 => {
                    // Photo (kind 20)
                    rsx! {
                        PhotoCard { event: event }
                    }
                }
                22 => {
                    // Video (kind 22)
                    rsx! {
                        VideoCard { event: event }
                    }
                }
                1040 => {
                    // Voice Message (kind 1040)
                    rsx! {
                        VoiceMessageCard { event: event }
                    }
                }
                1068 => {
                    // Poll (kind 1068)
                    rsx! {
                        PollCard { event: event }
                    }
                }
                _ => {
                    // Default: render as embedded note
                    rsx! {
                        {render_embedded_note(&event, metadata_clone.as_ref())}
                    }
                }
            }
        } else {
            // Loading state - show link
            let event_str = event_id.to_hex();
            let short = if event_str.len() > 16 {
                format!("note:{}...{}", &event_str[..8], &event_str[event_str.len()-4..])
            } else {
                format!("note:{}", event_str)
            };

            rsx! {
                Link {
                    to: Route::Note { note_id: event_id.to_hex() },
                    class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 font-medium hover:underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    "{short}"
                }
            }
        }
    } else {
        // Fallback if parsing fails
        rsx! {
            span {
                class: "text-blue-500 dark:text-blue-400 font-medium",
                "{mention}"
            }
        }
    }
}

fn render_embedded_note(event: &Event, metadata: Option<&Metadata>) -> Element {
    let event_id = event.id.to_hex();
    let content = &event.content;
    let pubkey = event.pubkey;
    let pubkey_str = pubkey.to_hex();

    // Truncate content if too long (character-aware)
    let display_content = {
        let char_count = content.chars().count();
        if char_count > 280 {
            let truncated: String = content.chars().take(280).collect();
            format!("{}...", truncated)
        } else {
            content.clone()
        }
    };

    // Get display name
    let display_name = if let Some(meta) = metadata {
        meta.display_name.clone()
            .or_else(|| meta.name.clone())
            .unwrap_or_else(|| format!("{}...{}", &pubkey_str[..8], &pubkey_str[pubkey_str.len()-4..]))
    } else {
        format!("{}...{}", &pubkey_str[..8], &pubkey_str[pubkey_str.len()-4..])
    };

    rsx! {
        Link {
            to: Route::Note { note_id: event_id.clone() },
            class: "block my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div {
                class: "border border-border rounded-lg p-3 hover:bg-accent/10 transition cursor-pointer",

                // Author info
                div {
                    class: "flex items-center gap-2 mb-2",

                    // Avatar
                    if let Some(meta) = metadata {
                        if let Some(picture) = &meta.picture {
                            img {
                                class: "w-8 h-8 rounded-full",
                                src: "{picture}",
                                alt: "Avatar"
                            }
                        } else {
                            div {
                                class: "w-8 h-8 rounded-full bg-blue-500 flex items-center justify-center text-white text-xs font-bold",
                                "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                            }
                        }
                    } else {
                        div {
                            class: "w-8 h-8 rounded-full bg-gray-400 flex items-center justify-center text-white text-xs",
                            "?"
                        }
                    }

                    span {
                        class: "font-semibold text-sm",
                        "{display_name}"
                    }
                }

                // Note content
                div {
                    class: "text-sm text-muted-foreground whitespace-pre-wrap break-words",
                    "{display_content}"
                }
            }
        }
    }
}

fn render_youtube_embed(url: &str) -> Element {
    // Extract video ID from YouTube URL
    let video_id = extract_youtube_id(url);

    if let Some(id) = video_id {
        let embed_url = format!("https://www.youtube.com/embed/{}", id);
        rsx! {
            iframe {
                src: "{embed_url}",
                class: "w-full aspect-video",
                allow: "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture",
                allowfullscreen: true,
            }
        }
    } else {
        // Fallback to link if we can't extract ID
        rsx! {
            a {
                href: "{url}",
                target: "_blank",
                rel: "noopener noreferrer",
                class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 underline",
                "{url}"
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

            // Twitter embed using blockquote (widgets.js will transform it automatically)
            blockquote {
                class: "twitter-tweet",
                "data-theme": "dark",
                "data-dnt": "true", // Do not track
                p { "Loading tweet..." }
                a {
                    href: "{tweet_url}",
                    "View tweet"
                }
            }
        }
    }
}

#[component]
fn TwitchStreamRenderer(channel: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let parent_domain = if cfg!(debug_assertions) {
        "localhost"
    } else {
        "nostr.blue"
    };
    let embed_url = format!("https://player.twitch.tv/?channel={}&parent={}", channel, parent_domain);

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
                    div {
                        class: "text-center",
                        div {
                            class: "text-purple-500 text-4xl mb-2",
                            "▶"
                        }
                        div {
                            class: "text-lg font-medium",
                            "Watch {channel} on Twitch"
                        }
                        div {
                            class: "text-sm text-muted-foreground mt-1",
                            "Click to load stream"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TwitchClipRenderer(clip_slug: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let parent_domain = if cfg!(debug_assertions) {
        "localhost"
    } else {
        "nostr.blue"
    };
    let embed_url = format!("https://clips.twitch.tv/embed?clip={}&parent={}", clip_slug, parent_domain);

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
                    div {
                        class: "text-center",
                        div {
                            class: "text-purple-500 text-4xl mb-2",
                            "▶"
                        }
                        div {
                            class: "text-lg font-medium",
                            "Watch Twitch Clip"
                        }
                        div {
                            class: "text-sm text-muted-foreground mt-1",
                            "Click to load clip"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TwitchVodRenderer(vod_id: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let parent_domain = if cfg!(debug_assertions) {
        "localhost"
    } else {
        "nostr.blue"
    };
    let embed_url = format!("https://player.twitch.tv/?video={}&parent={}", vod_id, parent_domain);

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
                    div {
                        class: "text-center",
                        div {
                            class: "text-purple-500 text-4xl mb-2",
                            "▶"
                        }
                        div {
                            class: "text-lg font-medium",
                            "Watch Twitch VOD"
                        }
                        div {
                            class: "text-sm text-muted-foreground mt-1",
                            "Click to load video"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ArticleMentionRenderer(mention: String) -> Element {
    // Extract the identifier from "nostr:naddr..." or just "naddr..."
    let identifier = mention.strip_prefix("nostr:").unwrap_or(&mention);

    // Parse the naddr coordinate and extract data we need, including relay hints
    let coord_data = nostr_sdk::nips::nip19::Nip19Coordinate::from_bech32(identifier)
        .ok()
        .map(|coord| {
            let relay_hints: Vec<String> = coord.relays.iter()
                .map(|r| r.to_string())
                .collect();
            (coord.public_key.to_hex(), coord.identifier.clone(), coord.kind.as_u16(), relay_hints)
        });

    // Always call hooks unconditionally
    let mut article_event = use_signal(|| None::<Event>);
    let mut author_metadata = use_signal(|| None::<Metadata>);
    let mut loading = use_signal(|| true);

    // Clone for use in effect
    let coord_data_for_effect = coord_data.clone();

    // Fetch the event by coordinate
    use_effect(move || {
        if let Some((ref pubkey, ref ident, kind, ref relays)) = coord_data_for_effect {
            let pubkey = pubkey.clone();
            let ident = ident.clone();
            let relay_hints = relays.clone();
            spawn(async move {
                loading.set(true);

                // Fetch event by coordinate with the correct kind from naddr and relay hints
                match crate::stores::nostr_client::fetch_event_by_coordinate_with_relays(
                        kind,
                        pubkey.clone(),
                        ident,
                        relay_hints
                    ).await {
                        Ok(Some(event)) => {
                            let author_pubkey = event.pubkey;
                            article_event.set(Some(event));

                            // Fetch author metadata using Outbox
                            let metadata_filter = Filter::new()
                                .author(author_pubkey)
                                .kind(Kind::Metadata)
                                .limit(1);

                            if let Ok(metadata_events) = nostr_client::fetch_events_aggregated_outbox(
                                metadata_filter,
                                std::time::Duration::from_secs(5)
                            ).await {
                                if let Some(metadata_event) = metadata_events.into_iter().next() {
                                    if let Ok(meta) = serde_json::from_str::<Metadata>(&metadata_event.content) {
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

        // Render embedded preview based on kind
        let has_event = article_event.read().is_some();
        let event_clone = article_event.read().clone();
        let metadata_clone = author_metadata.read().clone();

        if has_event {
            let event = event_clone.unwrap();

            // Route to appropriate card based on event kind
            match kind {
                30311 => {
                    // Live Stream (kind 30311) - wrap with stop_propagation for embedded use
                    rsx! {
                        div {
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            LiveStreamCard { event: event }
                        }
                    }
                }
                30023 => {
                    // Article (kind 30023)
                    rsx! {
                        {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                    }
                }
                _ => {
                    // Default: render as article
                    rsx! {
                        {render_embedded_article(&event, metadata_clone.as_ref(), &naddr_for_link)}
                    }
                }
            }
        } else if *loading.read() {
            // Loading state
            rsx! {
                div {
                    class: "my-2 p-3 border border-border rounded-lg bg-accent/5 animate-pulse",
                    div { class: "h-4 bg-muted rounded w-3/4 mb-2" }
                    div { class: "h-3 bg-muted rounded w-1/2" }
                }
            }
        } else {
            // Fallback if article not found
            rsx! {
                Link {
                    to: Route::ArticleDetail { naddr: naddr_for_link.clone() },
                    class: "text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300 font-medium hover:underline",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    "📄 Article"
                }
            }
        }
    } else {
        // Fallback if parsing fails
        rsx! {
            span {
                class: "text-blue-500 dark:text-blue-400 font-medium",
                "{mention}"
            }
        }
    }
}

fn render_embedded_article(event: &Event, metadata: Option<&Metadata>, naddr: &str) -> Element {
    use crate::utils::article_meta::{get_title, get_summary, get_image};

    let title = get_title(event);
    let summary = get_summary(event);
    let image_url = get_image(event);
    let pubkey_str = event.pubkey.to_hex();

    // Get display name
    let display_name = if let Some(meta) = metadata {
        meta.display_name.clone()
            .or_else(|| meta.name.clone())
            .unwrap_or_else(|| format!("{}...{}", &pubkey_str[..8], &pubkey_str[pubkey_str.len()-4..]))
    } else {
        format!("{}...{}", &pubkey_str[..8], &pubkey_str[pubkey_str.len()-4..])
    };

    // Truncate summary if too long (character-aware)
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
            to: Route::ArticleDetail { naddr: naddr.to_string() },
            class: "block my-2",
            onclick: move |e: MouseEvent| e.stop_propagation(),
            div {
                class: "border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition cursor-pointer",

                // Cover image if available
                if let Some(img_url) = image_url {
                    div {
                        class: "aspect-video w-full bg-muted overflow-hidden",
                        img {
                            src: "{img_url}",
                            alt: "{title}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    }
                }

                // Article info
                div {
                    class: "p-3",

                    // Title
                    h4 {
                        class: "font-bold text-base mb-1 line-clamp-2",
                        "{title}"
                    }

                    // Summary
                    if !display_summary.is_empty() {
                        p {
                            class: "text-sm text-muted-foreground mb-2 line-clamp-2",
                            "{display_summary}"
                        }
                    }

                    // Author
                    div {
                        class: "flex items-center gap-2",
                        if let Some(meta) = metadata {
                            if let Some(picture) = &meta.picture {
                                img {
                                    class: "w-6 h-6 rounded-full",
                                    src: "{picture}",
                                    alt: "Avatar"
                                }
                            } else {
                                div {
                                    class: "w-6 h-6 rounded-full bg-blue-500 flex items-center justify-center text-white text-xs font-bold",
                                    "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                                }
                            }
                        } else {
                            div {
                                class: "w-6 h-6 rounded-full bg-gray-400 flex items-center justify-center text-white text-xs",
                                "?"
                            }
                        }

                        span {
                            class: "text-xs text-muted-foreground",
                            "{display_name}"
                        }

                        span {
                            class: "text-xs text-muted-foreground",
                            "• Article"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn WavlakeTrackRenderer(track_id: String) -> Element {
    // Use use_resource to make fetch reactive to track_id changes
    let track_resource = use_resource(move || {
        let id = track_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_track(&id).await
        }
    });

    match track_resource.read_unchecked().as_ref() {
        // Loading state
        None => rsx! {
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
        },
        // Error state
        Some(Err(e)) => rsx! {
            div {
                class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div {
                    class: "flex items-center gap-2 text-red-500 text-sm",
                    icons::MusicIcon { class: "w-4 h-4" }
                    span { "Unable to load track: {e}" }
                }
            }
        },
        // Success state - render track card
        Some(Ok(track)) => {
        let track_clone = track.clone();

        let handle_play = move |_: MouseEvent| {
            let music_track = MusicTrack {
                id: track_clone.id.clone(),
                title: track_clone.title.clone(),
                artist: track_clone.artist.clone(),
                album: Some(track_clone.album_title.clone()),
                media_url: track_clone.media_url.clone(),
                album_art_url: Some(track_clone.album_art_url.clone()),
                artist_art_url: Some(track_clone.artist_art_url.clone()),
                duration: Some(track_clone.duration),
                artist_id: Some(track_clone.artist_id.clone()),
                album_id: Some(track_clone.album_id.clone()),
                artist_npub: track_clone.artist_npub.clone(),
            };
            music_player::play_track(music_track, None, None);
        };

        rsx! {
            div {
                class: "my-2 border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition bg-card",
                onclick: move |e: MouseEvent| e.stop_propagation(),

                div {
                    class: "flex items-center gap-4 p-4",

                    // Album art
                    div {
                        class: "relative w-16 h-16 flex-shrink-0 rounded overflow-hidden bg-muted group",
                        img {
                            src: "{track.album_art_url}",
                            alt: "Album art",
                            class: "w-full h-full object-cover"
                        }

                        // Play button overlay
                        button {
                            class: "absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition",
                            onclick: handle_play,
                            dangerous_inner_html: icons::PLAY
                        }
                    }

                    // Track info
                    div {
                        class: "flex-1 min-w-0",
                        div {
                            class: "font-semibold text-sm truncate",
                            "{track.title}"
                        }
                        div {
                            class: "text-xs text-muted-foreground truncate",
                            a {
                                href: "/music/artist/{track.artist_id}",
                                class: "hover:text-foreground hover:underline",
                                onclick: move |e| e.stop_propagation(),
                                "{track.artist}"
                            }
                        }
                        div {
                            class: "text-xs text-muted-foreground/80 truncate mt-1",
                            a {
                                href: "/music/album/{track.album_id}",
                                class: "hover:text-foreground hover:underline",
                                onclick: move |e| e.stop_propagation(),
                                "{track.album_title}"
                            }
                        }
                    }

                    // Duration and Wavlake badge
                    div {
                        class: "flex flex-col items-end gap-1 flex-shrink-0",
                        div {
                            class: "text-xs text-muted-foreground",
                            {
                                let mins = track.duration / 60;
                                let secs = track.duration % 60;
                                format!("{:02}:{:02}", mins, secs)
                            }
                        }
                        div {
                            class: "flex items-center gap-1 text-xs text-purple-400",
                            icons::MusicIcon { class: "w-3 h-3" }
                            "Wavlake"
                        }
                    }
                }
            }
        }
        },
    }
}

#[component]
fn WavlakeAlbumRenderer(album_id: String) -> Element {
    // Use use_resource to make fetch reactive to album_id changes
    let album_resource = use_resource(move || {
        let id = album_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_album(&id).await
        }
    });

    match album_resource.read_unchecked().as_ref() {
        // Loading state
        None => rsx! {
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
        },
        // Error state
        Some(Err(e)) => rsx! {
            div {
                class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div {
                    class: "flex items-center gap-2 text-red-500 text-sm",
                    icons::DiscIcon { class: "w-4 h-4" }
                    span { "Unable to load album: {e}" }
                }
            }
        },
        // Success state - render album card with track list
        Some(Ok(album)) => {
        let tracks: Vec<MusicTrack> = album.tracks.iter().map(|track| MusicTrack {
            id: track.id.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: Some(track.album_title.clone()),
            media_url: track.media_url.clone(),
            album_art_url: Some(track.album_art_url.clone()),
            artist_art_url: Some(track.artist_art_url.clone()),
            duration: Some(track.duration),
            artist_id: Some(track.artist_id.clone()),
            album_id: Some(track.album_id.clone()),
            artist_npub: track.artist_npub.clone(),
        }).collect();

        rsx! {
            div {
                class: "my-2 border border-border rounded-lg overflow-hidden bg-card",
                onclick: move |e: MouseEvent| e.stop_propagation(),

                // Album header
                div {
                    class: "flex gap-4 p-4 border-b border-border",

                    // Album art
                    if let Some(art_url) = &album.album_art_url {
                        img {
                            src: "{art_url}",
                            alt: "Album art",
                            class: "w-32 h-32 rounded object-cover flex-shrink-0"
                        }
                    } else {
                        div {
                            class: "w-32 h-32 rounded bg-muted flex items-center justify-center flex-shrink-0",
                            icons::DiscIcon { class: "w-16 h-16 text-muted-foreground" }
                        }
                    }

                    // Album info
                    div {
                        class: "flex-1 min-w-0",
                        div {
                            class: "text-xs text-muted-foreground mb-1",
                            "ALBUM"
                        }
                        div {
                            class: "font-bold text-lg truncate mb-1",
                            "{album.title}"
                        }
                        div {
                            class: "text-sm text-muted-foreground truncate mb-2",
                            a {
                                href: if let Some(first_track) = album.tracks.first() {
                                    format!("/music/artist/{}", first_track.artist_id)
                                } else {
                                    "#".to_string()
                                },
                                class: "hover:text-foreground hover:underline",
                                onclick: move |e| e.stop_propagation(),
                                "{album.artist}"
                            }
                        }
                        div {
                            class: "flex items-center gap-3 text-xs text-muted-foreground",
                            span {
                                {album.release_date.split('T').next().unwrap_or("Unknown").split('-').next().unwrap_or("Unknown")}
                            }
                            span { "•" }
                            span {
                                "{album.tracks.len()} "
                                {if album.tracks.len() == 1 { "track" } else { "tracks" }}
                            }
                            span { "•" }
                            span {
                                class: "flex items-center gap-1 text-purple-400",
                                icons::MusicIcon { class: "w-3 h-3" }
                                "Wavlake"
                            }
                        }
                    }
                }

                // Track list
                div {
                    class: "divide-y divide-border",
                    for (index, track_data) in album.tracks.iter().enumerate() {
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
                                        music_player::play_track(track_clone.clone(), Some(playlist.clone()), Some(index));
                                    },

                                    // Track number / play icon
                                    div {
                                        class: "w-8 text-center text-sm text-muted-foreground flex-shrink-0",
                                        span { class: "group-hover:hidden", "{index + 1}" }
                                        div {
                                            class: "hidden group-hover:flex items-center justify-center",
                                            dangerous_inner_html: icons::PLAY
                                        }
                                    }

                                    // Track info
                                    div {
                                        class: "flex-1 min-w-0",
                                        div {
                                            class: "font-medium text-sm truncate",
                                            "{track_title}"
                                        }
                                        div {
                                            class: "text-xs text-muted-foreground truncate",
                                            "{track_artist}"
                                        }
                                    }

                                    // Duration
                                    div {
                                        class: "text-xs text-muted-foreground flex-shrink-0",
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
        },
    }
}

#[component]
fn WavlakeArtistRenderer(artist_id: String) -> Element {
    // Use use_resource to make fetch reactive to artist_id changes
    let artist_resource = use_resource(move || {
        let id = artist_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_artist(&id).await
        }
    });

    // Always call hooks unconditionally
    let nav = use_navigator();

    match artist_resource.read_unchecked().as_ref() {
        // Loading state
        None => rsx! {
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
        },
        // Error state
        Some(Err(e)) => rsx! {
            div {
                class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div {
                    class: "flex items-center gap-2 text-red-500 text-sm",
                    icons::UserIcon { class: "w-4 h-4" }
                    span { "Unable to load artist: {e}" }
                }
            }
        },
        // Success state - render artist card
        Some(Ok(artist)) => {

        rsx! {
            div {
                class: "my-2 border border-border rounded-lg overflow-hidden hover:bg-accent/10 transition bg-card cursor-pointer",
                onclick: {
                    let artist_id_nav = artist.id.clone();
                    let navigator = nav.clone();
                    move |e: MouseEvent| {
                        e.stop_propagation();
                        // Navigate to artist page
                        navigator.push(Route::MusicArtist { artist_id: artist_id_nav.clone() });
                    }
                },

                div {
                    class: "flex items-center gap-4 p-4",

                    // Artist image
                    if let Some(art_url) = &artist.artist_art_url {
                        if !art_url.is_empty() {
                            img {
                                src: "{art_url}",
                                alt: "Artist",
                                class: "w-20 h-20 rounded-full object-cover flex-shrink-0"
                            }
                        } else {
                            div {
                                class: "w-20 h-20 rounded-full bg-muted flex items-center justify-center flex-shrink-0",
                                icons::UserIcon { class: "w-10 h-10 text-muted-foreground" }
                            }
                        }
                    } else {
                        div {
                            class: "w-20 h-20 rounded-full bg-muted flex items-center justify-center flex-shrink-0",
                            icons::UserIcon { class: "w-10 h-10 text-muted-foreground" }
                        }
                    }

                    // Artist info
                    div {
                        class: "flex-1 min-w-0",
                        div {
                            class: "text-xs text-muted-foreground mb-1",
                            "ARTIST"
                        }
                        div {
                            class: "font-bold text-lg truncate mb-1",
                            "{artist.name}"
                        }
                        div {
                            class: "flex items-center gap-2 text-xs text-muted-foreground",
                            span {
                                "{artist.albums.len()} "
                                {if artist.albums.len() == 1 { "album" } else { "albums" }}
                            }
                            span { "•" }
                            span {
                                class: "flex items-center gap-1 text-purple-400",
                                icons::MusicIcon { class: "w-3 h-3" }
                                "Wavlake"
                            }
                        }
                    }

                    // Arrow icon
                    div {
                        class: "flex-shrink-0 text-muted-foreground",
                        dangerous_inner_html: r#"<svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="w-5 h-5"><path stroke-linecap="round" stroke-linejoin="round" d="M8.25 4.5l7.5 7.5-7.5 7.5" /></svg>"#
                    }
                }
            }
        }
        },
    }
}

#[component]
fn WavlakePlaylistRenderer(playlist_id: String) -> Element {
    // Use use_resource to make fetch reactive to playlist_id changes
    let playlist_resource = use_resource(move || {
        let id = playlist_id.clone();
        async move {
            let api = WavlakeAPI::new();
            api.get_playlist(&id).await
        }
    });

    match playlist_resource.read_unchecked().as_ref() {
        // Loading state
        None => rsx! {
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
        },
        // Error state
        Some(Err(e)) => rsx! {
            div {
                class: "my-2 p-3 border border-border rounded-lg bg-red-500/10 border-red-500/30",
                onclick: move |e: MouseEvent| e.stop_propagation(),
                div {
                    class: "flex items-center gap-2 text-red-500 text-sm",
                    icons::MusicIcon { class: "w-4 h-4" }
                    span { "Unable to load playlist: {e}" }
                }
            }
        },
        // Success state - render playlist card with track list
        Some(Ok(playlist)) => {
        let tracks: Vec<MusicTrack> = playlist.tracks.iter().map(|track| MusicTrack {
            id: track.id.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: Some(track.album_title.clone()),
            media_url: track.media_url.clone(),
            album_art_url: Some(track.album_art_url.clone()),
            artist_art_url: Some(track.artist_art_url.clone()),
            duration: Some(track.duration),
            artist_id: Some(track.artist_id.clone()),
            album_id: Some(track.album_id.clone()),
            artist_npub: track.artist_npub.clone(),
        }).collect();

        rsx! {
            div {
                class: "my-2 border border-border rounded-lg overflow-hidden bg-card",
                onclick: move |e: MouseEvent| e.stop_propagation(),

                // Playlist header
                div {
                    class: "flex gap-4 p-4 border-b border-border",

                    // Playlist cover (use first track's album art)
                    if let Some(first_track) = playlist.tracks.first() {
                        img {
                            src: "{first_track.album_art_url}",
                            alt: "Playlist cover",
                            class: "w-32 h-32 rounded object-cover flex-shrink-0"
                        }
                    } else {
                        div {
                            class: "w-32 h-32 rounded bg-muted flex items-center justify-center flex-shrink-0",
                            icons::MusicIcon { class: "w-16 h-16 text-muted-foreground" }
                        }
                    }

                    // Playlist info
                    div {
                        class: "flex-1 min-w-0",
                        div {
                            class: "text-xs text-muted-foreground mb-1",
                            "PLAYLIST"
                        }
                        div {
                            class: "font-bold text-lg truncate mb-1",
                            "{playlist.title}"
                        }
                        div {
                            class: "flex items-center gap-3 text-xs text-muted-foreground",
                            span {
                                "{playlist.tracks.len()} "
                                {if playlist.tracks.len() == 1 { "track" } else { "tracks" }}
                            }
                            span { "•" }
                            span {
                                class: "flex items-center gap-1 text-purple-400",
                                icons::MusicIcon { class: "w-3 h-3" }
                                "Wavlake"
                            }
                        }
                    }
                }

                // Track list
                div {
                    class: "divide-y divide-border max-h-96 overflow-y-auto",
                    for (index, track_data) in playlist.tracks.iter().enumerate() {
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
                                        music_player::play_track(track_clone.clone(), Some(playlist_clone.clone()), Some(index));
                                    },

                                    // Album art thumbnail
                                    div {
                                        class: "relative w-10 h-10 flex-shrink-0 rounded overflow-hidden bg-muted group-hover:opacity-80",
                                        img {
                                            src: "{track_album_art}",
                                            alt: "Album art",
                                            class: "w-full h-full object-cover"
                                        }
                                        div {
                                            class: "absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition",
                                            dangerous_inner_html: icons::PLAY_SMALL
                                        }
                                    }

                                    // Track info
                                    div {
                                        class: "flex-1 min-w-0",
                                        div {
                                            class: "font-medium text-sm truncate",
                                            "{track_title}"
                                        }
                                        div {
                                            class: "text-xs text-muted-foreground truncate",
                                            "{track_artist}"
                                        }
                                    }

                                    // Duration
                                    div {
                                        class: "text-xs text-muted-foreground flex-shrink-0",
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
        },
    }
}

/// Renders a YouTube embed with click-to-load for privacy/performance
#[component]
fn YouTubeRenderer(video_id: String) -> Element {
    let mut is_visible = use_signal(|| false);
    let thumbnail_url = format!("https://img.youtube.com/vi/{}/maxresdefault.jpg", video_id);
    let embed_url = format!("https://www.youtube.com/embed/{}?autoplay=1", video_id);

    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden bg-black aspect-video max-w-full",
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    class: "w-full aspect-video",
                    allow: "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture",
                    allowfullscreen: true,
                    frame_border: "0"
                }
            } else {
                div {
                    class: "relative w-full aspect-video cursor-pointer group",
                    onclick: move |_| is_visible.set(true),
                    img {
                        src: "{thumbnail_url}",
                        alt: "YouTube video thumbnail",
                        class: "w-full h-full object-cover"
                    }
                    // Play button overlay
                    div {
                        class: "absolute inset-0 flex items-center justify-center bg-black/30 group-hover:bg-black/40 transition",
                        div {
                            class: "w-16 h-16 bg-red-600 rounded-full flex items-center justify-center shadow-lg group-hover:scale-110 transition",
                            svg {
                                class: "w-8 h-8 text-white ml-1",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    d: "M8 5v14l11-7z"
                                }
                            }
                        }
                    }
                    // YouTube branding
                    div {
                        class: "absolute bottom-2 right-2 px-2 py-1 bg-black/70 rounded text-white text-xs font-medium",
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
    let embed_url = format!("https://open.spotify.com/embed/{}/{}?utm_source=generator&theme=0", content_type, content_id);

    // Tracks are shorter, albums/playlists/episodes are taller
    let height = match content_type.as_str() {
        "track" => "152",
        _ => "352",
    };

    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    width: "100%",
                    height: "{height}",
                    frame_border: "0",
                    allow: "autoplay; clipboard-write; encrypted-media; fullscreen; picture-in-picture"
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#1DB954]/10 border border-[#1DB954]/30 rounded-lg cursor-pointer hover:bg-[#1DB954]/20 transition",
                    onclick: move |_| is_visible.set(true),
                    div {
                        class: "w-12 h-12 bg-[#1DB954] rounded-full flex items-center justify-center flex-shrink-0",
                        svg {
                            class: "w-7 h-7 text-black",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                d: "M12 0C5.4 0 0 5.4 0 12s5.4 12 12 12 12-5.4 12-12S18.66 0 12 0zm5.521 17.34c-.24.359-.66.48-1.021.24-2.82-1.74-6.36-2.101-10.561-1.141-.418.122-.779-.179-.899-.539-.12-.421.18-.78.54-.9 4.56-1.021 8.52-.6 11.64 1.32.42.18.479.659.301 1.02zm1.44-3.3c-.301.42-.841.6-1.262.3-3.239-1.98-8.159-2.58-11.939-1.38-.479.12-1.02-.12-1.14-.6-.12-.48.12-1.021.6-1.141C9.6 9.9 15 10.561 18.72 12.84c.361.181.54.78.241 1.2zm.12-3.36C15.24 8.4 8.82 8.16 5.16 9.301c-.6.179-1.2-.181-1.38-.721-.18-.601.18-1.2.72-1.381 4.26-1.26 11.28-1.02 15.721 1.621.539.3.719 1.02.419 1.56-.299.421-1.02.599-1.559.3z"
                            }
                        }
                    }
                    div {
                        class: "flex-1 min-w-0",
                        div {
                            class: "font-medium text-sm text-[#1DB954]",
                            "Spotify {content_type}"
                        }
                        div {
                            class: "text-xs text-muted-foreground",
                            "Click to load player"
                        }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL
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
        encoded_url
    );

    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    width: "100%",
                    height: "166",
                    frame_border: "0",
                    allow: "autoplay",
                    scrolling: "no"
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#ff5500]/10 border border-[#ff5500]/30 rounded-lg cursor-pointer hover:bg-[#ff5500]/20 transition",
                    onclick: move |_| is_visible.set(true),
                    div {
                        class: "w-12 h-12 bg-[#ff5500] rounded-full flex items-center justify-center flex-shrink-0",
                        svg {
                            class: "w-7 h-7 text-white",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                d: "M1.175 12.225c-.051 0-.094.046-.101.1l-.233 2.154.233 2.105c.007.058.05.098.101.098.05 0 .09-.04.099-.098l.255-2.105-.27-2.154c-.009-.06-.052-.1-.102-.1m-.899.828c-.06 0-.091.037-.104.094L0 14.479l.165 1.308c.014.057.045.094.09.094s.089-.037.099-.094l.19-1.308-.19-1.332c-.01-.057-.045-.094-.09-.094m1.83-1.229c-.061 0-.12.045-.12.104l-.21 2.563.225 2.458c0 .06.045.104.106.104.061 0 .12-.044.12-.104l.24-2.458-.24-2.563c0-.06-.059-.104-.12-.104m.945-.089c-.075 0-.135.06-.15.135l-.193 2.64.21 2.544c.016.077.075.138.149.138.075 0 .135-.061.15-.138l.24-2.544-.24-2.64c-.015-.075-.074-.135-.15-.135m.93-.104c-.09 0-.165.075-.18.165l-.178 2.73.195 2.61c.015.09.089.164.179.164.09 0 .164-.074.18-.164l.21-2.61-.21-2.73c-.015-.09-.09-.165-.18-.165m.964-.03c-.105 0-.195.09-.21.195l-.165 2.76.18 2.64c.015.105.105.18.21.18s.195-.075.21-.18l.195-2.64-.195-2.76c-.015-.105-.105-.195-.21-.195m1.005.15c-.12 0-.225.105-.225.225l-.15 2.595.165 2.655c0 .12.105.225.225.225s.225-.105.225-.225l.18-2.655-.18-2.595c0-.12-.105-.225-.225-.225m1.02-.135c-.135 0-.255.12-.255.255l-.135 2.58.15 2.685c0 .135.12.24.255.24s.255-.105.255-.24l.165-2.685-.165-2.58c0-.135-.12-.255-.255-.255m2.04.165c-.15 0-.285.135-.285.285l-.12 2.4.135 2.67c0 .15.135.285.285.285s.285-.135.285-.285l.15-2.67-.15-2.4c0-.15-.135-.285-.285-.285m-1.02-.15c-.15 0-.27.12-.27.27l-.135 2.55.135 2.67c0 .135.12.255.27.255.135 0 .255-.12.27-.255l.15-2.67-.15-2.55c-.015-.15-.135-.27-.27-.27m2.04-.105c-.165 0-.3.135-.315.3l-.105 2.415.12 2.685c.015.165.15.3.315.3.15 0 .285-.135.3-.3l.135-2.685-.135-2.415c-.015-.165-.15-.3-.3-.3m1.02.105c-.18 0-.33.15-.33.33l-.105 2.295.105 2.685c0 .165.15.315.33.315.165 0 .315-.15.33-.315l.12-2.685-.12-2.295c-.015-.18-.165-.33-.33-.33m1.02-.255c-.195 0-.345.15-.36.345l-.09 2.535.105 2.7c.015.18.165.33.345.33.195 0 .345-.15.36-.33l.12-2.7-.12-2.535c-.015-.195-.165-.345-.36-.345m1.034.035c-.21 0-.375.165-.375.375l-.09 2.52.09 2.685c0 .21.165.375.375.375.195 0 .36-.165.375-.375l.105-2.685-.105-2.52c-.015-.21-.18-.375-.375-.375m1.035-.18c-.225 0-.405.18-.405.405l-.075 2.295.075 2.685c0 .225.18.405.405.405.21 0 .39-.18.405-.405l.09-2.685-.09-2.295c-.015-.225-.195-.405-.405-.405m1.02-.24c-.225 0-.42.195-.42.42l-.06 2.13.075 2.685c0 .225.195.405.42.405.225 0 .405-.18.42-.405l.09-2.685-.09-2.13c-.015-.225-.195-.42-.42-.42m1.034-.09c-.24 0-.435.195-.435.435l-.06 1.83.06 2.685c0 .24.195.435.435.435.24 0 .435-.195.435-.435l.075-2.685-.075-1.83c0-.24-.195-.435-.435-.435m1.05.075c-.255 0-.465.21-.465.465l-.045 1.35.06 2.67c0 .255.21.465.465.465.24 0 .45-.21.465-.465l.06-2.67-.06-1.35c-.015-.255-.225-.465-.465-.465m1.035-.42c-.27 0-.495.225-.495.495l-.03.96.045 2.67c0 .27.225.495.495.495s.48-.225.495-.495l.06-2.67-.06-.96c-.015-.27-.225-.495-.495-.495m1.05.27c-.285 0-.51.24-.51.525l-.03.66.045 2.685c0 .285.225.51.51.51.27 0 .495-.225.51-.51l.06-2.685-.06-.66c-.015-.285-.24-.525-.51-.525m1.065.435c-.3 0-.54.24-.54.54v.195l.03 2.7c0 .285.24.525.54.525.285 0 .525-.24.54-.54l.045-2.685-.045-.195c-.015-.3-.255-.54-.54-.54m2.28 1.29c-.135 0-.27.015-.39.045-.105-.885-.87-1.575-1.8-1.575-.255 0-.51.06-.72.165-.09.045-.12.09-.12.18v5.295c0 .09.06.165.15.18.015 0 2.88 0 2.88 0 .945 0 1.71-.765 1.71-1.71s-.765-1.71-1.71-1.71"
                            }
                        }
                    }
                    div {
                        class: "flex-1 min-w-0",
                        div {
                            class: "font-medium text-sm text-[#ff5500]",
                            "SoundCloud"
                        }
                        div {
                            class: "text-xs text-muted-foreground",
                            "Click to load player"
                        }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL
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

    // Convert regular URL to embed URL if needed
    let final_embed_url = if embed_url.contains("embed.music.apple.com") {
        embed_url.clone()
    } else {
        // Convert music.apple.com/{region}/{type}/{name}/{id} to embed format
        embed_url.replace("music.apple.com", "embed.music.apple.com")
    };

    let height = if is_song { "175" } else { "450" };

    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            if *is_visible.read() {
                iframe {
                    src: "{final_embed_url}",
                    width: "100%",
                    height: "{height}",
                    frame_border: "0",
                    allow: "autoplay *; encrypted-media *; fullscreen *; clipboard-write",
                    style: "border-radius: 10px;"
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-gradient-to-r from-[#fc3c44]/10 to-[#fa57c1]/10 border border-[#fc3c44]/30 rounded-lg cursor-pointer hover:from-[#fc3c44]/20 hover:to-[#fa57c1]/20 transition",
                    onclick: move |_| is_visible.set(true),
                    div {
                        class: "w-12 h-12 bg-gradient-to-br from-[#fc3c44] to-[#fa57c1] rounded-xl flex items-center justify-center flex-shrink-0",
                        svg {
                            class: "w-7 h-7 text-white",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                d: "M23.994 6.124a9.23 9.23 0 00-.24-2.19c-.317-1.31-1.062-2.31-2.18-3.043a5.022 5.022 0 00-1.877-.726 10.496 10.496 0 00-1.564-.15c-.04-.003-.083-.01-.124-.013H5.99c-.042.003-.083.01-.124.013-.5.032-.999.09-1.486.191a5.023 5.023 0 00-1.815.74c-1.113.737-1.857 1.736-2.177 3.038-.2.808-.255 1.634-.254 2.465.002.05.007.1.01.15v11.28c-.003.05-.008.1-.01.15.001.83.057 1.658.255 2.465.32 1.303 1.064 2.302 2.177 3.039a5.023 5.023 0 001.815.74c.487.1.986.159 1.486.19.041.004.082.01.124.013h12.02c.042-.003.083-.01.124-.013.5-.031.999-.09 1.486-.19a5.023 5.023 0 001.815-.74c1.113-.738 1.857-1.737 2.177-3.04.2-.807.255-1.634.254-2.464-.002-.05-.007-.1-.01-.15V6.274c.003-.05.008-.1.01-.15zM17.5 17.5c0 .397-.063.79-.187 1.163a2.5 2.5 0 01-2.658 1.682c-.674-.099-1.261-.437-1.655-.97-.394-.534-.56-1.202-.46-1.877.098-.674.437-1.261.97-1.656.534-.394 1.202-.56 1.877-.46.28.04.544.126.784.251V9.13c0-.277.175-.524.437-.616l4.5-1.5a.642.642 0 01.842.608v1.5a.643.643 0 01-.437.608l-3.563 1.188a.643.643 0 00-.437.608v6.474z"
                            }
                        }
                    }
                    div {
                        class: "flex-1 min-w-0",
                        div {
                            class: "font-medium text-sm bg-gradient-to-r from-[#fc3c44] to-[#fa57c1] bg-clip-text text-transparent",
                            "Apple Music"
                        }
                        div {
                            class: "text-xs text-muted-foreground",
                            "Click to load player"
                        }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL
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
        encoded_path
    );

    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            if *is_visible.read() {
                iframe {
                    src: "{embed_url}",
                    width: "100%",
                    height: "120",
                    frame_border: "0"
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#5000ff]/10 border border-[#5000ff]/30 rounded-lg cursor-pointer hover:bg-[#5000ff]/20 transition",
                    onclick: move |_| is_visible.set(true),
                    div {
                        class: "w-12 h-12 bg-[#5000ff] rounded-full flex items-center justify-center flex-shrink-0",
                        svg {
                            class: "w-7 h-7 text-white",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                d: "M19.943 6.667c0-1.534-1.632-2.667-3.432-2.667-1.963 0-3.768 1.12-4.511 2.793-.743-1.673-2.548-2.793-4.511-2.793-1.8 0-3.432 1.133-3.432 2.667S1.98 10 1.98 13.333c0 .867.327 1.667.843 2.267.517.6 1.237 1.067 2.047 1.333.81.267 1.69.4 2.62.4h8.52c.93 0 1.81-.133 2.62-.4.81-.266 1.53-.733 2.047-1.333.516-.6.843-1.4.843-2.267 0-3.333-2.077-6.666-2.077-6.666z"
                            }
                        }
                    }
                    div {
                        class: "flex-1 min-w-0",
                        div {
                            class: "font-medium text-sm text-[#5000ff]",
                            "MixCloud"
                        }
                        div {
                            class: "text-xs text-muted-foreground truncate",
                            "{username}/{mix_name}"
                        }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL
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

    // Ensure URL is in embed format
    let final_embed_url = if embed_url.contains("/embed/") {
        embed_url.clone()
    } else {
        // Try to convert to embed URL
        embed_url.clone()
    };

    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden bg-black aspect-video max-w-full",
            if *is_visible.read() {
                iframe {
                    src: "{final_embed_url}",
                    class: "w-full aspect-video",
                    frame_border: "0",
                    allowfullscreen: true
                }
            } else {
                div {
                    class: "relative w-full aspect-video cursor-pointer group bg-[#85c742]/10",
                    onclick: move |_| is_visible.set(true),
                    // Rumble logo and play button
                    div {
                        class: "absolute inset-0 flex flex-col items-center justify-center gap-4",
                        div {
                            class: "w-20 h-20 bg-[#85c742] rounded-full flex items-center justify-center shadow-lg group-hover:scale-110 transition",
                            svg {
                                class: "w-10 h-10 text-white ml-1",
                                fill: "currentColor",
                                view_box: "0 0 24 24",
                                path {
                                    d: "M8 5v14l11-7z"
                                }
                            }
                        }
                        div {
                            class: "px-3 py-1.5 bg-[#85c742] rounded text-white text-sm font-bold",
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

    // Convert regular URL to embed URL if needed
    let final_embed_url = if embed_url.contains("embed.tidal.com") {
        embed_url.clone()
    } else if embed_url.contains("tidal.com/browse/track/") {
        // Convert tidal.com/browse/track/{id} to embed format
        let track_id = embed_url.split("/track/").nth(1)
            .and_then(|s| s.split(&['?', '#', '/'][..]).next())
            .unwrap_or("");
        format!("https://embed.tidal.com/tracks/{}?layout=gridify", track_id)
    } else {
        embed_url.clone()
    };

    rsx! {
        div {
            class: "my-2 rounded-lg overflow-hidden",
            if *is_visible.read() {
                iframe {
                    src: "{final_embed_url}",
                    width: "100%",
                    height: "96",
                    frame_border: "0",
                    allow: "encrypted-media"
                }
            } else {
                div {
                    class: "flex items-center gap-3 p-4 bg-[#000000]/10 border border-[#000000]/30 dark:bg-white/10 dark:border-white/30 rounded-lg cursor-pointer hover:bg-[#000000]/20 dark:hover:bg-white/20 transition",
                    onclick: move |_| is_visible.set(true),
                    div {
                        class: "w-12 h-12 bg-black dark:bg-white rounded-full flex items-center justify-center flex-shrink-0",
                        svg {
                            class: "w-6 h-6 text-white dark:text-black",
                            fill: "currentColor",
                            view_box: "0 0 24 24",
                            path {
                                d: "M12.012 3.992L8.008 7.996 4.004 3.992 0 7.996l4.004 4.004 4.004-4.004 4.004 4.004-4.004 4.004-4.004-4.004-4.004 4.004L0 20.008l4.004-4.004 4.004 4.004 4.004-4.004 4.004 4.004 4.004-4.004-4.004-4.004 4.004-4.004 4.004 4.004 4.004-4.004-4.004-4.004 4.004-4.004L20.02 0l-4.004 4.004L12.012 0 8.008 4.004l4.004 3.988z"
                            }
                        }
                    }
                    div {
                        class: "flex-1 min-w-0",
                        div {
                            class: "font-medium text-sm",
                            "Tidal"
                        }
                        div {
                            class: "text-xs text-muted-foreground",
                            "Click to load player"
                        }
                    }
                    div {
                        class: "text-muted-foreground",
                        dangerous_inner_html: icons::PLAY_SMALL
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

    // Fetch the live event by naddr
    use_effect(move || {
        let naddr_clone = naddr.clone();
        spawn(async move {
            match Nip19::from_bech32(&naddr_clone) {
                Ok(Nip19::Coordinate(coord)) => {
                    // Use helper that handles relay hints, ensure_relays_ready, and DB caching
                    let relay_hints: Vec<String> = coord.relays.iter()
                        .map(|r| r.to_string())
                        .collect();

                    match nostr_client::fetch_event_by_coordinate_with_relays(
                        coord.kind.as_u16(),
                        coord.public_key.to_hex(),
                        coord.identifier.clone(),
                        relay_hints,
                    ).await {
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
        div {
            class: "my-2",
            if *loading.read() {
                div {
                    class: "flex items-center gap-3 p-4 bg-purple-500/10 border border-purple-500/30 rounded-lg animate-pulse",
                    div {
                        class: "w-12 h-12 bg-purple-500/30 rounded-full"
                    }
                    div {
                        class: "flex-1",
                        div {
                            class: "h-4 bg-purple-500/30 rounded w-32 mb-2"
                        }
                        div {
                            class: "h-3 bg-purple-500/20 rounded w-24"
                        }
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-4 bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-200 rounded-lg text-sm",
                    "{err}"
                }
            } else if let Some(ev) = event.read().as_ref() {
                LiveStreamCard {
                    event: ev.clone()
                }
            }
        }
    }
}
