//! Mostro trade persistence
//!
//! Stores the user's local view of all Mostro trades they are a party to.
//! Backing store: NIP-78 (kind 30078) event with d-tag
//! `nostr.blue/p2p/trades`, content = a JSON array of [`Trade`] records.
//!
//! The daemon remains the authoritative source of truth (its database is
//! canonical). The local store is just a cache of trades the user has
//! taken or made, so they can browse their history offline and the UI can
//! show the trade view instantly on app load without re-querying relays.
//!
//! Trade records are denormalized snapshots at the moment they were
//! observed — they do NOT auto-update when the daemon's state changes.
//! The trade-detail page re-subscribes for live updates.

use dioxus::prelude::*;
use nostr::prelude::*;
use nostr_sdk::Event as NostrEvent;
use serde::{Deserialize, Serialize};
use std::result::Result;
use std::time::Duration;

use crate::platform::storage;
use crate::stores::auth_store;
use crate::stores::nostr_client;
use crate::stores::publish_queue::{self, types::QueueEventType};

/// NIP-78 d-tag for the trades list event.
pub const TRADES_D_TAG: &str = "nostr.blue/p2p/trades";

/// NIP-78 d-tag prefix for per-trade events.
const TRADE_D_TAG_PREFIX: &str = "nostr.blue/p2p/trade/";

/// Local cache key (in `platform::storage`).
const CACHE_KEY: &str = "mostro_trades_v1";

/// Bumped only if the on-wire `Trade` schema changes.
#[allow(dead_code)]
pub const TRADES_VERSION: u32 = 1;

/// What side of the trade the user is on.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TradeRole {
    /// The user is taking the order (buyer for sell orders, seller for buy orders).
    Taker,
    /// The user is the maker of the order.
    Maker,
}

/// Phase 3.5 (F15): who initiated a cancel. Drives cleanup timing — see
/// `Trade::cancel_initiator` doc.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CancelInitiator {
    /// The local user sent `Action::Cancel` (or accepted the peer's
    /// cooperative-cancel request). No slash is expected.
    User,
    /// The counterparty sent `Action::Cancel` / `CooperativeCancelInitiatedByPeer`
    /// and we accepted via `Action::Cancel`. No slash is expected from us.
    Peer,
    /// A solver/admin canceled the trade via `Action::AdminCancel`. A
    /// trailing `Action::BondSlashed` may follow within ~60s if bonds
    /// are enabled on the daemon and the admin directed a slash.
    Admin,
    /// The daemon itself canceled (timeout, expiry, etc.). Behavior
    /// matches `Admin` for safety.
    Daemon,
}

impl TradeRole {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeRole::Taker => "taker",
            TradeRole::Maker => "maker",
        }
    }
}

/// Lifecycle status of a trade from the user's perspective.
///
/// This is a derived view of the `Action` + `Status` we have most recently
/// observed from the daemon. It is not the canonical status.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TradeStatus {
    Pending,
    WaitingBuyerInvoice,
    WaitingSellerToPay,
    WaitingBond,
    WaitingTakerBond,
    WaitingMakerBond,
    Active,
    FiatSent,
    Settled,
    Success,
    Canceled,
    CancelPending,
    CooperativelyCanceled,
    CanceledByAdmin,
    Expired,
    Dispute,
    PaymentFailed,
}

impl TradeStatus {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            TradeStatus::Pending => "Pending",
            TradeStatus::WaitingBuyerInvoice => "Waiting for invoice",
            TradeStatus::WaitingSellerToPay => "Waiting for payment",
            TradeStatus::WaitingBond => "Waiting for bond",
            TradeStatus::WaitingTakerBond => "Waiting for taker bond",
            TradeStatus::WaitingMakerBond => "Waiting for maker bond",
            TradeStatus::Active => "In progress",
            TradeStatus::FiatSent => "Fiat sent",
            TradeStatus::Settled => "Settling",
            TradeStatus::Success => "Completed",
            TradeStatus::Canceled => "Canceled",
            TradeStatus::CancelPending => "Cancel pending",
            TradeStatus::CooperativelyCanceled => "Cooperatively canceled",
            TradeStatus::CanceledByAdmin => "Canceled by admin",
            TradeStatus::Expired => "Expired",
            TradeStatus::Dispute => "In dispute",
            TradeStatus::PaymentFailed => "Payment failed",
        }
    }

    /// Progress rank for monotonicity guard. Stale relay events that would
    /// regress the status are silently dropped.
    #[allow(dead_code)]
    pub fn progress_rank(&self) -> u8 {
        match self {
            TradeStatus::Pending => 0,
            TradeStatus::WaitingBond | TradeStatus::WaitingTakerBond | TradeStatus::WaitingMakerBond => 0,
            TradeStatus::WaitingBuyerInvoice | TradeStatus::WaitingSellerToPay => 1,
            TradeStatus::Active | TradeStatus::Dispute | TradeStatus::CancelPending => 2,
            TradeStatus::FiatSent => 3,
            TradeStatus::PaymentFailed => 4,
            TradeStatus::Settled => 5,
            TradeStatus::Success => 6,
            TradeStatus::Canceled
            | TradeStatus::Expired
            | TradeStatus::CooperativelyCanceled
            | TradeStatus::CanceledByAdmin => 7,
        }
    }

    /// Terminal statuses that block all further transitions.
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TradeStatus::Canceled
                | TradeStatus::Expired
                | TradeStatus::Success
                | TradeStatus::CooperativelyCanceled
                | TradeStatus::CanceledByAdmin
        )
    }
}

