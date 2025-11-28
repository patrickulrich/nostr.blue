use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use nostr_sdk::Client;
use nostr_sdk::prelude::*;
use nostr::Url;
use std::sync::Arc;
use std::sync::{OnceLock, Mutex};
use std::time::Duration;

#[cfg(target_arch = "wasm32")]
use nostr_indexeddb::WebDatabase;

use crate::stores::signer::SignerType;
use crate::stores::relay_metadata;
use crate::utils::mention_extractor::{extract_mentioned_pubkeys, create_mention_tags};

#[cfg(target_arch = "wasm32")]
use crate::services::admission_policy::NostrBlueAdmissionPolicy;

/// Global Nostr client instance
pub static NOSTR_CLIENT: GlobalSignal<Option<Arc<Client>>> = Signal::global(|| None);

/// Whether the client has finished initializing
pub static CLIENT_INITIALIZED: GlobalSignal<bool> = Signal::global(|| false);

/// Whether the client has a signer attached (can publish events)
pub static HAS_SIGNER: GlobalSignal<bool> = Signal::global(|| false);

/// The current signer type (if any)
pub static CURRENT_SIGNER: GlobalSignal<Option<SignerType>> = Signal::global(|| None);

/// Contacts cache for faster feed loading (5-minute TTL)
struct CachedContacts {
    pubkey: String,
    contacts: Vec<String>,
    cached_at: instant::Instant,
}

static CONTACTS_CACHE: OnceLock<Mutex<Option<CachedContacts>>> = OnceLock::new();

fn get_contacts_cache() -> &'static Mutex<Option<CachedContacts>> {
    CONTACTS_CACHE.get_or_init(|| Mutex::new(None))
}

/// Invalidate the contacts cache (call after follow/unfollow)
pub fn invalidate_contacts_cache() {
    // Use unwrap_or_else to recover from poisoned mutex instead of silently ignoring
    let mut cache = get_contacts_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *cache = None;
    log::debug!("Contacts cache invalidated");
}

/// Wait for at least one relay to be ready before fetching
/// This is needed because connect() is non-blocking and spawns background tasks
/// Ensure at least one relay is connected before fetching
/// Call this before any direct client.fetch_events() calls
pub async fn ensure_relays_ready(client: &Client) {
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;

    // First, check if any relay is already connected
    let relays = client.relays().await;
    let any_connected = relays.values().any(|r| r.status() == PoolRelayStatus::Connected);

    if any_connected {
        log::debug!("At least one relay is already connected, proceeding with fetch");
        return;
    }

    // No relays connected yet - call connect().await to actually establish connections
    // This is the key fix: in WASM, polling doesn't yield control to background tasks,
    // but connect().await properly drives the connection futures to completion
    log::info!("No relays connected, calling connect().await to establish connections...");
    client.connect().await;

    // Verify connection status after connect attempt
    let relays_after = client.relays().await;
    let connected_count = relays_after.values().filter(|r| r.status() == PoolRelayStatus::Connected).count();
    if connected_count == 0 {
        log::warn!("connect().await completed but no relays are connected - fetches may fail");
    } else {
        log::info!("connect().await completed, {} relay(s) connected", connected_count);
    }
}

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
/// Store for relay pool with fine-grained reactivity
#[derive(Clone, Debug, Default, Store)]
pub struct RelayPoolStore {
    pub data: Vec<RelayInfo>,
}

pub static RELAY_POOL: GlobalSignal<Store<RelayPoolStore>> = Signal::global(|| Store::new(RelayPoolStore::default()));

/// Default relays to connect to
const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.snort.social",
    "wss://nostr.wine",
    "wss://relay.nostr.band",
];

