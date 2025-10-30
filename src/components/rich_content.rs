use dioxus::prelude::*;
use crate::utils::content_parser::{parse_content, ContentToken};
use crate::routes::Route;
use nostr_sdk::{Tag, FromBech32, Metadata, PublicKey, Filter, Kind, Event, EventId};
use crate::stores::nostr_client;

#[component]
pub fn RichContent(content: String, tags: Vec<Tag>) -> Element {
    let tokens = parse_content(&content, &tags);

    rsx! {
        div {
            class: "whitespace-pre-wrap break-words space-y-2",
            for token in tokens.iter() {
                {render_token(token)}
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

    if let Some(pubkey) = pubkey_result {
        let pubkey_str = pubkey.to_hex();
        let mut metadata = use_signal(|| None::<Metadata>);

        // Fetch profile metadata
        use_effect(move || {
            spawn(async move {
                if let Some(client) = nostr_client::NOSTR_CLIENT.read().as_ref() {
                    let metadata_filter = Filter::new()
                        .author(pubkey)
                        .kind(Kind::Metadata)
                        .limit(1);

                    if let Ok(metadata_events) = client.fetch_events(
                        metadata_filter,
                        std::time::Duration::from_secs(5)
                    ).await {
                        if let Some(metadata_event) = metadata_events.into_iter().next() {
                            if let Ok(meta) = serde_json::from_str::<Metadata>(&metadata_event.content) {
                                metadata.set(Some(meta));
                            }
                        }
                    }
                }
            });
        });

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

    // Parse event ID from either nevent or note
    let event_id_result: Option<EventId> = if identifier.starts_with("nevent1") {
        nostr_sdk::nips::nip19::Nip19Event::from_bech32(identifier)
            .ok()
            .map(|nip19| nip19.event_id)
    } else if identifier.starts_with("note1") {
        nostr_sdk::EventId::from_bech32(identifier).ok()
    } else {
        None
    };

    // Handle naddr (parameterized replaceable event coordinate) - typically articles
    if identifier.starts_with("naddr1") {
        return rsx! {
            ArticleMentionRenderer { mention: mention.clone() }
        };
    }

    if let Some(event_id) = event_id_result {
        let mut embedded_event = use_signal(|| None::<Event>);
        let mut author_metadata = use_signal(|| None::<Metadata>);

        // Fetch the referenced event
        use_effect(move || {
            spawn(async move {
                if let Some(client) = nostr_client::NOSTR_CLIENT.read().as_ref() {
                    let event_filter = Filter::new()
                        .id(event_id)
                        .limit(1);

                    if let Ok(events) = client.fetch_events(
                        event_filter,
                        std::time::Duration::from_secs(5)
                    ).await {
                        if let Some(event) = events.into_iter().next() {
                            let author_pubkey = event.pubkey;
                            embedded_event.set(Some(event));

                            // Fetch author metadata
                            let metadata_filter = Filter::new()
                                .author(author_pubkey)
                                .kind(Kind::Metadata)
                                .limit(1);

                            if let Ok(metadata_events) = client.fetch_events(
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
                    }
                }
            });
        });

        // Render embedded note card
        let has_event = embedded_event.read().is_some();
        let event_clone = embedded_event.read().clone();
        let metadata_clone = author_metadata.read().clone();

        if has_event {
            rsx! {
                {render_embedded_note(&event_clone.unwrap(), metadata_clone.as_ref())}
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
    // Handle youtube.com/watch?v=ID
    if let Some(start) = url.find("v=") {
        let id_start = start + 2;
        let id = &url[id_start..];
        let id = id.split('&').next()?;
        return Some(id.to_string());
    }

    // Handle youtu.be/ID
    if url.contains("youtu.be/") {
        if let Some(start) = url.find("youtu.be/") {
            let id_start = start + 9;
            let id = &url[id_start..];
            let id = id.split('?').next()?;
            return Some(id.to_string());
        }
    }

    None
}

#[component]
fn ArticleMentionRenderer(mention: String) -> Element {
    // Extract the identifier from "nostr:naddr..." or just "naddr..."
    let identifier = mention.strip_prefix("nostr:").unwrap_or(&mention);

    // Parse the naddr coordinate
    if let Ok(nip19_coord) = nostr_sdk::nips::nip19::Nip19Coordinate::from_bech32(identifier) {
        let pubkey_str = nip19_coord.public_key.to_hex();
        let identifier_str = nip19_coord.identifier.clone();
        let naddr_for_link = identifier.to_string();

        let mut article_event = use_signal(|| None::<Event>);
        let mut author_metadata = use_signal(|| None::<Metadata>);
        let mut loading = use_signal(|| true);

        // Fetch the article
        use_effect(move || {
            let pubkey = pubkey_str.clone();
            let ident = identifier_str.clone();

            spawn(async move {
                loading.set(true);

                if let Some(client) = nostr_client::NOSTR_CLIENT.read().as_ref() {
                    // Fetch article by coordinate
                    match crate::stores::nostr_client::fetch_article_by_coordinate(
                        pubkey.clone(),
                        ident
                    ).await {
                        Ok(Some(event)) => {
                            let author_pubkey = event.pubkey;
                            article_event.set(Some(event));

                            // Fetch author metadata
                            let metadata_filter = Filter::new()
                                .author(author_pubkey)
                                .kind(Kind::Metadata)
                                .limit(1);

                            if let Ok(metadata_events) = client.fetch_events(
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
                }

                loading.set(false);
            });
        });

        // Render embedded article preview
        let has_article = article_event.read().is_some();
        let article_clone = article_event.read().clone();
        let metadata_clone = author_metadata.read().clone();

        if has_article {
            rsx! {
                {render_embedded_article(&article_clone.unwrap(), metadata_clone.as_ref(), &naddr_for_link)}
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
