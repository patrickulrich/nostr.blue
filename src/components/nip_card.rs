use dioxus::prelude::*;
use crate::routes::Route;
use crate::services::github_nips::OfficialNip;
use crate::utils::truncate_pubkey;

/// Card component for displaying an official NIP from GitHub
#[component]
pub fn OfficialNipCard(nip: OfficialNip) -> Element {
    let number = nip.number.clone();
    let title = nip.title.clone();
    let deprecated = nip.deprecated;
    let unrecommended = nip.unrecommended;

    rsx! {
        Link {
            to: Route::NipDetail { nip_id: number.clone() },
            class: "block group",

            div {
                class: "bg-card rounded-lg border border-border p-4 hover:border-primary/50 transition-all duration-200 hover:shadow-md",

                // Header with NIP number
                div {
                    class: "flex items-center justify-between mb-2",

                    span {
                        class: "text-sm font-mono text-primary font-bold",
                        "NIP-{number}"
                    }

                    // Status badges
                    div {
                        class: "flex gap-1",

                        if deprecated {
                            span {
                                class: "text-xs px-2 py-0.5 rounded-full bg-red-500/10 text-red-500",
                                "deprecated"
                            }
                        }

                        if unrecommended {
                            span {
                                class: "text-xs px-2 py-0.5 rounded-full bg-yellow-500/10 text-yellow-500",
                                "unrecommended"
                            }
                        }
                    }
                }

                // Title
                h3 {
                    class: "text-base font-medium text-foreground group-hover:text-primary transition-colors line-clamp-2",
                    "{title}"
                }
            }
        }
    }
}

