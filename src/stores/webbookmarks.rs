use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, EventBuilder, PublicKey, Timestamp};
use nostr::prelude::{WebBookmark, TagStandard, TagKind};
use crate::stores::{auth_store, nostr_client};
use std::time::Duration;

/// Global signal to track web bookmarks (kind 39701)
pub static WEB_BOOKMARKS: GlobalSignal<Vec<Event>> = Signal::global(|| Vec::new());

/// Add a new web bookmark
///
/// # Arguments
/// * `url` - The URL (with or without https:// - will be stripped)
/// * `title` - Optional title
/// * `description` - Optional description (stored in .content)
/// * `image_url` - Optional image URL (stored in custom tag)
/// * `published_at` - Optional published timestamp
/// * `hashtags` - Optional list of hashtags/tags
pub async fn add_webbookmark(
    url: String,
    title: Option<String>,
    description: Option<String>,
    image_url: Option<String>,
    published_at: Option<u64>,
    hashtags: Vec<String>,
) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Strip https:// or http:// from URL as per NIP-B0 spec
    let url_without_scheme = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string();

    log::info!("Creating web bookmark for URL: {}", url_without_scheme);

    // Build the WebBookmark using the SDK builder
    let mut bookmark = WebBookmark::new(
        description.unwrap_or_default(),
        url_without_scheme.clone()
    );

    // Add optional fields
    if let Some(t) = title {
        bookmark = bookmark.title(t);
    }

    if let Some(ts) = published_at {
        bookmark = bookmark.published_at(Timestamp::from(ts));
    }

    // Add hashtags (call hashtags() for each tag)
    for tag in hashtags {
        bookmark = bookmark.hashtags(tag);
    }

    // Build the event
    let mut builder = EventBuilder::web_bookmark(bookmark);

    // Add custom image tag if provided (not part of standard NIP-B0 but useful)
    if let Some(img) = image_url {
        use nostr_sdk::Tag;
        builder = builder.tag(Tag::custom(
            nostr_sdk::TagKind::custom("image"),
            vec![img]
        ));
    }

    // Publish the event
    match client.send_event_builder(builder).await {
        Ok(output) => {
            log::info!("Web bookmark published: {}", output.id());
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to publish web bookmark: {}", e);
            Err(format!("Failed to publish web bookmark: {}", e))
        }
    }
}

/// Update an existing web bookmark (creates new event with same URL)
///
/// Since web bookmarks are addressable events (same URL = same 'd' tag),
/// publishing a new event with the same URL will replace the old one.
pub async fn update_webbookmark(
    url: String,
    title: Option<String>,
    description: Option<String>,
    image_url: Option<String>,
    published_at: Option<u64>,
    hashtags: Vec<String>,
) -> Result<(), String> {
    // For addressable events, updating is the same as adding
    add_webbookmark(url, title, description, image_url, published_at, hashtags).await
}

/// Delete a web bookmark by publishing a deletion event
pub async fn delete_webbookmark(event: &Event) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    log::info!("Deleting web bookmark: {}", event.id);

    // Create deletion event (kind 5)
    use nostr::nips::nip09::EventDeletionRequest;
    let request = EventDeletionRequest::new().id(event.id);
    let builder = EventBuilder::delete(request);

    match client.send_event_builder(builder).await {
        Ok(_) => {
            log::info!("Web bookmark deleted");

            // Remove from local state immediately
            let mut bookmarks = WEB_BOOKMARKS.read().clone();
            bookmarks.retain(|e| e.id != event.id);
            *WEB_BOOKMARKS.write() = bookmarks;

            Ok(())
        }
        Err(e) => {
            log::error!("Failed to delete web bookmark: {}", e);
            Err(format!("Failed to delete web bookmark: {}", e))
        }
    }
}

/// Toggle favorite status for a web bookmark
/// This adds/removes a "favorite" hashtag to the bookmark
pub async fn toggle_favorite(event: &Event, is_favorite: bool) -> Result<(), String> {
    // Extract current metadata
    let url = get_url(event).ok_or("Bookmark missing URL")?;
    let title = get_title(event);
    let description = if event.content.is_empty() { None } else { Some(event.content.clone()) };
    let image_url = get_image(event);
    let published_at = get_published_at(event).map(|ts| ts.as_secs());

    // Get current hashtags and modify favorite status
    let mut hashtags = get_hashtags(event);
    if is_favorite {
        if !hashtags.contains(&"favorite".to_string()) {
            hashtags.push("favorite".to_string());
        }
    } else {
        hashtags.retain(|tag| tag != "favorite");
    }

    // Update the bookmark with new hashtags
    update_webbookmark(
        url,
        title,
        description,
        image_url,
        published_at,
        hashtags
    ).await
}

