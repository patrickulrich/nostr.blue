//! Mostro protocol flow message builders
//!
//! These functions construct the outbound `Message` values the client sends
//! to a Mostro daemon to progress through a trade's state machine. They do
//! NOT sign or publish — that is done by [`super::client::send_mostro_message`].
//!
//! Each builder applies the project's privacy-mode convention internally
//! (via [`maybe_trade_index`]) so callers never accidentally leak the
//! monotonic trade counter when privacy mode is on.
//!
//! Reference: the daemon source at `/home/patrick/mostro/src/` and the
//! `mostro-core` API surface at `/home/patrick/.cargo/registry/src/.../mostro-core-0.11.5/`.

use mostro_core::prelude::*;
use nostr::prelude::*;
use std::str::FromStr;
use uuid::Uuid;

use super::keys::MostroKeys;

/// Trade index helper.
///
/// In privacy mode the user sends `trade_index: None` on the wire. The
/// daemon treats a missing trade_index as 0 for the monotonic-sequence
/// check (`take_sell.rs:171`), which is fine because identity == trade
/// key in privacy mode and there's no per-trade reputation to preserve.
///
/// In normal mode the caller passes the `trade_index` they used to
/// derive the `trade_keys` for this trade.
#[allow(dead_code)]
pub fn maybe_trade_index(keys: &MostroKeys, trade_index: u32) -> Option<i64> {
    if keys.privacy_mode {
        None
    } else {
        Some(trade_index as i64)
    }
}

/// Validate a BOLT11 invoice string or Lightning Address.
///
/// Accepts:
/// - `lnbc...` / `lntb...` BOLT11 invoice strings (parsed with `lightning-invoice`)
/// - `user@domain` Lightning Address patterns
///
/// Returns `Ok(())` if valid, `Err(reason)` otherwise.
#[allow(dead_code)]
pub fn validate_invoice(input: &str) -> std::result::Result<(), String> {
    validate_invoice_with_amount(input, None)
}

#[allow(dead_code)]
pub fn validate_invoice_with_amount(
    input: &str,
    expected_sats: Option<u64>,
) -> std::result::Result<(), String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Invoice is empty".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("lnbc") || lower.starts_with("lntb") {
        match lightning_invoice::Bolt11Invoice::from_str(trimmed) {
            Ok(inv) => {
                if let Some(expected) = expected_sats {
                    if let Some(msats) = inv.amount_milli_satoshis() {
                        let invoice_sats = msats.div_ceil(1000);
                        if invoice_sats != expected && invoice_sats != 0 {
                            return Err(format!(
                                "Invoice amount ({invoice_sats} sats) does not match expected ({expected} sats)"
                            ));
                        }
                    }
                }
                Ok(())
            }
            Err(e) => Err(format!("Invalid BOLT11 invoice: {e}")),
        }
    } else if trimmed.contains('@') && trimmed.contains('.') {
        let parts: Vec<&str> = trimmed.split('@').collect();
        if parts.len() == 2 && !parts[0].is_empty() && parts[1].contains('.') {
            Ok(())
        } else {
            Err("Invalid Lightning Address format".to_string())
        }
    } else {
        Err("Expected a BOLT11 invoice (lnbc...) or Lightning Address (user@domain)".to_string())
    }
}

/// Build a request id counter (monotonic per session, just a u64).
#[allow(dead_code)]
pub fn next_request_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Take an existing `sell` order.
///
/// Per the daemon (`take_sell.rs`):
/// - No payload: pure "I'll take whatever amount" — daemon will reply
///   with `AddInvoice(SmallOrder)` (no-bond) or `PayBondInvoice` first
///   (bond case).
/// - `PaymentRequest(None, bolt11, Some(amount))`: "I'll take this much
///   and here's my payout invoice" — daemon will reply with
///   `WaitingSellerToPay` (or `PayBondInvoice` first).
#[allow(dead_code)]
pub fn take_sell(
    keys: &MostroKeys,
    order_id: Uuid,
    trade_index: u32,
    buyer_invoice: Option<(String, i64)>,
) -> Message {
    let payload = buyer_invoice.map(|(bolt11, amt)| {
        Payload::PaymentRequest(None, bolt11, Some(amt))
    });
    Message::new_order(
        Some(order_id),
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::TakeSell,
        payload,
    )
}

