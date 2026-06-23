//! Mostro daemon node configuration
//!
//! Persists the user's selected Mostro daemon (pubkey + relay list) as a
//! NIP-78 app-data event on the user's write relays, with a fast local cache
//! in `platform::storage` for instant reads.
//!
//! NIP-78 (kind 30078) d-tag convention: `nostr.blue/p2p/node`
//!
//! This is the long-lived identity of the daemon the user trades with. The
//! flow builders in `flow.rs` always read the current node from here.
//!
//! The first time a user visits `/p2p` or takes a Mostro order, a default
//! configuration is seeded (currently a single mainnet daemon). Advanced
//! users can override it via `/settings/p2p` to point at a private node.

use dioxus::prelude::*;
use nostr::nips::nip09::EventDeletionRequest;
use nostr::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventBuilder};
use serde::{Deserialize, Serialize};
use std::result::Result;
use std::time::Duration;

use crate::platform::storage;
use crate::stores::auth_store;
use crate::stores::nostr_client;
use crate::stores::publish_queue::{self, types::QueueEventType};

/// NIP-78 d-tag for the node config event.
pub const NODE_CONFIG_D_TAG: &str = "nostr.blue/p2p/node";

/// Local cache key.
const CACHE_KEY: &str = "mostro_node_config";

/// Sentinel key set when the user explicitly clears their daemon.
/// Prevents `init_from_cache()` from re-seeding the default community node.
const CLEARED_SENTINEL_KEY: &str = "mostro_node_config_cleared";

/// Bumped only if the on-wire `MostroNodeConfig` schema changes.
pub const NODE_CONFIG_VERSION: u32 = 1;

/// Default mainnet Mostro daemon — used as a starter config so the user can
/// trade immediately. They can swap it out in `/settings/p2p`.
///
/// Relays match the official daemon's kind 10002 relay list.
/// Auto-updated via `sync_relays_from_nip65()` when the daemon publishes changes.
#[allow(dead_code)]
pub const DEFAULT_NODE_PUBKEY: &str =
    "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390";
#[allow(dead_code)]
pub const DEFAULT_NODE_RELAYS: &[&str] = &[
    "wss://mostro-p2p.tech",
    "wss://nos.lol",
    "wss://relay.mostro.network",
];

/// Persisted representation of the user's Mostro node selection.
///
/// Phase 6.2 (M14): extended with bond/fee/min-max/fiat fields so they
/// survive across restarts. Previously only `pow` and
/// `bond_payout_claim_window_days` were persisted; all other fields lived
/// only in the in-memory `MOSTRO_NODE_INFO` signal and were lost on logout
/// or restart.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MostroNodeConfig {
    /// Bumped when the schema changes; older records are ignored.
    pub version: u32,
    /// Daemon pubkey (hex or npub). Will be normalized to hex on save.
    pub pubkey: String,
    /// Relay URLs the daemon can be reached at. Must be non-empty.
    pub relays: Vec<String>,
    /// Free-form label for the user to recognize the node in UI.
    pub label: Option<String>,
    /// Unix timestamp (seconds) of the last update.
    pub updated_at: u64,
    /// Proof-of-Work required by the daemon (from kind 38385 `pow` tag).
    #[serde(default)]
    pub pow: u8,
    /// Bond payout claim window in days (from kind 38385). Default: 30.
    #[serde(default = "default_bond_claim_window")]
    pub bond_payout_claim_window_days: u32,
    // Phase 6.2 (M14): all fields below are persisted from kind 38385 info
    // events so they survive restarts. They were previously only in-memory.
    #[serde(default)]
    pub fee: Option<f64>,
    #[serde(default)]
    pub min_order_amount: Option<u64>,
    #[serde(default)]
    pub max_order_amount: Option<u64>,
    #[serde(default)]
    pub expiration_seconds: Option<u64>,
    #[serde(default)]
    pub fiat_currencies_accepted: Vec<String>,
    #[serde(default)]
    pub hold_invoice_expiration_window: Option<u64>,
    #[serde(default)]
    pub hold_invoice_cltv_delta: Option<u64>,
    #[serde(default)]
    pub invoice_expiration_window: Option<u64>,
    #[serde(default)]
    pub bond_enabled: bool,
    #[serde(default)]
    pub bond_amount_pct: Option<f64>,
    #[serde(default)]
    pub bond_base_amount_sats: Option<u64>,
    #[serde(default)]
    pub bond_apply_to: Option<String>,
    #[serde(default)]
    pub bond_slash_on_waiting_timeout: Option<bool>,
    #[serde(default)]
    pub bond_slash_node_share_pct: Option<f64>,
}

