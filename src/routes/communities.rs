use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::routes::Route;
use nostr_sdk::{Event, Filter, Kind};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct Community {
    pub id: String,
    pub pubkey: String,
    pub d_tag: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    pub moderators: Vec<String>,
    pub event: Event,
    pub a_tag: String, // Format: "34550:pubkey:d_tag"
}

#[component]
pub fn Communities() -> Element {
    let mut communities = use_signal(|| Vec::<Community>::new());
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut search_query = use_signal(|| String::new());

    // Fetch communities on mount
    use_effect(move || {
        loading.set(true);
        error.set(None);

        spawn(async move {
            match fetch_communities().await {
                Ok(comms) => {
                    communities.set(comms);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    // Filter communities based on search
    let filtered_communities = use_memo(move || {
        let query = search_query.read().to_lowercase();
        if query.is_empty() {
            communities.read().clone()
        } else {
            communities.read()
                .iter()
                .filter(|c| {
                    c.name.as_ref().map(|n| n.to_lowercase().contains(&query)).unwrap_or(false) ||
                    c.description.as_ref().map(|d| d.to_lowercase().contains(&query)).unwrap_or(false) ||
                    c.d_tag.to_lowercase().contains(&query)
                })
                .cloned()
                .collect()
        }
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3",
                    h1 {
                        class: "text-xl font-bold flex items-center gap-2 mb-3",
                        span { "👥" }
                        "Communities"
                    }
                    p {
                        class: "text-sm text-muted-foreground mb-3",
                        "Discover communities and join the conversation"
                    }

                    // Search
                    div {
                        class: "relative",
                        span {
                            class: "absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground",
                            "🔍"
                        }
                        input {
                            class: "w-full pl-10 pr-4 py-2 border border-border rounded-lg bg-background focus:outline-none focus:ring-2 focus:ring-blue-500",
                            r#type: "text",
                            placeholder: "Search communities...",
                            value: "{search_query}",
                            oninput: move |evt| search_query.set(evt.value())
                        }
                    }
                }
            }

            // Content
            if *loading.read() {
                div {
                    class: "flex items-center justify-center py-20",
                    span {
                        class: "inline-block w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin"
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
                div {
                    class: "p-4",
                    div {
                        class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                        "❌ {err}"
                    }
                }
            } else if filtered_communities.read().is_empty() {
                div {
                    class: "flex flex-col items-center justify-center py-20 px-4 text-center",
                    div {
                        class: "text-6xl mb-4",
                        "👥"
                    }
                    h2 {
                        class: "text-2xl font-bold mb-2",
                        if !search_query.read().is_empty() {
                            "No communities found"
                        } else {
                            "No communities available"
                        }
                    }
                    p {
                        class: "text-muted-foreground max-w-sm",
                        if !search_query.read().is_empty() {
                            "Try a different search term"
                        } else {
                            "Connect to more relays to discover communities"
                        }
                    }
                }
            } else {
                div {
                    class: "p-4 space-y-4",
                    for community in filtered_communities.read().iter() {
                        CommunityCard { community: community.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn CommunityCard(community: Community) -> Element {
    let _a_tag_encoded = urlencoding::encode(&community.a_tag).to_string();

    rsx! {
        div {
            class: "border border-border rounded-lg p-4 hover:shadow-md transition-shadow bg-card",

            div {
                class: "flex items-start gap-3 mb-3",

                // Avatar/Image
                if let Some(image_url) = &community.image {
                    img {
                        class: "w-12 h-12 rounded-full object-cover",
                        src: "{image_url}",
                        alt: "Community image"
                    }
                } else {
                    div {
                        class: "w-12 h-12 rounded-full bg-gradient-to-br from-purple-400 to-blue-500 flex items-center justify-center text-white text-xl",
                        "👥"
                    }
                }

                // Info
                div {
                    class: "flex-1 min-w-0",
                    h3 {
                        class: "text-lg font-bold",
                        "{community.name.as_ref().unwrap_or(&community.d_tag)}"
                    }
                    p {
                        class: "text-sm text-muted-foreground",
                        "{community.d_tag}"
                    }
                }
            }

            // Description
            if let Some(desc) = &community.description {
                p {
                    class: "text-sm text-muted-foreground mb-3",
                    "{desc}"
                }
            }

            // Moderators count
            if !community.moderators.is_empty() {
                div {
                    class: "mb-3",
                    p {
                        class: "text-xs text-muted-foreground",
                        "👤 {community.moderators.len()} moderator"
                        if community.moderators.len() != 1 { "s" }
                    }
                }
            }

            // View button
            div {
                class: "pt-3 border-t border-border",
                Link {
                    to: Route::CommunityPage { a_tag: community.a_tag.clone() },
                    class: "w-full flex items-center justify-between px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg font-medium transition",
                    span {
                        class: "flex items-center gap-2",
                        "👥 View Community"
                    }
                    span { "→" }
                }
            }
        }
    }
}

// Fetch all communities from relays
async fn fetch_communities() -> Result<Vec<Community>, String> {
    log::info!("Fetching communities...");

    // Fetch kind 34550 (NIP-72 community definitions)
    let filter = Filter::new()
        .kind(Kind::Custom(34550))
        .limit(100);

    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Fetched {} community events", events.len());

            let mut communities: Vec<Community> = events.into_iter()
                .filter_map(|event| parse_community_event(event))
                .collect();

            // Sort by creation date (newest first)
            communities.sort_by(|a, b| b.event.created_at.cmp(&a.event.created_at));

            Ok(communities)
        }
        Err(e) => {
            log::error!("Failed to fetch communities: {}", e);
            Err(format!("Failed to fetch communities: {}", e))
        }
    }
}

// Parse a community event into a Community struct
pub fn parse_community_event(event: Event) -> Option<Community> {
    use nostr_sdk::{TagKind, SingleLetterTag, Alphabet};

    // Extract d tag (identifier)
    let d_tag = event.tags.iter()
        .find(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::D)))
        .and_then(|t| t.content())
        .map(|s| s.to_string())?;

    // Extract optional multi-letter tags by checking tag.as_slice()
    let name = event.tags.iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("name")
        })
        .and_then(|t| {
            let slice = t.as_slice();
            slice.get(1).map(|v| v.to_string())
        });

    let description = event.tags.iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("description")
        })
        .and_then(|t| {
            let slice = t.as_slice();
            slice.get(1).map(|v| v.to_string())
        });

    let image = event.tags.iter()
        .find(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("image")
        })
        .and_then(|t| {
            let slice = t.as_slice();
            slice.get(1).map(|v| v.to_string())
        });

    // Extract moderators (p tags)
    // Note: In NIP-72, moderators are marked with "moderator" as 4th element
    let moderators: Vec<String> = event.tags.iter()
        .filter(|t| t.kind() == TagKind::p())
        .filter(|t| {
            // Check if 4th element is "moderator"
            let slice = t.as_slice();
            slice.get(3).map(|s| s.as_str()) == Some("moderator")
        })
        .filter_map(|t| t.content().map(|s| s.to_string()))
        .collect();

    // Create a_tag in format "34550:pubkey:d_tag"
    let a_tag = format!("34550:{}:{}", event.pubkey.to_hex(), d_tag);

    Some(Community {
        id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        d_tag: d_tag.clone(),
        name,
        description,
        image,
        moderators,
        event,
        a_tag,
    })
}