/// Take an existing `buy` order.
///
/// Per the daemon (`take_buy.rs`): payload is usually `None` (we want
/// the full amount). The daemon replies with `PayInvoice(SmallOrder, bolt11)`
/// to the taker (we are now the seller).
#[allow(dead_code)]
pub fn take_buy(
    keys: &MostroKeys,
    order_id: Uuid,
    trade_index: u32,
    amount_override: Option<i64>,
) -> Message {
    let payload = amount_override.map(Payload::Amount);
    Message::new_order(
        Some(order_id),
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::TakeBuy,
        payload,
    )
}

/// Buyer sends their payout invoice.
///
/// Sent in response to `AddInvoice(SmallOrder)` from the daemon.
/// Per `add_invoice.rs` the payload is `PaymentRequest(None, bolt11, None)`.
#[allow(dead_code)]
pub fn add_invoice(
    keys: &MostroKeys,
    order_id: Uuid,
    trade_index: u32,
    bolt11: String,
) -> Message {
    Message::new_order(
        Some(order_id),
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::AddInvoice,
        Some(Payload::PaymentRequest(None, bolt11, None)),
    )
}

/// User provides a bond invoice in response to `Action::AddBondInvoice` from
/// the daemon. Payload is `PaymentRequest(None, bolt11, None)`.
#[allow(dead_code)]
pub fn add_bond_invoice(
    keys: &MostroKeys,
    order_id: Uuid,
    trade_index: u32,
    bolt11: String,
) -> Message {
    Message::new_order(
        Some(order_id),
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::AddBondInvoice,
        Some(Payload::PaymentRequest(None, bolt11, None)),
    )
}

/// Buyer notifies the daemon that fiat has been sent off-chain.
///
/// Per the daemon this is `Action::FiatSent` with no payload. After this,
/// the daemon sends `FiatSentOk(Peer)` to the seller, disclosing the
/// counterparty's trade pubkey — which is what unlocks chat.
#[allow(dead_code)]
pub fn fiat_sent(
    keys: &MostroKeys,
    order_id: Uuid,
    trade_index: u32,
    next_trade: Option<(String, u32)>,
) -> Message {
    let payload = next_trade.map(|(pk, idx)| Payload::NextTrade(pk, idx));
    Message::new_order(
        Some(order_id),
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::FiatSent,
        payload,
    )
}

/// Seller releases the hold invoice funds.
///
/// Per the daemon: no payload. After this, the daemon settles the hold
/// invoice and pays the buyer's invoice. Both sides receive `Rate` next.
#[allow(dead_code)]
pub fn release(
    keys: &MostroKeys,
    order_id: Uuid,
    trade_index: u32,
    next_trade: Option<(String, u32)>,
) -> Message {
    let payload = next_trade.map(|(pk, idx)| Payload::NextTrade(pk, idx));
    Message::new_order(
        Some(order_id),
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::Release,
        payload,
    )
}

/// Cancel a trade. Works at any state, but the daemon's behavior varies:
///
/// - For `Pending` / `WaitingPayment` / `WaitingBuyerInvoice` /
///   `WaitingTakerBond`: unilateral cancel is accepted. Daemon sends
///   `Canceled` to the user.
/// - For `Active` / `FiatSent` / `Dispute` / `SettledHoldInvoice`:
///   cooperative cancel. Daemon sends `CooperativeCancelInitiatedByYou`
///   to the user, and waits for the counterparty's `Action::Cancel`.
///   When both sides have agreed, both receive `Action::Canceled`.
#[allow(dead_code)]
pub fn cancel(keys: &MostroKeys, order_id: Uuid, trade_index: u32) -> Message {
    Message::new_order(
        Some(order_id),
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::Cancel,
        None,
    )
}

