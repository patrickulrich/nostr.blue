//! Mostro daemon discovery via kind 38385 info events.
//!
//! Searches connected relays for kind 38385 events tagged with
//! `#y=mostro` + `#z=info` to discover all active Mostro daemons
//! on the network. Also counts pending orders (kind 38383) per daemon
//! to show activity levels.

use std::collections::HashMap;

use nostr::prelude::*;
use nostr_sdk::prelude::{Alphabet, Kind as NostrKind, SingleLetterTag};

use super::communities::find_by_pubkey;
use super::node_config::{MostroNodeConfig, MostroNodeInfo, MOSTRO_NODE_INFO};

pub struct DiscoveredDaemon {
    pub pubkey: String,
    pub info: MostroNodeInfo,
    pub order_count: usize,
    pub is_trusted: bool,
    pub community_label: Option<&'static str>,
}

pub async fn discover_daemons() -> Result<Vec<DiscoveredDaemon>, String> {
    let now_secs = crate::platform::timestamp::now_secs();
    let lookback = now_secs.saturating_sub(72 * 3600);

    let info_filter = Filter::new()
        .kind(NostrKind::Custom(38385))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Y), "mostro")
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), "info")
        .limit(50);

    let order_filter = Filter::new()
        .kind(Kind::PeerToPeerOrder)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), "order")
        .since(Timestamp::from(lookback))
        .limit(500);

    let (info_result, order_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_aggregated(info_filter, std::time::Duration::from_secs(10)),
        crate::stores::nostr_client::fetch_events_aggregated(order_filter, std::time::Duration::from_secs(10)),
    );

    let info_events = info_result?;
    let order_events = order_result?;

    log::info!(
        "Daemon discovery: found {} info events, {} order events",
        info_events.len(),
        order_events.len()
    );

    let mut order_counts: HashMap<String, usize> = HashMap::new();
    for event in &order_events {
        let author = event.pubkey.to_hex();
        let has_d = event.tags.iter().any(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("d")
        });
        if has_d {
            *order_counts.entry(author).or_insert(0) += 1;
        }
    }

    let mut by_pubkey: HashMap<String, (i64, MostroNodeInfo)> = HashMap::new();
    for event in &info_events {
        let pk = event.pubkey.to_hex();
        let existing = by_pubkey.get(&pk).map(|(ts, _)| *ts).unwrap_or(0);
        if event.created_at.as_secs() as i64 > existing {
            if let Some(info) = MostroNodeInfo::from_event(event) {
                by_pubkey.insert(pk, (event.created_at.as_secs() as i64, info));
            }
        }
    }

    let mut daemons: Vec<DiscoveredDaemon> = by_pubkey
        .into_iter()
        .map(|(pk, (_, info))| {
            let community = find_by_pubkey(&pk);
            DiscoveredDaemon {
                pubkey: pk.clone(),
                order_count: order_counts.get(&pk).copied().unwrap_or(0),
                is_trusted: community.is_some(),
                community_label: community.map(|c| c.region),
                info,
            }
        })
        .collect();

    daemons.sort_by(|a, b| {
        match (a.is_trusted, b.is_trusted) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.order_count.cmp(&a.order_count),
        }
    });

    log::info!("Discovered {} unique Mostro daemons", daemons.len());
    Ok(daemons)
}

pub async fn switch_to_daemon(daemon: &DiscoveredDaemon) -> Result<(), String> {
    let pk = daemon.pubkey.clone();

    let relays = fetch_daemon_relays(&pk).await?;

    let label = daemon
        .community_label
        .map(|s| s.to_string())
        .or_else(|| daemon.info.lnd_node_alias.clone());

    let mut cfg = MostroNodeConfig::new(pk, relays, label)?;
    cfg.pow = daemon.info.pow;
    cfg.bond_payout_claim_window_days = daemon.info.bond_payout_claim_window_days;

    super::save_node_config(cfg).await?;

    *MOSTRO_NODE_INFO.write() = Some(daemon.info.clone());

    Ok(())
}

async fn fetch_daemon_relays(pubkey_hex: &str) -> Result<Vec<String>, String> {
    let pk = PublicKey::from_hex(pubkey_hex)
        .or_else(|_| PublicKey::from_bech32(pubkey_hex))
        .map_err(|e| format!("Bad daemon pubkey: {e}"))?;

    let filter = Filter::new()
        .author(pk)
        .kind(NostrKind::Custom(10002))
        .limit(1);

    let events = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        std::time::Duration::from_secs(5),
    )
    .await?;

    if let Some(event) = events.iter().max_by_key(|e| e.created_at) {
        let relays: Vec<String> = event
            .tags
            .iter()
            .filter_map(|t| {
                let slice = t.as_slice();
                if slice.first().map(|s| s.as_str()) == Some("r") {
                    slice.get(1).cloned()
                } else {
                    None
                }
            })
            .collect();
        if !relays.is_empty() {
            return Ok(relays);
        }
    }

    if let Some(community) = find_by_pubkey(pubkey_hex) {
        return Ok(community.relays.iter().map(|s| s.to_string()).collect());
    }

    Err(format!(
        "Could not determine relays for daemon {pubkey_hex}"
    ))
}
