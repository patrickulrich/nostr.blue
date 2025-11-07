use dioxus::prelude::*;
use nostr_sdk::Client;
use nostr_sdk::prelude::*;
use nostr::Url;
use std::sync::Arc;
use std::time::Duration;

#[cfg(target_arch = "wasm32")]
use nostr_indexeddb::WebDatabase;

use crate::stores::signer::SignerType;
use crate::stores::relay_metadata;
use crate::utils::mention_extractor::{extract_mentioned_pubkeys, create_mention_tags};

/// Global Nostr client instance
pub static NOSTR_CLIENT: GlobalSignal<Option<Arc<Client>>> = Signal::global(|| None);

/// Whether the client has finished initializing
pub static CLIENT_INITIALIZED: GlobalSignal<bool> = Signal::global(|| false);

/// Whether the client has a signer attached (can publish events)
pub static HAS_SIGNER: GlobalSignal<bool> = Signal::global(|| false);

/// The current signer type (if any)
pub static CURRENT_SIGNER: GlobalSignal<Option<SignerType>> = Signal::global(|| None);

/// Relay connection status
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum RelayStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Relay information
#[derive(Clone, Debug)]
pub struct RelayInfo {
    pub url: String,
    pub status: RelayStatus,
}

/// Global relay pool state
pub static RELAY_POOL: GlobalSignal<Vec<RelayInfo>> = Signal::global(Vec::new);

/// Default relays to connect to
const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.snort.social",
    "wss://nostr.wine",
    "wss://relay.nostr.band",
];

/// Initialize the Nostr client and connect to relays
pub async fn initialize_client() -> Result<Arc<Client>, String> {
    log::info!("Initializing Nostr client with IndexedDB...");

    // Configure relay options for better performance
    let relay_opts = RelayOptions::new()
        // Skip relays with average latency > 2 seconds
        .max_avg_latency(Some(Duration::from_secs(2)))
        // Verify that events match subscription filters
        .verify_subscriptions(true)
        // Ban relays that send mismatched events
        .ban_relay_on_mismatch(true)
        // Adjust retry interval based on success rate
        .adjust_retry_interval(true)
        // Initial retry interval: 10 seconds
        .retry_interval(Duration::from_secs(10))
        // Enable automatic reconnection
        .reconnect(true);

    // Create client with database
    #[cfg(target_arch = "wasm32")]
    let client = {
        // Open IndexedDB database
        let database = WebDatabase::open("nostr-blue-db")
            .await
            .map_err(|e| {
                log::error!("Failed to open IndexedDB: {}", e);
                format!("Failed to open IndexedDB: {}", e)
            })?;

        log::info!("IndexedDB opened successfully");

        // Enable gossip with in-memory storage
        // NostrGossipMemory is WASM-compatible and provides automatic relay routing
        let gossip = nostr_gossip_memory::store::NostrGossipMemory::unbounded();
        Client::builder()
            .database(database)
            .gossip(gossip)
            .build()
    };

    #[cfg(not(target_arch = "wasm32"))]
    let client = Client::builder().build();

    let client = Arc::new(client);

    // Add default relays with options (will be replaced if user has kind 10002)
    let mut relay_infos = Vec::new();
    for relay_url in DEFAULT_RELAYS {
        if let Ok(url) = Url::parse(relay_url) {
            match client.pool().add_relay(url.clone(), relay_opts.clone()).await {
                Ok(_) => {
                    relay_infos.push(RelayInfo {
                        url: relay_url.to_string(),
                        status: RelayStatus::Connected,
                    });
                    log::debug!("Added relay with opts: {}", relay_url);
                }
                Err(e) => {
                    log::error!("Failed to add relay {}: {}", relay_url, e);
                    relay_infos.push(RelayInfo {
                        url: relay_url.to_string(),
                        status: RelayStatus::Disconnected,
                    });
                }
            }
        }
    }

    RELAY_POOL.write().clone_from(&relay_infos);

    log::debug!("Connecting to relays...");
    client.connect().await;

    *NOSTR_CLIENT.write() = Some(client.clone());
    *CLIENT_INITIALIZED.write() = true;

    log::info!("Nostr client with IndexedDB initialized successfully");
    Ok(client)
}

