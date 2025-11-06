use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey};
use std::time::Duration;

use crate::stores::{auth_store, nostr_client};
use crate::utils::list_kinds::LIST_KINDS;

/// User list data structure
#[derive(Clone, Debug, PartialEq)]
pub struct UserList {
    pub id: String,
    pub kind: u16,
    pub name: String,
    pub description: String,
    pub identifier: String,
    pub tags: Vec<nostr_sdk::Tag>,
    pub created_at: u64,
    pub author: String,
    pub event: Event,
}

impl UserList {
    /// Create a UserList from a Nostr event
    pub fn from_event(event: Event) -> Option<Self> {
        // Must have a 'd' tag (identifier)
        let identifier = event.tags.iter()
            .find(|tag| tag.kind() == nostr_sdk::TagKind::d())
            .and_then(|tag| tag.content())
            .map(|s| s.to_string())?;

        // Get title tag or use identifier as fallback
        let name = event.tags.iter()
            .find(|tag| {
                let vec = (*tag).clone().to_vec();
                vec.first().map(|s| s.as_str()) == Some("title")
            })
            .and_then(|tag| tag.content())
            .map(|s| s.to_string())
            .or_else(|| Some(identifier.clone()))
            .unwrap_or_else(|| "Untitled List".to_string());

        // Get description tag
        let description = event.tags.iter()
            .find(|tag| {
                let vec = (*tag).clone().to_vec();
                vec.first().map(|s| s.as_str()) == Some("description")
            })
            .and_then(|tag| tag.content())
            .map(|s| s.to_string())
            .unwrap_or_default();

        Some(UserList {
            id: event.id.to_string(),
            kind: event.kind.as_u16(),
            name,
            description,
            identifier,
            tags: event.tags.clone().to_vec(),
            created_at: event.created_at.as_secs(),
            author: event.pubkey.to_string(),
            event,
        })
    }
}

/// Hook to fetch all user lists (NIP-51)
/// Returns (lists, loading, error)
pub fn use_user_lists() -> (Signal<Vec<UserList>>, Signal<bool>, Signal<Option<String>>) {
    let mut lists = use_signal(|| Vec::<UserList>::new());
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    use_effect(move || {
        let auth = auth_store::AUTH_STATE.read();
        if !auth.is_authenticated {
            lists.set(Vec::new());
            return;
        }

        let pubkey_str = match &auth.pubkey {
            Some(pk) => pk.clone(),
            None => return,
        };

        loading.set(true);
        error.set(None);

        spawn(async move {
            match fetch_user_lists(&pubkey_str).await {
                Ok(fetched_lists) => {
                    lists.set(fetched_lists);
                    error.set(None);
                }
                Err(e) => {
                    log::error!("Failed to fetch user lists: {}", e);
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    (lists, loading, error)
}

/// Fetch user lists from relays
async fn fetch_user_lists(pubkey_str: &str) -> Result<Vec<UserList>, String> {
    let client = nostr_client::NOSTR_CLIENT.read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    let pubkey = PublicKey::parse(pubkey_str)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    log::info!("Fetching lists for {}", pubkey_str);

    // Build filter for NIP-51 list kinds
    let mut filter = Filter::new().author(pubkey);

    // Add all list kinds
    for &kind in LIST_KINDS {
        filter = filter.kind(Kind::from(kind));
    }

    // Fetch events
    let events = client.fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch events: {}", e))?;

    // Parse events into UserList objects
    let mut lists: Vec<UserList> = events.into_iter()
        .filter_map(UserList::from_event)
        .collect();

    // Sort by creation time (newest first)
    lists.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    log::info!("Fetched {} lists", lists.len());
    Ok(lists)
}

/// Delete a list by publishing a deletion event (kind 5)
pub async fn delete_list(event: &Event) -> Result<(), String> {
    use nostr_sdk::{EventBuilder, Kind, Tag, TagStandard};

    let client = nostr_client::NOSTR_CLIENT.read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();

    if !auth_store::is_authenticated() {
        return Err("Must be logged in to delete lists".to_string());
    }

    log::info!("Deleting list: {}", event.id);

    // Build deletion event (kind 5)
    let tags = vec![
        Tag::event(event.id),
        Tag::from_standardized(TagStandard::Kind {
            kind: event.kind,
            uppercase: false,
        }),
    ];

    let builder = EventBuilder::new(Kind::EventDeletion, "Deleted list").tags(tags);

    // Publish deletion event
    client.send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish deletion: {}", e))?;

    log::info!("List deleted successfully");
    Ok(())
}
