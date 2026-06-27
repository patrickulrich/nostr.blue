//! Mostro deep link handler.
//!
//! Phase 11: handles `mostro:` URL scheme deep links that arrive from:
//! - Web: hash fragment (`#mostro:{order_id}?relays=...&mostro=...`)
//! - Android: intent filter (`mostro://...`)
//! - Desktop: protocol handler (`mostro://...`)
//!
//! Routing:
//! - If the `mostro` param matches the currently-selected daemon → navigate
//!   directly to `P2POrderDetail`.
//! - If different → show a toast suggesting the user switch daemons, then
//!   navigate.
//! - If the order can't be found (relay fetch returns nothing in 10s) →
//!   show "Order unavailable" toast.

use nostr::prelude::*;

use crate::stores::mostro::source_tag::ParsedSourceTag;

/// Parse a `mostro:` or `mostro://` URL into structured deep-link data.
///
/// Accepts both `mostro:{order_id}?...` (source-tag format) and
/// `mostro://{order_id}?...` (URL-scheme format with `//`).
/// Returns `None` for any parse failure.
pub fn parse_mostro_url(url: &str) -> Option<ParsedSourceTag> {
    let url = url.trim();

    // Accept both `mostro:` and `mostro://` prefixes.
    let after_scheme = url
        .strip_prefix("mostro://")
        .or_else(|| url.strip_prefix("mostro:"))?;

    // Reuse the source-tag parser which handles `id?params` format.
    crate::stores::mostro::source_tag::parse_source_tag(
        &format!("mostro:{after_scheme}"),
    )
}

/// Handle a parsed deep link: navigate to the order or prompt a daemon switch.
///
/// Returns `Ok(())` on success (navigated or switched), `Err(msg)` on failure.
pub async fn handle_mostro_deep_link(link: &ParsedSourceTag) -> Result<(), String> {
    let current = crate::stores::mostro::try_get_node_config();
    let matches_current = current
        .as_ref()
        .map(|c| c.pubkey == link.mostro_pubkey.to_hex())
        .unwrap_or(false);

    if !matches_current {
        // Safety guard: block switching daemons when the user has active
        // trades on the current daemon. See `switch_to_daemon` for rationale.
        let active = crate::stores::mostro::trade_store::active_trades_for_daemon();
        let current_pk = current
            .as_ref()
            .map(|c| c.pubkey.clone())
            .unwrap_or_default();
        if !active.is_empty() && !current_pk.is_empty() && current_pk != link.mostro_pubkey.to_hex() {
            return Err(format!(
                "Cannot switch daemons: you have {} active trade(s) on the current \
                 daemon. Please complete or cancel them before switching.",
                active.len()
            ));
        }

        // Different daemon — build a config from the link and switch.
        let cfg = crate::stores::mostro::node_config::MostroNodeConfig::new(
            link.mostro_pubkey.to_hex(),
            if link.relays.is_empty() {
                current
                    .as_ref()
                    .map(|c| c.relays.clone())
                    .unwrap_or_default()
            } else {
                link.relays.clone()
            },
            Some("auto (deep link)".to_string()),
        )?;

        crate::stores::mostro::node_config::save_config(cfg)
            .await
            .map_err(|e| format!("Failed to switch daemon: {e}"))?;
    }

    // Build the naddr for P2POrderDetail navigation.
    // Format: Coordinate { kind: 38383, author: daemon_pubkey, identifier: order_id }
    let order_id_str = link.order_id.to_string();
    let daemon_pk_hex = link.mostro_pubkey.to_hex();

    // Bug #10 fix: verify the order actually exists on relays before
    // navigating. Without this, deep links to expired or non-existent
    // orders would navigate to a bare "Order not found" page with no
    // deep-link-specific messaging. Now we surface a clear toast and
    // skip navigation.
    let relays = if link.relays.is_empty() {
        current.as_ref().map(|c| c.relays.clone()).unwrap_or_default()
    } else {
        link.relays.clone()
    };
    if verify_order_exists(&order_id_str, &daemon_pk_hex, &relays)
        .await
        .is_err()
    {
        crate::stores::mostro::enqueue_background_toast(
            "Deep link target not found".to_string(),
            format!(
                "Order {} was not found on the daemon's relays. The link may be expired.",
                order_id_str
            ),
        );
        return Ok(());
    }

    let naddr = build_naddr(&daemon_pk_hex, &order_id_str)
        .ok_or_else(|| "Failed to build naddr".to_string())?;

    // Store the naddr for the root component to pick up.
    crate::stores::mostro::deeplink::set_pending_deep_link(naddr);

    Ok(())
}

/// Build a NIP-19 naddr from a daemon pubkey and order ID.
fn build_naddr(pubkey_hex: &str, order_id: &str) -> Option<String> {
    let pk = PublicKey::from_hex(pubkey_hex).ok()?;
    let coordinate = nostr::nips::nip01::Coordinate::new(nostr::Kind::Custom(38383), pk)
        .identifier(order_id);
    let nip19 = nostr::nips::nip19::Nip19Coordinate::new(coordinate, vec![]);
    nip19.to_bech32().ok()
}

/// Try to fetch a kind 38383 order event by its d-tag (order UUID).
///
/// Queries the daemon's relays for the order. Returns `Ok(())` if found
/// within the timeout, `Err` otherwise.
pub async fn verify_order_exists(
    order_id: &str,
    daemon_pubkey_hex: &str,
    relays: &[String],
) -> Result<(), String> {
    let daemon_pk = PublicKey::from_hex(daemon_pubkey_hex)
        .map_err(|e| format!("Invalid daemon pubkey: {e}"))?;

    let filter = nostr_sdk::Filter::new()
        .kind(nostr::Kind::Custom(38383))
        .author(daemon_pk)
        .identifier(order_id.to_string())
        .limit(1);

    let client = crate::stores::nostr_client::get_client()
        .ok_or("Nostr client not initialized")?;

    let urls: Vec<nostr::Url> = relays
        .iter()
        .filter_map(|u| nostr::Url::parse(u).ok())
        .collect();

    if urls.is_empty() {
        return Err("No relays available".to_string());
    }

    let events = client
        .fetch_events_from(&urls, filter, std::time::Duration::from_secs(10))
        .await
        .map_err(|e| format!("Relay fetch failed: {e}"))?;

    if events.into_iter().next().is_some() {
        Ok(())
    } else {
        Err("Order not found on relays within 10s".to_string())
    }
}
