//! Relay management module
//!
//! This module provides centralized relay state management for the Nostr client.
//! It is organized into submodules for different concerns:
//!
//! - `signals` - Reactive state signals for relay pool and connection status
//! - `pool` - Functions for adding/removing relays and applying relay lists
//! - `connection` - Connection management and fetching from specific relays
//! - `nip65` - NIP-65 relay list metadata and NIP-17 DM relay lists
//! - `display` - Display-friendly relay info for UI components
//! - `scoring` - Relay performance scoring (internal, not re-exported)
//!
//! # Design Principles
//!
//! **IMPORTANT**: All functions in this module take `client: &Client` as a parameter
//! rather than calling `nostr_client::get_client()` internally. This avoids circular
//! dependencies and makes the code more testable.
//!
//! ```rust,ignore
//! // WRONG - creates circular dependency
//! pub async fn add_relay(url: &str) {
//!     let client = nostr_client::get_client().unwrap();  // DON'T DO THIS
//! }
//!
//! // CORRECT - parameterized, no circular dependency
//! pub async fn add_relay(client: &Client, url: &str) {
//!     // Works correctly
//! }
//! ```
#![allow(unused_imports)]
pub mod connection;
pub mod coverage;
pub mod display;
pub mod health;
pub mod hints;
pub mod nip65;
pub mod persistence;
pub mod pool;
pub mod room_relays;
pub mod scoring;
pub mod scoped_subs;
pub mod signals;
pub mod specialty;
pub use connection::{
    disconnect, ensure_chess_relays_connected, ensure_p2p_relays_connected, ensure_radio_relay_connected, ensure_relays_ready,
    ensure_video_relay_connected, fetch_event_by_coordinate_with_relays, fetch_events_from_relays,
    reconnect, try_connect_relays, wait_for_user_relays,
};
pub use coverage::{
    clear_coverage, cleanup_ephemeral_relays, connect_ephemeral_relays, coverage_size,
    get_relays_for_pubkey, prefetch_relay_lists_for_follows, record_provenance,
    record_relay_hint, record_relay_list_from_event, record_relay_list_from_event_by_map,
    record_user_relays, resolve_user_relays, start_provenance_recorder, RelayCoverageMap,
    RelayPurpose, RELAY_COVERAGE,
};
pub use display::{get_relay_display_info, RelayDisplayInfo};
pub use health::{
    connected_count, poll_relay_health, quarantine_dead_relays, quarantined_count,
    start_health_poll, sync_ui_signals, RelayHealthEntry, RelayHealthState, RELAY_HEALTH,
};
pub use hints::{get_write_relay_hints, make_naddr_with_hints};
pub use nip65::{
    add_indexer_relays_to_client, apply_local_relays_to_client, default_dm_relays,
    default_favorite_relays, default_indexer_relays, default_relays, default_search_relays,
    fetch_events_from_indexers, fetch_blocked_relays, fetch_favorite_relays, fetch_own_lists_from_indexers,
    fetch_outbox_relays, fetch_relay_list, fetch_search_relays, get_dm_relays,
    get_indexer_relay_urls, get_read_relays, get_write_relays, init_local_relays_from_cache,
    init_nip51_relay_lists, init_private_relay_lists, init_user_relay_lists,
    load_broadcast_relays, load_local_relays, parse_dm_relay_list, parse_relay_list_event,
    publish_blocked_relays, publish_dm_relay_list, publish_event_to_indexers, publish_favorite_relays,
    publish_indexer_relays, publish_outbox_relays, publish_proxy_relays, publish_relay_list,
    publish_search_relays, publish_trusted_relays, reset_dm_relays_to_default,
    reset_general_relays_to_default, save_broadcast_relays, save_local_relays,
    start_relay_list_subscription, stop_relay_list_subscription, wait_for_indexer_connected, RelayConfig,
    RelayListMetadata, BLOCKED_RELAYS, BROADCAST_RELAYS, DEFAULT_DM_RELAYS,
    DEFAULT_FAVORITE_RELAYS, DEFAULT_INDEXER_RELAYS, DEFAULT_NIP65_RELAYS,
    DEFAULT_SEARCH_RELAYS, FAVORITE_RELAYS, INDEXER_RELAYS, LOCAL_RELAYS, OUTBOX_RELAYS,
    PROXY_RELAYS, SEARCH_RELAYS, TRUSTED_RELAYS, USER_RELAY_METADATA,
};
pub use persistence::{
    apply_seeded_relays_to_pool, collect_relay_lists_from_disk, persist_public_relay_lists,
    write_seeded_relay_lists_to_signals, SeededRelays,
};
pub use pool::{
    add_relay, apply_relay_lists_to_client, is_relay_blocked, remove_relay,
    reset_pool_to_defaults, DEFAULT_RELAYS,
};
pub use room_relays::{effective_room_relays, user_nip65_relays};
pub use signals::{
    RelayInfo, RelaySource, RelayPoolStore, RelayPoolStoreStoreExt, RelayStatus, RELAY_CONNECTED,
    RELAY_POOL, USER_RELAYS_APPLIED,
};
pub use specialty::{
    add_relays, add_relays_from_strings, ensure_connected, ensure_dm_relays_connected,
    ensure_favorite_relays_connected, ensure_gif_relay, ensure_indexer_relays_connected,
    ensure_radio_relay, ensure_search_relays_connected,
    ensure_video_relay, get_connected, p2p_urls, remove_relays, urls as specialty_urls,
    resolve_p2p_relay_urls, specialty_relay_options, p2p_relay_options,
};
