//! Specialty relay management
//!
//! Provides unified handling for specialty relays (video, GIF, DVM, etc.)
//! using nostr-sdk's built-in relay management APIs.
//!
//! # Design
//!
//! Specialty relays are relays that host specific content types (video, GIFs, etc.)
//! and need to be added to the pool temporarily or for the session. This module
//! provides consistent patterns for:
//!
//! - Adding relays temporarily and tracking which were newly added
//! - Ensuring relays are connected before querying
//! - Cleaning up temporary relays after use
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;
use std::time::Duration;
/// Well-known specialty relay URLs
pub mod urls {
    pub const VIDEO: &str = "wss://relay.divine.video";
    pub const GIF: &str = "wss://relay.gifbuddy.lol";
    pub const RADIO: &str = "wss://relay.wavefunc.live";
    pub const RADIO_FALLBACK: &str = "wss://nos.lol";
    /// Basspistol collective music aggregator — primary host of kind-36787
    /// (Nostr music track) events.
    pub const MUSIC_BASSPISTOL: &str = "wss://drops.basspistol.org";
    /// nostria's music relay — additional kind-36787 breadth.
    pub const MUSIC_NOSTRIA: &str = "wss://ribo.nostria.app";
    /// Livelier bridge — Owncast → Nostr NIP-53 live streams (kind 30311);
    /// chat (kind 1311) flows through the events' `relays` tag.
    pub const LIVELIER: &str = "wss://livestream.livelier.live";
}
/// Default options for specialty relays
pub fn specialty_relay_options() -> RelayOptions {
    RelayOptions::new()
        .max_avg_latency(Some(Duration::from_secs(5)))
        .verify_subscriptions(true)
        .adjust_retry_interval(true)
        .reconnect(true)
}

