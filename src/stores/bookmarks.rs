use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, EventBuilder, Tag, PublicKey};
use crate::stores::{auth_store, nostr_client};
use std::time::Duration;

/// Global signal to track bookmarked event IDs
pub static BOOKMARKED_EVENTS: GlobalSignal<Vec<String>> = Signal::global(|| Vec::new());

/// Initialize bookmarks by fetching from relays
pub async fn init_bookmarks() -> Result<(), String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey = PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Loading bookmarks for {}", pubkey_str);

    // Fetch bookmark list (kind 30001 with d tag "bookmark")
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(30001)) // Bookmarks list
        .identifier("bookmark") // NIP-51 bookmark identifier
        .limit(1);

    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                // Extract event IDs from 'e' tags
                let bookmarked: Vec<String> = event.tags.iter()
                    .filter(|tag| tag.kind() == nostr_sdk::TagKind::e())
                    .filter_map(|tag| tag.content().map(|s| s.to_string()))
                    .collect();

                log::info!("Loaded {} bookmarks", bookmarked.len());
                *BOOKMARKED_EVENTS.write() = bookmarked;
                Ok(())
            } else {
                log::info!("No bookmarks found");
                *BOOKMARKED_EVENTS.write() = Vec::new();
                Ok(())
            }
        }
        Err(e) => {
            log::error!("Failed to fetch bookmarks: {}", e);
            Err(format!("Failed to fetch bookmarks: {}", e))
        }
    }
}

/// Check if an event is bookmarked
pub fn is_bookmarked(event_id: &str) -> bool {
    BOOKMARKED_EVENTS.read().contains(&event_id.to_string())
}

/// Add event to bookmarks
pub async fn bookmark_event(event_id: String) -> Result<(), String> {
    let mut bookmarks = BOOKMARKED_EVENTS.read().clone();

    // Don't add if already bookmarked
    if bookmarks.contains(&event_id) {
        return Ok(());
    }

    bookmarks.push(event_id);

    // Publish updated bookmark list
    publish_bookmarks(bookmarks.clone()).await?;

    // Update local state
    *BOOKMARKED_EVENTS.write() = bookmarks;

    Ok(())
}

/// Remove event from bookmarks
pub async fn unbookmark_event(event_id: String) -> Result<(), String> {
    let mut bookmarks = BOOKMARKED_EVENTS.read().clone();

    // Remove the event ID
    bookmarks.retain(|id| id != &event_id);

    // Publish updated bookmark list
    publish_bookmarks(bookmarks.clone()).await?;

    // Update local state
    *BOOKMARKED_EVENTS.write() = bookmarks;

    Ok(())
}

/// Publish bookmarks list to relays (NIP-51)
async fn publish_bookmarks(bookmarks: Vec<String>) -> Result<(), String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    log::info!("Publishing {} bookmarks", bookmarks.len());

    // Build tags for bookmark list
    let mut tags = vec![
        Tag::identifier("bookmark"), // d tag
    ];

    for event_id in bookmarks {
        tags.push(Tag::event(
            nostr_sdk::EventId::from_hex(&event_id)
                .map_err(|e| format!("Invalid event ID: {}", e))?,
        ));
    }

    // Build and publish event
    let builder = EventBuilder::new(Kind::from(30001), "").tags(tags);

    match client.send_event_builder(builder).await {
        Ok(_) => {
            log::info!("Bookmarks published successfully");
            Ok(())
        }
        Err(e) => {
            log::error!("Failed to publish bookmarks: {}", e);
            Err(format!("Failed to publish bookmarks: {}", e))
        }
    }
}

/// Fetch all bookmarked events
pub async fn fetch_bookmarked_events() -> Result<Vec<Event>, String> {
    let bookmarks = BOOKMARKED_EVENTS.read().clone();

    if bookmarks.is_empty() {
        return Ok(Vec::new());
    }

    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    // Create filter for bookmarked events
    let event_ids: Result<Vec<nostr_sdk::EventId>, _> = bookmarks
        .iter()
        .map(|id| nostr_sdk::EventId::from_hex(id))
        .collect();

    let event_ids = event_ids.map_err(|e| format!("Invalid event ID: {}", e))?;

    let filter = Filter::new().ids(event_ids);

    match client.fetch_events(filter, Duration::from_secs(15)).await {
        Ok(events) => {
            let event_vec: Vec<Event> = events.into_iter().collect();
            log::info!("Fetched {} bookmarked events", event_vec.len());
            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch bookmarked events: {}", e);
            Err(format!("Failed to fetch bookmarked events: {}", e))
        }
    }
}