/// Toggle archived status for a web bookmark
/// This adds/removes an "archived" hashtag to the bookmark
#[allow(dead_code)]
pub async fn toggle_archived(event: &Event, is_archived: bool) -> Result<(), String> {
    // Extract current metadata
    let url = get_url(event).ok_or("Bookmark missing URL")?;
    let title = get_title(event);
    let description = if event.content.is_empty() { None } else { Some(event.content.clone()) };
    let image_url = get_image(event);
    let published_at = get_published_at(event).map(|ts| ts.as_secs());

    // Get current hashtags and modify archived status
    let mut hashtags = get_hashtags(event);
    if is_archived {
        if !hashtags.contains(&"archived".to_string()) {
            hashtags.push("archived".to_string());
        }
    } else {
        hashtags.retain(|tag| tag != "archived");
    }

    // Update the bookmark with new hashtags
    update_webbookmark(
        url,
        title,
        description,
        image_url,
        published_at,
        hashtags
    ).await
}

/// Check if a bookmark is favorited
pub fn is_favorite(event: &Event) -> bool {
    get_hashtags(event).contains(&"favorite".to_string())
}

/// Check if a bookmark is archived
pub fn is_archived(event: &Event) -> bool {
    get_hashtags(event).contains(&"archived".to_string())
}

/// Get URL from web bookmark event (without scheme as per NIP-B0)
pub fn get_url(event: &Event) -> Option<String> {
    event.tags.identifier().map(|s| s.to_string())
}

/// Get title from web bookmark event
pub fn get_title(event: &Event) -> Option<String> {
    event.tags
        .find_standardized(TagKind::Title)
        .and_then(|tag| match tag {
            TagStandard::Title(t) => Some(t.to_string()),
            _ => None,
        })
}

/// Get published_at timestamp from web bookmark event
pub fn get_published_at(event: &Event) -> Option<Timestamp> {
    event.tags
        .find_standardized(TagKind::PublishedAt)
        .and_then(|tag| match tag {
            TagStandard::PublishedAt(ts) => Some(*ts),
            _ => None,
        })
}

/// Get hashtags from web bookmark event (excluding special tags like 'favorite' and 'archived')
pub fn get_hashtags(event: &Event) -> Vec<String> {
    event.tags.hashtags()
        .map(|s| s.to_string())
        .collect()
}

/// Get hashtags for display (excluding special system tags)
pub fn get_display_hashtags(event: &Event) -> Vec<String> {
    get_hashtags(event)
        .into_iter()
        .filter(|tag| tag != "favorite" && tag != "archived")
        .collect()
}

/// Get image URL from custom image tag
pub fn get_image(event: &Event) -> Option<String> {
    event.tags.iter()
        .find(|tag| tag.kind() == nostr_sdk::TagKind::custom("image"))
        .and_then(|tag| tag.content().map(|s| s.to_string()))
}

/// Extract domain from URL (for display purposes)
pub fn get_domain(url: &str) -> String {
    // Remove scheme if present
    let without_scheme = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");

    // Get domain part (before first /)
    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_string()
}

/// Load web bookmarks from followed users with pagination
pub async fn load_following_webbookmarks(until: Option<u64>, limit: usize) -> Result<Vec<Event>, String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    log::info!("Loading following web bookmarks (until: {:?})", until);

    // Fetch the user's contact list (people they follow)
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to empty", e);
            return Ok(Vec::new());
        }
    };

    if contacts.is_empty() {
        log::info!("User doesn't follow anyone");
        return Ok(Vec::new());
    }

    log::info!("User follows {} accounts", contacts.len());

    // Parse contact pubkeys
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }

    if authors.is_empty() {
        log::warn!("No valid contact pubkeys");
        return Ok(Vec::new());
    }

    // Create filter for web bookmarks from followed users
    let mut filter = Filter::new()
        .kind(Kind::WebBookmark)
        .authors(authors)
        .limit(limit);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    log::info!("Fetching web bookmarks from {} followed accounts", filter.authors.as_ref().map(|a| a.len()).unwrap_or(0));

    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let mut bookmarks: Vec<Event> = events.into_iter().collect();
            bookmarks.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            log::info!("Loaded {} web bookmarks from following", bookmarks.len());
            Ok(bookmarks)
        }
        Err(e) => {
            log::error!("Failed to fetch following web bookmarks: {}", e);
            Err(format!("Failed to fetch following web bookmarks: {}", e))
        }
    }
}

/// Load global web bookmarks with pagination
pub async fn load_global_webbookmarks(until: Option<u64>, limit: usize) -> Result<Vec<Event>, String> {
    log::info!("Loading global web bookmarks (until: {:?})", until);

    let mut filter = Filter::new()
        .kind(Kind::WebBookmark)
        .limit(limit);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let mut bookmarks: Vec<Event> = events.into_iter().collect();
            bookmarks.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            log::info!("Loaded {} global web bookmarks", bookmarks.len());
            Ok(bookmarks)
        }
        Err(e) => {
            log::error!("Failed to fetch global web bookmarks: {}", e);
            Err(format!("Failed to fetch global web bookmarks: {}", e))
        }
    }
}