/// Card component for displaying a custom NIP (kind 30817) from Nostr
#[component]
pub fn CustomNipCard(
    event: nostr_sdk::Event,
    #[props(default = None)] author_name: Option<String>,
    #[props(default = None)] author_picture: Option<String>,
) -> Element {
    use nostr_sdk::prelude::*;
    use crate::stores::nostr_client::get_client;

    // Extract metadata from event tags
    let identifier = event.tags.identifier().unwrap_or_default().to_string();
    let title = event.tags.iter()
        .find(|t| t.kind() == TagKind::Title)
        .and_then(|t| t.content().map(|s| s.to_string()))
        .unwrap_or_else(|| format!("Custom NIP: {}", identifier));

    // Extract related kinds from k tags
    let related_kinds: Vec<String> = event.tags.iter()
        .filter(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::K)))
        .filter_map(|t| t.content().map(|s| s.to_string()))
        .collect();

    let author_pubkey = event.pubkey.to_hex();
    let author_pubkey_for_fetch = author_pubkey.clone();

    // Clone author_name for use in effect and later
    let author_name_for_effect = author_name.clone();

    // State for author profile
    let mut author_metadata = use_signal(|| None::<Metadata>);

    // Fetch author's profile metadata if not provided
    // Only re-run when author pubkey changes (not every render)
    use_effect(use_reactive!(|(author_pubkey_for_fetch, author_name_for_effect)| {
        if author_name_for_effect.is_some() {
            return; // Skip fetch if already provided
        }

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

            // If not in database, fetch from relays
            if let Ok(Some(metadata)) = client.fetch_metadata(pubkey, std::time::Duration::from_secs(5)).await {
                author_metadata.set(Some(metadata));
            }
        });
    }));

    // Get display name from metadata or fallback (using UTF-8 safe truncation)
    let display_name = author_name.or_else(|| {
        author_metadata.read().as_ref()
            .and_then(|m| m.display_name.clone().or(m.name.clone()))
    }).unwrap_or_else(|| truncate_pubkey(&author_pubkey));

    let profile_picture = author_picture.clone().or_else(|| {
        author_metadata.read().as_ref()
            .and_then(|m| m.picture.clone())
    });

    // Generate avatar fallback
    let avatar_letter = display_name.chars().next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();

    // Generate naddr for linking
    let naddr = {
        let coord = Coordinate::new(event.kind, event.pubkey)
            .identifier(&identifier);
        let relays: Vec<RelayUrl> = vec![];
        Nip19Coordinate::new(coord, relays)
            .to_bech32()
            .unwrap_or_else(|_| identifier.clone())
    };

    // Format timestamp
    let timestamp = format_relative_time(event.created_at.as_secs());

    // Create preview from content
    let preview = {
        let content = &event.content;
        let char_count = content.chars().count();
        if char_count > 150 {
            let truncated: String = content.chars().take(150).collect();
            if let Some(last_space) = truncated.rfind(' ') {
                format!("{}...", &truncated[..last_space])
            } else {
                format!("{}...", truncated)
            }
        } else {
            content.clone()
        }
    };

    rsx! {
        Link {
            to: Route::NipDetail { nip_id: naddr.clone() },
            class: "block group",

            div {
                class: "bg-card rounded-lg border border-border p-4 hover:border-primary/50 transition-all duration-200 hover:shadow-md",

                // Header with author info
                div {
                    class: "flex items-center gap-3 mb-3",

                    // Avatar
                    if let Some(pic) = &profile_picture {
                        img {
                            src: "{pic}",
                            alt: "{display_name}",
                            class: "w-8 h-8 rounded-full object-cover",
                        }
                    } else {
                        div {
                            class: "w-8 h-8 rounded-full bg-primary/20 flex items-center justify-center text-sm font-medium",
                            "{avatar_letter}"
                        }
                    }

                    div {
                        class: "flex-1 min-w-0",
                        span {
                            class: "text-sm font-medium text-foreground truncate block",
                            "{display_name}"
                        }
                        span {
                            class: "text-xs text-muted-foreground",
                            "{timestamp}"
                        }
                    }

                    // Custom badge
                    span {
                        class: "text-xs px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-500",
                        "custom"
                    }
                }

                // Title
                h3 {
                    class: "text-base font-medium text-foreground group-hover:text-primary transition-colors mb-2 line-clamp-2",
                    "{title}"
                }

                // Preview text
                p {
                    class: "text-sm text-muted-foreground line-clamp-2 mb-3",
                    "{preview}"
                }

                // Related kinds tags
                if !related_kinds.is_empty() {
                    {
                        let extra_count = related_kinds.len().saturating_sub(3);
                        rsx! {
                            div {
                                class: "flex flex-wrap gap-1",
                                for kind in related_kinds.iter().take(3) {
                                    span {
                                        class: "text-xs px-2 py-0.5 rounded bg-muted text-muted-foreground font-mono",
                                        "kind:{kind}"
                                    }
                                }
                                if extra_count > 0 {
                                    span {
                                        class: "text-xs text-muted-foreground",
                                        "+{extra_count} more"
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

/// Skeleton component for loading state
#[component]
pub fn NipCardSkeleton() -> Element {
    rsx! {
        div {
            class: "bg-card rounded-lg border border-border p-4 animate-pulse",

            // Header skeleton
            div {
                class: "flex items-center gap-3 mb-3",
                div {
                    class: "w-8 h-8 rounded-full bg-muted",
                }
                div {
                    class: "flex-1",
                    div { class: "h-4 bg-muted rounded w-24 mb-1" }
                    div { class: "h-3 bg-muted rounded w-16" }
                }
            }

            // Title skeleton
            div { class: "h-5 bg-muted rounded w-3/4 mb-2" }

            // Preview skeleton
            div { class: "h-4 bg-muted rounded w-full mb-1" }
            div { class: "h-4 bg-muted rounded w-2/3" }
        }
    }
}

/// Format a timestamp as a relative time string
fn format_relative_time(timestamp_secs: u64) -> String {
    let now = instant::now() as u64 / 1000;
    let diff = now.saturating_sub(timestamp_secs);

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else if diff < 604800 {
        format!("{}d ago", diff / 86400)
    } else if diff < 2592000 {
        format!("{}w ago", diff / 604800)
    } else {
        // Format as date
        let date = chrono::DateTime::from_timestamp(timestamp_secs as i64, 0)
            .map(|dt| dt.format("%b %d, %Y").to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        date
    }
}
