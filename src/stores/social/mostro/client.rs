//! Mostro P2P exchange client (GiftWrap transport)
//!
//! This module wraps the `mostro-core` helpers for sending and receiving
//! trade messages over NIP-59 GiftWraps.
//!
//! Critical: do NOT use `client.unwrap_gift_wrap()` for Mostro messages.
//! Mostro uses an asymmetric wrap (identity key signs the seal, trade key
//! authors the rumor), which trips `nostr-sdk`'s `SenderMismatch` check.
//! Always use [`unwrap_mostro_response`].
//!
//! Reference: mostro-core `nip59::wrap_message` and `nip59::unwrap_message`.

use mostro_core::prelude::*;
use nostr::prelude::*;
use nostr_sdk::prelude::{Alphabet, Kind as NostrKind};
use std::collections::HashMap;

use crate::stores::publish_queue::{self, types::QueueEventType};
use crate::stores::nostr_client;

/// Send a Mostro `Message` to a daemon.
///
/// The message is wrapped with the given identity and trade keys, then
/// enqueued in the publish queue with `QueueEventType::DirectMessage` (since
/// the wire kind is 1059 GiftWrap).
///
/// In privacy mode, the caller passes the SAME `Keys` for both
/// `identity_keys` and `trade_keys` (per the protocol's full-privacy flow).
/// The `signed: true` default is preserved regardless of privacy mode
/// (the daemon always uses signed traffic).
#[allow(dead_code)]
pub async fn send_mostro_message(
    message: &Message,
    identity_keys: &nostr::Keys,
    trade_keys: &nostr::Keys,
    node_pubkey: PublicKey,
    node_relays: &[String],
    pow: u8,
) -> Result<(), String> {
    let opts = WrapOptions {
        pow,
        expiration: None,
        signed: true,
    };

    let event = mostro_core::nip59::wrap_message(
        message,
        identity_keys,
        trade_keys,
        node_pubkey,
        opts,
    )
    .await
    .map_err(|e| format!("mostro wrap failed: {e}"))?;

    publish_queue::enqueue(
        event,
        QueueEventType::DirectMessage,
        Some(node_relays.to_vec()),
        HashMap::new(),
    )
    .await;
    Ok(())
}

/// Try to unwrap a GiftWrap as a Mostro message addressed to `receiver_keys`.
///
/// Returns `Ok(None)` if the GiftWrap is not addressed to this key
/// (NIP-44 decrypt fails or the event is not a kind 1059). This is the
/// expected behavior when polling multiple candidate keys.
///
/// Returns `Err(_)` on structural problems (invalid JSON, signature mismatch,
/// unknown message variant, etc.) — these should be logged and skipped.
#[allow(dead_code)]
pub async fn unwrap_mostro_response(
    event: &Event,
    receiver_keys: &nostr::Keys,
) -> Result<Option<UnwrappedMessage>, String> {
    if event.kind != NostrKind::GiftWrap {
        return Ok(None);
    }
    mostro_core::nip59::unwrap_message(event, receiver_keys)
        .await
        .map_err(|e| format!("mostro unwrap failed: {e}"))
}

/// Build a filter that subscribes to all GiftWraps addressed to any of the
/// given active trade pubkeys.
///
/// IMPORTANT: do NOT use `.since(...)` for gift-wrap subscriptions — the
/// daemon randomizes gift-wrap `created_at` to defeat timing correlation, so
/// `since(now)` won't match new events. Use `.limit(0)` for "new only" or
/// `.since(...)` only on a one-shot `fetch_events` backfill.
#[allow(dead_code)]
pub fn active_trade_filter(trade_pubkeys: &[PublicKey]) -> Filter {
    Filter::new()
        .kind(NostrKind::GiftWrap)
        .custom_tags(
            SingleLetterTag::lowercase(Alphabet::P),
            trade_pubkeys.iter().map(|p| p.to_hex()),
        )
        .limit(0)
}

/// Build a filter for live updates to a specific order (kind 38383, NIP-33).
/// Safe to use `.since(...)` here — order events are not anonymized.
#[allow(dead_code)]
pub fn order_live_filter(maker_pubkey: PublicKey, d_tag: &str) -> Filter {
    Filter::new()
        .kind(NostrKind::Custom(38383))
        .author(maker_pubkey)
        .identifier(d_tag)
        .limit(0)
}

/// Build a filter for kind 38385 (Mostro node info) discovery.
#[allow(dead_code)]
pub fn node_info_filter() -> Filter {
    Filter::new()
        .kind(NostrKind::Custom(38385))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Z), "info")
        .custom_tag(SingleLetterTag::lowercase(Alphabet::Y), "mostro")
        .limit(0)
}