/// A single Mostro trade record persisted to NIP-78.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Trade {
    /// Stable identifier: order id when known, otherwise the `event_id` of
    /// the kind 38383 order at take-time.
    pub order_id: String,
    /// Kind 38383 `d` tag.
    pub d_tag: String,
    /// Maker pubkey (hex). The creator of the order.
    pub maker_pubkey: String,
    /// This user's trade pubkey for the order (used to derive SharedKey for chat).
    #[serde(default, alias = "maker_trade_pubkey", skip_serializing_if = "Option::is_none")]
    pub my_trade_pubkey: Option<String>,
    /// Counterparty trade pubkey. Disclosed via `Action::FiatSentOk(Peer)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterparty_pubkey: Option<String>,
    /// Solver/admin pubkey assigned to a dispute (from AdminTookDispute).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solver_pubkey: Option<String>,
    /// Last request_id sent for this trade, used to correlate daemon responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_request_id: Option<u64>,
    /// What side the user is on.
    pub role: TradeRole,
    /// `buy` or `sell`.
    pub kind: String,
    /// Fiat amount (string-encoded, since precision can be arbitrary).
    pub fiat_amount: String,
    /// Fiat currency code.
    pub fiat_code: String,
    /// Sats amount if known; `None` for "market rate" orders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sats_amount: Option<i64>,
    /// Premium percentage (signed).
    pub premium: f64,
    /// Payment methods (free-form strings).
    pub payment_methods: Vec<String>,
    /// Current derived status.
    pub status: TradeStatus,
    /// Unix timestamp (seconds) when the trade was initiated.
    pub created_at: i64,
    /// Unix timestamp (seconds) of the last status change.
    pub updated_at: i64,
    /// Trade index used by the user for this trade (None in privacy mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_index: Option<u32>,
    /// Optional cached hold invoice we owe payment on (taker / sell side).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_hold_invoice: Option<String>,
    /// Optional cached payout invoice we provided (buyer side).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my_payout_invoice: Option<String>,
    /// Daemon requested a bond invoice via `AddBondInvoice`.
    #[serde(default)]
    pub needs_bond_invoice: bool,
    /// Daemon requested a bond payout invoice after a slash (terminal trade).
    #[serde(default)]
    pub needs_bond_payout: bool,
    /// Free-form notes the user attached (not sent to the daemon).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Lower bound of the original range order fiat amount. None for fixed-amount orders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_fiat: Option<f64>,
    /// Upper bound of the original range order fiat amount. None for fixed-amount orders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_fiat: Option<f64>,
    /// Dispute ID assigned by the daemon. Set when a dispute is opened or restored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispute_id: Option<String>,
    /// Payment failure details from the daemon (retry count, interval).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_failed_attempts: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_failed_retries_interval: Option<u32>,
    /// Whether fiat has been marked as sent (for cooperative cancel UX).
    #[serde(default)]
    pub fiat_was_sent: bool,
    /// Whether the pending hold invoice is a bond invoice (vs trade escrow).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_bond_invoice: Option<bool>,
    /// Timestamp when the counterparty's bond was slashed (for payout deadline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bond_slashed_at: Option<i64>,
    /// Computed payout claim deadline (slashed_at + claim_window_days * 86400).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bond_payout_deadline: Option<i64>,
    /// Phase 3.5 (F15): who initiated the cancel (User / Peer / Admin).
    ///
    /// Drives cleanup timing:
    /// - `User`-initiated cancels are deleted instantly (no slash expected
    ///   from a cancel the user themselves triggered).
    /// - `Admin`-initiated cancels with bonds enabled on the daemon get a
    ///   60-second grace window to receive trailing `BondSlashed` notices.
    /// - `Peer`-initiated (counterparty's cooperative cancel accepted by
    ///   us) behaves like `User` — instant cleanup.
    ///
    /// `None` for non-canceled trades or legacy records from before this
    /// field was added (cleanup falls back to the 30-day MAX_AGE_SECS
    /// sweep for those).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_initiator: Option<CancelInitiator>,
    /// Parent order ID for child orders created from range order NextTrade.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_order_id: Option<String>,
    /// Child order ID set on parent when a child is created via NextTrade.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_order_id: Option<String>,
    /// Next trade pubkey announced in a NextTrade payload (stored before sending).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_trade_pubkey: Option<String>,
    /// Next trade index announced in a NextTrade payload (stored before sending).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_trade_index: Option<u32>,
    /// Daemon pubkey (hex) that this trade belongs to. Empty for legacy trades.
    #[serde(default)]
    pub daemon_pubkey: String,
    /// Order expiration timestamp (unix seconds). Populated from the NIP-69
    /// `expires_at` tag or computed from creation + expiration hours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl Trade {
    pub fn is_buyer(&self) -> bool {
        matches!(
            (self.kind.to_lowercase().as_str(), self.role),
            ("buy", TradeRole::Maker) | ("sell", TradeRole::Taker)
        )
    }

    pub fn is_seller(&self) -> bool {
        !self.is_buyer()
    }

    #[allow(dead_code)]
    pub fn is_range_order(&self) -> bool {
        self.min_fiat.is_some() && self.max_fiat.is_some()
    }

    /// Returns true if `order_id` is a local placeholder (not yet
    /// confirmed by the daemon with a real UUID). Bug #11 fix: used
    /// to guard outbound actions that require a real order_id.
    pub fn is_placeholder(&self) -> bool {
        self.order_id.starts_with("maker-") || self.order_id.starts_with("taker-")
    }

    pub fn should_send_next_trade(&self) -> bool {
        if self.role != TradeRole::Maker {
            return false;
        }
        let (Some(min), Some(max)) = (self.min_fiat, self.max_fiat) else {
            return false;
        };
        let taken: f64 = self.fiat_amount.parse().unwrap_or(0.0);
        (max - taken) >= min
    }

    /// Build a new trade record from a take-action context.
    #[allow(dead_code, clippy::too_many_arguments)]
    pub fn new_pending(
        order_id: String,
        d_tag: String,
        maker_pubkey: String,
        role: TradeRole,
        kind: String,
        fiat_amount: String,
        fiat_code: String,
        sats_amount: Option<i64>,
        premium: f64,
        payment_methods: Vec<String>,
        trade_index: Option<u32>,
    ) -> Self {
        let now = crate::platform::timestamp::now_secs() as i64;
        Self {
            order_id,
            d_tag,
            maker_pubkey,
            my_trade_pubkey: None,
            counterparty_pubkey: None,
            solver_pubkey: None,
            last_request_id: None,
            role,
            kind,
            fiat_amount,
            fiat_code,
            sats_amount,
            premium,
            payment_methods,
            status: TradeStatus::Pending,
            created_at: now,
            updated_at: now,
            trade_index,
            pending_hold_invoice: None,
            my_payout_invoice: None,
            needs_bond_invoice: false,
            needs_bond_payout: false,
            note: None,
            min_fiat: None,
            max_fiat: None,
            dispute_id: None,
            payment_failed_attempts: None,
            payment_failed_retries_interval: None,
            fiat_was_sent: false,
            is_bond_invoice: None,
            bond_slashed_at: None,
            bond_payout_deadline: None,
            cancel_initiator: None,
            parent_order_id: None,
            child_order_id: None,
            next_trade_pubkey: None,
            next_trade_index: None,
            daemon_pubkey: super::node_config::try_get()
                .map(|c| c.pubkey)
                .unwrap_or_default(),
            expires_at: None,
        }
    }

    /// Build a new trade record with explicit timestamps. Used by tests
    /// and by callers that have already obtained a timestamp.
    #[allow(dead_code, clippy::too_many_arguments)]
    pub fn new_pending_at(
        now: i64,
        order_id: String,
        d_tag: String,
        maker_pubkey: String,
        role: TradeRole,
        kind: String,
        fiat_amount: String,
        fiat_code: String,
        sats_amount: Option<i64>,
        premium: f64,
        payment_methods: Vec<String>,
        trade_index: Option<u32>,
    ) -> Self {
        Self {
            order_id,
            d_tag,
            maker_pubkey,
            my_trade_pubkey: None,
            counterparty_pubkey: None,
            solver_pubkey: None,
            last_request_id: None,
            role,
            kind,
            fiat_amount,
            fiat_code,
            sats_amount,
            premium,
            payment_methods,
            status: TradeStatus::Pending,
            created_at: now,
            updated_at: now,
            trade_index,
            pending_hold_invoice: None,
            my_payout_invoice: None,
            needs_bond_invoice: false,
            needs_bond_payout: false,
            note: None,
            min_fiat: None,
            max_fiat: None,
            dispute_id: None,
            payment_failed_attempts: None,
            payment_failed_retries_interval: None,
            fiat_was_sent: false,
            is_bond_invoice: None,
            bond_slashed_at: None,
            bond_payout_deadline: None,
            cancel_initiator: None,
            parent_order_id: None,
            child_order_id: None,
            next_trade_pubkey: None,
            next_trade_index: None,
            daemon_pubkey: String::new(),
            expires_at: None,
        }
    }
}