/// Relay options for P2P daemon relays. No `sleep_when_idle` because these
/// relays must maintain persistent GiftWrap subscriptions.
pub fn p2p_relay_options() -> RelayOptions {
    RelayOptions::new()
        .max_avg_latency(Some(Duration::from_secs(5)))
        .verify_subscriptions(true)
        .adjust_retry_interval(true)
        .reconnect(true)
}
/// Add relays temporarily, returning which ones were newly added.
/// Uses SDK's add_relay() which returns `Result<bool, Error>`:
/// - `Ok(true)`  → newly added
/// - `Ok(false)` → already existed in the pool
/// - `Err(_)`    → real failure (invalid URL, pool error, etc.)
pub async fn add_relays(client: &Client, relay_urls: &[RelayUrl]) -> Vec<RelayUrl> {
    let mut added = Vec::new();
    for relay_url in relay_urls {
        match client.add_relay(relay_url.clone()).await {
            Ok(true) => {
                log::debug!("Added temporary relay: {}", relay_url);
                added.push(relay_url.clone());
            }
            Ok(false) => {
                log::debug!("Relay already existed: {}", relay_url);
            }
            Err(e) => {
                log::debug!("Could not add relay {}: {}", relay_url, e);
            }
        }
    }
    added
}
/// Add relays from string URLs, returning which ones were newly added.
#[allow(dead_code)]
pub async fn add_relays_from_strings(client: &Client, urls: &[String]) -> Vec<RelayUrl> {
    let relay_urls: Vec<RelayUrl> = urls
        .iter()
        .filter_map(|u| RelayUrl::parse(u).ok())
        .collect();
    add_relays(client, &relay_urls).await
}
/// Remove specified relays from the pool.
pub async fn remove_relays(client: &Client, relays: &[RelayUrl]) {
    for relay_url in relays {
        if let Err(e) = client.remove_relay(relay_url.clone()).await {
            log::debug!("Could not remove relay {}: {}", relay_url, e);
        }
    }
}
/// Check which of the given relays are actually connected.
#[allow(dead_code)]
pub async fn get_connected(client: &Client, relay_urls: &[RelayUrl]) -> Vec<RelayUrl> {
    let relays = client.relays().await;
    relay_urls
        .iter()
        .filter(|r| {
            relays
                .get(*r)
                .map(|relay| relay.is_connected())
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}
/// Ensure a specialty relay is connected (session-persistent).
/// Adds the relay if not present and waits for connection.
pub async fn ensure_connected(client: &Client, relay_url: &str) -> bool {
    let Ok(url) = RelayUrl::parse(relay_url) else {
        log::warn!("Invalid relay URL: {}", relay_url);
        return false;
    };
    let relays = client.relays().await;
    match relays.get(&url) {
        Some(relay) => {
            if relay.status() == RelayStatus::Connected {
                return true;
            }
        }
        None => {
            match client
                .pool()
                .add_relay(url.clone(), specialty_relay_options())
                .await
            {
                Ok(true) => log::info!("Added specialty relay: {}", relay_url),
                Ok(false) => log::debug!("Specialty relay already existed: {}", relay_url),
                Err(e) => {
                    log::warn!("Failed to add specialty relay {}: {}", relay_url, e);
                    return false;
                }
            }
        }
    }
    if let Err(e) = client.pool().connect_relay(url.clone()).await {
        log::warn!("Failed to connect to specialty relay {}: {}", relay_url, e);
        return false;
    }
    let relays = client.relays().await;
    if let Some(relay) = relays.get(&url) {
        relay.wait_for_connection(Duration::from_secs(30)).await;
        if relay.status() == RelayStatus::Connected {
            log::info!("Specialty relay connected: {}", relay_url);
            return true;
        }
    }
    log::warn!("Specialty relay connection timeout: {}", relay_url);
    false
}
/// Ensure video relay is connected (session-persistent).
pub async fn ensure_video_relay(client: &Client) -> bool {
    ensure_connected(client, urls::VIDEO).await
}
/// Ensure video relay is connected, bounded by a timeout race.
///
/// `ensure_connected` internally waits up to 30s for the connection, which
/// would stall callers when the relay is unreachable. This races it against a
/// plain sleep and gives up after `timeout`, returning whether the relay was
/// confirmed connected in time. On timeout the connection attempt itself is
/// unaffected: `Relay::connect` is fire-and-forget (the SDK owns the
/// connection task), so the relay keeps retrying in the pool and later
/// surfaces pick it up once it eventually connects.
pub async fn ensure_video_relay_connected_bounded(client: &Client, timeout: Duration) -> bool {
    use futures::future::{select, Either};
    use futures::pin_mut;

    let ensure_fut = ensure_video_relay(client);
    let sleep_fut = crate::platform::timer::sleep(timeout);
    pin_mut!(ensure_fut, sleep_fut);
    match select(ensure_fut, sleep_fut).await {
        Either::Left((connected, _)) => connected,
        Either::Right(_) => {
            log::warn!(
                "Video relay connection wait exceeded {:?}; proceeding without it",
                timeout
            );
            false
        }
    }
}
/// Ensure GIF relay is connected (session-persistent).
pub async fn ensure_gif_relay(client: &Client) -> bool {
    ensure_connected(client, urls::GIF).await
}
/// Ensure radio relay is connected (session-persistent).
pub async fn ensure_radio_relay(client: &Client) -> bool {
    let a = ensure_connected(client, urls::RADIO).await;
    let b = ensure_connected(client, urls::RADIO_FALLBACK).await;
    a || b
}
/// Ensure music relays are connected (session-persistent). These host the bulk
/// of kind-36787 (Nostr music track) events that general-purpose relays lack.
/// The connects run in parallel AND are bounded (~3s): `ensure_connected`
/// waits up to 30s per relay on a cold connect, which used to stall the
/// /music critical path. REQs queue on still-Connecting relays (the pool
/// owns the connect task), so giving up early just means the fetch runs on
/// whatever is connected and picks the music relays up next refresh.
pub async fn ensure_music_relays(client: &Client) -> bool {
    use futures::future::{select, Either};
    use futures::pin_mut;

    let ensure_fut = async {
        let (a, b) = tokio::join!(
            ensure_connected(client, urls::MUSIC_BASSPISTOL),
            ensure_connected(client, urls::MUSIC_NOSTRIA),
        );
        a || b
    };
    let sleep_fut = crate::platform::timer::sleep(Duration::from_secs(3));
    pin_mut!(ensure_fut, sleep_fut);
    match select(ensure_fut, sleep_fut).await {
        Either::Left((connected, _)) => connected,
        Either::Right(_) => {
            log::debug!(
                "Music relay connection wait exceeded 3s; proceeding without them"
            );
            false
        }
    }
}
/// Ensure the Livelier livestream bridge relay is in the pool and connecting.
///
/// Non-blocking: the SDK's `connect_relay` is fire-and-forget (it early-returns
/// for relays already Connecting/Connected), and `fetch_events_from` queues
/// REQs for still-Connecting relays, so callers never wait on the socket.
/// Once connected, the relay joins connected-pool snapshots automatically.
///
/// Uses the default `specialty_relay_options()` (READ|WRITE|PING): the WRITE
/// flag is required so pool-member `send_event_to` can deliver kind-1311 chat
/// when a bridged 30311 event's `relays` tag points at this relay
/// (`can_write()` = WRITE|GOSSIP).
pub async fn ensure_livestream_relays_connected(client: &Client) {
    let Ok(url) = RelayUrl::parse(urls::LIVELIER) else {
        log::warn!("Invalid Livelier relay URL: {}", urls::LIVELIER);
        return;
    };
    let already_in_pool = client.relays().await.contains_key(&url);
    if !already_in_pool {
        match client
            .pool()
            .add_relay(url.clone(), specialty_relay_options())
            .await
        {
            Ok(true) => log::info!("Added Livelier livestream relay: {}", urls::LIVELIER),
            Ok(false) => {}
            Err(e) => {
                log::warn!("Failed to add Livelier relay {}: {}", urls::LIVELIER, e);
                return;
            }
        }
    }
    if let Err(e) = client.pool().connect_relay(url).await {
        log::debug!("Livelier relay connect initiated (may be in-flight): {}", e);
    }
}
/// Ensure DM inbox relays are connected with privacy-respecting fallback.
///
/// Fallback behavior:
/// - If user configured kind 10050 relays: ONLY use those (privacy-first)
/// - If no 10050 configured: fallback to 10002 → defaults (UX-first)
///
/// This follows NIP-17 privacy requirements: users choose 10050 relays specifically
/// for DM privacy, so we shouldn't leak DM subscriptions to other relays.
///
/// Returns the list of relay URLs that successfully connected.
pub async fn ensure_dm_relays_connected(client: &Client) -> Vec<String> {
    // Tier 1: Try kind 10050 DM relays
    let dm_relays_10050 = super::nip65::get_dm_relays_10050_only();
    let user_configured_dm_relays = !dm_relays_10050.is_empty();

    if user_configured_dm_relays {
        log::info!("Trying {} kind 10050 DM relays...", dm_relays_10050.len());
        let connected = try_connect_relay_list(client, &dm_relays_10050).await;
        if !connected.is_empty() {
            log::info!(
                "Connected to {} kind 10050 DM relays: {:?}",
                connected.len(),
                connected
            );
            return connected;
        }
        // User explicitly configured DM relays - don't fallback (privacy-first per NIP-17)
        log::warn!(
            "No kind 10050 relays connected. Skipping fallback to preserve DM privacy. \
             User configured {} DM relays but none are reachable.",
            dm_relays_10050.len()
        );
        return Vec::new();
    }

    // No 10050 configured - user hasn't set DM relay preferences, fallback is OK
    log::info!("No kind 10050 relays configured, trying kind 10002...");

    // Tier 2: Fall back to kind 10002 write relays
    let write_relays = super::nip65::get_write_relays();
    if !write_relays.is_empty() {
        log::info!("Trying {} kind 10002 write relays...", write_relays.len());
        let connected = try_connect_relay_list(client, &write_relays).await;
        if !connected.is_empty() {
            log::info!(
                "Connected to {} kind 10002 write relays for DMs: {:?}",
                connected.len(),
                connected
            );
            return connected;
        }
        log::warn!("No kind 10002 relays connected, falling back to defaults...");
    } else {
        log::info!("No kind 10002 relays configured, trying defaults...");
    }

    // Tier 3: Fall back to hardcoded defaults
    let defaults = super::nip65::default_dm_relays();
    log::info!("Trying {} default DM relays...", defaults.len());
    let connected = try_connect_relay_list(client, &defaults).await;
    if !connected.is_empty() {
        log::info!(
            "Connected to {} default DM relays: {:?}",
            connected.len(),
            connected
        );
        return connected;
    }

    log::error!("No DM relays could be connected from any tier!");
    Vec::new()
}

/// Maximum concurrent relay connection attempts to prevent overwhelming WASM event loop
const MAX_CONCURRENT_RELAY_CONNECTIONS: usize = 5;

/// Helper to force connection to a list of relays in parallel with bounded concurrency.
/// Uses ensure_connected which adds to pool + waits for connection.
async fn try_connect_relay_list(client: &Client, relays: &[String]) -> Vec<String> {
    use futures::stream::{self, StreamExt};

    stream::iter(relays.iter().cloned())
        .map(|relay_url| {
            let client = client.clone();
            async move {
                if ensure_connected(&client, &relay_url).await {
                    Some(relay_url)
                } else {
                    None
                }
            }
        })
        .buffer_unordered(MAX_CONCURRENT_RELAY_CONNECTIONS)
        .filter_map(|result| async { result })
        .collect()
        .await
}
/// Ensure search relays are connected (session-persistent).
/// Adds user's search relays (kind 10007) or defaults to the pool and connects them.
/// Returns the list of relay URLs that successfully connected.
///
/// Connects run **in parallel** (bounded) and the whole wait is raced against
/// ~6s: `ensure_connected` waits up to 30s per relay on a cold connect, which
/// previously made the first search stall for up to 30s sequentially. REQs
/// queue on still-Connecting relays (the pool owns the connection task), so
/// giving up early just means this search runs on whatever made it in time;
/// later searches pick the stragglers up (session cache).
///
/// NIP-50 capability gating is **fail-open**: relays whose fetched NIP-11
/// document definitively lacks NIP-50 are dropped, but unknown documents
/// (no CORS / no doc / no `supported_nips` list) stay eligible. NIP-11
/// fetches are kicked here so the docs fill asynchronously for later runs.
pub async fn ensure_search_relays_connected(client: &Client) -> Vec<String> {
    use dioxus::prelude::ReadableExt;
    use futures::pin_mut;
    use futures::StreamExt;
    let search_relays = {
        let relays = super::nip65::SEARCH_RELAYS.peek().clone();
        if relays.is_empty() {
            super::nip65::default_search_relays()
        } else {
            relays
        }
    };
    if search_relays.is_empty() {
        return Vec::new();
    }

    // Kick NIP-11 doc fetches (session cache) and fail-open gate on what we
    // already know: only exclude relays that definitively lack NIP-50.
    super::nip11_info::ensure_nip11_for(search_relays.clone());
    let eligible: Vec<String> = search_relays
        .iter()
        .filter(|url| {
            match super::nip11_info::advertises_nip(url, 50) {
                Some(false) => {
                    log::debug!("Search relay {url} advertises no NIP-50 support; skipping");
                    false
                }
                _ => true,
            }
        })
        .cloned()
        .collect();

    // Parallel + bounded: every connect races a ~6s deadline individually
    // (they all run concurrently, so the whole set is bounded by the same
    // ~6s instead of stacking sequentially). REQs queue on still-Connecting
    // relays (the pool owns the connection task), so giving up early just
    // means this search runs on whatever made it in time; later searches
    // pick the stragglers up (session cache).
    const SEARCH_CONNECT_BOUND: Duration = Duration::from_secs(6);
    let outcomes = futures::stream::iter(eligible.clone())
        .map(|url| async move {
            use futures::future::{select, Either};
            use futures::pin_mut;
            let ok = {
                let ensure_fut = ensure_connected(client, &url);
                let sleep_fut = crate::platform::timer::sleep(SEARCH_CONNECT_BOUND);
                pin_mut!(ensure_fut, sleep_fut);
                match select(ensure_fut, sleep_fut).await {
                    Either::Left((ok, _)) => ok,
                    Either::Right(_) => {
                        log::warn!("Search relay {url} connection wait exceeded 6s");
                        false
                    }
                }
            };
            (url, ok)
        })
        .buffer_unordered(5)
        .collect::<Vec<(String, bool)>>()
        .await;
    let connected: Vec<String> = outcomes
        .into_iter()
        .filter_map(|(url, ok)| if ok { Some(url) } else { None })
        .collect();

    if connected.is_empty() {
        log::error!("No search relays could be connected!");
    } else {
        log::info!(
            "Search relays connected: {}/{} - {:?}",
            connected.len(),
            eligible.len(),
            connected
        );
    }
    connected
}
#[allow(dead_code)]
pub async fn ensure_indexer_relays_connected(client: &Client) -> Vec<String> {
    let indexer_relays = {
        let relays = super::nip65::INDEXER_RELAYS.peek().clone();
        if relays.is_empty() {
            super::nip65::default_indexer_relays()
        } else {
            relays
        }
    };
    let mut connected = Vec::new();
    for relay_url in &indexer_relays {
        if ensure_connected(client, relay_url).await {
            connected.push(relay_url.clone());
        }
    }
    if connected.is_empty() {
        log::warn!("No indexer relays could be connected");
    } else {
        log::info!("Indexer relays connected: {}/{}", connected.len(), indexer_relays.len());
    }
    connected
}
#[allow(dead_code)]
pub async fn ensure_favorite_relays_connected(client: &Client) -> Vec<String> {
    let favorite_relays = {
        let relays = super::nip65::FAVORITE_RELAYS.peek().clone();
        if relays.is_empty() {
            super::nip65::default_favorite_relays()
        } else {
            relays
        }
    };
    let mut connected = Vec::new();
    for relay_url in &favorite_relays {
        if ensure_connected(client, relay_url).await {
            connected.push(relay_url.clone());
        }
    }
    if connected.is_empty() {
        log::warn!("No favorite relays could be connected");
    } else {
        log::info!("Favorite relays connected: {}/{}", connected.len(), favorite_relays.len());
    }
    connected
}

pub mod p2p_urls {
    pub const MOSTRO_DEFAULT_RELAYS: &[&str] = &[
        "wss://mostro-p2p.tech",
        "wss://nos.lol",
        "wss://relay.mostro.network",
    ];
}

pub async fn ensure_p2p_relays_connected(client: &Client) -> Vec<String> {
    let relay_urls = resolve_p2p_relay_urls();
    if relay_urls.is_empty() {
        log::warn!("No P2P relay URLs resolved");
        return Vec::new();
    }
    let mut all_urls = Vec::with_capacity(relay_urls.len());
    for relay_url in &relay_urls {
        let Ok(url) = nostr::Url::parse(relay_url) else {
            log::warn!("Invalid P2P relay URL: {}", relay_url);
            continue;
        };
        let pool = client.pool();
        let relays = pool.relays().await;
        let already_in_pool = relays.iter().any(|(u, _)| u.as_str() == relay_url.as_str());
        if already_in_pool {
            log::debug!("P2P relay already in pool: {}", relay_url);
        } else {
            let opts = p2p_relay_options();
            match pool.add_relay(url.clone(), opts).await {
                Ok(true) => {
                    log::info!("Added P2P relay to pool: {}", relay_url);
                }
                Ok(false) => {
                    log::debug!("P2P relay already existed: {}", relay_url);
                }
                Err(e) => {
                    log::warn!("Failed to add P2P relay {}: {}", relay_url, e);
                    continue;
                }
            }
        }
        if let Err(e) = pool.connect_relay(url.clone()).await {
            log::debug!("P2P relay connect initiated (may already be connecting): {}", e);
        }
        all_urls.push(relay_url.clone());
    }
    if all_urls.is_empty() {
        log::warn!("No P2P relays could be added from: {:?}", relay_urls);
    } else {
        log::info!(
            "P2P relays added/connecting: {}/{} - {:?}",
            all_urls.len(),
            relay_urls.len(),
            all_urls
        );
    }
    all_urls
}

pub fn resolve_p2p_relay_urls() -> Vec<String> {
    if let Some(cfg) = crate::stores::mostro::try_get_node_config() {
        if !cfg.relays.is_empty() {
            return cfg.relays;
        }
    }
    p2p_urls::MOSTRO_DEFAULT_RELAYS
        .iter()
        .map(|s| s.to_string())
        .collect()
}
