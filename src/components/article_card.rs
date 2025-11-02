use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, PublicKey};
use nostr_sdk::prelude::NostrDatabaseExt;
use crate::routes::Route;
use crate::stores::nostr_client::get_client;
use crate::utils::article_meta::{
    get_title, get_summary, get_image, get_published_at,
    get_hashtags, get_identifier, calculate_read_time
};
use std::time::Duration;

#[component]
pub fn ArticleCard(event: NostrEvent) -> Element {
    // Extract article metadata
    let title = get_title(&event);
    let summary = get_summary(&event);
    let image_url = get_image(&event);
    let published_at = get_published_at(&event);
    let hashtags = get_hashtags(&event);
    let identifier = get_identifier(&event);
    let read_time = calculate_read_time(&event.content);

    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_fetch = author_pubkey.clone();

    // State for author profile
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // Fetch author's profile metadata
    use_effect(move || {
        let pubkey_str = author_pubkey_for_fetch.clone();

        spawn(async move {
            let pubkey = match PublicKey::from_hex(&pubkey_str) {
                Ok(pk) => pk,
                Err(_) => return,
            };

            let client = match get_client() {
                Some(c) => c,
                None => return,
            };

            // Check database first (instant, no network)
            if let Ok(Some(metadata)) = client.database().metadata(pubkey).await {
                author_metadata.set(Some(metadata));
                return;
            }

            // If not in database, fetch from relays (auto-caches to database)
            if let Ok(Some(metadata)) = client.fetch_metadata(pubkey, Duration::from_secs(5)).await {
                author_metadata.set(Some(metadata));
            }
        });
    });

    // Format timestamp
    let timestamp = format_timestamp(published_at);

    // Get display name from metadata or fallback
    let display_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            if author_pubkey.len() > 16 {
                format!("{}...{}", &author_pubkey[..8], &author_pubkey[author_pubkey.len()-8..])
            } else {
                author_pubkey.clone()
            }
        });

    let profile_picture = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone());

    // Generate avatar fallback (first letter of display name)
    let avatar_letter = display_name.chars().next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    // Check if article has valid identifier (required for linking)
    let _has_identifier = identifier.is_some();

    // Create naddr for linking to article detail using proper NIP-19 encoding
    let naddr_opt = identifier.clone().and_then(|id| {
        use nostr::prelude::*;

        let coord = Coordinate::new(
            event.kind,
            event.pubkey
        ).identifier(id);

        // Get relay URLs from global state (or use empty vec for now)
        let relays = vec![]; // TODO: Could add relay hints from RELAY_POOL

        let nip19_coord = Nip19Coordinate::new(coord, relays);
        nip19_coord.to_bech32().ok()
    });

    // Show first 3 hashtags
    let displayed_tags: Vec<String> = hashtags.iter().take(3).cloned().collect();

    // Generate content preview if no summary
    let preview_text = if let Some(sum) = summary {
        sum
    } else {
        // Create preview from content (first 150 characters)
        let content = event.content.clone();
        let char_count = content.chars().count();
        if char_count > 150 {
            let truncated: String = content.chars().take(150).collect();
            if let Some(last_space) = truncated.rfind(' ') {
                format!("{}...", &truncated[..last_space])
            } else {
                format!("{}...", truncated)
            }
        } else {
            content
        }
    };

    rsx! {
        div {
            class: "group bg-card rounded-lg border border-border overflow-hidden hover:border-primary/50 transition-all duration-200 hover:shadow-lg",

            // Link wrapper - only clickable if has identifier
            if let Some(naddr) = naddr_opt {
                Link {
                    to: Route::ArticleDetail { naddr: naddr.clone() },
                    class: "block",

                    // Cover image
                    if let Some(img_url) = image_url {
                        div {
                            class: "aspect-video w-full bg-muted overflow-hidden",
                            img {
                                src: "{img_url}",
                                alt: "{title}",
                                class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200",
                                loading: "lazy",
                            }
                        }
                    }

                    // Card content
                    div {
                        class: "p-4 space-y-3",

                        // Hashtags
                        if !displayed_tags.is_empty() {
                            div {
                                class: "flex flex-wrap gap-2",
                                for tag in displayed_tags {
                                    span {
                                        class: "px-2 py-1 text-xs rounded-full bg-primary/10 text-primary font-medium",
                                        "#{tag}"
                                    }
                                }
                            }
                        }

                        // Title
                        h3 {
                            class: "text-xl font-bold line-clamp-2 group-hover:text-primary transition-colors",
                            "{title}"
                        }

                        // Preview/Summary
                        p {
                            class: "text-sm text-muted-foreground line-clamp-3",
                            "{preview_text}"
                        }

                        // Author info
                        div {
                            class: "flex items-center justify-between pt-2",

                            // Author avatar and name
                            div {
                                class: "flex items-center gap-2",

                                // Avatar
                                Link {
                                    to: Route::Profile { pubkey: author_pubkey.clone() },
                                    onclick: move |e: Event<MouseData>| {
                                        e.stop_propagation();
                                    },
                                    class: "flex-shrink-0",

                                    div {
                                        class: "w-8 h-8 rounded-full overflow-hidden bg-muted flex items-center justify-center",
                                        if let Some(pic_url) = profile_picture {
                                            img {
                                                src: "{pic_url}",
                                                alt: "{display_name}",
                                                class: "w-full h-full object-cover",
                                                loading: "lazy",
                                            }
                                        } else {
                                            span {
                                                class: "text-xs font-semibold text-muted-foreground",
                                                "{avatar_letter}"
                                            }
                                        }
                                    }
                                }

                                // Name and time
                                div {
                                    class: "flex flex-col min-w-0",
                                    Link {
                                        to: Route::Profile { pubkey: author_pubkey },
                                        onclick: move |e: Event<MouseData>| {
                                            e.stop_propagation();
                                        },
                                        class: "text-sm font-medium hover:underline truncate",
                                        "{display_name}"
                                    }
                                    span {
                                        class: "text-xs text-muted-foreground",
                                        "{timestamp}"
                                    }
                                }
                            }

                            // Read time
                            div {
                                class: "flex items-center gap-1 text-xs text-muted-foreground flex-shrink-0",
                                span { "📄" }
                                span { "{read_time} min read" }
                            }
                        }
                    }
                }
            } else {
                // Show warning if no identifier
                div {
                    class: "p-4 bg-yellow-500/10 border-l-4 border-yellow-500",
                    p {
                        class: "text-sm text-yellow-700 dark:text-yellow-300",
                        "⚠️ This article is missing a required identifier (d tag) and cannot be displayed properly."
                    }
                }
            }
        }
    }
}

