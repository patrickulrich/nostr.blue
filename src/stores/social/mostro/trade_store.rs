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
use nostr_sdk::{Event as NostrEvent, EventBuilder, Tag};
use serde::{Deserialize, Serialize};
use std::result::Result;
use std::time::Duration;

use crate::platform::storage;
use crate::stores::auth_store;
use crate::stores::nostr_client;
use crate::stores::publish_queue::{self, types::QueueEventType};

/// NIP-78 d-tag for the trades list event.
pub const TRADES_D_TAG: &str = "nostr.blue/p2p/trades";

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
    fn progress_rank(&self) -> u8 {
        match self {
            TradeStatus::Pending => 0,
            TradeStatus::WaitingBond | TradeStatus::WaitingTakerBond => 0,
            TradeStatus::WaitingBuyerInvoice | TradeStatus::WaitingSellerToPay => 1,
            TradeStatus::PaymentFailed => 1,
            TradeStatus::Active | TradeStatus::Dispute | TradeStatus::CancelPending => 2,
            TradeStatus::FiatSent => 3,
            TradeStatus::Settled => 4,
            TradeStatus::Success => 5,
            TradeStatus::Canceled
            | TradeStatus::Expired
            | TradeStatus::CooperativelyCanceled
            | TradeStatus::CanceledByAdmin => 6,
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

    /// Determine the next status given a daemon action and the user's role.
    /// Returns `None` for actions that don't change status (no-ops, echoes).
    #[allow(dead_code)]
    pub fn from_action(
        action: mostro_core::prelude::Action,
        _role: TradeRole,
    ) -> Option<TradeStatus> {
        use mostro_core::prelude::Action;
        match action {
            Action::NewOrder => None,
            Action::TakeSell | Action::TakeBuy => None,
            Action::PayInvoice => Some(TradeStatus::WaitingSellerToPay),
            Action::PayBondInvoice => Some(TradeStatus::WaitingTakerBond),
            Action::AddInvoice | Action::BuyerInvoiceAccepted => {
                Some(TradeStatus::WaitingBuyerInvoice)
            }
            Action::AddBondInvoice => Some(TradeStatus::WaitingBond),
            Action::WaitingSellerToPay => Some(TradeStatus::WaitingSellerToPay),
            Action::WaitingBuyerInvoice => Some(TradeStatus::WaitingBuyerInvoice),
            Action::HoldInvoicePaymentAccepted => Some(TradeStatus::Active),
            Action::BuyerTookOrder => Some(TradeStatus::Active),
            Action::FiatSent | Action::FiatSentOk => Some(TradeStatus::FiatSent),
            Action::Release | Action::Released => Some(TradeStatus::Settled),
            Action::HoldInvoicePaymentSettled => Some(TradeStatus::Settled),
            Action::Rate | Action::RateReceived => None,
            Action::Cancel | Action::CooperativeCancelInitiatedByYou
            | Action::CooperativeCancelInitiatedByPeer => Some(TradeStatus::CancelPending),
            Action::CooperativeCancelAccepted => Some(TradeStatus::CooperativelyCanceled),
            Action::Canceled => Some(TradeStatus::Canceled),
            Action::HoldInvoicePaymentCanceled => Some(TradeStatus::Canceled),
            Action::Dispute | Action::DisputeInitiatedByYou
            | Action::DisputeInitiatedByPeer
            | Action::AdminTakeDispute | Action::AdminTookDispute => {
                Some(TradeStatus::Dispute)
            }
            Action::AdminCanceled => Some(TradeStatus::CanceledByAdmin),
            Action::AdminSettled => Some(TradeStatus::Success),
            Action::PaymentFailed => Some(TradeStatus::PaymentFailed),
            Action::BondSlashed => None,
            Action::BondInvoiceAccepted => None,
            Action::BondPayoutCompleted => None,
            Action::PurchaseCompleted => Some(TradeStatus::Success),
            Action::InvoiceUpdated => None,
            Action::CantDo => None,
            Action::RestoreSession | Action::LastTradeIndex => None,
            Action::RateUser => None,
            Action::SendDm => None,
            _ => None,
        }
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
    /// Maker's trade pubkey (used to derive the SharedKey for chat).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maker_trade_pubkey: Option<String>,
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
            maker_trade_pubkey: None,
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
            parent_order_id: None,
            child_order_id: None,
            next_trade_pubkey: None,
            next_trade_index: None,
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
            maker_trade_pubkey: None,
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
            parent_order_id: None,
            child_order_id: None,
            next_trade_pubkey: None,
            next_trade_index: None,
        }
    }
}