/// Bump the `updated_at` and set a new status. Returns the mutated clone.
///
/// Enforces monotonicity: terminal statuses block all transitions, and
/// backward progress-rank changes are silently ignored (returns the
/// trade unchanged). The `Dispute`, `CancelPending`, `PaymentFailed`,
/// `CooperativelyCanceled`, and `CanceledByAdmin` statuses are exceptions —
/// they can be entered from any non-terminal rank because they represent
/// external events that override normal progression.
#[allow(dead_code)]
pub fn apply_status(trade: &Trade, new_status: TradeStatus) -> Trade {
    if !is_status_transition_allowed(&trade.status, &new_status) {
        return trade.clone();
    }
    let mut t = trade.clone();
    t.status = new_status;
    t.updated_at = crate::platform::timestamp::now_secs() as i64;
    t
}

/// Pure monotonicity predicate — no platform/timestamp dependency.
///
/// Extracted so callers (e.g. `apply_mostro_action`) can check whether a
/// transition would be allowed WITHOUT calling `apply_status` (which would
/// invoke `timestamp::now_secs` and break tests that run with the `web`
/// feature on a non-wasm target).
///
/// Returns `true` if `apply_status(trade, new_status)` would actually
/// change the trade's status; `false` if it would return the trade
/// unchanged (terminal block or backwards-rank regression).
///
/// Phase 2.4c (U4): also allows the trade to leave `CooperativelyCanceled`
/// for `Dispute` (when fiat has been sent — the user pressed cancel
/// prematurely but the counterparty hasn't accepted, and the user now
/// wants to dispute instead). `CooperativelyCanceled` itself stays in
/// `is_terminal()` so cleanup, active-trades filtering, and the
/// background monitor's lifecycle continue to treat it as terminal for
/// their respective purposes.
#[allow(dead_code)]
pub fn is_status_transition_allowed(
    current: &TradeStatus,
    new_status: &TradeStatus,
) -> bool {
    // DEFENSIVE: allow Dispute to override CooperativelyCanceled.
    //
    // This handles the edge case where a `DisputeInitiatedByPeer` action
    // arrives while the local status is `CooperativelyCanceled`. In
    // practice this is rare — the daemon's FSM gates disputes by status —
    // but it CAN happen if the counterparty disputes right as the
    // cooperative cancel is being finalized. If the daemon rejects the
    // dispute, it sends `CantDo` (which is a no-op + toast), so this
    // exception is harmless: it only allows the transition when the
    // daemon actually emits a Dispute action.
    //
    // The reference FSM (mostro/mobile order_state.actions) permits
    // `dispute` from `CooperativelyCanceled` when `fiat_was_sent`.
    if matches!(current, TradeStatus::CooperativelyCanceled)
        && matches!(new_status, TradeStatus::Dispute)
    {
        return true;
    }
    if current.is_terminal() {
        return false;
    }
    if new_status.progress_rank() < current.progress_rank()
        && !matches!(
            new_status,
            TradeStatus::Dispute
                | TradeStatus::CancelPending
                | TradeStatus::PaymentFailed
                | TradeStatus::CooperativelyCanceled
                | TradeStatus::CanceledByAdmin
        )
    {
        return false;
    }
    true
}

