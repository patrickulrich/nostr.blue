//! NIP-19 relay hints for addressable events
//!
//! Provides functions for generating relay hints used in NIP-19 bech32 encoding
//! of addressable events (naddr). These hints help other clients locate events.

use nostr_sdk::prelude::*;

/// Check if a URL is a public relay (not localhost/private IP)
fn is_public_relay(url: &str) -> bool {
    let url_lower = url.to_lowercase();

    // Reject localhost
    if url_lower.contains("localhost") || url_lower.contains("127.0.0.1") {
        return false;
    }

    // Reject IPv6 loopback
    if url_lower.contains("[::1]") {
        return false;
    }

    // Extract host for private IP checks
    if let Some(host_start) = url_lower.find("://") {
        let after_scheme = &url_lower[host_start + 3..];
        let host = after_scheme.split(&['/', ':'][..]).next().unwrap_or("");

        // 10.x.x.x
        if host.starts_with("10.") {
            return false;
        }
        // 192.168.x.x
        if host.starts_with("192.168.") {
            return false;
        }
        // 172.16-31.x.x
        if host.starts_with("172.") {
            if let Some(second) = host.split('.').nth(1) {
                if let Ok(n) = second.parse::<u8>() {
                    if (16..=31).contains(&n) {
                        return false;
                    }
                }
            }
        }
        // fc00::/7 (private IPv6)
        if host.starts_with("fc") || host.starts_with("fd") {
            return false;
        }
    }

    true
}

/// Get URLs of write-enabled relays for use as relay hints in NIP-19 naddrs
/// Returns up to 2 relay URLs that have the WRITE flag enabled.
/// Per NIP-19, relay hints help other clients locate addressable events.
/// Filters out localhost and private IP addresses.
pub async fn get_write_relay_hints(client: &Client) -> Vec<String> {
    let relays = client.relays().await;
    let mut write_relays: Vec<String> = relays
        .iter()
        .filter(|(_, relay)| relay.flags().has_write())
        .map(|(url, _)| url.to_string())
        .filter(|url| is_public_relay(url))
        .collect();

    // Sort first for deterministic selection, then truncate to 2 per NIP-19 best practice
    write_relays.sort();
    write_relays.truncate(2);
    write_relays
}

/// Create an naddr (NIP-19) with relay hints for an addressable event
/// This includes relay hints from the user's write relays for better discoverability.
pub async fn make_naddr_with_hints(
    client: &Client,
    kind: u16,
    pubkey: &PublicKey,
    identifier: &str,
) -> Result<String, String> {
    use nostr::nips::nip19::{Nip19Coordinate, ToBech32};
    use nostr::types::url::RelayUrl;

    let coordinate = Coordinate::new(Kind::from(kind), *pubkey).identifier(identifier);

    // Get write relay hints
    let relay_hints = get_write_relay_hints(client).await;
    let relay_urls: Vec<RelayUrl> = relay_hints
        .iter()
        .filter_map(|r| RelayUrl::parse(r).ok())
        .collect();

    let nip19 = Nip19Coordinate::new(coordinate, relay_urls);
    nip19
        .to_bech32()
        .map_err(|e| format!("Failed to encode naddr: {}", e))
}