/// Bump the `updated_at` and set a new status. Returns the mutated clone.
///
/// Enforces monotonicity: terminal statuses block all transitions, and
/// backward progress-rank changes are silently ignored (returns the
/// trade unchanged). The `Dispute`, `CancelPending`, and `PaymentFailed`
/// statuses are exceptions — they can be entered from any non-terminal
/// rank because they represent external events that override normal progression.
#[allow(dead_code)]
pub fn apply_status(trade: &Trade, new_status: TradeStatus) -> Trade {
    if trade.status.is_terminal() {
        return trade.clone();
    }
    if new_status.progress_rank() < trade.status.progress_rank()
        && !matches!(
            new_status,
            TradeStatus::Dispute
                | TradeStatus::CancelPending
                | TradeStatus::PaymentFailed
                | TradeStatus::CooperativelyCanceled
                | TradeStatus::CanceledByAdmin
        )
    {
        return trade.clone();
    }
    let mut t = trade.clone();
    t.status = new_status;
    t.updated_at = crate::platform::timestamp::now_secs() as i64;
    t
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

/// Add or update a trade. Upserts by `order_id`, falling back to
/// `trade_index` or `maker_trade_pubkey` for placeholder-to-UUID migration.
///
/// The change is reflected in the global signal and the local cache
/// immediately. The NIP-78 publish happens separately via [`publish`].
#[allow(dead_code)]
pub fn upsert(trade: Trade) {
    let mut list = TRADES.write();
    if let Some(existing) = list.iter_mut().find(|t| {
        t.order_id == trade.order_id
            || (trade.trade_index.is_some() && t.trade_index == trade.trade_index)
            || (trade.maker_trade_pubkey.is_some()
                && trade.maker_trade_pubkey == t.maker_trade_pubkey)
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

/// Verify a NIP-78 event is a valid trades record owned by the user.
fn evaluate_event(event: &NostrEvent, user_pubkey: &PublicKey) -> Option<Vec<Trade>> {
    if event.pubkey != *user_pubkey {
        return None;
    }
    if event.verify().is_err() {
        return None;
    }
    let parsed: Vec<Trade> = serde_json::from_str(&event.content).ok()?;
    Some(parsed)
}

/// Refresh from relays (best-effort). On failure, leaves the existing
/// global state alone (so the user can still see cached data offline).
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
            if let Some(list) = fresh {
                *TRADES.write() = list.clone();
                write_cache(&list)?;
                Ok(list.len())
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

/// Publish the current trade list to the user's write relays as a
/// NIP-78 (kind 30078) event with d-tag `nostr.blue/p2p/trades`.
#[allow(dead_code)]
pub async fn publish() -> Result<(), String> {
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    let list = TRADES.read().clone();
    let content = serde_json::to_string(&list)
        .map_err(|e| format!("Failed to serialize trades: {e}"))?;

    let builder = EventBuilder::new(Kind::from(30078), content).tag(Tag::identifier(TRADES_D_TAG));
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

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn default_test_trade(status: TradeStatus) -> Trade {
        Trade {
            order_id: "test-order-123".to_string(),
            d_tag: "test-order-123".to_string(),
            maker_pubkey: "maker".to_string(),
            maker_trade_pubkey: None,
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
            parent_order_id: None,
            child_order_id: None,
            next_trade_pubkey: None,
            next_trade_index: None,
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
            maker_trade_pubkey: None,
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
            parent_order_id: None,
            child_order_id: None,
            next_trade_pubkey: None,
            next_trade_index: None,
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
                maker_trade_pubkey: None,
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
                parent_order_id: None,
                child_order_id: None,
                next_trade_pubkey: None,
                next_trade_index: None,
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
