use dioxus::prelude::*;
use dioxus::signals::ReadableExt;
use futures::future::join_all;
use nostr_sdk::Client;
use nostr_sdk::prelude::*;
use nostr::Url;
use std::sync::Arc;
use std::sync::{OnceLock, Mutex};
use std::time::Duration;

#[cfg(target_arch = "wasm32")]
use nostr_indexeddb::WebDatabase;

use crate::stores::signer::SignerType;
use crate::stores::pinned_notes;
use crate::stores::relay;
use crate::utils::mention_extractor::{extract_mentioned_pubkeys, create_mention_tags};

#[cfg(target_arch = "wasm32")]
use crate::services::admission_policy::NostrBlueAdmissionPolicy;

// Re-export relay types for backward compatibility
// New code should use crate::stores::relay directly
pub use crate::stores::relay::{
    RelayInfo, RelayPoolStoreStoreExt, RelayStatus, RELAY_CONNECTED, RELAY_POOL, USER_RELAYS_APPLIED,
};

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
/// Delegates to relay::connection::ensure_relays_ready for the actual implementation.
///
/// This is needed because connect() is non-blocking and spawns background tasks.
/// Call this before any direct client.fetch_events() calls.
pub async fn ensure_relays_ready(client: &Client) {
    relay::connection::ensure_relays_ready(client).await;
}

/// Create an naddr (NIP-19) with relay hints for an addressable event
/// This includes relay hints from the user's write relays for better discoverability.
/// Delegates to relay::hints::make_naddr_with_hints
pub async fn make_naddr_with_hints(
    kind: u16,
    pubkey: &nostr::PublicKey,
    identifier: &str,
) -> std::result::Result<String, String> {
    let client = get_client().ok_or("Client not initialized")?;
    relay::make_naddr_with_hints(&client, kind, pubkey, identifier).await
}

// RelayStatus, RelayInfo, RelayPoolStore, RELAY_POOL are now re-exported from relay module
// See the `pub use crate::stores::relay::...` at the top of this file

/// Result of publishing an event, including relay success/failure tracking
/// Enables debugging which relays accepted/rejected events
#[derive(Clone, Debug)]
pub struct PublishResult {
    /// The event ID that was published
    pub event_id: String,
    /// URLs of relays that successfully accepted the event
    pub successful_relays: Vec<String>,
    /// URLs of relays that failed to accept the event (with error messages)
    pub failed_relays: Vec<(String, String)>,
}

impl PublishResult {
    /// Create from SDK Output
    pub fn from_output(output: nostr_relay_pool::Output<nostr::EventId>) -> Self {
        let successful: Vec<String> = output.success
            .iter()
            .map(|url| url.to_string())
            .collect();
        let failed: Vec<(String, String)> = output.failed
            .iter()
            .map(|(url, reason)| (url.to_string(), reason.clone()))
            .collect();

        Self {
            event_id: output.id().to_hex(),
            successful_relays: successful,
            failed_relays: failed,
        }
    }

    /// Get total number of relays attempted
    pub fn total_attempted(&self) -> usize {
        self.successful_relays.len() + self.failed_relays.len()
    }

    /// Get number of successful relays
    pub fn success_count(&self) -> usize {
        self.successful_relays.len()
    }

    /// Check if publish was at least partially successful
    pub fn is_success(&self) -> bool {
        !self.successful_relays.is_empty()
    }

    /// Check if any relays failed
    pub fn has_failures(&self) -> bool {
        !self.failed_relays.is_empty()
    }

    /// Get success rate as percentage (0.0 - 100.0)
    pub fn success_rate(&self) -> f32 {
        let total = self.total_attempted();
        if total == 0 {
            0.0
        } else {
            (self.successful_relays.len() as f32 / total as f32) * 100.0
        }
    }
}

