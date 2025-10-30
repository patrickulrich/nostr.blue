use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, PublicKey, Filter, Kind};
use crate::routes::Route;
use crate::stores::nostr_client::get_client;
use crate::stores::bookmarks;
use crate::components::{ArticleContent, icons::*};
use crate::utils::article_meta::{
    get_title, get_summary, get_image, get_published_at,
    get_hashtags, calculate_read_time
};
use std::time::Duration;

#[component]
pub fn ArticleDetail(naddr: String) -> Element {
    // State for the article
    let mut article = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // Decode naddr and fetch article
    use_effect(move || {
        let naddr_str = naddr.clone();

        spawn(async move {
            loading.set(true);
            error.set(None);

            // Decode the naddr
            match decode_naddr(&naddr_str) {
                Ok((pubkey, identifier)) => {
                    // Fetch the article
                    match crate::stores::nostr_client::fetch_article_by_coordinate(
                        pubkey.clone(),
                        identifier
                    ).await {
                        Ok(Some(event)) => {
                            // Fetch author metadata
                            if let Some(client) = get_client() {
                                if let Ok(pk) = PublicKey::from_hex(&pubkey) {
                                    let filter = Filter::new()
                                        .author(pk)
                                        .kind(Kind::Metadata)
                                        .limit(1);

                                    if let Ok(events) = client.fetch_events(filter, Duration::from_secs(5)).await {
                                        if let Some(meta_event) = events.into_iter().next() {
                                            if let Ok(metadata) = serde_json::from_str::<nostr_sdk::Metadata>(&meta_event.content) {
                                                author_metadata.set(Some(metadata));
                                            }
                                        }
                                    }
                                }
                            }

                            article.set(Some(event));
                            loading.set(false);
                        }
                        Ok(None) => {
                            error.set(Some("Article not found".to_string()));
                            loading.set(false);
                        }
                        Err(e) => {
                            error.set(Some(e));
                            loading.set(false);
                        }
                    }
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    let is_loading = *loading.read();
    let error_msg = error.read();
    let article_data = article.read();

    rsx! {
        div {
            class: "min-h-screen",

            // Back button header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3",
                    Link {
                        to: Route::Articles {},
                        class: "inline-flex items-center gap-2 text-sm hover:text-primary transition",
                        span { "←" }
                        span { "Back to Articles" }
                    }
                }
            }

            // Content
            div {
                class: "max-w-4xl mx-auto px-4 py-8",

                // Loading state
                if is_loading {
                    div {
                        class: "space-y-4 animate-pulse",
                        div { class: "h-12 bg-muted rounded w-3/4" }
                        div { class: "h-6 bg-muted rounded w-1/2" }
                        div { class: "h-64 bg-muted rounded" }
                        div { class: "h-4 bg-muted rounded w-full" }
                        div { class: "h-4 bg-muted rounded w-full" }
                        div { class: "h-4 bg-muted rounded w-3/4" }
                    }
                }

                // Error state
                if let Some(err) = error_msg.as_ref() {
                    div {
                        class: "text-center py-12",
                        div {
                            class: "text-6xl mb-4",
                            "❌"
                        }
                        h3 {
                            class: "text-xl font-semibold mb-2",
                            "Error Loading Article"
                        }
                        p {
                            class: "text-muted-foreground mb-4",
                            "{err}"
                        }
                        Link {
                            to: Route::Articles {},
                            class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 inline-block",
                            "Back to Articles"
                        }
                    }
                }

                // Article content
                if let Some(event) = article_data.as_ref() {
                    {
                        let title = get_title(event);
                        let summary = get_summary(event);
                        let image_url = get_image(event);
                        let published_at = get_published_at(event);
                        let hashtags = get_hashtags(event);
                        let read_time = calculate_read_time(&event.content);
                        let author_pubkey = event.pubkey.to_hex();
                        let event_id = event.id.to_string();

                        let display_name = author_metadata.read().as_ref()
                            .and_then(|m| m.display_name.clone().or(m.name.clone()))
                            .unwrap_or_else(|| format!("{}...{}", &author_pubkey[..8], &author_pubkey[author_pubkey.len()-8..]));

                        let profile_picture = author_metadata.read().as_ref()
                            .and_then(|m| m.picture.clone());

                        let avatar_letter = display_name.chars().next()
                            .unwrap_or('?')
                            .to_uppercase()
                            .to_string();

                        let timestamp = format_timestamp(published_at);

                        rsx! {
                            article {
                                class: "space-y-6",

                                // Title
                                h1 {
                                    class: "text-4xl md:text-5xl font-bold leading-tight",
                                    "{title}"
                                }

                                // Summary
                                if let Some(sum) = summary {
                                    p {
                                        class: "text-xl text-muted-foreground leading-relaxed",
                                        "{sum}"
                                    }
                                }

                                // Author info and metadata
                                div {
                                    class: "flex items-center justify-between py-4 border-y border-border",

                                    // Author
                                    Link {
                                        to: Route::Profile { pubkey: author_pubkey.clone() },
                                        class: "flex items-center gap-3 hover:opacity-80 transition",

                                        // Avatar
                                        div {
                                            class: "w-12 h-12 rounded-full overflow-hidden bg-muted flex items-center justify-center",
                                            if let Some(pic_url) = profile_picture {
                                                img {
                                                    src: "{pic_url}",
                                                    alt: "{display_name}",
                                                    class: "w-full h-full object-cover",
                                                }
                                            } else {
                                                span {
                                                    class: "text-lg font-semibold text-muted-foreground",
                                                    "{avatar_letter}"
                                                }
                                            }
                                        }

                                        // Name and date
                                        div {
                                            div {
                                                class: "font-semibold",
                                                "{display_name}"
                                            }
                                            div {
                                                class: "text-sm text-muted-foreground",
                                                "{timestamp} · {read_time} min read"
                                            }
                                        }
                                    }
                                }

                                // Hashtags
                                if !hashtags.is_empty() {
                                    div {
                                        class: "flex flex-wrap gap-2",
                                        for tag in hashtags.clone() {
                                            Link {
                                                to: Route::Hashtag { tag: tag.clone() },
                                                class: "px-3 py-1 text-sm rounded-full bg-primary/10 text-primary font-medium hover:bg-primary/20 transition",
                                                "#{tag}"
                                            }
                                        }
                                    }
                                }

                                // Cover image
                                if let Some(img_url) = image_url {
                                    div {
                                        class: "rounded-lg overflow-hidden",
                                        img {
                                            src: "{img_url}",
                                            alt: "{title}",
                                            class: "w-full h-auto",
                                        }
                                    }
                                }

                                // Article content (markdown)
                                ArticleContent {
                                    content: event.content.clone(),
                                }

                                // Footer with action buttons
                                div {
                                    class: "pt-8 border-t border-border",
                                    div {
                                        class: "flex items-center justify-center gap-4",

                                        // Like button (placeholder - implement later)
                                        button {
                                            class: "flex items-center gap-2 px-4 py-2 rounded-lg hover:bg-accent transition",
                                            HeartIcon { class: "w-5 h-5" }
                                            span { "Like" }
                                        }

                                        // Bookmark button
                                        button {
                                            class: "flex items-center gap-2 px-4 py-2 rounded-lg hover:bg-accent transition",
                                            onclick: move |_| {
                                                let event_id_clone = event_id.clone();
                                                spawn(async move {
                                                    if bookmarks::is_bookmarked(&event_id_clone) {
                                                        let _ = bookmarks::unbookmark_event(event_id_clone).await;
                                                    } else {
                                                        let _ = bookmarks::bookmark_event(event_id_clone).await;
                                                    }
                                                });
                                            },
                                            BookmarkIcon { class: "w-5 h-5" }
                                            span {
                                                if bookmarks::is_bookmarked(&event_id) {
                                                    "Bookmarked"
                                                } else {
                                                    "Bookmark"
                                                }
                                            }
                                        }

                                        // Share button (placeholder - implement later)
                                        button {
                                            class: "flex items-center gap-2 px-4 py-2 rounded-lg hover:bg-accent transition",
                                            ShareIcon { class: "w-5 h-5" }
                                            span { "Share" }
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

/// Decode naddr to extract pubkey and identifier
fn decode_naddr(naddr: &str) -> Result<(String, String), String> {
    use nostr::nips::nip19::{Nip19Coordinate, FromBech32};

    // Decode naddr string to Nip19Coordinate
    // This preserves relay hints if present in the naddr
    match Nip19Coordinate::from_bech32(naddr) {
        Ok(nip19_coord) => {
            // Extract coordinate fields
            let pubkey = nip19_coord.public_key.to_hex();
            let identifier = nip19_coord.identifier.clone();

            // Log relay hints if present
            if !nip19_coord.relays.is_empty() {
                log::debug!("Article naddr contains {} relay hints", nip19_coord.relays.len());
                for relay in &nip19_coord.relays {
                    log::debug!("  Relay hint: {}", relay);
                }
            }

            Ok((pubkey, identifier))
        }
        Err(e) => Err(format!("Invalid naddr format: {}", e)),
    }
}

/// Format timestamp to human-readable string
fn format_timestamp(timestamp: u64) -> String {
    use chrono::{DateTime, Utc};

    let dt = DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or_else(|| Utc::now());

    dt.format("%B %d, %Y").to_string()
}