/// Initialize the Nostr client and connect to relays
pub async fn initialize_client() -> std::result::Result<Arc<Client>, String> {
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
            .admit_policy(NostrBlueAdmissionPolicy::default())
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

    RELAY_POOL.read().data().write().clone_from(&relay_infos);

    // Store client and mark initialized BEFORE connecting
    // This allows the UI to start loading while relays connect in background
    *NOSTR_CLIENT.write() = Some(client.clone());
    *CLIENT_INITIALIZED.write() = true;

    // Connect to relays in background - spawn the future so it gets polled to completion
    // In WASM, simply dropping the Future won't reliably execute it
    log::debug!("Spawning background relay connections...");
    #[cfg(target_arch = "wasm32")]
    {
        let client_for_connect = client.clone();
        wasm_bindgen_futures::spawn_local(async move {
            client_for_connect.connect().await;
            log::info!("Background relay connections completed");
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let client_for_connect = client.clone();
        tokio::spawn(async move {
            client_for_connect.connect().await;
            log::info!("Background relay connections completed (non-WASM)");
        });
    }

    log::info!("Nostr client initialized (relays connecting in background)");
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
pub async fn set_signer(signer: SignerType) -> std::result::Result<(), String> {
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
async fn apply_relay_lists_to_client(client: Arc<Client>) -> std::result::Result<(), String> {
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
    RELAY_POOL.read().data().write().clone_from(&relay_infos);

    log::info!("Relay lists applied successfully");
    Ok(())
}

/// Switch to read-only mode (removes signer)
pub async fn set_read_only() -> std::result::Result<(), String> {
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
pub async fn add_relay(relay_url: &str) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    let url = Url::parse(relay_url).map_err(|e| format!("Invalid URL: {}", e))?;

    client.add_relay(url).await.map_err(|e| e.to_string())?;

    // Update relay pool state
    let store = RELAY_POOL.read();
    let mut data = store.data();
    let mut relays = data.write();
    relays.push(RelayInfo {
        url: relay_url.to_string(),
        status: RelayStatus::Connecting,
    });

    log::info!("Added relay: {}", relay_url);
    Ok(())
}

/// Remove a relay
#[allow(dead_code)]
pub async fn remove_relay(relay_url: &str) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    let url = Url::parse(relay_url).map_err(|e| format!("Invalid URL: {}", e))?;

    client.remove_relay(url).await.map_err(|e| e.to_string())?;

    // Update relay pool state
    let store = RELAY_POOL.read();
    let mut data = store.data();
    let mut relays = data.write();
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
) -> std::result::Result<Vec<nostr::Event>, String> {
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

    // Wait for at least one relay to be ready (non-blocking connect() may not have finished)
    ensure_relays_ready(&client).await;

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
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    // Wait for at least one relay to be ready (non-blocking connect() may not have finished)
    ensure_relays_ready(&client).await;

    // Use gossip for automatic relay routing
    client.fetch_events(filter, timeout).await
        .map(|events| events.into_iter().collect())
        .map_err(|e| format!("Failed to fetch events: {}", e))
}

/// Fetch events from database only (instant, for initial display)
///
/// This is Phase 1 of profile loading - shows cached data immediately.
/// Call `fetch_profile_events_from_relays` afterward for fresh data.
pub async fn fetch_profile_events_db(
    filter: Filter,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    match client.database().query(filter).await {
        Ok(events) => {
            let count = events.len();
            log::info!("Profile DB: loaded {} events instantly", count);
            Ok(events.into_iter().collect())
        }
        Err(e) => {
            log::warn!("Profile DB query failed: {}", e);
            Ok(Vec::new()) // Return empty on error, relay fetch will get data
        }
    }
}

/// Fetch events from relays only (for background refresh)
///
/// This is Phase 2 of profile loading - fetches fresh data from relays.
/// Uses gossip/outbox routing for efficient relay selection.
pub async fn fetch_profile_events_from_relays(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    // Ensure relays are ready
    ensure_relays_ready(&client).await;

    // Use gossip for automatic relay routing (NIP-65 outbox)
    match client.fetch_events(filter, timeout).await {
        Ok(events) => {
            let count = events.len();
            log::info!("Profile relays: fetched {} events", count);
            Ok(events.into_iter().collect())
        }
        Err(e) => {
            log::warn!("Profile relay fetch failed: {}", e);
            Err(format!("Relay fetch failed: {}", e))
        }
    }
}

/// Publish a text note (kind 1 event)
pub async fn publish_note(content: String, tags: Vec<Vec<String>>) -> std::result::Result<String, String> {
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
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing reaction to event: {}", event_id);

    // Parse event ID and author pubkey
    use nostr::{EventId, PublicKey};
    use nostr::nips::nip25::ReactionTarget;
    use nostr_sdk::nips::nip01::Coordinate;

    let target_event_id = EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let target_pubkey = PublicKey::from_hex(&event_author)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Try to fetch the original event to get its kind and coordinate
    // This enables proper NIP-25 compliance with 'a' and 'k' tags
    let (event_kind, event_coordinate) = match client.database().event_by_id(&target_event_id).await {
        Ok(Some(event)) => {
            let kind = Some(event.kind);
            // For addressable events (30000-39999), include coordinate
            let coordinate = if event.kind.is_addressable() {
                // Use SDK's identifier() method for d-tag lookup
                event.tags.identifier().map(|id| Coordinate {
                    kind: event.kind,
                    public_key: event.pubkey,
                    identifier: id.to_string(),
                })
            } else {
                None
            };
            (kind, coordinate)
        }
        _ => (None, None), // If we can't fetch it, continue without kind/coordinate
    };

    // Use EventBuilder::reaction() with ReactionTarget for proper NIP-25 compliance
    let target = ReactionTarget {
        event_id: target_event_id,
        public_key: target_pubkey,
        coordinate: event_coordinate,
        kind: event_kind,
        relay_hint: None,
    };

    let builder = nostr::EventBuilder::reaction(target, content);

    // Publish using gossip - automatic relay routing
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish reaction: {}", e))?;

    let reaction_id = output.id().to_hex();
    log::info!("Reaction published successfully: {}", reaction_id);
    Ok(reaction_id)
}

/// Fetch a user's contact list (kind 3 event)
/// NIP-02: https://github.com/nostr-protocol/nips/blob/master/02.md
/// Uses a 5-minute cache to speed up repeated calls
pub async fn fetch_contacts(pubkey_str: String) -> std::result::Result<Vec<String>, String> {
    // Check cache first (5-minute TTL)
    {
        let cache = get_contacts_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(ref cached) = *cache {
            if cached.pubkey == pubkey_str
               && cached.cached_at.elapsed() < Duration::from_secs(300) {
                log::info!("Contacts cache hit ({} contacts)", cached.contacts.len());
                let contacts = cached.contacts.clone();
                drop(cache); // Release lock before spawning

                // Background refresh (don't await)
                let pk = pubkey_str.clone();
                spawn(async move {
                    let _ = fetch_contacts_from_relay(pk).await;
                });

                return Ok(contacts);
            }
        }
    }

    // Cache miss - fetch from relay
    fetch_contacts_from_relay(pubkey_str).await
}

/// Internal function to fetch contacts from relay and update cache
async fn fetch_contacts_from_relay(pubkey_str: String) -> std::result::Result<Vec<String>, String> {
    log::info!("Fetching contacts from relay for: {}", pubkey_str);

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
                // Use SDK's public_keys() method to extract p-tags
                let contacts: Vec<String> = event.tags.public_keys()
                    .map(|pk| pk.to_string())
                    .collect();
                log::info!("Found {} contacts from relay", contacts.len());

                // Update cache
                {
                    let mut cache = get_contacts_cache().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    *cache = Some(CachedContacts {
                        pubkey: pubkey_str,
                        contacts: contacts.clone(),
                        cached_at: instant::Instant::now(),
                    });
                }

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
pub async fn publish_contacts(contacts: Vec<String>) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing contact list with {} contacts", contacts.len());

    // Parse contacts into Contact structs for proper NIP-02 compliance
    use nostr::PublicKey;
    use nostr_sdk::nips::nip02::Contact;
    let contact_list: Vec<Contact> = contacts
        .into_iter()
        .filter_map(|contact_str| {
            // Try to parse as hex or NIP-19
            PublicKey::from_hex(&contact_str)
                .or_else(|_| PublicKey::parse(&contact_str))
                .ok()
                .map(|pubkey| Contact::new(pubkey))
        })
        .collect();

    log::info!("Parsed {} valid contacts", contact_list.len());

    // Use EventBuilder::contact_list() for proper NIP-02 compliance
    // This allows for relay URLs and petnames (aliases) to be added in the future
    let builder = nostr::EventBuilder::contact_list(contact_list);

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
pub async fn follow_user(pubkey_to_follow: String) -> std::result::Result<(), String> {
    // Invalidate contacts cache since we're modifying it
    invalidate_contacts_cache();

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
pub async fn unfollow_user(pubkey_to_unfollow: String) -> std::result::Result<(), String> {
    // Invalidate contacts cache since we're modifying it
    invalidate_contacts_cache();

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
pub async fn is_following(pubkey: String) -> std::result::Result<bool, String> {
    // Normalize pubkey to canonical hex format
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;

    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    let contacts = fetch_contacts(current_pubkey).await?;
    Ok(contacts.contains(&normalized_pubkey))
}

/// Fetch the mute list (kind 10000) from relays
/// NIP-51: https://github.com/nostr-protocol/nips/blob/master/51.md
async fn fetch_mute_list() -> std::result::Result<Option<nostr::Event>, String> {
    let _client = get_client().ok_or("Client not initialized")?;

    let current_pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;

    let pubkey = nostr::PublicKey::from_hex(&current_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = nostr::Filter::new()
        .author(pubkey)
        .kind(nostr::Kind::from(10000))
        .limit(1);

    // Fetch from database/relays
    match fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => Ok(events.into_iter().next()),
        Err(e) => {
            log::error!("Failed to fetch mute list: {}", e);
            Ok(None)
        }
    }
}

/// Get all muted event IDs
pub async fn get_muted_posts() -> std::result::Result<Vec<String>, String> {
    match fetch_mute_list().await? {
        Some(event) => {
            // Use SDK's event_ids() method to extract e-tags
            let muted_posts: Vec<String> = event.tags.event_ids()
                .map(|id| id.to_string())
                .collect();
            Ok(muted_posts)
        }
        None => Ok(Vec::new()),
    }
}

/// Get all blocked user pubkeys
pub async fn get_blocked_users() -> std::result::Result<Vec<String>, String> {
    match fetch_mute_list().await? {
        Some(event) => {
            // Use SDK's public_keys() method to extract p-tags
            let blocked_users: Vec<String> = event.tags.public_keys()
                .map(|pk| pk.to_string())
                .collect();
            Ok(blocked_users)
        }
        None => Ok(Vec::new()),
    }
}

/// Check if a post is muted
pub async fn is_post_muted(event_id: String) -> std::result::Result<bool, String> {
    let muted_posts = get_muted_posts().await?;
    Ok(muted_posts.contains(&event_id))
}

/// Check if a user is blocked
pub async fn is_user_blocked(pubkey: String) -> std::result::Result<bool, String> {
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;
    let blocked_users = get_blocked_users().await?;
    Ok(blocked_users.contains(&normalized_pubkey))
}

/// Mute a post (add to mute list kind 10000)
/// NIP-51: https://github.com/nostr-protocol/nips/blob/master/51.md
pub async fn mute_post(event_id: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Muting post: {}", event_id);

    // Parse event ID
    use nostr::EventId;
    let target_event_id = EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Fetch current mute list
    let mute_event = fetch_mute_list().await?;

    // Build new mute list
    let mut muted_posts = Vec::new();
    let mut blocked_users = Vec::new();
    let mut hashtags = Vec::new();
    let mut words = Vec::new();
    let mut other_tags = Vec::new(); // Preserve unknown/custom tags
    let mut existing_content = String::new(); // Preserve existing content

    if let Some(event) = mute_event {
        // Preserve the existing content before consuming the event
        existing_content = event.content.clone();

        // Extract existing muted posts, blocked users, hashtags, and words
        for tag in event.tags.iter() {
            if tag.kind() == nostr::TagKind::e() {
                if let Some(id) = tag.content() {
                    if let Ok(eid) = EventId::from_hex(id) {
                        muted_posts.push(eid);
                    }
                }
            } else if tag.kind() == nostr::TagKind::p() {
                if let Some(pk) = tag.content() {
                    if let Ok(pubkey) = nostr::PublicKey::from_hex(pk) {
                        blocked_users.push(pubkey);
                    }
                }
            } else if tag.kind() == nostr::TagKind::t() {
                // Hashtag tag
                if let Some(hashtag) = tag.content() {
                    hashtags.push(hashtag.to_string());
                }
            } else if tag.kind() == nostr::TagKind::Custom("word".into()) {
                // Word tag
                if let Some(word) = tag.content() {
                    words.push(word.to_string());
                }
            } else {
                // Preserve all other tags (e.g., 'a' address tags, future extensions)
                other_tags.push(tag.clone());
            }
        }
    }

    // Add new muted post if not already present
    if !muted_posts.contains(&target_event_id) {
        muted_posts.push(target_event_id);
    }

    // Build tags manually to preserve all custom tags
    let mut all_tags = Vec::new();

    // Add e tags for muted posts
    for event_id in muted_posts {
        all_tags.push(nostr::Tag::event(event_id));
    }

    // Add p tags for blocked users
    for pubkey in blocked_users {
        all_tags.push(nostr::Tag::public_key(pubkey));
    }

    // Add t tags for hashtags
    for hashtag in hashtags {
        all_tags.push(nostr::Tag::hashtag(hashtag));
    }

    // Add word tags
    for word in words {
        all_tags.push(nostr::Tag::custom(nostr::TagKind::Custom("word".into()), vec![word]));
    }

    // Re-attach preserved tags
    all_tags.extend(other_tags);

    let builder = nostr::EventBuilder::new(nostr::Kind::from(10000), existing_content).tags(all_tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish mute list: {}", e))?;

    log::info!("Post muted successfully");
    Ok(())
}

/// Unmute a post (remove from mute list)
pub async fn unmute_post(event_id: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Unmuting post: {}", event_id);

    // Parse event ID
    use nostr::EventId;
    let target_event_id = EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Fetch current mute list
    let mute_event = fetch_mute_list().await?
        .ok_or("No mute list found")?;

    // Preserve the existing content before consuming the event
    let existing_content = mute_event.content.clone();

    // Build new mute list without the target post
    let mut muted_posts = Vec::new();
    let mut blocked_users = Vec::new();
    let mut hashtags = Vec::new();
    let mut words = Vec::new();
    let mut other_tags = Vec::new(); // Preserve unknown/custom tags

    for tag in mute_event.tags.iter() {
        if tag.kind() == nostr::TagKind::e() {
            if let Some(id) = tag.content() {
                if let Ok(eid) = EventId::from_hex(id) {
                    if eid != target_event_id {
                        muted_posts.push(eid);
                    }
                }
            }
        } else if tag.kind() == nostr::TagKind::p() {
            if let Some(pk) = tag.content() {
                if let Ok(pubkey) = nostr::PublicKey::from_hex(pk) {
                    blocked_users.push(pubkey);
                }
            }
        } else if tag.kind() == nostr::TagKind::t() {
            // Hashtag tag
            if let Some(hashtag) = tag.content() {
                hashtags.push(hashtag.to_string());
            }
        } else if tag.kind() == nostr::TagKind::Custom("word".into()) {
            // Word tag
            if let Some(word) = tag.content() {
                words.push(word.to_string());
            }
        } else {
            // Preserve all other tags (e.g., 'a' address tags, future extensions)
            other_tags.push(tag.clone());
        }
    }

    // Build tags manually to preserve all custom tags
    let mut all_tags = Vec::new();

    // Add e tags for muted posts
    for event_id in muted_posts {
        all_tags.push(nostr::Tag::event(event_id));
    }

    // Add p tags for blocked users
    for pubkey in blocked_users {
        all_tags.push(nostr::Tag::public_key(pubkey));
    }

    // Add t tags for hashtags
    for hashtag in hashtags {
        all_tags.push(nostr::Tag::hashtag(hashtag));
    }

    // Add word tags
    for word in words {
        all_tags.push(nostr::Tag::custom(nostr::TagKind::Custom("word".into()), vec![word]));
    }

    // Re-attach preserved tags
    all_tags.extend(other_tags);

    let builder = nostr::EventBuilder::new(nostr::Kind::from(10000), existing_content).tags(all_tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish mute list: {}", e))?;

    log::info!("Post unmuted successfully");
    Ok(())
}

/// Block a user (add to mute list kind 10000)
/// NIP-51: https://github.com/nostr-protocol/nips/blob/master/51.md
pub async fn block_user(pubkey: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Normalize pubkey
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;
    log::info!("Blocking user: {}", normalized_pubkey);

    // Parse pubkey
    let target_pubkey = nostr::PublicKey::from_hex(&normalized_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch current mute list
    let mute_event = fetch_mute_list().await?;

    // Build new mute list
    let mut muted_posts = Vec::new();
    let mut blocked_users = Vec::new();
    let mut hashtags = Vec::new();
    let mut words = Vec::new();
    let mut other_tags = Vec::new(); // Preserve unknown/custom tags
    let mut existing_content = String::new(); // Preserve existing content

    if let Some(event) = mute_event {
        // Preserve the existing content before consuming the event
        existing_content = event.content.clone();

        // Extract existing muted posts, blocked users, hashtags, and words
        for tag in event.tags.iter() {
            if tag.kind() == nostr::TagKind::e() {
                if let Some(id) = tag.content() {
                    if let Ok(eid) = nostr::EventId::from_hex(id) {
                        muted_posts.push(eid);
                    }
                }
            } else if tag.kind() == nostr::TagKind::p() {
                if let Some(pk) = tag.content() {
                    if let Ok(pubkey) = nostr::PublicKey::from_hex(pk) {
                        blocked_users.push(pubkey);
                    }
                }
            } else if tag.kind() == nostr::TagKind::t() {
                // Hashtag tag
                if let Some(hashtag) = tag.content() {
                    hashtags.push(hashtag.to_string());
                }
            } else if tag.kind() == nostr::TagKind::Custom("word".into()) {
                // Word tag
                if let Some(word) = tag.content() {
                    words.push(word.to_string());
                }
            } else {
                // Preserve all other tags (e.g., 'a' address tags, future extensions)
                other_tags.push(tag.clone());
            }
        }
    }

    // Add new blocked user if not already present
    if !blocked_users.contains(&target_pubkey) {
        blocked_users.push(target_pubkey);
    }

    // Build tags manually to preserve all custom tags
    let mut all_tags = Vec::new();

    // Add e tags for muted posts
    for event_id in muted_posts {
        all_tags.push(nostr::Tag::event(event_id));
    }

    // Add p tags for blocked users
    for pubkey in blocked_users {
        all_tags.push(nostr::Tag::public_key(pubkey));
    }

    // Add t tags for hashtags
    for hashtag in hashtags {
        all_tags.push(nostr::Tag::hashtag(hashtag));
    }

    // Add word tags
    for word in words {
        all_tags.push(nostr::Tag::custom(nostr::TagKind::Custom("word".into()), vec![word]));
    }

    // Re-attach preserved tags
    all_tags.extend(other_tags);

    let builder = nostr::EventBuilder::new(nostr::Kind::from(10000), existing_content).tags(all_tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish mute list: {}", e))?;

    log::info!("User blocked successfully");
    Ok(())
}

/// Unblock a user (remove from mute list)
pub async fn unblock_user(pubkey: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Normalize pubkey
    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;
    log::info!("Unblocking user: {}", normalized_pubkey);

    // Parse pubkey
    let target_pubkey = nostr::PublicKey::from_hex(&normalized_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch current mute list
    let mute_event = fetch_mute_list().await?
        .ok_or("No mute list found")?;

    // Preserve the existing content before consuming the event
    let existing_content = mute_event.content.clone();

    // Build new mute list without the target user
    let mut muted_posts = Vec::new();
    let mut blocked_users = Vec::new();
    let mut hashtags = Vec::new();
    let mut words = Vec::new();
    let mut other_tags = Vec::new(); // Preserve unknown/custom tags

    for tag in mute_event.tags.iter() {
        if tag.kind() == nostr::TagKind::e() {
            if let Some(id) = tag.content() {
                if let Ok(eid) = nostr::EventId::from_hex(id) {
                    muted_posts.push(eid);
                }
            }
        } else if tag.kind() == nostr::TagKind::p() {
            if let Some(pk) = tag.content() {
                if let Ok(pubkey) = nostr::PublicKey::from_hex(pk) {
                    if pubkey != target_pubkey {
                        blocked_users.push(pubkey);
                    }
                }
            }
        } else if tag.kind() == nostr::TagKind::t() {
            // Hashtag tag
            if let Some(hashtag) = tag.content() {
                hashtags.push(hashtag.to_string());
            }
        } else if tag.kind() == nostr::TagKind::Custom("word".into()) {
            // Word tag
            if let Some(word) = tag.content() {
                words.push(word.to_string());
            }
        } else {
            // Preserve all other tags (e.g., 'a' address tags, future extensions)
            other_tags.push(tag.clone());
        }
    }

    // Build tags manually to preserve all custom tags
    let mut all_tags = Vec::new();

    // Add e tags for muted posts
    for event_id in muted_posts {
        all_tags.push(nostr::Tag::event(event_id));
    }

    // Add p tags for blocked users
    for pubkey in blocked_users {
        all_tags.push(nostr::Tag::public_key(pubkey));
    }

    // Add t tags for hashtags
    for hashtag in hashtags {
        all_tags.push(nostr::Tag::hashtag(hashtag));
    }

    // Add word tags
    for word in words {
        all_tags.push(nostr::Tag::custom(nostr::TagKind::Custom("word".into()), vec![word]));
    }

    // Re-attach preserved tags
    all_tags.extend(other_tags);

    let builder = nostr::EventBuilder::new(nostr::Kind::from(10000), existing_content).tags(all_tags);

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish mute list: {}", e))?;

    log::info!("User unblocked successfully");
    Ok(())
}

/// Report a post (publish kind 1984 event)
/// NIP-56: https://github.com/nostr-protocol/nips/blob/master/56.md
pub async fn report_post(
    event_id: String,
    author_pubkey: String,
    report_type: String,
    details: Option<String>,
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Reporting post: {} for: {}", event_id, report_type);

    // Parse event ID and pubkey
    use nostr::{EventId, PublicKey, Tag};
    let target_event_id = EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let target_pubkey = PublicKey::from_hex(&author_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Build report event (kind 1984)
    // NIP-56: Required 'p' tag for user, 'e' tag for event, report type as 3rd entry
    let tags = vec![
        Tag::public_key(target_pubkey),
        Tag::custom(
            nostr::TagKind::e(),
            vec![target_event_id.to_hex(), String::new(), report_type],
        ),
    ];

    let content = details.unwrap_or_default();
    let builder = nostr::EventBuilder::new(nostr::Kind::from(1984), content).tags(tags);

    match client.send_event_builder(builder).await {
        Ok(output) => {
            let report_id = output.id().to_hex();
            log::info!("Report published successfully: {}", report_id);
            Ok(report_id)
        }
        Err(e) => {
            log::error!("Failed to publish report: {}", e);
            Err(format!("Failed to publish report: {}", e))
        }
    }
}

/// Publish a repost (kind 6 event) of another event
/// NIP-18: https://github.com/nostr-protocol/nips/blob/master/18.md
pub async fn publish_repost(
    event_id: String,
    _event_author: String,
    relay_url: Option<String>,
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing repost of event: {}", event_id);

    // Parse event ID
    use nostr::{EventId, RelayUrl};
    let target_event_id = EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Fetch the original event from database to get full event data
    // This is required for EventBuilder::repost() to serialize the event properly
    let event = client.database().event_by_id(&target_event_id).await
        .map_err(|e| format!("Failed to fetch event from database: {}", e))?
        .ok_or_else(|| format!("Event not found: {}", event_id))?;

    // Parse relay URL if provided
    let relay = relay_url.and_then(|url| RelayUrl::parse(&url).ok());

    // Use EventBuilder::repost() for proper NIP-18 compliance
    // This automatically:
    // - Serializes the event JSON into content field
    // - Adds 'e' tag with relay hint
    // - Adds 'p' tag for event author
    // - Uses Kind 6 for text notes, Kind 16 (generic repost) for others
    let builder = nostr::EventBuilder::repost(&event, relay);

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
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    log::info!("Fetching articles with limit: {}", limit);

    use nostr::{Filter, Kind, Timestamp};

    let mut filter = Filter::new()
        .kind(Kind::LongFormTextNote)
        .limit(limit);

    if let Some(until_timestamp) = until {
        filter = filter.until(Timestamp::from(until_timestamp));
    }

    // Ensure relays are ready before fetching
    ensure_relays_ready(&client).await;

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
/// Legacy function - use fetch_event_by_coordinate for new code
pub async fn fetch_article_by_coordinate(
    pubkey: String,
    identifier: String,
) -> std::result::Result<Option<nostr::Event>, String> {
    fetch_event_by_coordinate(30023, pubkey, identifier).await
}

/// Fetch any addressable event by coordinate (kind:pubkey:identifier)
/// Works for articles (30023), livestreams (30311), and other addressable events
/// Fetch addressable event by coordinate with two-phase loading (DB first, then relay)
/// Optionally uses relay hints for faster fetching
pub async fn fetch_event_by_coordinate(
    kind: u16,
    pubkey: String,
    identifier: String,
) -> std::result::Result<Option<nostr::Event>, String> {
    fetch_event_by_coordinate_with_relays(kind, pubkey, identifier, Vec::new()).await
}

/// Fetch addressable event by coordinate with relay hints
/// Two-phase loading: DB first (instant), then relay (if not found or for freshness)
pub async fn fetch_event_by_coordinate_with_relays(
    kind: u16,
    pubkey: String,
    identifier: String,
    relay_hints: Vec<String>,
) -> std::result::Result<Option<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    use nostr::{Filter, Kind, PublicKey};

    let author = PublicKey::from_hex(&pubkey)
        .or_else(|_| PublicKey::parse(&pubkey))
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    let filter = Filter::new()
        .kind(Kind::from(kind))
        .author(author)
        .identifier(identifier.clone())
        .limit(1);

    // PHASE 1: Check database first (instant)
    if let Ok(db_events) = client.database().query(filter.clone()).await {
        if let Some(event) = db_events.into_iter().next() {
            log::debug!("Found event kind {} in DB: {}:{}", kind, pubkey, identifier);
            return Ok(Some(event));
        }
    }

    log::info!("Fetching event kind {} from relay: {}:{}", kind, pubkey, identifier);

    // PHASE 2: Fetch from relays
    // Try relay hints first if provided
    if !relay_hints.is_empty() {
        let relay_urls: Vec<nostr_sdk::RelayUrl> = relay_hints.iter()
            .filter_map(|r| nostr_sdk::RelayUrl::parse(r).ok())
            .collect();

        // Add relay hints temporarily and fetch
        for relay_url in &relay_urls {
            if let Err(e) = client.add_relay(relay_url.as_str()).await {
                log::debug!("Could not add relay hint {}: {}", relay_url, e);
            }
        }

        // Try fetching with shorter timeout for relay hints
        if let Ok(events) = client.fetch_events(filter.clone(), std::time::Duration::from_secs(5)).await {
            if let Some(event) = events.into_iter().next() {
                return Ok(Some(event));
            }
        }
    }

    // Fallback: standard relay fetch with longer timeout
    ensure_relays_ready(&client).await;

    match client.fetch_events(filter, std::time::Duration::from_secs(10)).await {
        Ok(events) => {
            Ok(events.into_iter().next())
        }
        Err(e) => {
            log::error!("Failed to fetch event: {}", e);
            Err(format!("Failed to fetch event: {}", e))
        }
    }
}

/// Publish profile metadata (Kind 0)
///
/// Updates the user's Nostr profile with the provided metadata
pub async fn publish_metadata(metadata: Metadata) -> std::result::Result<String, String> {
    let client = NOSTR_CLIENT.read();
    let client = client.as_ref().ok_or("Client not initialized")?;

    // Verify signer is available
    if !*HAS_SIGNER.read() {
        return Err("No signer available".to_string());
    }

    log::info!("Publishing profile metadata");

    // Build event and publish using gossip routing (client handles signing)
    let builder = EventBuilder::metadata(&metadata);
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish metadata: {}", e))?;

    log::info!("Metadata published successfully");

    // Return event ID
    Ok(output.id().to_hex())
}

/// Update just the profile picture
#[allow(dead_code)]
pub async fn update_profile_picture(url: String) -> std::result::Result<(), String> {
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
pub async fn update_profile_banner(url: String) -> std::result::Result<(), String> {
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
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Validate required fields
    if identifier.trim().is_empty() {
        return Err("Identifier cannot be empty".to_string());
    }

    if title.trim().is_empty() {
        return Err("Title cannot be empty".to_string());
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
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|e| {
            log::error!("Failed to get system time: {}", e);
            "0".to_string()
        });

    tags.push(Tag::custom(
        nostr::TagKind::Custom("published_at".into()),
        vec![timestamp]
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

    // Extract extension from URL (handles query params and fragments)
    let path = url_lower
        .split('?').next()?  // Remove query string
        .split('#').next()?; // Remove fragment
    let extension = path.split('.').last()?;

    match extension {
        // Image types
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

        // Audio types
        "mp3" => Some("audio/mpeg".to_string()),
        "m4a" | "mp4" | "aac" => Some("audio/mp4".to_string()),
        "ogg" | "opus" => Some("audio/ogg".to_string()),
        "wav" => Some("audio/wav".to_string()),
        "webm" | "weba" => Some("audio/webm".to_string()),
        "flac" => Some("audio/flac".to_string()),

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
) -> std::result::Result<String, String> {
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
) -> std::result::Result<String, String> {
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

/// Publish a voice message (Kind 1222)
/// NIP-A0: https://github.com/nostr-protocol/nips/blob/master/A0.md
pub async fn publish_voice_message(
    audio_url: String,
    duration: f64,
    waveform: Vec<u8>,
    hashtags: Vec<String>,
    mime_type: Option<String>,
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing voice message: {}", audio_url);

    // Parse URL
    let url = nostr::Url::parse(&audio_url)
        .map_err(|e| format!("Invalid audio URL: {}", e))?;

    // Build event using EventBuilder::voice_message
    let mut builder = nostr::EventBuilder::voice_message(url);

    // Build tags
    use nostr::Tag;
    let mut tags = Vec::new();

    // Add imeta tag with duration and waveform (NIP-92)
    let waveform_str = waveform.iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ");

    let mut imeta_fields = vec![
        format!("url {}", audio_url),
        format!("duration {}", duration.round() as u64),
        format!("waveform {}", waveform_str),
    ];

    // Add MIME type - use provided mime_type, fallback to detection, or default
    let final_mime_type = mime_type
        .or_else(|| detect_mime_type(&audio_url))
        .unwrap_or_else(|| "audio/webm".to_string());
    imeta_fields.push(format!("m {}", final_mime_type));

    tags.push(Tag::custom(
        nostr::TagKind::Custom("imeta".into()),
        imeta_fields
    ));

    // Add hashtags
    for hashtag in hashtags {
        tags.push(Tag::hashtag(hashtag));
    }

    // Add tags to builder
    builder = builder.tags(tags);

    // Publish
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish voice message: {}", e))?;

    let event_id = output.id().to_hex();
    log::info!("Voice message published successfully: {}", event_id);
    Ok(event_id)
}

/// Publish a voice message reply (Kind 1244) following NIP-22
/// NIP-A0: https://github.com/nostr-protocol/nips/blob/master/A0.md
/// NIP-22: https://github.com/nostr-protocol/nips/blob/master/22.md
pub async fn publish_voice_message_reply(
    audio_url: String,
    duration: f64,
    waveform: Vec<u8>,
    reply_to: nostr::Event,
    mime_type: Option<String>,
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing voice message reply to: {}", reply_to.id.to_hex());

    // Parse URL
    let url = nostr::Url::parse(&audio_url)
        .map_err(|e| format!("Invalid audio URL: {}", e))?;

    // Determine root and parent for NIP-22 structure
    // Check if reply_to has a root tag marker (NIP-10/NIP-22)
    // Extract root event ID, author pubkey, and relay URL
    let (root_event_id, root_pubkey, root_relay_url): (Option<String>, Option<PublicKey>, Option<RelayUrl>) = {
        // First, try to find modern NIP-10/NIP-22 lowercase 'e' tag with marker="root"
        let modern_root = reply_to.tags.iter().find_map(|tag| {
            if let Some(nostr::TagStandard::Event { event_id, relay_url, marker, public_key, .. }) = tag.as_standardized() {
                // Check for lowercase 'e' tag with marker="root" (NIP-10/NIP-22)
                if marker == &Some(nostr_sdk::nips::nip10::Marker::Root) {
                    return Some((
                        Some(event_id.to_hex()),
                        *public_key,  // Public key from the tag
                        relay_url.clone(),  // Relay URL from the tag
                    ));
                }
            }
            None
        });

        if let Some(result) = modern_root {
            result
        } else {
            // Fallback: Legacy uppercase 'E'/'P' tag support
            // NIP-10 deprecated positional convention: first 'E' tag = root, first 'P' tag = root author
            let uppercase_e_tags: Vec<_> = reply_to.tags.iter()
                .filter_map(|tag| {
                    let tag_vec = tag.clone().to_vec();
                    if tag_vec.len() >= 2 && tag_vec[0] == "E" {
                        Some((
                            tag_vec[1].clone(),
                            if tag_vec.len() >= 3 && !tag_vec[2].is_empty() {
                                RelayUrl::parse(&tag_vec[2]).ok()
                            } else {
                                None
                            }
                        ))
                    } else {
                        None
                    }
                })
                .collect();

            if let Some((root_event_id, relay)) = uppercase_e_tags.first() {
                // Per deprecated NIP-10 positional convention, the first 'P' tag corresponds to the root author
                // Note: This is a heuristic and may not be accurate if the event has multiple 'P' tags
                // for different purposes (e.g., mentions). Modern events should use marker-based tags.
                let root_pubkey = reply_to.tags.iter().find_map(|p_tag| {
                    let p_vec = p_tag.clone().to_vec();
                    if p_vec.len() >= 2 && p_vec[0] == "P" {
                        PublicKey::from_hex(&p_vec[1]).ok()
                    } else {
                        None
                    }
                });

                (Some(root_event_id.clone()), root_pubkey, relay.clone())
            } else {
                (None, None, None)
            }
        }
    };

    let parent_id = reply_to.id.to_hex();
    let parent_pubkey = reply_to.pubkey;
    let parent_kind = reply_to.kind;

    // Create CommentTarget for parent
    use nostr::prelude::*;
    let parent_target = if parent_kind.as_u16() == 1222 || parent_kind.as_u16() == 1244 {
        // Voice message or voice reply
        let event_id = EventId::parse(&parent_id)
            .map_err(|e| format!("Failed to parse parent event ID: {}", e))?;
        CommentTarget::event(event_id, parent_kind, Some(parent_pubkey), None)
    } else {
        return Err("Can only reply to voice messages (Kind 1222 or 1244)".to_string());
    };

    // Create root target if different from parent
    let root_target = if let Some(root_id) = root_event_id {
        if root_id != parent_id {
            let event_id = EventId::parse(&root_id)
                .map_err(|e| format!("Failed to parse root event ID: {}", e))?;
            // Include root author and relay URL for proper NIP-22/NIP-10 compliance
            use std::borrow::Cow;
            Some(CommentTarget::event(
                event_id,
                nostr::Kind::VoiceMessage,
                root_pubkey,  // Root author's public key
                root_relay_url.as_ref().map(Cow::Borrowed)  // Relay hint/URL as Cow
            ))
        } else {
            None
        }
    } else {
        None
    };

    // Build event using EventBuilder::voice_message_reply
    let mut builder = nostr::EventBuilder::voice_message_reply(url, root_target, parent_target);

    // Build tags
    use nostr::Tag;
    let mut tags = Vec::new();

    // Add imeta tag with duration and waveform (NIP-92)
    let waveform_str = waveform.iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(" ");

    let mut imeta_fields = vec![
        format!("url {}", audio_url),
        format!("duration {}", duration.round() as u64),
        format!("waveform {}", waveform_str),
    ];

    // Add MIME type - use provided mime_type, fallback to detection, or default
    let final_mime_type = mime_type
        .or_else(|| detect_mime_type(&audio_url))
        .unwrap_or_else(|| "audio/webm".to_string());
    imeta_fields.push(format!("m {}", final_mime_type));

    tags.push(Tag::custom(
        nostr::TagKind::Custom("imeta".into()),
        imeta_fields
    ));

    // Add p tag for parent author
    tags.push(Tag::public_key(parent_pubkey));

    // Add p tags for anyone else mentioned in the parent (using SDK's public_keys())
    for public_key in reply_to.tags.public_keys() {
        // Don't duplicate the parent author
        if public_key != &parent_pubkey {
            tags.push(Tag::public_key(*public_key));
        }
    }

    // Add tags to builder
    builder = builder.tags(tags);

    // Publish
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish voice message reply: {}", e))?;

    let event_id = output.id().to_hex();
    log::info!("Voice message reply published successfully: {}", event_id);
    Ok(event_id)
}

/// Get the current user's public key
pub async fn get_user_pubkey() -> std::result::Result<PublicKey, String> {
    let signer = get_signer().ok_or("No signer available")?;
    signer.public_key().await
        .map_err(|e| format!("Failed to get public key: {}", e))
}

/// Publish a poll vote (Kind 1018) following NIP-88
/// NIP-88: https://github.com/nostr-protocol/nips/blob/master/88.md
/// Votes are published to the relays specified in the poll event
pub async fn publish_poll_vote(
    poll_id: nostr::EventId,
    response: nostr::nips::nip88::PollResponse,
    poll_relays: Vec<nostr::RelayUrl>,
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Validate that the poll_id matches the poll referenced in the PollResponse
    let referenced_poll_id = match &response {
        nostr::nips::nip88::PollResponse::SingleChoice { poll_id: ref_id, .. } => ref_id,
        nostr::nips::nip88::PollResponse::MultipleChoice { poll_id: ref_id, .. } => ref_id,
    };

    if *referenced_poll_id != poll_id {
        return Err(format!(
            "Poll ID mismatch: expected {}, but PollResponse references {}",
            poll_id.to_hex(),
            referenced_poll_id.to_hex()
        ));
    }

    log::info!("Publishing poll vote for poll: {}", poll_id.to_hex());

    // Build event using EventBuilder::poll_response
    let builder = nostr::EventBuilder::poll_response(response);

    // NIP-88: Votes should be published to the relays specified in the poll
    let output = if !poll_relays.is_empty() {
        // Track which relays we actually add (to clean up later)
        let mut added_relays = Vec::new();
        for relay_url in &poll_relays {
            // add_relay returns Ok if added, Err if already present or failed
            if client.add_relay(relay_url.as_str()).await.is_ok() {
                added_relays.push(relay_url.clone());
            }
        }

        // Use non-blocking relay ready check instead of blocking connect()
        ensure_relays_ready(&client).await;

        // Check if any poll relays are actually connected
        let relays_status = client.relays().await;
        let connected_poll_relays: Vec<_> = poll_relays.iter()
            .filter(|r| {
                if let Ok(url) = nostr::RelayUrl::parse(r.as_str()) {
                    relays_status.get(&url).map(|relay| relay.is_connected()).unwrap_or(false)
                } else {
                    false
                }
            })
            .collect();

        if connected_poll_relays.is_empty() {
            log::warn!("None of the {} poll relays are connected, falling back to default relays", poll_relays.len());
        } else {
            log::debug!("{}/{} poll relays connected", connected_poll_relays.len(), poll_relays.len());
        }

        // Publish to poll-specified relays
        let relay_urls: Vec<nostr::Url> = poll_relays.iter()
            .filter_map(|r| nostr::Url::parse(r.as_str()).ok())
            .collect();

        let result = if !relay_urls.is_empty() {
            log::info!("Publishing vote to {} poll-specified relays", relay_urls.len());
            client.send_event_builder_to(relay_urls, builder).await
                .map_err(|e| format!("Failed to publish poll vote to poll relays: {}", e))
        } else {
            // Fallback if URL parsing failed
            client.send_event_builder(builder).await
                .map_err(|e| format!("Failed to publish poll vote: {}", e))
        };

        // Cleanup: remove only the relays we added
        for relay_url in added_relays {
            if let Err(e) = client.remove_relay(relay_url.as_str()).await {
                log::debug!("Could not remove poll relay {}: {}", relay_url, e);
            }
        }

        result?
    } else {
        // No poll relays specified, use default relays
        client.send_event_builder(builder).await
            .map_err(|e| format!("Failed to publish poll vote: {}", e))?
    };

    let event_id = output.id().to_hex();
    log::info!("Poll vote published successfully: {}", event_id);
    Ok(event_id)
}

/// Publish a poll (Kind 1068) following NIP-88
/// NIP-88: https://github.com/nostr-protocol/nips/blob/master/88.md
pub async fn publish_poll(
    title: String,
    poll_type: nostr::nips::nip88::PollType,
    options: Vec<nostr::nips::nip88::PollOption>,
    relays: Vec<String>,
    ends_at: Option<nostr::Timestamp>,
    hashtags: Vec<String>,
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    // Validate inputs
    if title.trim().is_empty() {
        return Err("Poll title cannot be empty".to_string());
    }

    if options.len() < 2 {
        return Err("Poll must have at least 2 options".to_string());
    }

    if options.len() > 10 {
        return Err("Poll cannot have more than 10 options".to_string());
    }

    log::info!("Publishing poll: {}", title);

    // Parse relay URLs
    let relay_urls: Vec<nostr::RelayUrl> = relays
        .into_iter()
        .filter_map(|r| nostr::RelayUrl::parse(&r).ok())
        .collect();

    // Build poll struct
    let poll = nostr::nips::nip88::Poll {
        title,
        r#type: poll_type,
        options,
        relays: relay_urls,
        ends_at,
    };

    // Build event using EventBuilder::poll
    let mut builder = nostr::EventBuilder::poll(poll);

    // Add hashtags
    use nostr::Tag;
    for hashtag in hashtags {
        builder = builder.tags([Tag::hashtag(hashtag)]);
    }

    // Publish
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish poll: {}", e))?;

    let event_id = output.id().to_hex();
    log::info!("Poll published successfully: {}", event_id);
    Ok(event_id)
}