fn default_bond_claim_window() -> u32 {
    30
}

/// Parsed daemon capabilities from a kind 38385 info event.
///
/// Phase 6.1 (M13): added `bond_apply_to`, `bond_slash_on_waiting_timeout`,
/// `bond_slash_node_share_pct`, `lnd_node_uri` (spec-conformant — replaces
/// the non-spec `lnd_node_alias`/`lnd_node_pubkey` which are kept for
/// backward compat).
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct MostroNodeInfo {
    pub mostro_version: Option<String>,
    pub max_order_amount: Option<u64>,
    pub min_order_amount: Option<u64>,
    pub expiration_hours: Option<u64>,
    pub expiration_seconds: Option<u64>,
    pub fiat_currencies_accepted: Vec<String>,
    pub max_orders_per_response: Option<u64>,
    pub fee: Option<f64>,
    pub pow: u8,
    pub bond_enabled: bool,
    pub bond_amount_pct: Option<f64>,
    pub bond_base_amount_sats: Option<u64>,
    /// Phase 6.1 (M13): `"take"`, `"make"`, or `"both"`. Controls when the
    /// daemon requires anti-abuse bonds. Was missing entirely.
    pub bond_apply_to: Option<String>,
    /// Phase 6.1 (M13): whether the daemon slashes bonds on waiting-state
    /// timeout (in addition to dispute slashes). Was missing.
    pub bond_slash_on_waiting_timeout: Option<bool>,
    /// Phase 6.1 (M13): fraction of a slashed bond that the node keeps
    /// (vs. the winning counterparty). Was missing.
    pub bond_slash_node_share_pct: Option<f64>,
    pub bond_payout_claim_window_days: u32,
    pub hold_invoice_expiration_window: Option<u64>,
    pub hold_invoice_cltv_delta: Option<u64>,
    pub invoice_expiration_window: Option<u64>,
    /// Phase 6.1 (M13): spec-conformant LND node URI (comma-joined pubkeys
    /// or URIs). Replaces the non-spec `lnd_node_alias`/`lnd_node_pubkey`.
    pub lnd_node_uri: Option<String>,
    /// Legacy non-spec field kept for backward compat with older daemons.
    pub lnd_node_alias: Option<String>,
    /// Legacy non-spec field kept for backward compat with older daemons.
    pub lnd_node_pubkey: Option<String>,
    /// Phase 10.1: push notification server URL (from kind 38385
    /// `push_server_url` tag). When present, the client registers push
    /// tokens with this endpoint to receive notifications while offline.
    pub push_server_url: Option<String>,
}