/// Get the current client instance
pub fn get_client() -> Option<Arc<Client>> {
    NOSTR_CLIENT.read().clone()
}

/// Check if the client has a signer attached
#[allow(dead_code)]
pub fn has_signer() -> bool {
    *HAS_SIGNER.read()
}

/// Get the current signer
pub fn get_signer() -> Option<SignerType> {
    CURRENT_SIGNER.read().clone()
}

/// Initialize client with a signer (enables publishing)
pub async fn set_signer(signer: SignerType) -> Result<(), String> {
    log::info!("Setting signer: {}", signer.backend_name());

    // Get existing client - don't recreate!
    let client = get_client().ok_or("Client not initialized")?;

    // Just update the signer, keep all relay connections
    let nostr_signer = signer.as_nostr_signer();
    client.set_signer(nostr_signer).await;

    *HAS_SIGNER.write() = true;
    *CURRENT_SIGNER.write() = Some(signer.clone());

    // Load user's relay lists (kind 10002/10050) in background
    let client_clone = client.clone();
    spawn(async move {
        if let Err(e) = relay_metadata::init_user_relay_lists(client_clone.clone()).await {
            log::warn!("Failed to load user relay lists: {}", e);
        } else {
            // Apply relay lists to client
            if let Err(e) = apply_relay_lists_to_client(client_clone).await {
                log::error!("Failed to apply relay lists: {}", e);
            }
        }
    });

    log::info!("Signer updated successfully");
    Ok(())
}

/// Apply user's relay lists to the client connections
async fn apply_relay_lists_to_client(client: Arc<Client>) -> Result<(), String> {
    let metadata = relay_metadata::USER_RELAY_METADATA
        .read()
        .clone()
        .ok_or("No relay metadata available")?;

    log::info!("Applying {} relays from kind 10002 to client", metadata.relays.len());

    // Add user's configured relays with read/write flags
    for relay in &metadata.relays {
        if let Ok(url) = RelayUrl::parse(&relay.url) {
            let result = match (relay.read, relay.write) {
                (true, true) => {
                    client.add_relay(url.clone()).await.map_err(|e| e.to_string())
                }
                (true, false) => {
                    client.add_read_relay(url.clone()).await.map_err(|e| e.to_string())
                }
                (false, true) => {
                    client.add_write_relay(url.clone()).await.map_err(|e| e.to_string())
                }
                _ => continue, // Skip invalid configurations
            };

            match result {
                Ok(_) => log::info!("Added relay from kind 10002: {} (read: {}, write: {})",
                    relay.url, relay.read, relay.write),
                Err(e) => log::warn!("Failed to add relay {}: {}", relay.url, e),
            }
        }
    }

    // Wait for newly added relays to connect
    log::info!("Waiting for user's relays to connect...");
    client.connect().await;

    // Update RELAY_POOL to reflect ALL connected relays (defaults + user's relays)
    let pool_relays = client.pool().relays().await;
    let mut relay_infos = Vec::new();
    for (url, _relay) in pool_relays {
        relay_infos.push(RelayInfo {
            url: url.to_string(),
            status: RelayStatus::Connected,
        });
    }

    log::info!("Updating RELAY_POOL with {} total connected relays", relay_infos.len());
    RELAY_POOL.write().clone_from(&relay_infos);

    log::info!("Relay lists applied successfully");
    Ok(())
}

/// Switch to read-only mode (removes signer)
pub async fn set_read_only() -> Result<(), String> {
    log::info!("Switching to read-only mode");

    // Get existing client
    let client = get_client().ok_or("Client not initialized")?;

    // Remove signer
    client.unset_signer().await;

    *HAS_SIGNER.write() = false;
    *CURRENT_SIGNER.write() = None;

    log::info!("Switched to read-only mode");
    Ok(())
}