/// Fetch the daemon's current PoW requirement from kind 38385 events.
/// Returns `None` if the info event can't be fetched or has no `pow` tag.
#[allow(dead_code)]
pub async fn fetch_daemon_pow(node_pubkey: PublicKey, relays: &[String]) -> Option<u8> {
    let client = crate::stores::nostr_client::get_client()?;
    let filter = Filter::new()
        .author(node_pubkey)
        .kind(NostrKind::Custom(38385))
        .limit(1);
    let urls: Vec<nostr::Url> = relays.iter().filter_map(|u| nostr::Url::parse(u).ok()).collect();
    let events = client.fetch_events_from(&urls, filter, std::time::Duration::from_secs(5))
        .await
        .ok()?;
    let event = events.iter().max_by_key(|e| e.created_at)?;
    for tag in event.tags.iter() {
        if tag.kind() == nostr_sdk::prelude::TagKind::Custom(std::borrow::Cow::Borrowed("pow")) {
            if let Some(val) = tag.content() {
                return val.parse().ok();
            }
        }
    }
    Some(0)
}

/// Add the configured Mostro daemon relays to the nostr-sdk client pool
/// and connect them using the specialty relay pattern (with relay options,
/// connection verification, and bounded concurrency).
///
/// Delegates to [`crate::stores::relay::specialty::ensure_p2p_relays_connected`].
/// Must be called before `subscribe_to` or `fetch_events_from` with
/// node relay URLs, since those methods require relays to already be in the pool.
pub async fn ensure_node_relays_connected() {
    let client = match nostr_client::get_client() {
        Some(c) => c,
        None => return,
    };
    crate::stores::relay::specialty::ensure_p2p_relays_connected(&client).await;
}

/// Build a one-shot backfill filter for GiftWraps addressed to a trade pubkey.
/// Uses `.since()` + `.limit()` to fetch historical events that may have been
/// missed before the live subscription was active.
pub fn active_trade_backfill_filter(
    trade_pubkey: PublicKey,
    since: Timestamp,
) -> Filter {
    Filter::new()
        .kind(NostrKind::GiftWrap)
        .custom_tags(
            SingleLetterTag::lowercase(Alphabet::P),
            [trade_pubkey.to_hex()],
        )
        .since(since)
        .limit(200)
}

/// Build a map from trade pubkey → (trade_index, order_id) for all active trades.
/// Used for O(1) routing of incoming GiftWraps: read the outer `p` tag,
/// look up which trade it belongs to, then unwrap with the correct key.
pub fn build_trade_key_map() -> std::collections::HashMap<PublicKey, (u32, String)> {
    let keys_state = super::keys::try_get();
    let keys = match keys_state {
        Some(k) => k,
        None => return std::collections::HashMap::new(),
    };
    let mut map = std::collections::HashMap::new();
    for trade in super::trade_store::active_trades() {
        let idx = match trade.trade_index {
            Some(i) => i,
            None => continue,
        };
        if let Ok(tk) = keys.get_trade_key_by_index(idx) {
            map.insert(tk.public_key(), (idx, trade.order_id.clone()));
        }
    }
    map
}