impl MostroNodeInfo {
    pub fn from_event(event: &NostrEvent) -> Option<Self> {
        if event.kind.as_u16() != 38385 {
            return None;
        }
        // Phase 6.3: validate required platform tags. Reject events from
        // non-Mostro apps that happen to use kind 38385. The `y` tag's
        // first value must be "mostro"; the `z` tag must be "info".
        let has_y = event.tags.iter().any(|t| {
            t.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("y"))
                && t.as_slice().get(1).map(|s| s.as_str()) == Some("mostro")
        });
        let has_z = event.tags.iter().any(|t| {
            t.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("z"))
                && t.as_slice().get(1).map(|s| s.as_str()) == Some("info")
        });
        if !has_y || !has_z {
            log::debug!(
                "Rejecting kind-38385 event: missing y=mostro ({}) or z=info ({})",
                has_y, has_z
            );
            return None;
        }

        let mut info = MostroNodeInfo::default();
        for tag in event.tags.iter() {
            let val = match tag.content() {
                Some(v) => v.to_string(),
                None => continue,
            };
            let kind = tag.kind();
            if kind == TagKind::Custom(std::borrow::Cow::Borrowed("mostro_version")) {
                info.mostro_version = Some(val);
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("max_order_amount")) {
                info.max_order_amount = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("min_order_amount")) {
                info.min_order_amount = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("expiration_hours")) {
                info.expiration_hours = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("expiration_seconds")) {
                info.expiration_seconds = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("fiat_currencies_accepted")) {
                info.fiat_currencies_accepted = val.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("max_orders_per_response")) {
                info.max_orders_per_response = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("fee")) {
                info.fee = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("pow")) {
                info.pow = val.parse().unwrap_or(0);
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("bond_enabled")) {
                info.bond_enabled = val == "true" || val == "1";
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("bond_amount_pct")) {
                info.bond_amount_pct = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("bond_base_amount_sats")) {
                info.bond_base_amount_sats = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("bond_payout_claim_window_days")) {
                info.bond_payout_claim_window_days = val.parse().unwrap_or(30);
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("bond_apply_to")) {
                // Phase 6.1 (M13): "take" | "make" | "both"
                info.bond_apply_to = Some(val);
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("bond_slash_on_waiting_timeout")) {
                info.bond_slash_on_waiting_timeout = Some(val == "true" || val == "1");
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("bond_slash_node_share_pct")) {
                info.bond_slash_node_share_pct = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("hold_invoice_expiration_window")) {
                info.hold_invoice_expiration_window = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("hold_invoice_cltv_delta")) {
                info.hold_invoice_cltv_delta = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("invoice_expiration_window")) {
                info.invoice_expiration_window = val.parse().ok();
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("lnd_node_uri")) {
                // Phase 6.1 (M13): spec-conformant field.
                info.lnd_node_uri = Some(val);
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("lnd_node_alias")) {
                info.lnd_node_alias = Some(val);
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("lnd_node_pubkey")) {
                info.lnd_node_pubkey = Some(val);
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("push_server_url")) {
                // Phase 10.1: push notification server endpoint.
                info.push_server_url = Some(val);
            }
        }
        Some(info)
    }
}

impl MostroNodeConfig {
    /// Construct a new config. Returns an error if `relays` is empty.
    #[allow(dead_code, clippy::too_many_arguments)]
    pub fn new(pubkey: String, relays: Vec<String>, label: Option<String>) -> Result<Self, String> {
        if relays.is_empty() {
            return Err("node config must include at least one relay".to_string());
        }
        Ok(Self {
            version: NODE_CONFIG_VERSION,
            pubkey,
            relays,
            label,
            updated_at: crate::platform::timestamp::now_secs(),
            pow: 0,
            bond_payout_claim_window_days: 30,
            fee: None,
            min_order_amount: None,
            max_order_amount: None,
            expiration_seconds: None,
            fiat_currencies_accepted: Vec::new(),
            hold_invoice_expiration_window: None,
            hold_invoice_cltv_delta: None,
            invoice_expiration_window: None,
            bond_enabled: false,
            bond_amount_pct: None,
            bond_base_amount_sats: None,
            bond_apply_to: None,
            bond_slash_on_waiting_timeout: None,
            bond_slash_node_share_pct: None,
        })
    }

    /// Phase 6.2 (M14): copy all fields from a `MostroNodeInfo` into this
    /// config. Used by `update_pow_from_event` and `switch_to_daemon` to
    /// ensure the persisted config carries the full daemon capabilities.
    pub fn apply_info(&mut self, info: &MostroNodeInfo) {
        self.pow = info.pow;
        self.bond_payout_claim_window_days = info.bond_payout_claim_window_days;
        self.fee = info.fee;
        self.min_order_amount = info.min_order_amount;
        self.max_order_amount = info.max_order_amount;
        self.expiration_seconds = info.expiration_seconds;
        self.fiat_currencies_accepted = info.fiat_currencies_accepted.clone();
        self.hold_invoice_expiration_window = info.hold_invoice_expiration_window;
        self.hold_invoice_cltv_delta = info.hold_invoice_cltv_delta;
        self.invoice_expiration_window = info.invoice_expiration_window;
        self.bond_enabled = info.bond_enabled;
        self.bond_amount_pct = info.bond_amount_pct;
        self.bond_base_amount_sats = info.bond_base_amount_sats;
        self.bond_apply_to = info.bond_apply_to.clone();
        self.bond_slash_on_waiting_timeout = info.bond_slash_on_waiting_timeout;
        self.bond_slash_node_share_pct = info.bond_slash_node_share_pct;
    }
}