/// Add a custom relay
#[allow(dead_code)]
pub async fn add_relay(relay_url: &str) -> Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    let url = Url::parse(relay_url).map_err(|e| format!("Invalid URL: {}", e))?;

    client.add_relay(url).await.map_err(|e| e.to_string())?;

    // Update relay pool state
    let mut relays = RELAY_POOL.write();
    relays.push(RelayInfo {
        url: relay_url.to_string(),
        status: RelayStatus::Connecting,
    });

    log::info!("Added relay: {}", relay_url);
    Ok(())
}

/// Remove a relay
#[allow(dead_code)]
pub async fn remove_relay(relay_url: &str) -> Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    let url = Url::parse(relay_url).map_err(|e| format!("Invalid URL: {}", e))?;

    client.remove_relay(url).await.map_err(|e| e.to_string())?;

    // Update relay pool state
    let mut relays = RELAY_POOL.write();
    relays.retain(|r| r.url != relay_url);

    log::info!("Removed relay: {}", relay_url);
    Ok(())
}

/// Disconnect from all relays
#[allow(dead_code)]
pub async fn disconnect() {
    if let Some(client) = get_client() {
        client.disconnect().await;
        log::info!("Disconnected from all relays");
    }
}

/// Reconnect to all relays
#[allow(dead_code)]
pub async fn reconnect() {
    if let Some(client) = get_client() {
        client.connect().await;
        log::info!("Reconnected to relays");
    }
}

/// Fetch events using aggregated pattern: database first, then relays
///
/// This function:
/// 1. Queries local IndexedDB cache first (instant)
/// 2. If cache hit, returns immediately and syncs in background
/// 3. If cache miss, fetches from relays
pub async fn fetch_events_aggregated(
    filter: Filter,
    timeout: Duration,
) -> Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    // Try database first (fast)
    match client.database().query(filter.clone()).await {
        Ok(db_events) => {
            let db_count = db_events.len();
            if db_count > 0 {
                log::info!("Loaded {} events from IndexedDB cache", db_count);

                // Start background relay sync for updates
                let client_clone = client.clone();
                let filter_clone = filter.clone();
                spawn(async move {
                    if let Err(e) = client_clone.fetch_events(filter_clone, timeout).await {
                        log::warn!("Background relay sync failed: {}", e);
                    }
                });

                return Ok(db_events.into_iter().collect());
            }
        }
        Err(e) => {
            log::warn!("Database query failed: {}, falling back to relays", e);
        }
    }

    // Fallback to relays if DB is empty or failed
    log::info!("Fetching from relays (database empty or failed)");
    client
        .fetch_events(filter, timeout)
        .await
        .map(|events| events.into_iter().collect())
        .map_err(|e| e.to_string())
}

/// Fetch events using gossip (automatic relay routing)
pub async fn fetch_events_aggregated_outbox(
    filter: Filter,
    timeout: Duration,
) -> Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    // Use gossip for automatic relay routing
    client.fetch_events(filter, timeout).await
        .map(|events| events.into_iter().collect())
        .map_err(|e| format!("Failed to fetch events: {}", e))
}