/// Set a new status with an explicit timestamp. Pure logic, no platform
/// dependency — used by tests and by callers that already have a timestamp.
#[allow(dead_code)]
pub fn apply_status_at(trade: &Trade, new_status: TradeStatus, now: i64) -> Trade {
    let mut t = trade.clone();
    t.status = new_status;
    t.updated_at = now;
    t
}

/// Global reactive list of all trades.
#[allow(dead_code)]
pub static TRADES: GlobalSignal<Vec<Trade>> = Signal::global(Vec::new);

/// Read from local cache. Returns empty Vec if nothing cached.
fn read_cache() -> Vec<Trade> {
    storage::get::<String>(CACHE_KEY)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn write_cache(trades: &[Trade]) -> Result<(), String> {
    let json = serde_json::to_string(trades)
        .map_err(|e| format!("failed to serialize trades: {e}"))?;
    storage::set(CACHE_KEY, &json).map_err(|e| format!("failed to cache trades: {e}"))
}

/// Synchronously load the cache into the global signal. Call this at app
/// init for instant trade list availability.
#[allow(dead_code)]
pub fn init_from_cache() {
    if TRADES.read().is_empty() {
        let cached = read_cache();
        if !cached.is_empty() {
            *TRADES.write() = cached;
        }
    }
}

/// Look up a trade by its `order_id`. Returns `None` if no such trade.
#[allow(dead_code)]
pub fn find_by_order_id(order_id: &str) -> Option<Trade> {
    TRADES.read().iter().find(|t| t.order_id == order_id).cloned()
}

/// Return all trades that are not in a terminal state (Success, Canceled, Expired).
#[allow(dead_code)]
pub fn active_trades() -> Vec<Trade> {
    TRADES.read().iter().filter(|t| !t.status.is_terminal()).cloned().collect()
}

/// Return active trades for the currently configured daemon.
/// Legacy trades (empty daemon_pubkey) are always included.
#[allow(dead_code)]
pub fn active_trades_for_daemon() -> Vec<Trade> {
    let daemon_pk = super::node_config::try_get().map(|c| c.pubkey).unwrap_or_default();
    TRADES.read().iter().filter(|t| {
        !t.status.is_terminal() && (t.daemon_pubkey.is_empty() || t.daemon_pubkey == daemon_pk)
    }).cloned().collect()
}

/// Return all trades for the currently configured daemon (including terminal).
/// Legacy trades (empty daemon_pubkey) are always included.
#[allow(dead_code)]
pub fn all_trades_for_daemon() -> Vec<Trade> {
    let daemon_pk = super::node_config::try_get().map(|c| c.pubkey).unwrap_or_default();
    TRADES.read().iter().filter(|t| {
        t.daemon_pubkey.is_empty() || t.daemon_pubkey == daemon_pk
    }).cloned().collect()
}

/// Add or update a trade. Upserts by **(order_id, role)**, falling back to
/// `(trade_index, role)` or `(my_trade_pubkey, role)` for placeholder-to-UUID
/// migration.
///
/// **Role-scoping is load-bearing:** a single user can only legitimately be
/// EITHER maker OR taker for a given order (a self-take is blocked in
/// `take_order`). Previously upsert matched on `order_id` alone, so a taker
/// record (from a self-take) silently overwrote the maker record —
/// destroying the maker's handle and making the order vanish from My Trades.
/// Requiring the role to match means a maker and taker record for the same
/// order coexist as distinct entries instead of one clobbering the other.
///
/// The change is reflected in the global signal and the local cache
/// immediately. The NIP-78 publish happens separately via [`publish`].
#[allow(dead_code)]
pub fn upsert(trade: Trade) {
    let mut list = TRADES.write();
    if let Some(existing) = list.iter_mut().find(|t| {
        // Primary key: (order_id, role). Same order_id but different role
        // (e.g. self-take) does NOT match → both records coexist.
        (t.order_id == trade.order_id && t.role == trade.role)
            // Placeholder→UUID migration within the SAME role: a `maker-{N}`
            // / `taker-{N}` placeholder is reconciled to its real UUID via
            // trade_index / my_trade_pubkey (both role-scoped).
            || (trade.trade_index.is_some()
                && t.trade_index == trade.trade_index
                && t.role == trade.role)
            || (trade.my_trade_pubkey.is_some()
                && trade.my_trade_pubkey == t.my_trade_pubkey
                && t.role == trade.role)
    }) {
        *existing = trade;
    } else {
        list.push(trade);
    }
    let snapshot = list.clone();
    drop(list);
    let _ = write_cache(&snapshot);
}

/// Remove a trade by `order_id`. No-op if not found.
#[allow(dead_code)]
pub fn remove(order_id: &str) {
    let mut list = TRADES.write();
    list.retain(|t| t.order_id != order_id);
    let snapshot = list.clone();
    drop(list);
    let _ = write_cache(&snapshot);
}

/// Wipe all trades. Used on logout or "Reset Mostro".
#[allow(dead_code)]
pub fn clear_all() {
    let _ = storage::delete(CACHE_KEY);
    *TRADES.write() = Vec::new();
}

/// C6: insert a placeholder trade for the next slice of a range order.
///
/// When a maker sends `Action::Release` or `Action::FiatSent` with a
/// `Payload::NextTrade(pubkey, idx)` payload on a range order, the daemon
/// creates a child order and sends a `NewOrder` ACK addressed to that
/// next-trade pubkey. For the background trade monitor (and the per-trade
/// subscription on the trade detail page) to receive and route that ACK,
/// there must be a trade record matching `my_trade_pubkey = next_pk`.
///
/// This helper inserts such a placeholder. When the daemon's ACK arrives
/// with the real child `order_id`, `upsert`'s `my_trade_pubkey` match
/// (line 610-611) replaces the placeholder `order_id` with the real one —
/// the standard placeholder→UUID migration path.
#[allow(dead_code)]
pub fn insert_range_child_placeholder(
    parent: &Trade,
    next_trade_pubkey: String,
    next_trade_index: u32,
) {
    let placeholder_id = format!("range-child-pending-{}", uuid::Uuid::new_v4());
    let now = crate::platform::timestamp::now_secs() as i64;
    let placeholder = Trade {
        order_id: placeholder_id,
        d_tag: String::new(),
        maker_pubkey: parent.maker_pubkey.clone(),
        my_trade_pubkey: Some(next_trade_pubkey),
        counterparty_pubkey: None,
        solver_pubkey: None,
        last_request_id: None,
        role: TradeRole::Maker,
        kind: parent.kind.clone(),
        fiat_amount: String::new(),
        fiat_code: parent.fiat_code.clone(),
        sats_amount: None,
        premium: parent.premium,
        payment_methods: parent.payment_methods.clone(),
        status: TradeStatus::Pending,
        created_at: now,
        updated_at: now,
        trade_index: Some(next_trade_index),
        pending_hold_invoice: None,
        my_payout_invoice: None,
        needs_bond_invoice: false,
        needs_bond_payout: false,
        note: None,
        min_fiat: parent.min_fiat,
        max_fiat: parent.max_fiat,
        dispute_id: None,
        payment_failed_attempts: None,
        payment_failed_retries_interval: None,
        fiat_was_sent: false,
        is_bond_invoice: None,
        bond_slashed_at: None,
        bond_payout_deadline: None,
        cancel_initiator: None,
        parent_order_id: Some(parent.order_id.clone()),
        child_order_id: None,
        next_trade_pubkey: None,
        next_trade_index: None,
        daemon_pubkey: parent.daemon_pubkey.clone(),
        expires_at: None,
    };
    log::info!(
        "C6: inserting range-child placeholder for parent {} (next_trade_pubkey={}, idx={})",
        parent.order_id,
        placeholder.my_trade_pubkey.as_deref().unwrap_or("?"),
        next_trade_index,
    );
    upsert(placeholder);
}

/// Verify a NIP-78 event is a valid trades record owned by the user.
///
/// Phase 1.2 (C4): the event content may be either NIP-44-encrypted (new
/// format) or plaintext JSON (legacy, pre-upgrade). The
/// `private_app_data::decrypt_from_self_or_legacy` helper tries decrypt
/// first, then falls back to plaintext parse. The caller is responsible
/// for triggering an encrypted re-publish when `looks_encrypted(content)`
/// returns false (legacy migration path).
fn evaluate_event(event: &NostrEvent, user_pubkey: &PublicKey) -> Option<Vec<Trade>> {
    if event.pubkey != *user_pubkey {
        return None;
    }
    if event.verify().is_err() {
        return None;
    }
    let parsed: Vec<Trade> =
        crate::stores::private_app_data::decrypt_from_self_or_legacy(&event.content).ok()?;
    Some(parsed)
}

/// Refresh from relays (best-effort). On failure, leaves the existing
/// global state alone (so the user can still see cached data offline).
///
/// Phase 1.7 (M11): previously this overwrote ALL local state with whatever
/// was on relays — including regressing local state to a stale relay copy
/// (e.g., if the background monitor had applied a newer status update that
/// hadn't yet propagated to relays). The fix is a per-trade merge:
/// - For each remote trade, find the matching local trade by `order_id`.
/// - If the local trade is missing, insert the remote one (subject to
///   `apply_status` for monotonicity safety).
/// - If both exist, keep whichever has the newer `updated_at`, BUT route
///   the remote status through `apply_status` so monotonicity guards
///   still apply.
/// - Local trades that aren't in the remote list are preserved (the relay
///   event may be older than local state).
#[allow(dead_code)]
pub async fn refresh_from_relays() -> Result<usize, String> {
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
        .identifier(TRADES_D_TAG)
        .limit(1);
    nostr_client::ensure_relays_ready(&client).await;

    match client.fetch_events(filter, Duration::from_secs(5)).await {
        Ok(events) => {
            let fresh = events.iter().find_map(|e| evaluate_event(e, &pubkey));
            if let Some(remote_list) = fresh {
                let count = merge_remote_trades(remote_list);
                let current = TRADES.read().clone();
                write_cache(&current)?;
                Ok(count)
            } else {
                Ok(TRADES.read().len())
            }
        }
        Err(e) => {
            log::warn!("Failed to fetch Mostro trades: {e}");
            Ok(TRADES.read().len())
        }
    }
}

/// Merge a remote list of trades into the local `TRADES` signal, applying
/// monotonicity guards and preserving newer local state. Returns the
/// resulting total count.
fn merge_remote_trades(remote_list: Vec<Trade>) -> usize {
    let mut current = TRADES.write();
    for remote in remote_list {
        match current.iter().position(|t| t.order_id == remote.order_id) {
            Some(idx) => {
                let local = current[idx].clone();
                // Keep whichever side is newer. When the remote is newer,
                // route its status through `apply_status` so we don't
                // regress (e.g., a stale relay event shouldn't undo a
                // local Active → FiatSent transition).
                if remote.updated_at > local.updated_at {
                    let merged_status = apply_status(&local, remote.status);
                    let mut merged = remote;
                    // Preserve local-only fields that the relay event
                    // might lack (e.g. my_trade_pubkey populated by the
                    // take flow but not yet re-published).
                    if merged.my_trade_pubkey.is_none() && local.my_trade_pubkey.is_some() {
                        merged.my_trade_pubkey = local.my_trade_pubkey.clone();
                    }
                    // apply_status may have rejected the regression;
                    // restore the resulting status + timestamp.
                    merged.status = merged_status.status;
                    merged.updated_at = merged_status.updated_at;
                    current[idx] = merged;
                }
                // else: local is newer or equal — keep local as-is.
            }
            None => {
                // Remote trade not present locally — insert it.
                // Status comes straight from the relay event; no local
                // state to regress.
                current.push(remote);
            }
        }
    }
    current.len()
}

/// Publish the current trade list to the user's write relays as a
/// NIP-78 (kind 30078) event with d-tag `nostr.blue/p2p/trades`.
///
/// Phase 1.2 (C4): the JSON content is encrypted via NIP-44 to self
/// (using the Mostro identity key) before publishing, so that the
/// sensitive cryptographic material in `Trade` (trade pubkeys, invoices,
/// solver pubkeys, etc.) is not visible on public relays. When Mostro
/// keys are unavailable (rare — trades only exist when keys exist), the
/// publish is skipped and only the local cache is updated.
#[allow(dead_code)]
pub async fn publish() -> Result<(), String> {
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    let list = TRADES.read().clone();
    let builder = match crate::stores::private_app_data::build_encrypted_event_builder(
        TRADES_D_TAG,
        &list,
    ) {
        Ok(b) => b,
        Err(e) => {
            log::debug!("Skipping encrypted trades publish: {e}");
            write_cache(&list)?;
            return Ok(());
        }
    };
    let event = publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign trades: {e}"))?;

    publish_queue::enqueue_and_await(
        event,
        QueueEventType::Other("p2p_trades".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await
    .map_err(|e| format!("Failed to publish trades: {e}"))?;

    write_cache(&list)?;
    Ok(())
}

/// Publish a single trade as its own NIP-78 event (per-trade persistence).
/// This avoids the multi-device conflict of the bulk snapshot approach.
///
/// Phase 1.2 (C4): the trade JSON is NIP-44-encrypted to self before
/// publishing. See `publish()` for the full rationale.
#[allow(dead_code)]
pub async fn publish_single(order_id: &str) -> Result<(), String> {
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    let trade = TRADES
        .read()
        .iter()
        .find(|t| t.order_id == order_id)
        .cloned()
        .ok_or_else(|| format!("Trade {order_id} not found"))?;

    let d_tag = format!("{TRADE_D_TAG_PREFIX}{order_id}");
    let builder = match crate::stores::private_app_data::build_encrypted_event_builder(
        &d_tag,
        &trade,
    ) {
        Ok(b) => b,
        Err(e) => {
            log::debug!("Skipping encrypted single-trade publish: {e}");
            return Ok(());
        }
    };
    let event = publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign trade: {e}"))?;

    publish_queue::enqueue(
        event,
        QueueEventType::Other("p2p_trade_single".to_string()),
        None,
        std::collections::HashMap::new(),
    )
    .await;

    Ok(())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn default_test_trade(status: TradeStatus) -> Trade {
        Trade {
            order_id: "test-order-123".to_string(),
            d_tag: "test-order-123".to_string(),
            maker_pubkey: "maker".to_string(),
            my_trade_pubkey: None,
            counterparty_pubkey: None,
            solver_pubkey: None,
            last_request_id: None,
            role: TradeRole::Taker,
            kind: "sell".to_string(),
            fiat_amount: "100".to_string(),
            fiat_code: "USD".to_string(),
            sats_amount: Some(50_000),
            premium: 0.0,
            payment_methods: vec![],
            status,
            created_at: 1_700_000_000,
            updated_at: 1_700_000_000,
            trade_index: Some(0),
            pending_hold_invoice: None,
            my_payout_invoice: None,
            needs_bond_invoice: false,
            needs_bond_payout: false,
            note: None,
            min_fiat: None,
            max_fiat: None,
            dispute_id: None,
            payment_failed_attempts: None,
            payment_failed_retries_interval: None,
            fiat_was_sent: false,
            is_bond_invoice: None,
            bond_slashed_at: None,
            bond_payout_deadline: None,
            cancel_initiator: None,
            parent_order_id: None,
            child_order_id: None,
            next_trade_pubkey: None,
            next_trade_index: None,
            daemon_pubkey: String::new(),
            expires_at: None,
        }
    }

    #[test]
    fn test_d_tag_matches_convention() {
        assert!(TRADES_D_TAG.starts_with("nostr.blue/"));
        assert!(TRADES_D_TAG.ends_with("/trades"));
    }

    #[test]
    fn test_trades_version_is_positive() {
        assert!(TRADES_VERSION >= 1);
    }

    #[test]
    fn test_trade_serde_roundtrip() {
        let trade = Trade {
            order_id: "abc-123".to_string(),
            d_tag: "order-xyz".to_string(),
            maker_pubkey: "makerhex".to_string(),
            my_trade_pubkey: None,
            counterparty_pubkey: Some("counterpartyhex".to_string()),
            solver_pubkey: None,
            last_request_id: None,
            role: TradeRole::Taker,
            kind: "sell".to_string(),
            fiat_amount: "100".to_string(),
            fiat_code: "EUR".to_string(),
            sats_amount: Some(150_000),
            premium: 1.5,
            payment_methods: vec!["SEPA".to_string()],
            status: TradeStatus::Active,
            created_at: 1_700_000_000,
            updated_at: 1_700_000_500,
            trade_index: Some(0),
            pending_hold_invoice: None,
            my_payout_invoice: None,
            needs_bond_invoice: false,
            needs_bond_payout: false,
            note: None,
            min_fiat: None,
            max_fiat: None,
            dispute_id: None,
            payment_failed_attempts: None,
            payment_failed_retries_interval: None,
            fiat_was_sent: false,
            is_bond_invoice: None,
            bond_slashed_at: None,
            bond_payout_deadline: None,
            cancel_initiator: None,
            parent_order_id: None,
            child_order_id: None,
            next_trade_pubkey: None,
            next_trade_index: None,
            daemon_pubkey: String::new(),
            expires_at: None,
        };
        let json = serde_json::to_string(&trade).unwrap();
        let parsed: Trade = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.order_id, trade.order_id);
        assert_eq!(parsed.kind, trade.kind);
        assert_eq!(parsed.status, trade.status);
    }

    #[test]
    fn test_trade_vec_serde_roundtrip() {
        let trades = vec![
            Trade {
                order_id: "1".to_string(),
                d_tag: "d1".to_string(),
                maker_pubkey: "mk".to_string(),
                my_trade_pubkey: None,
                counterparty_pubkey: None,
            solver_pubkey: None,
            last_request_id: None,
                role: TradeRole::Maker,
                kind: "buy".to_string(),
                fiat_amount: "50".to_string(),
                fiat_code: "USD".to_string(),
                sats_amount: None,
                premium: 0.0,
                payment_methods: vec![],
                status: TradeStatus::Pending,
                created_at: 1_700_000_000,
                updated_at: 1_700_000_000,
                trade_index: None,
            pending_hold_invoice: None,
            my_payout_invoice: None,
            needs_bond_invoice: false,
            needs_bond_payout: false,
            note: None,
            min_fiat: None,
            max_fiat: None,
            dispute_id: None,
            payment_failed_attempts: None,
            payment_failed_retries_interval: None,
            fiat_was_sent: false,
            is_bond_invoice: None,
            bond_slashed_at: None,
            bond_payout_deadline: None,
            cancel_initiator: None,
            parent_order_id: None,
            child_order_id: None,
            next_trade_pubkey: None,
            next_trade_index: None,
            daemon_pubkey: String::new(),
                expires_at: None,
            },
        ];
        let json = serde_json::to_string(&trades).unwrap();
        let parsed: Vec<Trade> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn test_trade_status_label_is_non_empty() {
        for s in [
            TradeStatus::Pending,
            TradeStatus::WaitingBuyerInvoice,
            TradeStatus::WaitingSellerToPay,
            TradeStatus::WaitingBond,
            TradeStatus::WaitingTakerBond,
            TradeStatus::WaitingMakerBond,
            TradeStatus::Active,
            TradeStatus::FiatSent,
            TradeStatus::Settled,
            TradeStatus::Success,
            TradeStatus::Canceled,
            TradeStatus::CancelPending,
            TradeStatus::CooperativelyCanceled,
            TradeStatus::CanceledByAdmin,
            TradeStatus::Expired,
            TradeStatus::Dispute,
            TradeStatus::PaymentFailed,
        ] {
            assert!(!s.label().is_empty());
        }
    }

    #[test]
    fn test_apply_status_updates_timestamp() {
        let trade = Trade::new_pending_at(
            1_700_000_000,
            "1".into(),
            "d".into(),
            "m".into(),
            TradeRole::Taker,
            "sell".into(),
            "100".into(),
            "EUR".into(),
            Some(1000),
            0.0,
            vec![],
            Some(0),
        );
        let updated = apply_status_at(&trade, TradeStatus::Active, 1_700_000_010);
        assert!(updated.updated_at >= trade.updated_at);
        assert_eq!(updated.status, TradeStatus::Active);
    }

    #[test]
    fn test_new_pending_at_smoke() {
        let trade = Trade::new_pending_at(
            1_700_000_000,
            "1".into(),
            "d".into(),
            "m".into(),
            TradeRole::Taker,
            "sell".into(),
            "100".into(),
            "EUR".into(),
            Some(1000),
            0.0,
            vec![],
            Some(0),
        );
        assert_eq!(trade.status, TradeStatus::Pending);
        assert_eq!(trade.created_at, 1_700_000_000);
        assert_eq!(trade.updated_at, 1_700_000_000);
    }
}
