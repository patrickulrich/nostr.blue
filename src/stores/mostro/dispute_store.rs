//! Mostro dispute events (NIP-33 kind 38386).
//!
//! Disputes are published by the daemon as parameterized replaceable
//! events. Tags:
//! - `d` = dispute_id (UUID)
//! - `s` = dispute status (kebab-case: initiated, in-progress, seller-refunded,
//!   settled, released)
//! - `initiator` = "buyer" or "seller"
//! - `y` = platform tag (e.g. ["mostro", optional_instance_name])
//! - `z` = "dispute"
//!
//! Content is empty — all data is in tags.

use dioxus::prelude::*;
use nostr::prelude::*;
use nostr_sdk::Event;
use serde::{Deserialize, Serialize};

/// NIP-33 dispute event kind (matches `mostro_core::prelude::NOSTR_DISPUTE_EVENT_KIND`).
pub const DISPUTE_EVENT_KIND: u16 = 38386;

/// Lifecycle status of a dispute as published by the daemon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisputeStatus {
    Initiated,
    InProgress,
    SellerRefunded,
    Settled,
    Released,
}

impl DisputeStatus {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "initiated" => Some(Self::Initiated),
            "in-progress" => Some(Self::InProgress),
            "seller-refunded" => Some(Self::SellerRefunded),
            "settled" => Some(Self::Settled),
            "released" => Some(Self::Released),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Initiated => "Initiated",
            Self::InProgress => "In Progress",
            Self::SellerRefunded => "Seller Refunded",
            Self::Settled => "Settled",
            Self::Released => "Released",
        }
    }
}

/// Who initiated the dispute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DisputeInitiator {
    Buyer,
    Seller,
    Unknown,
}

impl DisputeInitiator {
    pub fn from_str(s: &str) -> Self {
        match s {
            "buyer" => Self::Buyer,
            "seller" => Self::Seller,
            _ => Self::Unknown,
        }
    }
}

/// A parsed dispute record from a kind 38386 event.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Dispute {
    pub dispute_id: String,
    pub status: DisputeStatus,
    pub initiator: DisputeInitiator,
    pub daemon_pubkey: String,
    pub created_at: i64,
}

/// Global reactive list of disputes.
pub static DISPUTES: GlobalSignal<Vec<Dispute>> = Signal::global(Vec::new);

/// Parse a kind 38386 event into a `Dispute` record.
/// Returns `None` if the event is not a valid dispute event.
pub fn parse_dispute_event(event: &Event) -> Option<Dispute> {
    if event.kind.as_u16() != DISPUTE_EVENT_KIND {
        return None;
    }

    let dispute_id = event.tags.identifier()?.to_string();

    let status = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("s")))
        .and_then(|t| t.content())
        .and_then(DisputeStatus::from_str)?;

    let initiator = event
        .tags
        .iter()
        .find(|t| t.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("initiator")))
        .and_then(|t| t.content())
        .map(DisputeInitiator::from_str)
        .unwrap_or(DisputeInitiator::Unknown);

    Some(Dispute {
        dispute_id,
        status,
        initiator,
        daemon_pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs() as i64,
    })
}

/// Add or update a dispute in the global list (upsert by `dispute_id`).
pub fn upsert(dispute: Dispute) {
    let mut list = DISPUTES.write();
    if let Some(existing) = list.iter_mut().find(|d| d.dispute_id == dispute.dispute_id) {
        *existing = dispute;
    } else {
        list.push(dispute);
    }
}

/// Look up a dispute by its ID.
#[allow(dead_code)]
pub fn find_by_id(id: &str) -> Option<Dispute> {
    DISPUTES.read().iter().find(|d| d.dispute_id == id).cloned()
}

/// Filter disputes for a specific daemon pubkey.
pub fn filter_for_daemon(daemon_pk: &str) -> Vec<Dispute> {
    DISPUTES
        .read()
        .iter()
        .filter(|d| d.daemon_pubkey == daemon_pk)
        .cloned()
        .collect()
}

/// Clear all disputes (used on logout/daemon switch).
#[allow(dead_code)]
pub fn clear_all() {
    *DISPUTES.write() = Vec::new();
}

/// C2: defense-in-depth dispute auto-close on trade release.
///
/// When the seller releases funds during an open dispute, the daemon
/// auto-closes the dispute to `Settled` and republishes the kind 38386
/// event (see `mostro/src/app/dispute.rs:252-333` and
/// `docs/DISPUTE_AUTO_CLOSE_ON_USER_RESOLUTION.md`). The dispute monitor
/// at `client.rs:1699-1751` picks up the republished event eventually,
/// but there's a latency window between the GiftWrap-channel `Released`
/// arriving and the kind 38386 republish.
///
/// This helper lets `apply_mostro_action` proactively advance the dispute
/// store when it sees a `Released` action on a trade that has an open
/// dispute — closing the gap.
#[allow(dead_code)]
pub fn mark_auto_closed_by_release(dispute_id: &str) {
    let mut list = DISPUTES.write();
    let mut changed = false;
    if let Some(d) = list.iter_mut().find(|d| d.dispute_id == dispute_id) {
        if matches!(d.status, DisputeStatus::Initiated | DisputeStatus::InProgress) {
            d.status = DisputeStatus::Settled;
            changed = true;
        }
    }
    drop(list);
    if changed {
        log::debug!(
            "dispute {dispute_id} auto-advanced to Settled via seller release (C2 defense-in-depth)"
        );
    }
}

/// C2: defense-in-depth dispute auto-close on cooperative cancel.
///
/// Mirrors `mark_auto_closed_by_release` but for the cancel path — the
/// daemon auto-closes to `SellerRefunded` when a cooperative cancel
/// completes during a dispute.
#[allow(dead_code)]
pub fn mark_auto_closed_by_cancel(dispute_id: &str) {
    let mut list = DISPUTES.write();
    let mut changed = false;
    if let Some(d) = list.iter_mut().find(|d| d.dispute_id == dispute_id) {
        if matches!(d.status, DisputeStatus::Initiated | DisputeStatus::InProgress) {
            d.status = DisputeStatus::SellerRefunded;
            changed = true;
        }
    }
    drop(list);
    if changed {
        log::debug!(
            "dispute {dispute_id} auto-advanced to SellerRefunded via cooperative cancel (C2 defense-in-depth)"
        );
    }
}