// DEFAULT_RELAYS is now defined in relay::pool
// Re-export for backward compatibility
pub use crate::stores::relay::pool::DEFAULT_RELAYS;

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

        // Configure client options for gossip-discovered relays
        // This is CRITICAL: Without this, gossip relays won't verify events match filters
        let client_opts = ClientOptions::new()
            .verify_subscriptions(true)
            .ban_relay_on_mismatch(true)
            .max_avg_latency(Duration::from_secs(2));

        Client::builder()
            .database(database)
            .gossip(gossip)
            .admit_policy(NostrBlueAdmissionPolicy)
            .opts(client_opts)
            .build()
    };

    #[cfg(not(target_arch = "wasm32"))]
    let client = Client::builder().build();

    let client = Arc::new(client);

    // Add default relays with options in PARALLEL (will be replaced if user has kind 10002)
    // This significantly speeds up initialization by not waiting for each relay sequentially
    let relay_futures: Vec<_> = DEFAULT_RELAYS
        .iter()
        .filter_map(|relay_url| {
            Url::parse(relay_url).ok().map(|url| {
                let opts = relay_opts.clone();
                let pool = client.pool();
                let url_str = relay_url.to_string();
                async move {
                    match pool.add_relay(url, opts).await {
                        Ok(_) => {
                            log::debug!("Added relay with opts: {}", url_str);
                            RelayInfo::new(url_str, RelayStatus::Connected)
                        }
                        Err(e) => {
                            log::error!("Failed to add relay {}: {}", url_str, e);
                            RelayInfo::new(url_str, RelayStatus::Disconnected)
                        }
                    }
                }
            })
        })
        .collect();

    let relay_infos: Vec<RelayInfo> = join_all(relay_futures).await;

    RELAY_POOL.read().data().write().clone_from(&relay_infos);

    // Store client first (so it's available for queries)
    *NOSTR_CLIENT.write() = Some(client.clone());

    // Add discovery relays for gossip bootstrapping
    // These are used by the SDK to find users' relay lists (NIP-65/NIP-17)
    // when gossip data is outdated or missing
    log::info!("Adding discovery relays for gossip...");
    for discovery_url in &["wss://relay.damus.io", "wss://purplepag.es", "wss://nos.lol"] {
        if let Err(e) = client.add_discovery_relay(*discovery_url).await {
            log::warn!("Failed to add discovery relay {}: {}", discovery_url, e);
        }
    }

    // Spawn relay connections in background (required for WASM - can't block main thread)
    log::info!("Spawning relay connections...");
    #[cfg(target_arch = "wasm32")]
    {
        let client_for_connect = client.clone();
        wasm_bindgen_futures::spawn_local(async move {
            client_for_connect.connect().await;
            log::info!("Background relay connections initiated");
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let client_for_connect = client.clone();
        tokio::spawn(async move {
            client_for_connect.connect().await;
            log::info!("Background relay connections initiated");
        });
    }

    // Wait for at least one relay to connect before marking initialized
    // This ensures CLIENT_INITIALIZED means "ready to fetch events"
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;
    const TIMEOUT_MS: u64 = 3000;
    const POLL_INTERVAL_MS: u64 = 100;

    #[cfg(target_arch = "wasm32")]
    {
        use gloo_timers::future::TimeoutFuture;
        let start = instant::Instant::now();

        loop {
            // Yield to allow background connection task to progress
            TimeoutFuture::new(POLL_INTERVAL_MS as u32).await;

            let relays_now = client.relays().await;
            let connected = relays_now.values().any(|r| r.status() == PoolRelayStatus::Connected);

            if connected {
                log::info!("First relay connected after {}ms", start.elapsed().as_millis());
                if !*RELAY_CONNECTED.peek() {
                    *RELAY_CONNECTED.write() = true;
                }
                break;
            }

            if start.elapsed().as_millis() > TIMEOUT_MS as u128 {
                log::warn!("Relay connection timeout after {}ms, proceeding anyway", TIMEOUT_MS);
                // Signal false so downstream watchers know init completed without relay
                // They can retry via ensure_relays_ready when a relay connects later
                *RELAY_CONNECTED.write() = false;
                break;
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_millis(TIMEOUT_MS);

        loop {
            tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;

            let relays_now = client.relays().await;
            let connected = relays_now.values().any(|r| r.status() == PoolRelayStatus::Connected);

            if connected {
                log::info!("First relay connected after {:?}", start.elapsed());
                if !*RELAY_CONNECTED.peek() {
                    *RELAY_CONNECTED.write() = true;
                }
                break;
            }

            if start.elapsed() > timeout {
                log::warn!("Relay connection timeout after {:?}, proceeding anyway", timeout);
                // Signal false so downstream watchers know init completed without relay
                // They can retry via ensure_relays_ready when a relay connects later
                *RELAY_CONNECTED.write() = false;
                break;
            }
        }
    }

    // Now mark as initialized - relays are ready (or timed out)
    *CLIENT_INITIALIZED.write() = true;

    log::info!("Nostr client initialized with relays ready");
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

    // Load user's relay configuration in background
    // SDK gossip handles dynamic relay routing automatically, so we only need to:
    // 1. Load user's relay metadata for Settings UI display
    // 2. Apply local relays (browser-only storage)
    // 3. Load NIP-51 lists (search/blocked - not handled by gossip)
    let client_clone = client.clone();
    spawn(async move {
        // Apply local relays FIRST (browser-only storage) - this is instant
        relay::apply_local_relays_to_client(client_clone.clone()).await;

        // Signal immediately after local relays are applied
        // SDK gossip discovers relays dynamically per-pubkey, so we don't need
        // to wait for Settings metadata before feed fetching can begin
        *relay::USER_RELAYS_APPLIED.write() = true;
        log::info!("User relays applied, feed fetching unblocked");

        // Load user's relay metadata for Settings UI (slow network fetch)
        // This is non-blocking for feeds - only needed for Settings display
        if let Err(e) = relay::init_user_relay_lists(client_clone.clone()).await {
            log::warn!("Failed to load user relay lists: {}", e);
        }

        // Load NIP-51 relay lists (search/blocked) - non-blocking for feed
        if let Err(e) = relay::init_nip51_relay_lists(client_clone.clone()).await {
            log::warn!("Failed to load NIP-51 relay lists: {}", e);
        }

        // After NIP-51 lists load, remove any blocked relays that were added before
        relay::pool::remove_blocked_relays_from_pool(&client_clone).await;
    });

    // Load user's pinned notes (kind 10001) in background
    spawn(async move {
        if let Err(e) = pinned_notes::init_pinned_notes().await {
            log::warn!("Failed to load user pinned notes: {}", e);
        }
    });

    log::info!("Signer updated successfully");
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
/// Delegates to relay::pool::add_relay for the actual implementation.
#[allow(dead_code)]
pub async fn add_relay(relay_url: &str) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;
    relay::pool::add_relay(&client, relay_url).await
}

/// Remove a relay
/// Delegates to relay::pool::remove_relay for the actual implementation.
#[allow(dead_code)]
pub async fn remove_relay(relay_url: &str) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;
    relay::pool::remove_relay(&client, relay_url).await
}

/// Disconnect from all relays
/// Delegates to relay::connection::disconnect for the actual implementation.
#[allow(dead_code)]
pub async fn disconnect() {
    if let Some(client) = get_client() {
        relay::connection::disconnect(&client).await;
    }
}

/// Reconnect to all relays
/// Delegates to relay::connection::reconnect for the actual implementation.
#[allow(dead_code)]
pub async fn reconnect() {
    if let Some(client) = get_client() {
        relay::connection::reconnect(&client).await;
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

/// Ensure the video relay is connected
/// Delegates to relay::connection::ensure_video_relay_connected
async fn ensure_video_relay_connected(client: &Client) {
    relay::connection::ensure_video_relay_connected(client).await;
}

/// Fetch video events, ensuring relay.divine.video is included
///
/// This function adds the video-specific relay to the pool before fetching,
/// ensuring video content is discovered from the Divine relay in addition
/// to relays selected via the outbox model.
pub async fn fetch_video_events(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    // Ensure video relay is in the pool
    ensure_video_relay_connected(&client).await;

    // Use standard aggregated fetch (DB first, then relays including video relay)
    fetch_events_aggregated(filter, timeout).await
}

/// Fetch events directly from relays, bypassing cache
///
/// Use this for discovery features where fresh data from the network is needed.
/// Results are still stored in the database for future caching.
pub async fn fetch_events_from_relays(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    // Wait for at least one relay to be ready
    ensure_relays_ready(&client).await;

    // Log relay status for debugging
    let relays = client.relays().await;
    let connected: Vec<_> = relays.iter()
        .filter(|(_, r)| r.status() == nostr_relay_pool::RelayStatus::Connected)
        .map(|(url, _)| url.to_string())
        .collect();
    log::info!("fetch_events_from_relays: {} relays connected: {:?}", connected.len(), connected);

    let result = client
        .fetch_events(filter.clone(), timeout)
        .await
        .map(|events| {
            let events: Vec<_> = events.into_iter().collect();
            log::info!("fetch_events_from_relays: received {} events", events.len());
            events
        })
        .map_err(|e| {
            log::error!("fetch_events_from_relays error: {}", e);
            e.to_string()
        });

    result
}

/// Fetch events using gossip (automatic relay routing)
///
/// This function waits for user relay lists (kind 10002) to be applied before
/// fetching, ensuring gossip routing uses the correct relays for signed-in users.
pub async fn fetch_events_aggregated_outbox(
    filter: Filter,
    timeout: Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    // Wait for user relays if signed in (up to 2 seconds)
    // This ensures gossip routing uses the user's configured relays
    if *HAS_SIGNER.peek() && !*USER_RELAYS_APPLIED.peek() {
        log::debug!("Waiting for user relay lists to be applied...");
        let start = instant::Instant::now();

        #[cfg(target_arch = "wasm32")]
        {
            while !*USER_RELAYS_APPLIED.peek() && start.elapsed() < Duration::from_secs(2) {
                gloo_timers::future::TimeoutFuture::new(50).await;
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            while !*USER_RELAYS_APPLIED.peek() && start.elapsed() < Duration::from_secs(2) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        if *USER_RELAYS_APPLIED.peek() {
            log::debug!("User relay lists applied after {}ms", start.elapsed().as_millis());
        } else {
            log::warn!("User relay lists not applied after timeout, proceeding with defaults");
        }
    }

    // Wait for at least one relay to be ready (non-blocking connect() may not have finished)
    ensure_relays_ready(&client).await;

    // Capture authors for client-side filtering (defense-in-depth)
    let filter_authors = filter.authors.clone();

    // Use gossip for automatic relay routing
    let events = client.fetch_events(filter, timeout).await
        .map_err(|e| format!("Failed to fetch events: {}", e))?;

    // Client-side author filtering (defense-in-depth against misbehaving relays)
    // Even with verify_subscriptions enabled, some relays may still send unmatched events
    let filtered_events: Vec<nostr::Event> = if let Some(ref authors) = filter_authors {
        let author_set: std::collections::HashSet<_> = authors.iter().collect();
        events.into_iter()
            .filter(|e| author_set.contains(&e.pubkey))
            .collect()
    } else {
        events.into_iter().collect()
    };

    Ok(filtered_events)
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

/// Extract quote tags from content containing nostr: URIs (NIP-18 compliance)
/// Returns q tags for note1/nevent1/naddr1 references
fn extract_quote_tags(content: &str) -> Vec<nostr::Tag> {
    use nostr::nips::nip19::Nip19;
    use nostr::event::tag::TagStandard;
    use nostr_sdk::nips::nip01::Coordinate;

    let mut tags = Vec::new();

    // Match nostr:note1..., nostr:nevent1..., nostr:naddr1...
    let re = match regex::Regex::new(r"nostr:(note1[a-z0-9]+|nevent1[a-z0-9]+|naddr1[a-z0-9]+)") {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to compile quote regex: {}", e);
            return tags;
        }
    };

    for cap in re.captures_iter(content) {
        if let Some(bech32_match) = cap.get(1) {
            let bech32 = bech32_match.as_str();
            match Nip19::from_bech32(bech32) {
                Ok(nip19) => {
                    let tag = match nip19 {
                        Nip19::EventId(id) => Some(nostr::Tag::from_standardized_without_cell(
                            TagStandard::Quote {
                                event_id: id,
                                relay_url: None,
                                public_key: None,
                            }
                        )),
                        Nip19::Event(nevent) => Some(nostr::Tag::from_standardized_without_cell(
                            TagStandard::Quote {
                                event_id: nevent.event_id,
                                relay_url: nevent.relays.first().cloned(),
                                public_key: nevent.author,
                            }
                        )),
                        Nip19::Coordinate(coord) => Some(nostr::Tag::from_standardized_without_cell(
                            TagStandard::QuoteAddress {
                                coordinate: Coordinate::new(coord.kind, coord.public_key)
                                    .identifier(coord.identifier.clone()),
                                relay_url: coord.relays.first().cloned(),
                            }
                        )),
                        _ => None,
                    };
                    if let Some(t) = tag {
                        tags.push(t);
                    }
                }
                Err(e) => {
                    log::debug!("Failed to parse nostr URI '{}': {}", bech32, e);
                }
            }
        }
    }

    log::debug!("Extracted {} quote tags from content", tags.len());
    tags
}

/// Publish a text note (kind 1 event) with relay feedback
/// Returns PublishResult with success/failure tracking per relay
pub async fn publish_note_tracked(content: String, tags: Vec<Vec<String>>) -> std::result::Result<PublishResult, String> {
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

    // Extract and add quote tags (NIP-18 compliance)
    let quote_tags = extract_quote_tags(&content);
    mention_tags.extend(quote_tags);

    // Build the event
    let builder = nostr::EventBuilder::text_note(&content).tags(mention_tags);

    // Publish using gossip - automatic relay routing
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish: {}", e))?;

    let result = PublishResult::from_output(output);

    // Log relay feedback
    log::info!(
        "Note published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a text note (kind 1 event)
/// For relay feedback, use publish_note_tracked instead
pub async fn publish_note(content: String, tags: Vec<Vec<String>>) -> std::result::Result<String, String> {
    publish_note_tracked(content, tags)
        .await
        .map(|result| result.event_id)
}

/// Publish a reaction (kind 7 event) with relay feedback
/// NIP-25: https://github.com/nostr-protocol/nips/blob/master/25.md
/// NIP-30: Custom emoji support via emoji_tag parameter
pub async fn publish_reaction_tracked(
    event_id: String,
    event_author: String,
    content: String,
    emoji_tag: Option<(String, String)>, // (shortcode, url) for custom emoji reactions
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Publishing reaction to event: {}", event_id);

    // Parse event ID and author pubkey
    use nostr::{EventId, PublicKey, Tag, Url};
    use nostr::nips::nip25::ReactionTarget;
    use nostr::event::tag::TagStandard;
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

    let mut builder = nostr::EventBuilder::reaction(target, content);

    // Add emoji tag for custom emojis (NIP-30)
    if let Some((shortcode, url_str)) = emoji_tag {
        if let Ok(parsed_url) = Url::parse(&url_str) {
            builder = builder.tag(Tag::from_standardized_without_cell(
                TagStandard::Emoji { shortcode, url: parsed_url }
            ));
            log::info!("Added custom emoji tag to reaction");
        } else {
            log::warn!("Failed to parse custom emoji URL: {}", url_str);
        }
    }

    // Publish using gossip - automatic relay routing
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish reaction: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Reaction published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a reaction (kind 7 event) to another event
/// For relay feedback, use publish_reaction_tracked instead
pub async fn publish_reaction(
    event_id: String,
    event_author: String,
    content: String,
    emoji_tag: Option<(String, String)>,
) -> std::result::Result<String, String> {
    publish_reaction_tracked(event_id, event_author, content, emoji_tag)
        .await
        .map(|result| result.event_id)
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

    // Fetch from database/relays using outbox routing for better discovery
    // This routes the query to the author's preferred write relays
    match fetch_events_aggregated_outbox(filter, Duration::from_secs(10)).await {
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

/// Publish a contact list (kind 3 event) with relay feedback
/// NIP-02: https://github.com/nostr-protocol/nips/blob/master/02.md
pub async fn publish_contacts_tracked(contacts: Vec<String>) -> std::result::Result<PublishResult, String> {
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
                .map(Contact::new)
        })
        .collect();

    log::info!("Parsed {} valid contacts", contact_list.len());

    // Use EventBuilder::contact_list() for proper NIP-02 compliance
    // This allows for relay URLs and petnames (aliases) to be added in the future
    let builder = nostr::EventBuilder::contact_list(contact_list);

    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish contact list: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Contact list published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a contact list (kind 3 event)
/// For relay feedback, use publish_contacts_tracked instead
pub async fn publish_contacts(contacts: Vec<String>) -> std::result::Result<String, String> {
    publish_contacts_tracked(contacts)
        .await
        .map(|result| result.event_id)
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

// ============================================================================
// Mute List Tag Helpers (NIP-51)
// ============================================================================

/// Extracted tag categories from a mute list event (kind 10000)
/// Used to reduce code duplication in mute/unmute/block/unblock operations
struct MuteListTags {
    event_ids: Vec<nostr::EventId>,   // Muted posts (e tags)
    pubkeys: Vec<nostr::PublicKey>,   // Blocked users (p tags)
    hashtags: Vec<String>,            // Muted hashtags (t tags)
    words: Vec<String>,               // Muted words (word tags)
    other_tags: Vec<nostr::Tag>,      // Preserve unknown tags
}

/// Extract categorized tags from a kind 10000 mute list event
fn extract_mute_list_tags(event: &nostr::Event) -> MuteListTags {
    let mut tags = MuteListTags {
        event_ids: Vec::new(),
        pubkeys: Vec::new(),
        hashtags: Vec::new(),
        words: Vec::new(),
        other_tags: Vec::new(),
    };

    for tag in event.tags.iter() {
        if tag.kind() == nostr::TagKind::e() {
            if let Some(id) = tag.content() {
                if let Ok(eid) = nostr::EventId::from_hex(id) {
                    tags.event_ids.push(eid);
                }
            }
        } else if tag.kind() == nostr::TagKind::p() {
            if let Some(pk) = tag.content() {
                if let Ok(pubkey) = nostr::PublicKey::from_hex(pk) {
                    tags.pubkeys.push(pubkey);
                }
            }
        } else if tag.kind() == nostr::TagKind::t() {
            if let Some(hashtag) = tag.content() {
                tags.hashtags.push(hashtag.to_string());
            }
        } else if tag.kind() == nostr::TagKind::Custom("word".into()) {
            if let Some(word) = tag.content() {
                tags.words.push(word.to_string());
            }
        } else {
            // Preserve all other tags (e.g., 'a' address tags, future extensions)
            tags.other_tags.push(tag.clone());
        }
    }

    tags
}

/// Rebuild tags vec from categorized structure
fn rebuild_mute_list_tags(tags: &MuteListTags) -> Vec<nostr::Tag> {
    let mut all_tags = Vec::new();

    // Add e tags for muted posts
    for event_id in &tags.event_ids {
        all_tags.push(nostr::Tag::event(*event_id));
    }

    // Add p tags for blocked users
    for pubkey in &tags.pubkeys {
        all_tags.push(nostr::Tag::public_key(*pubkey));
    }

    // Add t tags for hashtags
    for hashtag in &tags.hashtags {
        all_tags.push(nostr::Tag::hashtag(hashtag.clone()));
    }

    // Add word tags
    for word in &tags.words {
        all_tags.push(nostr::Tag::custom(nostr::TagKind::Custom("word".into()), vec![word.clone()]));
    }

    // Re-attach preserved tags
    all_tags.extend(tags.other_tags.clone());

    all_tags
}

/// Mute a post (add to mute list kind 10000)
/// NIP-51: https://github.com/nostr-protocol/nips/blob/master/51.md
pub async fn mute_post(event_id: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish events.".to_string());
    }

    log::info!("Muting post: {}", event_id);

    let target_event_id = nostr::EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Fetch current mute list and extract tags
    let mute_event = fetch_mute_list().await?;
    let (mut tags, existing_content) = match mute_event {
        Some(event) => {
            let content = event.content.clone();
            (extract_mute_list_tags(&event), content)
        }
        None => (MuteListTags {
            event_ids: Vec::new(),
            pubkeys: Vec::new(),
            hashtags: Vec::new(),
            words: Vec::new(),
            other_tags: Vec::new(),
        }, String::new())
    };

    // Add new muted post if not already present
    if !tags.event_ids.contains(&target_event_id) {
        tags.event_ids.push(target_event_id);
    }

    let all_tags = rebuild_mute_list_tags(&tags);
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

    let target_event_id = nostr::EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Fetch current mute list and extract tags
    let mute_event = fetch_mute_list().await?
        .ok_or("No mute list found")?;
    let existing_content = mute_event.content.clone();
    let mut tags = extract_mute_list_tags(&mute_event);

    // Remove the target post
    tags.event_ids.retain(|eid| *eid != target_event_id);

    let all_tags = rebuild_mute_list_tags(&tags);
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

    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;
    log::info!("Blocking user: {}", normalized_pubkey);

    let target_pubkey = nostr::PublicKey::from_hex(&normalized_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch current mute list and extract tags
    let mute_event = fetch_mute_list().await?;
    let (mut tags, existing_content) = match mute_event {
        Some(event) => {
            let content = event.content.clone();
            (extract_mute_list_tags(&event), content)
        }
        None => (MuteListTags {
            event_ids: Vec::new(),
            pubkeys: Vec::new(),
            hashtags: Vec::new(),
            words: Vec::new(),
            other_tags: Vec::new(),
        }, String::new())
    };

    // Add new blocked user if not already present
    if !tags.pubkeys.contains(&target_pubkey) {
        tags.pubkeys.push(target_pubkey);
    }

    let all_tags = rebuild_mute_list_tags(&tags);
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

    let normalized_pubkey = crate::utils::nip19::normalize_pubkey(&pubkey)?;
    log::info!("Unblocking user: {}", normalized_pubkey);

    let target_pubkey = nostr::PublicKey::from_hex(&normalized_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch current mute list and extract tags
    let mute_event = fetch_mute_list().await?
        .ok_or("No mute list found")?;
    let existing_content = mute_event.content.clone();
    let mut tags = extract_mute_list_tags(&mute_event);

    // Remove the target user
    tags.pubkeys.retain(|pk| *pk != target_pubkey);

    let all_tags = rebuild_mute_list_tags(&tags);
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

/// Publish a repost (kind 6 event) with relay feedback
/// NIP-18: https://github.com/nostr-protocol/nips/blob/master/18.md
pub async fn publish_repost_tracked(
    event_id: String,
    _event_author: String,
    relay_url: Option<String>,
) -> std::result::Result<PublishResult, String> {
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

    let result = PublishResult::from_output(output);

    log::info!(
        "Repost published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a repost (kind 6 event) of another event
/// For relay feedback, use publish_repost_tracked instead
pub async fn publish_repost(
    event_id: String,
    event_author: String,
    relay_url: Option<String>,
) -> std::result::Result<String, String> {
    publish_repost_tracked(event_id, event_author, relay_url)
        .await
        .map(|result| result.event_id)
}

/// Delete a repost event (Kind 6) using NIP-9 Event Deletion
pub async fn delete_repost(repost_event_id: String) -> std::result::Result<(), String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached. Cannot delete events.".to_string());
    }

    log::info!("Deleting repost: {}", repost_event_id);

    let event_id = nostr::EventId::from_hex(&repost_event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;

    // Create deletion event (kind 5) using NIP-9
    // Include k-tag per NIP-9 recommendation for better relay interoperability
    use nostr::nips::nip09::EventDeletionRequest;
    let request = EventDeletionRequest::new().id(event_id);
    let builder = nostr::EventBuilder::delete(request)
        .tag(nostr::Tag::custom(nostr::TagKind::k(), vec![nostr::Kind::Repost.as_u16().to_string()]));

    client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish deletion: {}", e))?;

    log::info!("Repost deleted successfully");
    Ok(())
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
/// Delegates to relay::connection::fetch_event_by_coordinate_with_relays
pub async fn fetch_event_by_coordinate_with_relays(
    kind: u16,
    pubkey: String,
    identifier: String,
    relay_hints: Vec<String>,
) -> std::result::Result<Option<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;
    relay::fetch_event_by_coordinate_with_relays(&client, kind, &pubkey, &identifier, relay_hints).await
}

/// Publish profile metadata (Kind 0) with relay feedback
///
/// Updates the user's Nostr profile with the provided metadata
pub async fn publish_metadata_tracked(metadata: Metadata) -> std::result::Result<PublishResult, String> {
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

    let result = PublishResult::from_output(output);

    log::info!(
        "Metadata published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish profile metadata (Kind 0)
/// For relay feedback, use publish_metadata_tracked instead
pub async fn publish_metadata(metadata: Metadata) -> std::result::Result<String, String> {
    publish_metadata_tracked(metadata)
        .await
        .map(|result| result.event_id)
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

/// Publish a long-form article (Kind 30023) with relay feedback
/// NIP-23: https://github.com/nostr-protocol/nips/blob/master/23.md
pub async fn publish_article_tracked(
    title: String,
    summary: String,
    content: String,
    identifier: String,
    cover_image: String,
    hashtags: Vec<String>,
) -> std::result::Result<PublishResult, String> {
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
        Tag::title(title.clone()),
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

    // Add published_at timestamp (WASM-compatible)
    let timestamp = ((js_sys::Date::now() / 1000.0) as u64).to_string();

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

    let result = PublishResult::from_output(output);

    log::info!(
        "Article '{}' published: {} ({}/{} relays succeeded)",
        title,
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a long-form article (Kind 30023)
/// For relay feedback, use publish_article_tracked instead
pub async fn publish_article(
    title: String,
    summary: String,
    content: String,
    identifier: String,
    cover_image: String,
    hashtags: Vec<String>,
) -> std::result::Result<String, String> {
    publish_article_tracked(title, summary, content, identifier, cover_image, hashtags)
        .await
        .map(|result| result.event_id)
}

/// Detect MIME type from URL file extension
fn detect_mime_type(url: &str) -> Option<String> {
    let url_lower = url.to_lowercase();

    // Extract extension from URL (handles query params and fragments)
    let path = url_lower
        .split('?').next()?  // Remove query string
        .split('#').next()?; // Remove fragment
    let extension = path.split('.').next_back()?;

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

/// Publish a picture post (Kind 20) with relay feedback
/// NIP-68: https://github.com/nostr-protocol/nips/blob/master/68.md
pub async fn publish_picture_tracked(
    title: String,
    caption: String,
    image_urls: Vec<String>,
    hashtags: Vec<String>,
    location: String,
) -> std::result::Result<PublishResult, String> {
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
        Tag::title(title.clone()),
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

    let result = PublishResult::from_output(output);

    log::info!(
        "Picture '{}' published: {} ({}/{} relays succeeded)",
        title,
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a picture post (Kind 20)
/// For relay feedback, use publish_picture_tracked instead
pub async fn publish_picture(
    title: String,
    caption: String,
    image_urls: Vec<String>,
    hashtags: Vec<String>,
    location: String,
) -> std::result::Result<String, String> {
    publish_picture_tracked(title, caption, image_urls, hashtags, location)
        .await
        .map(|result| result.event_id)
}

/// Publish a video post (Kind 21 for landscape, Kind 22 for portrait) with relay feedback
/// NIP-71: https://github.com/nostr-protocol/nips/blob/master/71.md
pub async fn publish_video_tracked(
    title: String,
    description: String,
    video_url: String,
    thumbnail_url: String,
    hashtags: Vec<String>,
    is_portrait: bool,
) -> std::result::Result<PublishResult, String> {
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

    // Build tags per NIP-71
    use nostr::Tag;
    let mut tags = vec![
        Tag::title(title.clone()),
    ];

    // Build imeta tag with video metadata (NIP-71 + NIP-92)
    let mut imeta_fields = vec![
        format!("url {}", video_url),
    ];

    // Detect video mime type from extension (video-specific, not using detect_mime_type)
    let video_mime = {
        let url_lower = video_url.to_lowercase();
        let path = url_lower.split('?').next().unwrap_or(&url_lower);
        let ext = path.split('.').next_back().unwrap_or("");
        match ext {
            "mp4" | "m4v" => "video/mp4",
            "webm" => "video/webm",
            "mov" => "video/quicktime",
            "avi" => "video/x-msvideo",
            "mkv" => "video/x-matroska",
            "m3u8" => "application/x-mpegURL",
            "ts" => "video/MP2T",
            _ => "video/mp4", // Default to mp4
        }
    };
    imeta_fields.push(format!("m {}", video_mime));

    // Add thumbnail as image in imeta if provided
    if !thumbnail_url.is_empty() {
        imeta_fields.push(format!("image {}", thumbnail_url));
    }

    tags.push(Tag::custom(
        nostr::TagKind::Custom("imeta".into()),
        imeta_fields
    ));

    // Add hashtags
    for hashtag in hashtags {
        tags.push(Tag::hashtag(hashtag));
    }

    // Content is just the description per NIP-71
    let content = description;

    // Build the event
    let builder = nostr::EventBuilder::new(nostr::Kind::from(kind), content)
        .tags(tags);

    // Publish
    let output = client.send_event_builder(builder).await
        .map_err(|e| format!("Failed to publish video: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Video '{}' published: {} ({}/{} relays succeeded)",
        title,
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a video post (Kind 21 for landscape, Kind 22 for portrait)
/// For relay feedback, use publish_video_tracked instead
pub async fn publish_video(
    title: String,
    description: String,
    video_url: String,
    thumbnail_url: String,
    hashtags: Vec<String>,
    is_portrait: bool,
) -> std::result::Result<String, String> {
    publish_video_tracked(title, description, video_url, thumbnail_url, hashtags, is_portrait)
        .await
        .map(|result| result.event_id)
}

/// Publish a voice message (Kind 1222) with relay feedback
/// NIP-A0: https://github.com/nostr-protocol/nips/blob/master/A0.md
pub async fn publish_voice_message_tracked(
    audio_url: String,
    duration: f64,
    waveform: Vec<u8>,
    hashtags: Vec<String>,
    mime_type: Option<String>,
) -> std::result::Result<PublishResult, String> {
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

    let result = PublishResult::from_output(output);

    log::info!(
        "Voice message published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a voice message (Kind 1222)
/// For relay feedback, use publish_voice_message_tracked instead
pub async fn publish_voice_message(
    audio_url: String,
    duration: f64,
    waveform: Vec<u8>,
    hashtags: Vec<String>,
    mime_type: Option<String>,
) -> std::result::Result<String, String> {
    publish_voice_message_tracked(audio_url, duration, waveform, hashtags, mime_type)
        .await
        .map(|result| result.event_id)
}

/// Publish a voice message reply (Kind 1244) with relay feedback
/// NIP-A0: https://github.com/nostr-protocol/nips/blob/master/A0.md
/// NIP-22: https://github.com/nostr-protocol/nips/blob/master/22.md
pub async fn publish_voice_message_reply_tracked(
    audio_url: String,
    duration: f64,
    waveform: Vec<u8>,
    reply_to: nostr::Event,
    mime_type: Option<String>,
) -> std::result::Result<PublishResult, String> {
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

    let result = PublishResult::from_output(output);

    log::info!(
        "Voice message reply published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a voice message reply (Kind 1244) following NIP-22
/// For relay feedback, use publish_voice_message_reply_tracked instead
pub async fn publish_voice_message_reply(
    audio_url: String,
    duration: f64,
    waveform: Vec<u8>,
    reply_to: nostr::Event,
    mime_type: Option<String>,
) -> std::result::Result<String, String> {
    publish_voice_message_reply_tracked(audio_url, duration, waveform, reply_to, mime_type)
        .await
        .map(|result| result.event_id)
}

/// Get user's public key from cache (no signer call needed)
///
/// This is much faster than calling signer().get_public_key() especially for:
/// - NIP-46 remote signers (avoids network roundtrip)
/// - Browser extensions (avoids extension API call)
///
/// Use this when you just need the pubkey, not for signing operations.
pub fn get_cached_pubkey() -> std::result::Result<PublicKey, String> {
    let pubkey_str = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;
    PublicKey::parse(&pubkey_str)
        .map_err(|e| format!("Invalid cached pubkey: {}", e))
}

/// Get the current user's public key (uses cache, no signer call)
pub async fn get_user_pubkey() -> std::result::Result<PublicKey, String> {
    get_cached_pubkey()
}

/// Publish a poll vote (Kind 1018) with relay feedback
/// NIP-88: https://github.com/nostr-protocol/nips/blob/master/88.md
/// Votes are published to the relays specified in the poll event
pub async fn publish_poll_vote_tracked(
    poll_id: nostr::EventId,
    response: nostr::nips::nip88::PollResponse,
    poll_relays: Vec<nostr::RelayUrl>,
) -> std::result::Result<PublishResult, String> {
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
        // Add poll relays temporarily using specialty helpers
        let added_relays = relay::add_relays(&client, &poll_relays).await;

        // Use non-blocking relay ready check instead of blocking connect()
        ensure_relays_ready(&client).await;

        // Check if any poll relays are actually connected
        let connected_poll_relays = relay::get_connected(&client, &poll_relays).await;

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
        relay::remove_relays(&client, &added_relays).await;

        result?
    } else {
        // No poll relays specified, use default relays
        client.send_event_builder(builder).await
            .map_err(|e| format!("Failed to publish poll vote: {}", e))?
    };

    let result = PublishResult::from_output(output);

    log::info!(
        "Poll vote published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a poll vote (Kind 1018) following NIP-88
/// For relay feedback, use publish_poll_vote_tracked instead
pub async fn publish_poll_vote(
    poll_id: nostr::EventId,
    response: nostr::nips::nip88::PollResponse,
    poll_relays: Vec<nostr::RelayUrl>,
) -> std::result::Result<String, String> {
    publish_poll_vote_tracked(poll_id, response, poll_relays)
        .await
        .map(|result| result.event_id)
}

/// Publish a poll (Kind 1068) with relay feedback
/// NIP-88: https://github.com/nostr-protocol/nips/blob/master/88.md
pub async fn publish_poll_tracked(
    title: String,
    poll_type: nostr::nips::nip88::PollType,
    options: Vec<nostr::nips::nip88::PollOption>,
    relays: Vec<String>,
    ends_at: Option<nostr::Timestamp>,
    hashtags: Vec<String>,
) -> std::result::Result<PublishResult, String> {
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
        title: title.clone(),
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

    let result = PublishResult::from_output(output);

    log::info!(
        "Poll '{}' published: {} ({}/{} relays succeeded)",
        title,
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a poll (Kind 1068) following NIP-88
/// For relay feedback, use publish_poll_tracked instead
pub async fn publish_poll(
    title: String,
    poll_type: nostr::nips::nip88::PollType,
    options: Vec<nostr::nips::nip88::PollOption>,
    relays: Vec<String>,
    ends_at: Option<nostr::Timestamp>,
    hashtags: Vec<String>,
) -> std::result::Result<String, String> {
    publish_poll_tracked(title, poll_type, options, relays, ends_at, hashtags)
        .await
        .map(|result| result.event_id)
}

// =============================================================================
// Custom NIPs (Kind 30817) - Addressable events for community NIP proposals
// =============================================================================

/// Kind 30817 - Custom NIP (addressable event)
pub const KIND_CUSTOM_NIP: u16 = 30817;

/// Fetch custom NIPs (kind 30817) from relays
pub async fn fetch_custom_nips(
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let filter = {
        let mut f = Filter::new()
            .kind(Kind::Custom(KIND_CUSTOM_NIP))
            .limit(limit);

        if let Some(until_ts) = until {
            f = f.until(Timestamp::from(until_ts));
        }

        f
    };

    fetch_events_aggregated(filter, Duration::from_secs(10)).await
}

/// Fetch a specific custom NIP by decoding an naddr identifier
pub async fn fetch_custom_nip_by_naddr(
    naddr: &str,
) -> std::result::Result<Option<nostr::Event>, String> {
    use nostr::nips::nip19::Nip19;

    // Decode naddr to get coordinate
    let nip19 = Nip19::from_bech32(naddr)
        .map_err(|e| format!("Invalid naddr: {}", e))?;

    match nip19 {
        Nip19::Coordinate(nip19_coord) => {
            let coord = nip19_coord.coordinate;

            let filter = Filter::new()
                .kind(coord.kind)
                .author(coord.public_key)
                .identifier(coord.identifier);

            let events = fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
            Ok(events.into_iter().next())
        }
        _ => Err("Not a coordinate (naddr) identifier".to_string()),
    }
}

/// Publish a custom NIP as a kind 30817 addressable event with relay tracking
pub async fn publish_custom_nip_tracked(
    title: String,
    content: String,
    identifier: String,
    related_kinds: Vec<u32>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    use nostr::{EventBuilder, Kind, Tag, SingleLetterTag, Alphabet};

    // Build event with required d-tag and optional tags
    let mut builder = EventBuilder::new(Kind::Custom(KIND_CUSTOM_NIP), &content)
        .tag(Tag::identifier(&identifier))
        .tag(Tag::title(&title));

    // Add k tags for related event kinds
    for kind in related_kinds {
        builder = builder.tag(Tag::custom(
            TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::K)),
            vec![kind.to_string()],
        ));
    }

    let output = client.send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish custom NIP: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Custom NIP published: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a custom NIP as a kind 30817 addressable event
pub async fn publish_custom_nip(
    title: String,
    content: String,
    identifier: String,
    related_kinds: Vec<u32>,
) -> std::result::Result<String, String> {
    publish_custom_nip_tracked(title, content, identifier, related_kinds)
        .await
        .map(|result| result.event_id)
}

// ============================================================================
// Relay-Specific Publishing Functions
// Note: With NIP-65 gossip routing, SDK handles relay selection automatically.
// These functions are available for advanced use cases but not typically needed.
// ============================================================================

/// Publish a note to specific relays only
///
/// Useful for privacy-conscious publishing or targeting specific relay groups.
#[allow(dead_code)]
pub async fn publish_note_to_relays(
    content: String,
    tags: Vec<Vec<String>>,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    // Convert raw tags to nostr::Tag format
    let nostr_tags: Vec<nostr::Tag> = tags.iter()
        .filter_map(|tag| {
            if tag.is_empty() {
                return None;
            }
            Some(nostr::Tag::custom(
                nostr::TagKind::Custom(std::borrow::Cow::Owned(tag[0].clone())),
                tag[1..].to_vec(),
            ))
        })
        .collect();

    let builder = nostr::EventBuilder::text_note(&content)
        .tags(nostr_tags);

    // Parse relay URLs
    let urls: Vec<nostr::RelayUrl> = relay_urls
        .iter()
        .filter_map(|r| nostr::RelayUrl::parse(r).ok())
        .collect();

    if urls.is_empty() {
        return Err("No valid relay URLs provided".to_string());
    }

    let output = client.send_event_builder_to(urls.clone(), builder)
        .await
        .map_err(|e| format!("Failed to publish: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Note published to specific relays: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    if result.has_failures() {
        for (relay, error) in &result.failed_relays {
            log::warn!("Relay {} failed: {}", relay, error);
        }
    }

    Ok(result)
}

/// Publish a reaction to specific relays only
#[allow(dead_code)]
pub async fn publish_reaction_to_relays(
    event_id: String,
    event_pubkey: String,
    reaction: String,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    if !*HAS_SIGNER.read() {
        return Err("No signer attached".to_string());
    }

    use nostr::nips::nip25::ReactionTarget;

    let target_event_id = nostr::EventId::from_hex(&event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let target_pubkey = PublicKey::from_hex(&event_pubkey)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Create reaction target
    let target = ReactionTarget {
        event_id: target_event_id,
        public_key: target_pubkey,
        coordinate: None,
        kind: None,
        relay_hint: None,
    };

    let builder = EventBuilder::reaction(target, reaction);

    let urls: Vec<nostr::RelayUrl> = relay_urls
        .iter()
        .filter_map(|r| nostr::RelayUrl::parse(r).ok())
        .collect();

    if urls.is_empty() {
        return Err("No valid relay URLs provided".to_string());
    }

    let output = client.send_event_builder_to(urls, builder)
        .await
        .map_err(|e| format!("Failed to publish reaction: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Reaction published to specific relays: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    Ok(result)
}

/// Send a pre-signed event to specific relays
///
/// Takes an already-signed Event and sends it directly to the specified relays,
/// preserving the original cryptographic signature.
pub async fn send_presigned_event_to_relays(
    event: nostr::Event,
    relay_urls: Vec<String>,
) -> std::result::Result<PublishResult, String> {
    let client = get_client().ok_or("Client not initialized")?;

    let urls: Vec<nostr::RelayUrl> = relay_urls
        .iter()
        .filter_map(|r| nostr::RelayUrl::parse(r).ok())
        .collect();

    if urls.is_empty() {
        return Err("No valid relay URLs provided".to_string());
    }

    let output = client.send_event_to(urls, &event)
        .await
        .map_err(|e| format!("Failed to send event: {}", e))?;

    let result = PublishResult::from_output(output);

    log::info!(
        "Pre-signed event sent to specific relays: {} ({}/{} relays succeeded)",
        result.event_id,
        result.success_count(),
        result.total_attempted()
    );

    Ok(result)
}

// ============================================================================
// Progressive Loading / Streaming Functions
// ============================================================================

/// Stream events progressively with a callback for each event
///
/// Unlike fetch_events which waits for all events, this function calls the
/// provided callback as each event arrives, enabling progressive UI updates.
///
/// # Arguments
/// * `filter` - The filter to use for the subscription
/// * `timeout` - Maximum duration to wait for events
/// * `on_event` - Callback invoked for each event received
///
/// # Returns
/// Total count of events received
#[allow(dead_code)]
pub async fn stream_events_with_callback<F>(
    filter: Filter,
    timeout: std::time::Duration,
    mut on_event: F,
) -> std::result::Result<usize, String>
where
    F: FnMut(nostr::Event) + Send,
{
    use futures::StreamExt;

    let client = get_client().ok_or("Client not initialized")?;

    let mut stream = client.stream_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to create event stream: {}", e))?;

    let mut count = 0;

    while let Some(event) = stream.next().await {
        on_event(event);
        count += 1;
    }

    log::info!("Stream completed: received {} events", count);
    Ok(count)
}

/// Stream events with gossip routing, calling a callback for each batch
///
/// This function is optimized for progressive UI updates. It:
/// 1. Waits for user relay lists to be applied (like fetch_events_aggregated_outbox)
/// 2. Streams events as they arrive
/// 3. Calls the callback with batches of events for efficient UI updates
///
/// # Arguments
/// * `filter` - The filter to use for the subscription
/// * `timeout` - Maximum duration to wait for events
/// * `batch_size` - Number of events to collect before calling on_batch
/// * `on_batch` - Callback invoked with each batch of events
///
/// # Returns
/// Total count of events received
pub async fn stream_events_batched<F>(
    filter: Filter,
    timeout: std::time::Duration,
    batch_size: usize,
    mut on_batch: F,
) -> std::result::Result<usize, String>
where
    F: FnMut(Vec<nostr::Event>),
{
    use futures::StreamExt;

    let client = get_client().ok_or("Client not initialized")?;

    // Wait for user relays if signed in (up to 2 seconds)
    // This ensures gossip routing uses the user's configured relays
    if *HAS_SIGNER.peek() && !*USER_RELAYS_APPLIED.peek() {
        log::debug!("Streaming: Waiting for user relay lists to be applied...");
        let start = instant::Instant::now();

        #[cfg(target_arch = "wasm32")]
        {
            while !*USER_RELAYS_APPLIED.peek() && start.elapsed() < Duration::from_secs(2) {
                gloo_timers::future::TimeoutFuture::new(50).await;
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            while !*USER_RELAYS_APPLIED.peek() && start.elapsed() < Duration::from_secs(2) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }

        if *USER_RELAYS_APPLIED.peek() {
            log::debug!("Streaming: User relay lists applied after {}ms", start.elapsed().as_millis());
        } else {
            log::warn!("Streaming: User relay lists not applied after timeout, proceeding with defaults");
        }
    }

    // Wait for at least one relay to be ready
    ensure_relays_ready(&client).await;

    let mut stream = client.stream_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to create event stream: {}", e))?;

    let mut total_count = 0;
    let mut batch = Vec::with_capacity(batch_size);

    while let Some(event) = stream.next().await {
        batch.push(event);
        total_count += 1;

        // Deliver batch when we reach batch_size
        if batch.len() >= batch_size {
            let items = std::mem::take(&mut batch);
            batch.reserve(batch_size);
            on_batch(items);
        }
    }

    // Deliver any remaining events
    if !batch.is_empty() {
        on_batch(batch);
    }

    log::info!("Stream completed: received {} events in batches", total_count);
    Ok(total_count)
}

/// Stream events from connected relays only (bypasses gossip discovery)
///
/// FAST alternative to stream_events_batched that:
/// 1. Only queries already-connected relays (no relay discovery)
/// 2. Bypasses the gossip model - no NIP-65 lookups per author
/// 3. Returns results much faster but may miss events from unconnected relays
///
/// Use for initial feed load where speed is critical.
pub async fn stream_events_from_connected_relays_batched<F>(
    filter: Filter,
    timeout: std::time::Duration,
    batch_size: usize,
    mut on_batch: F,
) -> std::result::Result<usize, String>
where
    F: FnMut(Vec<nostr::Event>),
{
    use futures::StreamExt;
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;

    let client = get_client().ok_or("Client not initialized")?;
    ensure_relays_ready(&client).await;

    // Get connected relay URLs
    let relays = client.relays().await;
    let connected_urls: Vec<nostr::RelayUrl> = relays
        .iter()
        .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
        .filter_map(|(url, _)| nostr::RelayUrl::parse(url.as_str()).ok())
        .collect();

    if connected_urls.is_empty() {
        log::warn!("No connected relays, falling back to gossip stream");
        return stream_events_batched(filter, timeout, batch_size, on_batch).await;
    }

    log::info!("Fast streaming from {} connected relays (bypassing gossip)", connected_urls.len());

    // Capture authors for client-side filtering (defense-in-depth)
    // Relays may return events from any author, ignoring the filter
    let filter_authors = filter.authors.clone();
    let author_set: Option<std::collections::HashSet<_>> = filter_authors.as_ref()
        .map(|authors| authors.iter().collect());

    // Use stream_events_from which bypasses gossip entirely
    let mut stream = client
        .stream_events_from(connected_urls, filter, timeout)
        .await
        .map_err(|e| format!("Failed to create stream: {}", e))?;

    let mut total_count = 0;
    let mut filtered_count = 0;
    let mut batch = Vec::with_capacity(batch_size);

    while let Some(event) = stream.next().await {
        // Client-side author filtering (defense-in-depth against misbehaving relays)
        if let Some(ref authors) = author_set {
            if !authors.contains(&event.pubkey) {
                filtered_count += 1;
                continue;  // Skip events from non-followed authors
            }
        }

        batch.push(event);
        total_count += 1;

        if batch.len() >= batch_size {
            let items = std::mem::take(&mut batch);
            batch.reserve(batch_size);
            on_batch(items);
        }
    }

    if !batch.is_empty() {
        on_batch(batch);
    }

    if filtered_count > 0 {
        log::info!("Fast stream completed: {} events ({} filtered out from non-followed authors)", total_count, filtered_count);
    } else {
        log::info!("Fast stream completed: {} events from connected relays", total_count);
    }
    Ok(total_count)
}

/// Fetch events from connected relays only (bypasses gossip discovery)
///
/// FAST alternative to fetch_events_aggregated_outbox for pagination:
/// 1. Only queries already-connected relays (no relay discovery)
/// 2. Bypasses the gossip model - no NIP-65 lookups per author
/// 3. Returns results much faster but may miss events from unconnected relays
///
/// Includes client-side author filtering for defense-in-depth against
/// misbehaving relays that ignore filter authors.
pub async fn fetch_events_from_connected_relays(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    use nostr_relay_pool::RelayStatus as PoolRelayStatus;

    let client = get_client().ok_or("Client not initialized")?;
    ensure_relays_ready(&client).await;

    let relays = client.relays().await;
    let connected_urls: Vec<nostr::RelayUrl> = relays
        .iter()
        .filter(|(_, r)| r.status() == PoolRelayStatus::Connected)
        .filter_map(|(url, _)| nostr::RelayUrl::parse(url.as_str()).ok())
        .collect();

    if connected_urls.is_empty() {
        log::warn!("No connected relays, falling back to gossip fetch");
        return fetch_events_aggregated_outbox(filter.clone(), timeout).await;
    }

    log::info!("Fast fetching from {} connected relays (bypassing gossip)", connected_urls.len());

    // Capture authors for client-side filtering (defense-in-depth)
    let filter_authors = filter.authors.clone();
    let author_set: Option<std::collections::HashSet<_>> = filter_authors.as_ref()
        .map(|authors| authors.iter().collect());

    let events = client.fetch_events_from(connected_urls, filter, timeout).await
        .map_err(|e| format!("Failed to fetch events: {}", e))?;

    // Client-side author filtering (defense-in-depth against misbehaving relays)
    let result: Vec<nostr::Event> = events.into_iter()
        .filter(|event| {
            if let Some(ref authors) = author_set {
                authors.contains(&event.pubkey)
            } else {
                true
            }
        })
        .collect();

    log::info!("Fast fetch completed: {} events (after filtering)", result.len());
    Ok(result)
}

/// Fetch video events from connected relays (bypasses gossip)
///
/// Ensures video relay (relay.divine.video) is connected first,
/// then uses fast fetch (bypasses gossip) for the query.
pub async fn fetch_video_events_from_connected_relays(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let client = get_client().ok_or("Client not initialized")?;

    // Ensure video relay is in the pool
    ensure_video_relay_connected(&client).await;

    // Use fast fetch (bypasses gossip)
    fetch_events_from_connected_relays(filter, timeout).await
}

/// Stream events and collect them into a Vec
///
/// This is a convenience wrapper that collects all streamed events
/// into a vector with deduplication and sorting.
#[allow(dead_code)]
pub async fn stream_events_collected(
    filter: Filter,
    timeout: std::time::Duration,
) -> std::result::Result<Vec<nostr::Event>, String> {
    use futures::StreamExt;

    let client = get_client().ok_or("Client not initialized")?;

    let mut stream = client.stream_events(filter, timeout)
        .await
        .map_err(|e| format!("Failed to create event stream: {}", e))?;

    let mut events = Vec::new();

    while let Some(event) = stream.next().await {
        events.push(event);
    }

    // Deduplicate by event ID (events may come from multiple relays)
    events.sort_by(|a, b| a.id.cmp(&b.id));
    events.dedup_by(|a, b| a.id == b.id);

    // Sort by created_at descending (newest first)
    events.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    log::info!("Stream completed: collected {} unique events", events.len());
    Ok(events)
}

/// Generate an naddr for a custom NIP event
pub fn generate_custom_nip_naddr(
    pubkey: &PublicKey,
    identifier: &str,
    relays: Vec<String>,
) -> std::result::Result<String, String> {
    use nostr::nips::nip01::Coordinate;
    use nostr::nips::nip19::Nip19Coordinate;

    let coordinate = Coordinate::new(Kind::Custom(KIND_CUSTOM_NIP), *pubkey)
        .identifier(identifier);

    let relay_urls: Vec<nostr::RelayUrl> = relays
        .iter()
        .filter_map(|r| nostr::RelayUrl::parse(r).ok())
        .collect();

    let nip19_coord = Nip19Coordinate::new(coordinate, relay_urls);

    nip19_coord.to_bech32()
        .map_err(|e| format!("Failed to generate naddr: {}", e))
}

/// Search custom NIPs using NIP-50 full-text search
pub async fn search_custom_nips(
    query: &str,
    limit: usize,
) -> std::result::Result<Vec<nostr::Event>, String> {
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_CUSTOM_NIP))
        .search(query)
        .limit(limit);

    fetch_events_aggregated(filter, Duration::from_secs(10)).await
}

// Re-export RelayDisplayInfo from relay module for backward compatibility
pub use relay::RelayDisplayInfo;

/// Get display info for all connected relays (for Connections tab in settings)
///
/// This is a convenience wrapper that calls get_client() internally.
/// See [`relay::get_relay_display_info`] for the implementation.
pub async fn get_relay_display_info() -> Vec<RelayDisplayInfo> {
    let Some(client) = get_client() else {
        return vec![];
    };
    relay::get_relay_display_info(&client).await
}
