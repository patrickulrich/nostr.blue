//! [`MostroPrefsBlob`] — unified Mostro preferences encrypted to self via
//! the Mostro identity key (sync NIP-44).

use serde::{Deserialize, Serialize};

use crate::stores::mostro::creation_ledger::CreationLedgerEntry;
use crate::stores::mostro::node_config::MostroNodeConfig;
use crate::stores::mostro::trade_store::Trade;
use crate::stores::ui::p2p_settings::MostroSettings;
use crate::stores::user_prefs::MAX_RECENT_TRADES;

/// Unified Mostro preference blob.
///
/// Serialized to JSON, encrypted via NIP-44 to self using the **Mostro
/// identity key** (a separate NIP-06 keypair, always available locally —
/// see `private_app_data.rs`). Published as kind 30078 with d-tag
/// `nostr.blue/p2p`.
///
/// ## Trade history bounding
///
/// Trade history is bounded to [`MAX_RECENT_TRADES`] (50) most-recent
/// entries by `updated_at`. Older trades spill to
/// `nostr.blue/p2p/trades-archive` (a separate addressable event with the
/// same encryption). This keeps the active blob small enough to stay well
/// under common relay size caps (64–256 KB). The `archive_cursor` field
/// points at the archival spillover event for lazy loading.
///
/// ## Forward compatibility
///
/// Every field uses `#[serde(default)]` so blobs written by older versions
/// deserialize correctly.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MostroPrefsBlob {
    /// Schema version.
    #[serde(default = "default_version")]
    pub version: u32,

    /// Mostro user settings (fiat currency, notification toggles, etc.).
    #[serde(default)]
    pub settings: MostroSettings,

    /// Mostro daemon config (which daemon the user is on).
    /// `None` if no daemon selected.
    #[serde(default)]
    pub node_config: Option<MostroNodeConfig>,

    /// Most-recent trades (bounded to [`MAX_RECENT_TRADES`]), sorted by
    /// `updated_at` descending. Older trades are archived.
    #[serde(default)]
    pub recent_trades: Vec<Trade>,

    /// Cursor pointing at the archival spillover event.
    /// Format: `"<created_at>:<event_id>"`. `None` if no spillover.
    #[serde(default)]
    pub archive_cursor: Option<String>,

    /// Durable "orders I created/took" ledger — survives TRADES cache wipes
    /// and records order_ids/trade_indices for recovery. Newest first,
    /// bounded. `#[serde(default)]` for forward compat with older blobs.
    #[serde(default)]
    pub creation_ledger: Vec<CreationLedgerEntry>,
}

fn default_version() -> u32 {
    1
}

impl Default for MostroPrefsBlob {
    fn default() -> Self {
        Self {
            version: default_version(),
            settings: MostroSettings::default(),
            node_config: None,
            recent_trades: Vec::new(),
            archive_cursor: None,
            creation_ledger: Vec::new(),
        }
    }
}

impl MostroPrefsBlob {
    /// Bound `recent_trades` to [`MAX_RECENT_TRADES`], returning any
    /// spillover (trades that should be archived). Also sorts the remaining
    /// trades by `updated_at` descending.
    pub fn bound_trades(&mut self) -> Vec<Trade> {
        self.recent_trades
            .sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        if self.recent_trades.len() <= MAX_RECENT_TRADES {
            return Vec::new();
        }
        self.recent_trades.split_off(MAX_RECENT_TRADES)
    }

    /// Merge `remote` into `local` for trades: union by `order_id`,
    /// keeping the newer `updated_at` per trade.
    pub fn merge_trades(local: &[Trade], remote: &[Trade]) -> Vec<Trade> {
        use std::collections::HashMap;
        let mut by_order: HashMap<String, Trade> = HashMap::new();
        for t in local {
            by_order
                .entry(t.order_id.clone())
                .or_insert_with(|| t.clone());
        }
        for t in remote {
            let key = t.order_id.clone();
            match by_order.get(&key) {
                Some(existing) => {
                    if t.updated_at >= existing.updated_at {
                        by_order.insert(key, t.clone());
                    }
                }
                None => {
                    by_order.insert(key, t.clone());
                }
            }
        }
        let mut merged: Vec<Trade> = by_order.into_values().collect();
        merged.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        merged
    }
}