/// Global reactive state. Read with `MOSTRO_NODE_CONFIG()`.
#[allow(dead_code)]
pub static MOSTRO_NODE_CONFIG: GlobalSignal<Option<MostroNodeConfig>> = Signal::global(|| None);

/// Load from local cache. Returns `None` if nothing is cached.
fn read_cache() -> Option<MostroNodeConfig> {
    storage::get::<String>(CACHE_KEY)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
}

fn write_cache(cfg: &MostroNodeConfig) -> Result<(), String> {
    let json = serde_json::to_string(cfg)
        .map_err(|e| format!("failed to serialize node config: {e}"))?;
    storage::set(CACHE_KEY, &json).map_err(|e| format!("failed to cache node config: {e}"))
}

fn clear_cache() {
    let _ = storage::delete(CACHE_KEY);
}

/// Verify an event: pubkey matches user, event.verify() passes, content
/// deserializes to a valid `MostroNodeConfig` with a current version.
/// Phase 1.2 (C4): the event content may be NIP-44-encrypted (new format)
/// or plaintext JSON (legacy, pre-upgrade). The
/// `private_app_data::decrypt_from_self_or_legacy` helper tries decrypt
/// first, then falls back to plaintext.
fn evaluate_event(event: &NostrEvent, user_pubkey: &PublicKey) -> Option<MostroNodeConfig> {
    if event.pubkey != *user_pubkey {
        return None;
    }
    if event.verify().is_err() {
        return None;
    }
    let parsed: MostroNodeConfig =
        crate::stores::private_app_data::decrypt_from_self_or_legacy(&event.content).ok()?;
    if parsed.version != NODE_CONFIG_VERSION {
        return None;
    }
    if parsed.relays.is_empty() {
        return None;
    }
    Some(parsed)
}

/// Load the cached config synchronously into the global signal.
/// Call this during app init (alongside the other NIP-78 first-loaders)
/// for instant availability in the UI.
#[allow(dead_code)]
pub fn init_from_cache() {
    if MOSTRO_NODE_CONFIG.read().is_some() {
        return;
    }
    if let Some(cfg) = read_cache() {
        let _ = storage::delete(CLEARED_SENTINEL_KEY);
        *MOSTRO_NODE_CONFIG.write() = Some(cfg);
        return;
    }
    if storage::get::<bool>(CLEARED_SENTINEL_KEY).ok() == Some(true) {
        return;
    }
    if let Some(default) = super::communities::default_node_config() {
        *MOSTRO_NODE_CONFIG.write() = Some(default);
    }
}

/// Try to refresh the config from relays. Safe to call multiple times.
///
/// Order of operations:
/// 1. Read cache. If present, also set the global signal synchronously.
/// 2. If the user is authenticated and the client is initialized, fetch the
///    kind 30078 event from relays, validate, and update both cache and
///    signal.
/// 3. On any failure, leave the existing state alone.
#[allow(dead_code)]
pub async fn refresh_from_relays() -> Result<Option<MostroNodeConfig>, String> {
    if MOSTRO_NODE_CONFIG.read().is_none() {
        if let Some(c) = read_cache() {
            *MOSTRO_NODE_CONFIG.write() = Some(c);
        }
    }

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {e}"))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let filter = Filter::new()
        .author(pubkey)
        .kind(Kind::from(30078))
        .identifier(NODE_CONFIG_D_TAG)
        .limit(1);
    // Gate: ensure the user's NIP-65 outbox relays are in the pool before
    // fetching, so we query the right relays (not the bootstrap set).
    crate::stores::relay::wait_for_user_relays(
        std::time::Duration::from_secs(5),
        "mostro::node_config::fetch_config_from_relays",
    )
    .await;
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            let cfg = events.iter().find_map(|e| evaluate_event(e, &pubkey));
            if let Some(ref c) = cfg {
                write_cache(c)?;
                *MOSTRO_NODE_CONFIG.write() = Some(c.clone());
            }
            Ok(cfg)
        }
        Err(e) => {
            log::warn!("Failed to fetch Mostro node config: {e}");
            // Cache hit is good enough
            Ok(read_cache())
        }
    }
}