/// Publish a text note (kind 1 event)
pub async fn publish_note(content: String, tags: Vec<Vec<String>>) -> Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing note with {} characters", content.len());

    // Extract mentions from content and create p tags
    let mentioned_pubkeys = extract_mentioned_pubkeys(&content);
    let mut mention_tags = create_mention_tags(&mentioned_pubkeys);
    log::debug!("Extracted {} mentions from content", mentioned_pubkeys.len());

    // Track tagged pubkeys for Outbox routing (currently unused but prepared for future outbox implementation)
    let mut _tagged_pubkeys: Vec<PublicKey> = mentioned_pubkeys.clone();

    // Convert tags to nostr Tag format
    use nostr::Tag;
    use nostr_sdk::nips::nip10::Marker;
    let nostr_tags: Vec<Tag> = tags
        .into_iter()
        .filter_map(|tag_vec| {
            if tag_vec.is_empty() {
                return None;
            }
            // Convert string vector to Tag
            match tag_vec[0].as_str() {
                "e" if tag_vec.len() >= 4 && !tag_vec[3].is_empty() => {
                    // E-tag with marker (for threading)
                    let event_id = nostr::EventId::from_hex(&tag_vec[1]).ok()?;

                    // Parse marker from 4th element (NIP-10: only "root" and "reply")
                    let marker = match tag_vec[3].as_str() {
                        "root" => Some(Marker::Root),
                        "reply" => Some(Marker::Reply),
                        _ => None,
                    };

                    if let Some(m) = marker {
                        // Parse optional relay URL (3rd element)
                        let relay_url = if !tag_vec[2].is_empty() {
                            nostr_sdk::RelayUrl::parse(&tag_vec[2]).ok()
                        } else {
                            None
                        };

                        // Construct event tag with marker
                        let tag_standard = nostr::TagStandard::Event {
                            event_id,
                            relay_url,
                            marker: Some(m),
                            public_key: None,
                            uppercase: false,
                        };

                        Some(Tag::from(tag_standard))
                    } else {
                        // Invalid marker, fallback to simple event tag
                        Some(Tag::event(event_id))
                    }
                },
                "e" if tag_vec.len() >= 2 => {
                    // Simple e-tag without marker
                    Some(Tag::event(
                        nostr::EventId::from_hex(&tag_vec[1]).ok()?
                    ))
                },
                "p" if tag_vec.len() >= 2 => {
                    // Extract pubkey for Outbox routing (currently unused but prepared for future)
                    if let Ok(pubkey) = nostr::PublicKey::from_hex(&tag_vec[1]) {
                        _tagged_pubkeys.push(pubkey);
                        Some(Tag::public_key(pubkey))
                    } else {
                        None
                    }
                },
                _ => {
                    // Generic tag
                    Some(Tag::custom(
                        nostr::TagKind::Custom(tag_vec[0].clone().into()),
                        tag_vec[1..].to_vec()
                    ))
                }
            }
        })
        .collect();

    // Combine mention tags with other tags
    mention_tags.extend(nostr_tags);

    // Build the event
    let builder = nostr::EventBuilder::text_note(&content).tags(mention_tags);

    // Publish using gossip - automatic relay routing
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish: {}", e))?;

    let event_id = output.id().to_hex();
    log::info!("Note published successfully: {}", event_id);
    Ok(event_id)
}

/// Publish a reaction (kind 7 event) to another event
/// NIP-25: https://github.com/nostr-protocol/nips/blob/master/25.md
pub async fn publish_reaction(
    event_id: String,
    event_author: String,
    content: String,
) -> Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing reaction to event: {}", event_id);

    // Parse event ID and author pubkey
    use nostr::{EventId, PublicKey, Tag, Kind};
    let target_event_id = EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let target_pubkey = PublicKey::from_hex(&event_author)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // TODO: Consider using EventBuilder::reaction() with ReactionTarget for better tag support
    // This would require passing the event kind to automatically include 'a' and 'k' tags
    // Build reaction event (kind 7) manually for now
    // NIP-25: Must include 'e' tag for event, should include 'p' tag for author
    let tags = vec![
        Tag::event(target_event_id),
        Tag::public_key(target_pubkey),
    ];

    let builder = nostr::EventBuilder::new(Kind::Reaction, content).tags(tags);

    // Publish using gossip - automatic relay routing
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish reaction: {}", e))?;

    let reaction_id = output.id().to_hex();
    log::info!("Reaction published successfully: {}", reaction_id);
    Ok(reaction_id)
}