/// Apply a Mostro daemon action to a trade, returning the new status if changed.
/// This is the shared action→status logic used by both the home page and
/// trade detail page subscriptions. Returns `None` if the action doesn't
/// change status (no-ops like Rate, CantDo, etc.).
///
/// `trade` is modified in place. Returns the new `TradeStatus` if the action
/// produced one, plus a human-readable toast message (if any).
#[allow(clippy::type_complexity)]
pub fn apply_mostro_action(
    trade: &mut super::trade_store::Trade,
    action: mostro_core::prelude::Action,
    payload: &Option<mostro_core::prelude::Payload>,
    sender: PublicKey,
    my_pk_hex: &str,
) -> (Option<super::trade_store::TradeStatus>, Option<(String, String)>) {
    use mostro_core::prelude::{Action as A, Payload as P};
    use super::trade_store::TradeStatus as S;

    let kind = match payload {
        Some(p) => p,
        None => &P::Amount(0),
    };

    let toast = None;
    let status = match action {
        A::AddBondInvoice => {
            if let P::BondPayoutRequest(bpr) = kind {
                trade.bond_slashed_at = Some(bpr.slashed_at);
                let window_days = super::node_config::try_get()
                    .map(|n| n.bond_payout_claim_window_days)
                    .unwrap_or(30);
                trade.bond_payout_deadline =
                    Some(bpr.slashed_at + (window_days as i64 * 86400));
            }
            trade.needs_bond_invoice = true;
            Some(S::WaitingBond)
        }
        A::AddInvoice | A::BuyerInvoiceAccepted => {
            if let P::PaymentRequest(_, bolt11, _) = kind {
                trade.pending_hold_invoice = Some(bolt11.clone());
            }
            Some(S::WaitingBuyerInvoice)
        }
        A::PayInvoice => {
            if let P::PaymentRequest(_, bolt11, _) = kind {
                trade.pending_hold_invoice = Some(bolt11.clone());
            }
            Some(S::WaitingSellerToPay)
        }
        A::PayBondInvoice => {
            if let P::PaymentRequest(_, bolt11, _) = kind {
                trade.pending_hold_invoice = Some(bolt11.clone());
            }
            trade.is_bond_invoice = Some(true);
            Some(S::WaitingTakerBond)
        }
        A::WaitingSellerToPay => Some(S::WaitingSellerToPay),
        A::WaitingBuyerInvoice => Some(S::WaitingBuyerInvoice),
        A::HoldInvoicePaymentAccepted => {
            if let P::Order(order) = kind {
                if trade.counterparty_pubkey.is_none() {
                    let candidates = [
                        order.buyer_trade_pubkey.as_deref(),
                        order.seller_trade_pubkey.as_deref(),
                    ];
                    for pk in candidates.iter().flatten() {
                        if my_pk_hex != *pk && !pk.is_empty() {
                            trade.counterparty_pubkey = Some(pk.to_string());
                            break;
                        }
                    }
                }
            }
            Some(S::Active)
        }
        A::BuyerTookOrder => {
            if let P::Order(order) = kind {
                if let Some(buyer_pk) = &order.buyer_trade_pubkey {
                    if trade.counterparty_pubkey.is_none() {
                        trade.counterparty_pubkey = Some(buyer_pk.clone());
                    }
                }
            }
            Some(S::Active)
        }
        A::FiatSentOk => {
            if let P::Peer(peer) = kind {
                trade.counterparty_pubkey = Some(peer.pubkey.clone());
            }
            trade.fiat_was_sent = true;
            Some(S::FiatSent)
        }
        A::HoldInvoicePaymentSettled => Some(S::Settled),
        A::Released | A::PurchaseCompleted => Some(S::Success),
        A::Canceled | A::HoldInvoicePaymentCanceled => Some(S::Canceled),
        A::CooperativeCancelInitiatedByYou | A::CooperativeCancelInitiatedByPeer => {
            Some(S::CancelPending)
        }
        A::CooperativeCancelAccepted => Some(S::CooperativelyCanceled),
        A::DisputeInitiatedByYou | A::DisputeInitiatedByPeer => {
            if let P::Dispute(dispute_id, _) = kind {
                trade.dispute_id = Some(dispute_id.to_string());
            }
            Some(S::Dispute)
        }
        A::AdminTakeDispute | A::AdminTookDispute => {
            trade.solver_pubkey = Some(sender.to_hex());
            Some(S::Dispute)
        }
        A::AdminCanceled => Some(S::CanceledByAdmin),
        A::AdminSettled => Some(S::Success),
        A::PaymentFailed => {
            if let P::PaymentFailed(info) = kind {
                trade.payment_failed_attempts = Some(info.payment_attempts);
                trade.payment_failed_retries_interval = Some(info.payment_retries_interval);
            }
            Some(S::PaymentFailed)
        }
        A::BondInvoiceAccepted => {
            trade.needs_bond_invoice = false;
            None
        }
        A::InvoiceUpdated => {
            if let P::PaymentRequest(_, bolt11, _) = kind {
                trade.pending_hold_invoice = Some(bolt11.clone());
            }
            None
        }
        A::NewOrder => {
            if let P::Order(order) = kind {
                if let Some(real_id) = order.id {
                    trade.order_id = real_id.to_string();
                }
            }
            None
        }
        A::CantDo => None,
        A::Rate | A::RateReceived => None,
        A::BondSlashed | A::BondPayoutCompleted | A::TradePubkey => None,
        _ => None,
    };

    (status, toast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_info_filter_shape() {
        let f = node_info_filter();
        assert!(f.kinds.is_some());
    }

    #[test]
    fn test_order_live_filter_shape() {
        let pk = PublicKey::from_hex(
            "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390",
        )
        .unwrap();
        let f = order_live_filter(pk, "test-d-tag");
        assert!(f.kinds.is_some());
    }
}

#[allow(dead_code)]
pub async fn check_relay_health(relays: &[String]) -> (Vec<String>, Vec<String>) {
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return (vec![], relays.to_vec()),
    };
    let mut healthy = Vec::new();
    let mut unhealthy = Vec::new();
    for relay_url in relays {
        let url = relay_url.clone();
        match client.add_relay(&url).await {
            Ok(_) => {}
            Err(_) => {
                unhealthy.push(url);
                continue;
            }
        }
        match client.connect_relay(&url).await {
            Ok(()) => {
                let connected = client
                    .fetch_events(
                        Filter::new().limit(0),
                        std::time::Duration::from_secs(5),
                    )
                    .await;
                match connected {
                    Ok(_) => healthy.push(url),
                    Err(_) => {
                        unhealthy.push(url);
                    }
                }
            }
            Err(_) => {
                unhealthy.push(url);
            }
        }
    }
    (healthy, unhealthy)
}