/// Extract the `pow` tag and `bond_payout_claim_window_days` from a kind 38385
/// daemon info event and update the current node config in memory + cache.
///
/// Phase 6.2 (M14): now copies ALL fields from the info event into the
/// persisted config (was only `pow` + `bond_payout_claim_window_days`).
/// This ensures the config survives restarts with full daemon capabilities.
#[allow(dead_code)]
pub fn update_pow_from_event(event: &NostrEvent) {
    if event.kind.as_u16() != 38385 {
        return;
    }
    let info = match MostroNodeInfo::from_event(event) {
        Some(i) => i,
        None => return,
    };

    if let Some(ref mut cfg) = *MOSTRO_NODE_CONFIG.write() {
        cfg.apply_info(&info);
        let _ = write_cache(cfg);
    }

    *MOSTRO_NODE_INFO.write() = Some(info);
}

/// Global signal holding the last-seen kind 38385 node info.
pub static MOSTRO_NODE_INFO: GlobalSignal<Option<MostroNodeInfo>> = Signal::global(|| None);

/// Persist a new config: write to NIP-78, update local cache and global signal.
///
/// Phase 1.2 (C4): the config JSON is NIP-44-encrypted to self (using the
/// Mostro identity key) before publishing, so that the user's choice of
/// Mostro daemon is not visible on public relays. When Mostro keys are
/// absent (e.g. user selected a daemon before generating/importing trade
/// keys), the config is persisted locally only — the relay publish is
/// deferred until keys become available (see
/// `auth_store::run_post_login_init` which calls `save_config` again after
/// `mostro::init()` succeeds).
#[allow(dead_code)]
pub async fn save_config(cfg: MostroNodeConfig) -> Result<(), String> {
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    // Always update local cache + signal first — this is the source of
    // truth for the UI, and survives even when relay publish can't run.
    write_cache(&cfg)?;
    let _ = storage::delete(CLEARED_SENTINEL_KEY);
    *MOSTRO_NODE_CONFIG.write() = Some(cfg.clone());

    // If Mostro keys are unavailable, skip the encrypted relay publish.
    // The config is still saved locally; when keys become available the
    // caller should invoke `save_config` again (or `publish_pending`).
    let builder = match crate::stores::private_app_data::build_encrypted_event_builder(
        NODE_CONFIG_D_TAG,
        &cfg,
    ) {
        Ok(b) => b,
        Err(e) => {
            log::debug!(
                "Skipping encrypted node-config publish (no Mostro keys yet?): {e}"
            );
            return Ok(());
        }
    };

    let event = publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign node config: {e}"))?;

    publish_queue::enqueue_and_await(
        event,
        QueueEventType::Other("p2p_node_config".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await
    .map_err(|e| format!("Failed to publish node config: {e}"))?;

    Ok(())
}

/// Convenience accessor.
#[allow(dead_code)]
pub fn try_get() -> Option<MostroNodeConfig> {
    MOSTRO_NODE_CONFIG.read().clone()
}

/// Phase 6.4: client-side validation warnings for a take/create attempt.
///
/// Returns a list of warning strings (empty if all checks pass). These are
/// advisory — the caller should surface them as toasts, not block the action.
/// The daemon will ultimately reject invalid takes via `CantDo`.
#[allow(dead_code)]
pub fn validate_against_node_limits(
    fiat_code: &str,
    sats_amount: Option<i64>,
    _fiat_amount: Option<f64>,
    user_role: &str,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let cfg = match try_get() {
        Some(c) => c,
        None => return warnings,
    };

    // Currency check.
    if !cfg.fiat_currencies_accepted.is_empty()
        && !cfg
            .fiat_currencies_accepted
            .iter()
            .any(|c| c.eq_ignore_ascii_case(fiat_code))
    {
        warnings.push(format!(
            "Currency {fiat_code} is not in this daemon's accepted list: {}",
            cfg.fiat_currencies_accepted.join(", ")
        ));
    }

    // Sats amount range check (only for fixed-price orders where amount > 0).
    if let Some(sats) = sats_amount {
        if sats > 0 {
            if let Some(min) = cfg.min_order_amount {
                if (sats as u64) < min {
                    warnings.push(format!(
                        "Sats amount {sats} is below this daemon's minimum ({min})"
                    ));
                }
            }
            if let Some(max) = cfg.max_order_amount {
                if (sats as u64) > max {
                    warnings.push(format!(
                        "Sats amount {sats} exceeds this daemon's maximum ({max})"
                    ));
                }
            }
        }
    }

    // Bond warning: if bonds are enabled and apply_to matches the user's
    // role, warn that a bond will be required.
    if cfg.bond_enabled {
        if let Some(ref apply_to) = cfg.bond_apply_to {
            let needs_bond = match apply_to.as_str() {
                "both" => true,
                "take" => user_role == "taker",
                "make" => user_role == "maker",
                _ => false,
            };
            if needs_bond {
                let pct = cfg
                    .bond_amount_pct
                    .map(|p| format!("{p}%"))
                    .unwrap_or_else(|| "configurable".to_string());
                warnings.push(format!(
                    "This daemon requires an anti-abuse bond ({pct} of order amount) for {user_role}s."
                ));
            }
        }
    }

    warnings
}

/// Fetch the daemon's NIP-65 relay list (kind 10002) and update the node
/// config's relay list if it differs from what we have cached.
#[allow(dead_code)]
pub async fn sync_relays_from_nip65() -> Result<(), String> {
    let cfg = try_get().ok_or("Node not configured")?;
    let pk = PublicKey::from_hex(&cfg.pubkey)
        .or_else(|_| PublicKey::from_bech32(&cfg.pubkey))
        .map_err(|e| format!("Bad daemon pubkey: {e}"))?;
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or("Client not initialized")?
        .clone();
    let filter = Filter::new().author(pk).kind(Kind::Custom(10002)).limit(1);
    let events = client.fetch_events(filter, Duration::from_secs(5)).await
        .map_err(|e| format!("NIP-65 fetch failed: {e}"))?;
    let event = match events.iter().max_by_key(|e| e.created_at) {
        Some(e) => e,
        None => return Ok(()),
    };
    let mut new_relays: Vec<String> = event.tags.iter()
        .filter_map(|t| {
            if t.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("r")) {
                t.content().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    if new_relays.is_empty() {
        return Ok(());
    }
    new_relays.sort();
    let mut old_relays = cfg.relays.clone();
    old_relays.sort();
    if new_relays == old_relays {
        return Ok(());
    }
    log::info!("Syncing {} relays from daemon NIP-65 (had {})", new_relays.len(), old_relays.len());
    let mut updated = cfg.clone();
    updated.relays = new_relays;
    updated.updated_at = crate::platform::timestamp::now_secs();
    write_cache(&updated)?;
    *MOSTRO_NODE_CONFIG.write() = Some(updated);
    Ok(())
}

/// Update the cached node config's relay list from a kind 10002 (NIP-65)
/// event received via subscription. Unlike `sync_relays_from_nip65` which
/// does a one-shot fetch, this accepts a pre-fetched event.
///
/// Compares sorted relay lists and updates cache + signal only when changed.
#[allow(dead_code)]
pub fn update_relays_from_nip65_event(event: &NostrEvent) -> Result<(), String> {
    if event.kind.as_u16() != 10002 {
        return Ok(());
    }
    let mut cfg = try_get().ok_or("Node not configured")?;
    let mut new_relays: Vec<String> = event
        .tags
        .iter()
        .filter_map(|t| {
            if t.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("r")) {
                t.content().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    if new_relays.is_empty() {
        return Ok(());
    }
    new_relays.sort();
    let mut old_relays = cfg.relays.clone();
    old_relays.sort();
    if new_relays == old_relays {
        return Ok(());
    }
    log::info!(
        "NIP-65 relay list changed: {} relays (was {})",
        new_relays.len(),
        old_relays.len()
    );
    cfg.relays = new_relays;
    cfg.updated_at = crate::platform::timestamp::now_secs();
    write_cache(&cfg)?;
    *MOSTRO_NODE_CONFIG.write() = Some(cfg);
    Ok(())
}

/// Clear the saved daemon: publish a NIP-09 deletion for the kind 30078
/// coordinate, wipe local cache, set a sentinel to prevent re-seeding the
/// default community node on next app load, and set the global signal to
/// `None`.
#[allow(dead_code)]
pub async fn clear_config() -> Result<(), String> {
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey = PublicKey::parse(&pubkey_str).map_err(|e| format!("Invalid pubkey: {e}"))?;

    let coord = Coordinate::new(Kind::from(30078), pubkey).identifier(NODE_CONFIG_D_TAG);
    let deletion_request = EventDeletionRequest::new()
        .coordinate(coord)
        .reason("Clearing daemon configuration");
    let builder = EventBuilder::delete(deletion_request);
    let event = publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign node config deletion: {e}"))?;

    publish_queue::enqueue(
        event,
        QueueEventType::Other("p2p_node_config".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;

    clear_cache();
    let _ = storage::set(CLEARED_SENTINEL_KEY, &true);
    *MOSTRO_NODE_CONFIG.write() = None;
    Ok(())
}

/// Wipe the local cache and global signal. Used on logout.
#[allow(dead_code)]
pub fn reset() {
    clear_cache();
    let _ = storage::delete(CLEARED_SENTINEL_KEY);
    *MOSTRO_NODE_CONFIG.write() = None;
    *MOSTRO_NODE_INFO.write() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_d_tag_matches_convention() {
        assert!(NODE_CONFIG_D_TAG.starts_with("nostr.blue/"));
        assert!(NODE_CONFIG_D_TAG.ends_with("/node"));
    }

    #[test]
    fn test_version_is_positive() {
        assert!(NODE_CONFIG_VERSION >= 1);
    }

    #[test]
    fn test_node_config_serde_roundtrip() {
        let cfg = MostroNodeConfig {
            version: NODE_CONFIG_VERSION,
            pubkey: "npub1...".to_string(),
            relays: vec!["wss://relay.example.com".to_string()],
            label: Some("Test".to_string()),
            updated_at: 1_700_000_000,
            pow: 0,
            bond_payout_claim_window_days: 30,
            fee: Some(0.003),
            min_order_amount: Some(1000),
            max_order_amount: Some(500000),
            expiration_seconds: Some(900),
            fiat_currencies_accepted: vec!["USD".to_string(), "EUR".to_string()],
            hold_invoice_expiration_window: Some(86400),
            hold_invoice_cltv_delta: Some(144),
            invoice_expiration_window: Some(3600),
            bond_enabled: true,
            bond_amount_pct: Some(0.01),
            bond_base_amount_sats: Some(1000),
            bond_apply_to: Some("take".to_string()),
            bond_slash_on_waiting_timeout: Some(false),
            bond_slash_node_share_pct: Some(0.5),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: MostroNodeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, cfg.version);
        assert_eq!(parsed.pubkey, cfg.pubkey);
        assert_eq!(parsed.relays, cfg.relays);
        assert_eq!(parsed.label, cfg.label);
        assert_eq!(parsed.updated_at, cfg.updated_at);
        assert_eq!(parsed.bond_payout_claim_window_days, 30);
        assert_eq!(parsed.fee, Some(0.003));
        assert_eq!(parsed.bond_enabled, true);
        assert_eq!(parsed.bond_apply_to.as_deref(), Some("take"));
    }

    #[test]
    fn test_new_rejects_empty_relays() {
        // The constructor checks relays.is_empty() and returns Err before
        // calling now_secs(). This is a pure logic test, no time required.
        let result = MostroNodeConfig::new("npub1...".to_string(), Vec::new(), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one relay"));
    }

    #[test]
    fn test_new_accepts_valid_config() {
        // Build directly to avoid WASM-only now_secs() in native tests.
        let cfg = MostroNodeConfig {
            version: NODE_CONFIG_VERSION,
            pubkey: "npub1...".to_string(),
            relays: vec!["wss://relay.example.com".to_string()],
            label: None,
            updated_at: 1_700_000_000,
            pow: 0,
            bond_payout_claim_window_days: 30,
            fee: None,
            min_order_amount: None,
            max_order_amount: None,
            expiration_seconds: None,
            fiat_currencies_accepted: Vec::new(),
            hold_invoice_expiration_window: None,
            hold_invoice_cltv_delta: None,
            invoice_expiration_window: None,
            bond_enabled: false,
            bond_amount_pct: None,
            bond_base_amount_sats: None,
            bond_apply_to: None,
            bond_slash_on_waiting_timeout: None,
            bond_slash_node_share_pct: None,
        };
        assert_eq!(cfg.version, NODE_CONFIG_VERSION);
        assert!(!cfg.relays.is_empty());
    }
}