/// Fetch a user's contact list (kind 3 event)
/// NIP-02: https://github.com/nostr-protocol/nips/blob/master/02.md
pub async fn fetch_contacts(pubkey_str: String) -> Result<Vec<String>, String> {
    log::info!("Fetching contacts for: {}", pubkey_str);

    // Parse pubkey
    use nostr::{PublicKey, Filter, Kind};
    let pubkey = PublicKey::from_hex(&pubkey_str)
        .or_else(|_| PublicKey::parse(&pubkey_str))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Create filter for kind 3 (contact list)
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::ContactList)
        .limit(1);

    // Fetch from database/relays using aggregated pattern
    match fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                // Extract pubkeys from 'p' tags
                let mut contacts = Vec::new();
                for tag in event.tags.iter() {
                    // Check if this is a p-tag (public key tag)
                    if tag.kind() == nostr::TagKind::p() {
                        if let Some(pubkey_str) = tag.content() {
                            contacts.push(pubkey_str.to_string());
                        }
                    }
                }
                log::info!("Found {} contacts", contacts.len());
                Ok(contacts)
            } else {
                log::info!("No contact list found");
                Ok(Vec::new())
            }
        }
        Err(e) => {
            log::error!("Failed to fetch contacts: {}", e);
            Err(format!("Failed to fetch contacts: {}", e))
        }
    }
}

/// Publish a contact list (kind 3 event)
/// NIP-02: https://github.com/nostr-protocol/nips/blob/master/02.md
pub async fn publish_contacts(contacts: Vec<String>) -> Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing contact list with {} contacts", contacts.len());

    // Build tags for each contact
    use nostr::{PublicKey, Tag, Kind};
    let mut tags = Vec::new();
    for contact_str in contacts {
        if let Ok(pubkey) = PublicKey::from_hex(&contact_str)
            .or_else(|_| PublicKey::parse(&contact_str)) {
            tags.push(Tag::public_key(pubkey));
        }
    }

    // Build and publish the event
    let builder = nostr::EventBuilder::new(Kind::ContactList, "").tags(tags);

    match client.send_event_builder(builder).await {
        Ok(output) => {
            let event_id = output.id().to_string();
            log::info!("Contact list published successfully: {}", event_id);
            Ok(event_id)
        }
        Err(e) => {
            log::error!("Failed to publish contact list: {}", e);
            Err(format!("Failed to publish contact list: {}", e))
        }
    }
}

/// Follow a user (adds to contact list and publishes)
pub async fn follow_user(pubkey_to_follow: String) -> Result<(), String> {
    // Normalize pubkey to canonical hex format
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_to_follow)?;

    // Get current user's pubkey
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    // Fetch current contacts
    let mut contacts = fetch_contacts(current_pubkey.clone()).await?;

    // Add new contact if not already following
    if !contacts.contains(&normalized_pubkey) {
        contacts.push(normalized_pubkey.clone());
        log::info!("Following new user: {}", normalized_pubkey);

        // Publish updated contact list
        publish_contacts(contacts).await?;
    } else {
        log::info!("Already following: {}", normalized_pubkey);
    }

    Ok(())
}

/// Unfollow a user (removes from contact list and publishes)
pub async fn unfollow_user(pubkey_to_unfollow: String) -> Result<(), String> {
    // Normalize pubkey to canonical hex format
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey_to_unfollow)?;

    // Get current user's pubkey
    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    // Fetch current contacts
    let mut contacts = fetch_contacts(current_pubkey.clone()).await?;

    // Remove contact if following
    if let Some(pos) = contacts.iter().position(|x| x == &normalized_pubkey) {
        contacts.remove(pos);
        log::info!("Unfollowing user: {}", normalized_pubkey);

        // Publish updated contact list
        publish_contacts(contacts).await?;
    } else {
        log::info!("Not following: {}", normalized_pubkey);
    }

    Ok(())
}

/// Check if current user is following a specific pubkey
pub async fn is_following(pubkey: String) -> Result<bool, String> {
    // Normalize pubkey to canonical hex format
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;

    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    let contacts = fetch_contacts(current_pubkey).await?;
    Ok(contacts.contains(&normalized_pubkey))
}