/// Skeleton loader for article cards
#[component]
pub fn ArticleCardSkeleton() -> Element {
    rsx! {
        div {
            class: "bg-card rounded-lg border border-border overflow-hidden animate-pulse",

            // Image skeleton
            div {
                class: "aspect-video w-full bg-muted",
            }

            // Content skeleton
            div {
                class: "p-4 space-y-3",

                // Tags skeleton
                div {
                    class: "flex gap-2",
                    div { class: "h-6 w-16 bg-muted rounded-full" }
                    div { class: "h-6 w-20 bg-muted rounded-full" }
                }

                // Title skeleton
                div { class: "h-6 bg-muted rounded w-3/4" }
                div { class: "h-6 bg-muted rounded w-1/2" }

                // Summary skeleton
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-full" }
                div { class: "h-4 bg-muted rounded w-2/3" }

                // Author skeleton
                div {
                    class: "flex items-center justify-between pt-2",
                    div {
                        class: "flex items-center gap-2",
                        div { class: "w-8 h-8 rounded-full bg-muted" }
                        div {
                            div { class: "h-4 w-24 bg-muted rounded mb-1" }
                            div { class: "h-3 w-16 bg-muted rounded" }
                        }
                    }
                    div { class: "h-4 w-20 bg-muted rounded" }
                }
            }
        }
    }
}

/// Format timestamp to relative time
fn format_timestamp(timestamp: u64) -> String {
    use chrono::{DateTime, Utc, Local};

    let dt = DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or_else(|| Utc::now());
    let local_dt = dt.with_timezone(&Local);
    let now = Local::now();
    let duration = now.signed_duration_since(local_dt);

    if duration.num_seconds() < 60 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        let mins = duration.num_minutes();
        format!("{}m ago", mins)
    } else if duration.num_hours() < 24 {
        let hours = duration.num_hours();
        format!("{}h ago", hours)
    } else if duration.num_days() < 30 {
        let days = duration.num_days();
        format!("{}d ago", days)
    } else if duration.num_days() < 365 {
        let months = duration.num_days() / 30;
        format!("{}mo ago", months)
    } else {
        let years = duration.num_days() / 365;
        format!("{}y ago", years)
    }
}