/// Accept a counterparty's cooperative cancel request.
///
/// Sent in response to `CooperativeCancelInitiatedByPeer`. Daemon replies
/// with `Action::Canceled` to both parties on receipt.
#[allow(dead_code)]
pub fn accept_cancel(keys: &MostroKeys, order_id: Uuid, trade_index: u32) -> Message {
    Message::new_order(
        Some(order_id),
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::CooperativeCancelAccepted,
        None,
    )
}

/// Open a dispute on an active trade.
///
/// Per the daemon (`dispute.rs`): preconditions are status `Active` or
/// `FiatSent`, and the sender must be either the buyer or seller on the
/// order. The daemon replies with `DisputeInitiatedByYou` to the sender
/// and `DisputeInitiatedByPeer` to the counterparty, each with
/// `Payload::Dispute(dispute_id, None)`.
#[allow(dead_code)]
pub fn dispute(keys: &MostroKeys, order_id: Uuid, trade_index: u32) -> Message {
    Message::new_order(
        Some(order_id),
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::Dispute,
        None,
    )
}

/// Submit a rating for a completed trade (1..=5).
#[allow(dead_code)]
pub fn rate_user(
    keys: &MostroKeys,
    order_id: Uuid,
    trade_index: u32,
    rating: u8,
) -> Result<Message, String> {
    if !(1..=5).contains(&rating) {
        return Err(format!("rating {rating} out of range (1..=5)"));
    }
    Ok(Message::new_order(
        Some(order_id),
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::RateUser,
        Some(Payload::RatingUser(rating)),
    ))
}

/// Maker publishes a new order.
///
/// Per the daemon (`new_order.rs`): payload is `Order(SmallOrder)`. The
/// daemon assigns the final order id and acknowledges with `NewOrder`
/// (with the same payload, but the order now has its canonical id). On
/// a taker, the maker receives `BuyerTookOrder(SmallOrder)`.
#[allow(dead_code)]
pub fn new_order(
    keys: &MostroKeys,
    order: SmallOrder,
    trade_index: u32,
) -> Message {
    Message::new_order(
        order.id,
        Some(next_request_id()),
        maybe_trade_index(keys, trade_index),
        Action::NewOrder,
        Some(Payload::Order(order)),
    )
}

/// Ask the daemon to restore the user's session state.
///
/// Per the daemon (`restore_session.rs`): payload is `None`. Reply is
/// `Action::RestoreData(RestoreSessionInfo)`.
#[allow(dead_code)]
pub fn restore_session() -> Message {
    Message::new_restore(None)
}

/// Ask the daemon for the last trade index it has seen for this user.
///
/// Built inline because `Message::new_last_trade_index` does not exist
/// in `mostro-core 0.11.5`. The daemon ignores the payload and replies
/// with `Message::Restore(MessageKind { action: LastTradeIndex, .. })`.
#[allow(dead_code)]
pub fn last_trade_index() -> Message {
    Message::Restore(MessageKind::new(
        None,
        Some(next_request_id()),
        None,
        Action::LastTradeIndex,
        None,
    ))
}

/// Request full `SmallOrder` details for a list of order IDs.
///
/// The daemon replies with `Action::Orders(Payload::Orders(Vec<SmallOrder>))`
/// where each `SmallOrder` includes `buyer_trade_pubkey` and
/// `seller_trade_pubkey` for role derivation during restore Stage 2.
#[allow(dead_code)]
pub fn request_orders(order_ids: Vec<Uuid>) -> Message {
    Message::new_order(
        None,
        Some(next_request_id()),
        None,
        Action::Orders,
        Some(Payload::Ids(order_ids)),
    )
}