/// Publish a repost (kind 6 event) of another event
/// NIP-18: https://github.com/nostr-protocol/nips/blob/master/18.md
pub async fn publish_repost(
    event_id: String,
    event_author: String,
    relay_url: Option<String>,
) -> Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing repost of event: {}", event_id);

    // Parse event ID and author pubkey
    use nostr::{EventId, PublicKey, Tag, Kind, TagKind};
    let _target_event_id = EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let target_pubkey = PublicKey::from_hex(&event_author)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Build repost event (kind 6) manually
    // NIP-18: Must include 'e' tag with relay, should include 'p' tag
    let relay = relay_url.unwrap_or_else(|| DEFAULT_RELAYS[0].to_string());

    let tags = vec![
        Tag::custom(TagKind::e(), vec![event_id, relay]),
        Tag::public_key(target_pubkey),
    ];

    let builder = nostr::EventBuilder::new(Kind::Repost, "").tags(tags);

    // Publish using gossip - automatic relay routing
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish repost: {}", e))?;

    let repost_id = output.id().to_hex();
    log::info!("Repost published successfully: {}", repost_id);
    Ok(repost_id)
}

/// Fetch articles (kind 30023 - NIP-23 long-form content)
/// Returns events sorted by created_at descending (newest first)
pub async fn fetch_articles(
    limit: usize,
    until: Option<u64>,
) -> Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    log::info!("Fetching articles with limit: {}", limit);

    use nostr::{Filter, Kind, Timestamp};

    let mut filter = Filter::new()
        .kind(Kind::LongFormTextNote)
        .limit(limit);

    if let Some(until_timestamp) = until {
        filter = filter.until(Timestamp::from(until_timestamp));
    }

    match client.fetch_events(filter, std::time::Duration::from_secs(10)).await {
        Ok(events) => {
            let mut sorted: Vec<_> = events.into_iter().collect();
            sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            log::info!("Fetched {} articles", sorted.len());
            Ok(sorted)
        }
        Err(e) => {
            log::error!("Failed to fetch articles: {}", e);
            Err(format!("Failed to fetch articles: {}", e))
        }
    }
}

/// Fetch a specific article by coordinate (kind:pubkey:identifier)
pub async fn fetch_article_by_coordinate(
    pubkey: String,
    identifier: String,
) -> Result<Option<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    log::info!("Fetching article {}:{}", pubkey, identifier);

    use nostr::{Filter, Kind, PublicKey};

    let author = PublicKey::from_hex(&pubkey)
        .or_else(|_| PublicKey::parse(&pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::LongFormTextNote)
        .author(author)
        .identifier(identifier)
        .limit(1);

    match client.fetch_events(filter, std::time::Duration::from_secs(10)).await {
        Ok(events) => {
            Ok(events.into_iter().next())
        }
        Err(e) => {
            log::error!("Failed to fetch article: {}", e);
            Err(format!("Failed to fetch article: {}", e))
        }
    }
}

/// Publish profile metadata (Kind 0)
///
/// Updates the user's Nostr profile with the provided metadata
pub async fn publish_metadata(metadata: Metadata) -> Result<String, String> {
    let client = NOSTR_CLIENT.read();
    let client = client.as_ref().ok_or("Client not initialized")?;

    let signer_type = get_signer().ok_or("No signer available")?;

    log::info!("Publishing profile metadata");

    // Build and sign event based on signer type
    let event = match signer_type {
        crate::stores::signer::SignerType::Keys(keys) => {
            EventBuilder::metadata(&metadata)
                .sign(&keys)
                .await
                .map_err(|e| format!("Failed to sign metadata event: {}", e))?
        }
        #[cfg(target_family = "wasm")]
        crate::stores::signer::SignerType::BrowserExtension(signer) => {
            EventBuilder::metadata(&metadata)
                .sign(signer.as_ref())
                .await
                .map_err(|e| format!("Failed to sign metadata event: {}", e))?
        }
        crate::stores::signer::SignerType::NostrConnect(nostr_connect) => {
            EventBuilder::metadata(&metadata)
                .sign(nostr_connect.as_ref())
                .await
                .map_err(|e| format!("Failed to sign metadata event: {}", e))?
        }
    };

    // Publish to relays
    client.send_event(&event).await
        .map_err(|e| format!("Failed to publish metadata: {}", e))?;

    log::info!("Metadata published successfully");

    // Return event ID
    Ok(event.id.to_hex())
}

