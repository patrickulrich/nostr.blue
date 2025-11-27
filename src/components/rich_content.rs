use dioxus::prelude::*;
use crate::utils::content_parser::{parse_content, ContentToken};
use crate::routes::Route;
use nostr_sdk::{Tag, FromBech32, Metadata, PublicKey, Filter, Kind, Event, EventId};
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

fn extract_youtube_id(url: &str) -> Option<String> {
    // Handle youtube.com/watch?v=ID (including with playlist params)
    if let Some(start) = url.find("v=") {
        let id_start = start + 2;
        let id = &url[id_start..];
        let id = id.split('&').next()?;
        let id = id.split('#').next()?; // Handle hash fragments
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    // Handle youtu.be/ID
    if url.contains("youtu.be/") {
        if let Some(start) = url.find("youtu.be/") {
            let id_start = start + 9;
            let id = &url[id_start..];
            let id = id.split('?').next()?;
            let id = id.split('#').next()?;
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }

    // Handle youtube.com/shorts/ID
    if let Some(start) = url.find("/shorts/") {
        let id_start = start + 8;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?; // Handle trailing slashes
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    // Handle youtube.com/embed/ID
    if let Some(start) = url.find("/embed/") {
        let id_start = start + 7;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    // Handle youtube.com/live/ID
    if let Some(start) = url.find("/live/") {
        let id_start = start + 6;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    // Handle youtube.com/v/ID (older embed format)
    if let Some(start) = url.find("/v/") {
        let id_start = start + 3;
        let id = &url[id_start..];
        let id = id.split('?').next()?;
        let id = id.split('#').next()?;
        let id = id.split('/').next()?;
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }

    None
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