/// Send a free-form text chat message to the counterparty.
///
/// Per `send_dm.rs`: action is `SendDm`, payload is `TextMessage(body)`.
/// The wrapping itself uses [`mostro_core::chat::wrap_chat_message`]
/// (NIP-44 to a shared key), not the protocol `wrap_message`. The
/// caller must construct the `SharedKey` via
/// [`SharedKey::derive`](mostro_core::prelude::SharedKey::derive) and
/// pass the `trade_keys` used to derive it.
#[allow(dead_code)]
pub fn send_dm(body: String) -> Message {
    Message::new_dm(
        None,
        Some(next_request_id()),
        Action::SendDm,
        Some(Payload::TextMessage(body)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::nip06;

    fn test_keys() -> MostroKeys {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let id = nip06::derive_at(words, None, 38383, 0, 0).unwrap();
        MostroKeys {
            mnemonic: words.to_string(),
            trade_index: 0,
            identity_keys: id,
            privacy_mode: false,
        }
    }

    #[test]
    fn test_maybe_trade_index_normal_mode() {
        let k = test_keys();
        assert_eq!(maybe_trade_index(&k, 5), Some(5));
    }

    #[test]
    fn test_maybe_trade_index_privacy_mode_strips_value() {
        let mut k = test_keys();
        k.privacy_mode = true;
        assert_eq!(maybe_trade_index(&k, 5), None);
    }

    #[test]
    fn test_take_sell_no_payload() {
        let k = test_keys();
        let oid = Uuid::new_v4();
        let m = take_sell(&k, oid, 0, None);
        match m {
            Message::Order(kind) => {
                assert_eq!(kind.action, Action::TakeSell);
                assert!(kind.payload.is_none());
                assert_eq!(kind.id, Some(oid));
                assert_eq!(kind.trade_index, Some(0));
            }
            _ => panic!("expected Order variant"),
        }
    }

    #[test]
    fn test_take_sell_with_buyer_invoice() {
        let k = test_keys();
        let oid = Uuid::new_v4();
        let m = take_sell(&k, oid, 1, Some(("lnbc...".to_string(), 100_000)));
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::TakeSell);
        match kind.payload.as_ref().unwrap() {
            Payload::PaymentRequest(_, bolt11, amt) => {
                assert_eq!(bolt11, "lnbc...");
                assert_eq!(*amt, Some(100_000));
            }
            _ => panic!("expected PaymentRequest"),
        }
    }


    #[test]
    fn test_take_sell_privacy_mode_hides_index() {
        let mut k = test_keys();
        k.privacy_mode = true;
        let m = take_sell(&k, Uuid::new_v4(), 7, None);
        let kind = m.get_inner_message_kind();
        assert!(kind.trade_index.is_none());
    }

    #[test]
    fn test_take_buy_default_no_payload() {
        let k = test_keys();
        let m = take_buy(&k, Uuid::new_v4(), 0, None);
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::TakeBuy);
        assert!(kind.payload.is_none());
    }

    #[test]
    fn test_add_invoice_carries_bolt11() {
        let k = test_keys();
        let m = add_invoice(&k, Uuid::new_v4(), 0, "lnbc100...".to_string());
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::AddInvoice);
        match kind.payload.as_ref().unwrap() {
            Payload::PaymentRequest(_, b, a) => {
                assert_eq!(b, "lnbc100...");
                assert!(a.is_none());
            }
            _ => panic!("expected PaymentRequest"),
        }
    }

    #[test]
    fn test_add_bond_invoice_carries_bolt11() {
        let k = test_keys();
        let m = add_bond_invoice(&k, Uuid::new_v4(), 0, "lnbc50...".to_string());
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::AddBondInvoice);
        match kind.payload.as_ref().unwrap() {
            Payload::PaymentRequest(_, b, a) => {
                assert_eq!(b, "lnbc50...");
                assert!(a.is_none());
            }
            _ => panic!("expected PaymentRequest"),
        }
    }

    #[test]
    fn test_fiat_sent_no_payload() {
        let k = test_keys();
        let m = fiat_sent(&k, Uuid::new_v4(), 0, None);
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::FiatSent);
        assert!(kind.payload.is_none());
    }

    #[test]
    fn test_release_no_payload() {
        let k = test_keys();
        let m = release(&k, Uuid::new_v4(), 0, None);
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::Release);
        assert!(kind.payload.is_none());
    }

    #[test]
    fn test_release_with_next_trade() {
        let k = test_keys();
        let next_pk = "02abcdef".to_string();
        let m = release(&k, Uuid::new_v4(), 0, Some((next_pk.clone(), 5)));
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::Release);
        match kind.payload.as_ref().unwrap() {
            Payload::NextTrade(pk, idx) => {
                assert_eq!(pk, &next_pk);
                assert_eq!(*idx, 5);
            }
            _ => panic!("expected NextTrade"),
        }
    }

    #[test]
    fn test_cancel_no_payload() {
        let k = test_keys();
        let m = cancel(&k, Uuid::new_v4(), 0);
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::Cancel);
        assert!(kind.payload.is_none());
    }

    #[test]
    fn test_accept_cancel() {
        let k = test_keys();
        let m = accept_cancel(&k, Uuid::new_v4(), 0);
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::CooperativeCancelAccepted);
    }

    #[test]
    fn test_dispute_no_payload() {
        let k = test_keys();
        let m = dispute(&k, Uuid::new_v4(), 0);
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::Dispute);
        assert!(kind.payload.is_none());
    }

    #[test]
    fn test_rate_user_valid_range() {
        let k = test_keys();
        for r in 1..=5u8 {
            let m = rate_user(&k, Uuid::new_v4(), 0, r).unwrap();
            let kind = m.get_inner_message_kind();
            assert_eq!(kind.action, Action::RateUser);
            match kind.payload.as_ref().unwrap() {
                Payload::RatingUser(v) => assert_eq!(*v, r),
                _ => panic!("expected RatingUser"),
            }
        }
    }

    #[test]
    fn test_rate_user_rejects_out_of_range() {
        let k = test_keys();
        assert!(rate_user(&k, Uuid::new_v4(), 0, 0).is_err());
        assert!(rate_user(&k, Uuid::new_v4(), 0, 6).is_err());
    }

    #[test]
    fn test_new_order_carries_small_order() {
        let k = test_keys();
        let order = SmallOrder::new(
            None,
            Some(mostro_core::order::Kind::Sell),
            Some(Status::Pending),
            100_000,
            "eur".to_string(),
            None,
            None,
            100,
            "SEPA".to_string(),
            1,
            None,
            None,
            None,
            None,
            None,
        );
        let m = new_order(&k, order.clone(), 0);
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::NewOrder);
        assert!(matches!(kind.payload, Some(Payload::Order(_))));
    }

    #[test]
    fn test_restore_session_is_valid() {
        let m = restore_session();
        assert!(matches!(m, Message::Restore(_)));
        assert!(m.verify());
    }

    #[test]
    fn test_last_trade_index_is_valid() {
        let m = last_trade_index();
        assert!(matches!(m, Message::Restore(_)));
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::LastTradeIndex);
        assert!(kind.payload.is_none());
        assert!(kind.verify());
    }

    #[test]
    fn test_send_dm_carries_text() {
        let m = send_dm("hello".to_string());
        let kind = m.get_inner_message_kind();
        assert_eq!(kind.action, Action::SendDm);
        match kind.payload.as_ref().unwrap() {
            Payload::TextMessage(s) => assert_eq!(s, "hello"),
            _ => panic!("expected TextMessage"),
        }
    }

    #[test]
    fn test_request_id_is_monotonic() {
        let a = next_request_id();
        let b = next_request_id();
        assert!(b > a);
    }

    #[test]
    fn test_validate_invoice_empty() {
        assert!(validate_invoice("").is_err());
        assert!(validate_invoice("  ").is_err());
    }

    #[test]
    fn test_validate_invoice_garbage() {
        assert!(validate_invoice("not-an-invoice").is_err());
        assert!(validate_invoice("lnbc123").is_err());
    }

    #[test]
    fn test_validate_invoice_ln_address() {
        assert!(validate_invoice("user@domain.com").is_ok());
        assert!(validate_invoice("satoshi@getalby.com").is_ok());
    }

    #[test]
    fn test_validate_invoice_bad_ln_address() {
        assert!(validate_invoice("@domain.com").is_err());
        assert!(validate_invoice("user@").is_err());
    }
}