/// Update just the profile picture
#[allow(dead_code)]
pub async fn update_profile_picture(url: String) -> Result<(), String> {
    // Fetch current metadata
    let pubkey_str = crate::stores::auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let current_metadata = crate::stores::profiles::get_profile(&pubkey_str)
        .unwrap_or_default();

    // Validate URL by parsing it, then convert back to String
    let _validated_url = Url::parse(&url)
        .map_err(|e| format!("Invalid picture URL: {}", e))?;

    // Update picture field
    let updated_metadata = Metadata {
        picture: Some(url),
        ..current_metadata
    };

    publish_metadata(updated_metadata).await?;
    Ok(())
}

/// Update just the profile banner
#[allow(dead_code)]
pub async fn update_profile_banner(url: String) -> Result<(), String> {
    // Fetch current metadata
    let pubkey_str = crate::stores::auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    let current_metadata = crate::stores::profiles::get_profile(&pubkey_str)
        .unwrap_or_default();

    // Validate URL by parsing it, then convert back to String
    let _validated_url = Url::parse(&url)
        .map_err(|e| format!("Invalid banner URL: {}", e))?;

    // Update banner field
    let updated_metadata = Metadata {
        banner: Some(url),
        ..current_metadata
    };

    publish_metadata(updated_metadata).await?;
    Ok(())
}

/// Publish a long-form article (Kind 30023)
/// NIP-23: https://github.com/nostr-protocol/nips/blob/master/23.md
pub async fn publish_article(
    title: String,
    summary: String,
    content: String,
    identifier: String,
    cover_image: String,
    hashtags: Vec<String>,
) -> Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Get signer pubkey for the 'a' tag
    let signer = get_signer().ok_or("No signer available")?;
    let pubkey = signer.public_key().await?;

    log::info!("Publishing article: {}", title);

    // Build tags
    use nostr::Tag;
    use nostr_sdk::nips::nip01::Coordinate;

    let mut tags = vec![
        Tag::identifier(identifier.clone()),
        Tag::title(title),
        // Add 'a' tag for addressable event: <kind>:<pubkey>:<d-identifier>
        Tag::coordinate(
            Coordinate::new(
                nostr::Kind::from(30023),
                pubkey,
            ).identifier(identifier),
            None, // relay_url
        ),
    ];

    // Add optional summary
    if !summary.is_empty() {
        tags.push(Tag::custom(
            nostr::TagKind::Custom("summary".into()),
            vec![summary]
        ));
    }

    // Add optional cover image
    if !cover_image.is_empty() {
        tags.push(Tag::custom(
            nostr::TagKind::Custom("image".into()),
            vec![cover_image]
        ));
    }

    // Add published_at timestamp
    tags.push(Tag::custom(
        nostr::TagKind::Custom("published_at".into()),
        vec![format!("{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs())]
    ));

    // Add hashtags
    for hashtag in hashtags {
        tags.push(Tag::hashtag(hashtag));
    }

    // Build the event (Kind 30023 - LongFormTextNote)
    let builder = nostr::EventBuilder::new(nostr::Kind::from(30023), content)
        .tags(tags);

    // Publish
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish article: {}", e))?;

    let event_id = output.id().to_hex();
    log::info!("Article published successfully: {}", event_id);
    Ok(event_id)
}

/// Detect MIME type from URL file extension
fn detect_mime_type(url: &str) -> Option<String> {
    let url_lower = url.to_lowercase();

    // Extract extension from URL (handles query params)
    let path = url_lower.split('?').next()?;
    let extension = path.split('.').last()?;

    match extension {
        "jpg" | "jpeg" => Some("image/jpeg".to_string()),
        "png" => Some("image/png".to_string()),
        "gif" => Some("image/gif".to_string()),
        "webp" => Some("image/webp".to_string()),
        "svg" => Some("image/svg+xml".to_string()),
        "bmp" => Some("image/bmp".to_string()),
        "ico" => Some("image/x-icon".to_string()),
        "tiff" | "tif" => Some("image/tiff".to_string()),
        "avif" => Some("image/avif".to_string()),
        "heic" | "heif" => Some("image/heic".to_string()),
        _ => None,
    }
}

/// Publish a picture post (Kind 20)
/// NIP-68: https://github.com/nostr-protocol/nips/blob/master/68.md
pub async fn publish_picture(
    title: String,
    caption: String,
    image_urls: Vec<String>,
    hashtags: Vec<String>,
    location: String,
) -> Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    if image_urls.is_empty() {
        return Err("At least one image is required".to_string());
    }

    log::info!("Publishing picture post: {}", title);

    // Build tags
    use nostr::Tag;
    let mut tags = vec![
        Tag::title(title),
    ];

    // Add imeta tags for each image
    // Detect MIME type from extension or omit if unknown
    for url in &image_urls {
        let mut imeta_fields = vec![format!("url {}", url)];

        // Add MIME type if we can detect it from the extension
        if let Some(mime_type) = detect_mime_type(url) {
            imeta_fields.push(format!("m {}", mime_type));
        }

        tags.push(Tag::custom(
            nostr::TagKind::Custom("imeta".into()),
            imeta_fields
        ));
    }

    // Add location if provided
    if !location.is_empty() {
        tags.push(Tag::custom(
            nostr::TagKind::Custom("location".into()),
            vec![location]
        ));
    }

    // Add hashtags
    for hashtag in hashtags {
        tags.push(Tag::hashtag(hashtag));
    }

    // Build the event (Kind 20 - Picture)
    let builder = nostr::EventBuilder::new(nostr::Kind::from(20), caption)
        .tags(tags);

    // Publish
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish picture: {}", e))?;

    let event_id = output.id().to_hex();
    log::info!("Picture published successfully: {}", event_id);
    Ok(event_id)
}

