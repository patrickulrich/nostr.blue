//! Durable "orders I created/took" ledger.
//!
//! A slim, append-only record of every order the user initiated (as maker or
//! taker), persisted in the Mostro NIP-78 preference blob
//! (`nostr.blue/p2p`). It survives any wipe of the rich `TRADES` cache and
//! gives a permanent handle `(order_id, trade_index, role)` to re-derive the
//! trade key and recover/cancel an order even when no local `Trade` record
//! exists.
//!
//! This is the durability backstop: [`crate::stores::mostro::restore::recover_order_by_id`]
//! is the primary recovery primitive (it derives the trade_index itself by
//! scanning derived keys), but the ledger additionally records *which*
//! orders are ours — including completed/canceled ones no longer on the
//! public board — so My Trades can surface and reconcile them.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use super::trade_store::TradeRole;

/// Cap on retained entries (newest first). Keeps the NIP-78 blob small.
const MAX_LEDGER_ENTRIES: usize = 200;

/// One ledger row per order-involvement.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CreationLedgerEntry {
    /// Order id (UUID once ACK'd; `maker-{N}`/`taker-{N}` placeholder until then).
    pub order_id: String,
    /// Maker or Taker — the user's role for this order.
    pub role: TradeRole,
    /// `buy` or `sell`.
    #[serde(default)]
    pub kind: String,
    /// NIP-06 trade-index the user used (to re-derive the trade key).
    /// `None` in privacy mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trade_index: Option<u32>,
    /// The user's trade pubkey for this order (if known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub my_trade_pubkey: Option<String>,
    /// Daemon pubkey this order belongs to.
    #[serde(default)]
    pub daemon_pubkey: String,
    /// Unix-secs when the involvement was created.
    pub created_at: i64,
    /// Flips true once the daemon ACKs (placeholder → real UUID).
    #[serde(default)]
    pub confirmed: bool,
}

/// Global reactive ledger. Read with `CREATION_LEDGER()`. Bounded to
/// [`MAX_LEDGER_ENTRIES`] (newest first).
#[allow(dead_code)]
pub static CREATION_LEDGER: GlobalSignal<Vec<CreationLedgerEntry>> = Signal::global(Vec::new);

/// Monotonic version bumped on every mutation, so reactive consumers (the
/// app-shell persistence watcher) can subscribe without cloning the Vec.
/// Mirrors `trade_store::TRADES_VERSION`.
#[allow(dead_code)]
pub static CREATION_LEDGER_VERSION: GlobalSignal<u64> = Signal::global(|| 0);

fn bump_version() {
    *CREATION_LEDGER_VERSION.write() = CREATION_LEDGER_VERSION.read().wrapping_add(1);
}

/// Append or update a ledger entry. Dedup is by `(trade_index, role)` when
/// the index is known (so a placeholder `maker-{N}` is migrated in-place to
/// its real UUID on ACK), otherwise by `(order_id, role)`. Newest first;
/// bounded to [`MAX_LEDGER_ENTRIES`].
#[allow(dead_code)]
pub fn append(entry: CreationLedgerEntry) {
    let mut list = CREATION_LEDGER.write();
    let role = entry.role;
    let idx = entry.trade_index;
    if let Some(existing) = list.iter_mut().find(|e| {
        (idx.is_some() && e.trade_index == idx && e.role == role)
            || (e.order_id == entry.order_id && e.role == role)
    }) {
        *existing = entry;
    } else {
        list.insert(0, entry);
    }
    list.truncate(MAX_LEDGER_ENTRIES);
    drop(list);
    bump_version();
}

/// Mark a (real-UUID) order as confirmed by the daemon. Migrates a matching
/// placeholder entry's `order_id` to the real UUID and sets `confirmed`.
#[allow(dead_code)]
pub fn confirm(real_order_id: &str, trade_index: Option<u32>, role: TradeRole) {
    let mut list = CREATION_LEDGER.write();
    // Prefer matching by (trade_index, role); fall back to order_id.
    let found = list.iter_mut().find(|e| {
        (trade_index.is_some() && e.trade_index == trade_index && e.role == role)
            || (e.order_id == real_order_id && e.role == role)
    });
    let mut changed = false;
    if let Some(existing) = found {
        if !existing.confirmed || existing.order_id != real_order_id {
            existing.order_id = real_order_id.to_string();
            existing.confirmed = true;
            changed = true;
        }
    }
    drop(list);
    if changed {
        bump_version();
    }
}

/// All ledger entries for a given order id (any role).
#[allow(dead_code)]
pub fn entries_for_order(order_id: &str) -> Vec<CreationLedgerEntry> {
    CREATION_LEDGER
        .read()
        .iter()
        .filter(|e| e.order_id == order_id)
        .cloned()
        .collect()
}
