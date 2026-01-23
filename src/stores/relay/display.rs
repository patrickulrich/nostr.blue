//! Relay display utilities for UI components
//!
//! This module provides display-friendly relay information that composes
//! data from the SDK's relay primitives (status, stats, flags).
//!
//! **IMPORTANT**: All functions take `client: &Client` as a parameter
//! rather than calling `nostr_client::get_client()` internally. This avoids
//! circular dependencies and follows the relay module design principle.

use nostr_relay_pool::RelayStatus;
use nostr_sdk::Client;

/// Display-friendly relay information for UI components
///
/// This is a snapshot of relay state, not reactive. Call `get_relay_display_info()`
/// to get fresh data when needed.
#[derive(Clone, Debug, PartialEq)]
pub struct RelayDisplayInfo {
    pub url: String,
    /// Relay connection status - keep as enum (has Display impl)
    pub status: RelayStatus,
    pub bytes_sent: usize,
    pub bytes_received: usize,
    // Flag breakdown
    pub has_read: bool,
    pub has_write: bool,
    /// Whether this relay was added via gossip (outbox model) vs manual config
    pub is_gossip: bool,
    // Stats
    pub connection_attempts: usize,
    /// Total successful connections over lifetime
    pub successful_connections: usize,
    pub success_rate: f64,
}

impl RelayDisplayInfo {
    /// Get a display-friendly status string
    pub fn status_str(&self) -> &'static str {
        match self.status {
            RelayStatus::Connected => "Connected",
            RelayStatus::Connecting => "Connecting",
            RelayStatus::Disconnected => "Disconnected",
            RelayStatus::Initialized => "Initialized",
            RelayStatus::Terminated => "Terminated",
            RelayStatus::Pending => "Pending",
            RelayStatus::Banned => "Banned",
            RelayStatus::Sleeping => "Sleeping",
        }
    }

    /// Get a numeric order for sorting by status (lower = better)
    pub fn status_order(&self) -> u8 {
        match self.status {
            RelayStatus::Connected => 0,
            RelayStatus::Connecting => 1,
            RelayStatus::Pending => 2,
            RelayStatus::Initialized => 3,
            RelayStatus::Disconnected => 4,
            RelayStatus::Sleeping => 5,
            RelayStatus::Terminated => 6,
            RelayStatus::Banned => 7,
        }
    }
}

/// Get display information for all relays in the pool
///
/// Takes `&Client` parameter (not global signal) per relay module design.
/// Returns relays sorted by status (connected first) then by URL.
pub async fn get_relay_display_info(client: &Client) -> Vec<RelayDisplayInfo> {
    let relays = client.relays().await;
    let mut result = Vec::with_capacity(relays.len());

    for (url, relay) in relays {
        let stats = relay.stats();
        let flags = relay.flags();

        result.push(RelayDisplayInfo {
            url: url.to_string(),
            status: relay.status(),
            bytes_sent: stats.bytes_sent(),
            bytes_received: stats.bytes_received(),
            has_read: flags.has_read(),
            has_write: flags.has_write(),
            is_gossip: flags.has_gossip(),
            connection_attempts: stats.attempts(),
            successful_connections: stats.success(),
            success_rate: {
                let rate = stats.success_rate();
                if rate.is_finite() { rate * 100.0 } else { 0.0 }
            },
        });
    }

    // Sort by status (Connected first) then by URL
    result.sort_by(|a, b| {
        a.status_order()
            .cmp(&b.status_order())
            .then_with(|| a.url.cmp(&b.url))
    });

    result
}