/// Publish a video post (Kind 21 for landscape, Kind 22 for portrait)
/// NIP-71: https://github.com/nostr-protocol/nips/blob/master/71.md
pub async fn publish_video(
    title: String,
    description: String,
    video_url: String,
    thumbnail_url: String,
    hashtags: Vec<String>,
    is_portrait: bool,
) -> Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Validate required fields
    if video_url.trim().is_empty() {
        return Err("Video URL is required".to_string());
    }

    if title.trim().is_empty() {
        return Err("Title is required".to_string());
    }

    let kind = if is_portrait { 22 } else { 21 };
    log::info!("Publishing video (kind {}): {}", kind, title);

    // Build tags
    use nostr::Tag;
    let mut tags = vec![
        Tag::title(title.clone()),
        Tag::custom(
            nostr::TagKind::Custom("url".into()),
            vec![video_url.clone()]
        ),
    ];

    // Add thumbnail if provided
    if !thumbnail_url.is_empty() {
        tags.push(Tag::custom(
            nostr::TagKind::Custom("thumb".into()),
            vec![thumbnail_url]
        ));
    }

    // Add summary (description)
    if !description.is_empty() {
        tags.push(Tag::custom(
            nostr::TagKind::Custom("summary".into()),
            vec![description.clone()]
        ));
    }

    // Add hashtags
    for hashtag in hashtags {
        tags.push(Tag::hashtag(hashtag));
    }

    // Content includes title and video URL
    let content = if description.is_empty() {
        format!("{}\n\n{}", title, video_url)
    } else {
        format!("{}\n\n{}\n\n{}", title, description, video_url)
    };

    // Build the event
    let builder = nostr::EventBuilder::new(nostr::Kind::from(kind), content)
        .tags(tags);

    // Publish
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish video: {}", e))?;

    let event_id = output.id().to_hex();
    log::info!("Video published successfully: {}", event_id);
    Ok(event_id)
}
