//! Mostro trade detail screen — full live view
//!
//! `/p2p/trade/:order_id` — the primary trade interaction page.
//!
//! Two relay subscriptions run concurrently:
//! 1. **Protocol**: GiftWraps addressed to the user's trade pubkey → daemon messages
//! 2. **Chat**: GiftWraps addressed to the SharedKey's public key → P2P chat

use crate::components::p2p::hold_invoice_panel::HoldInvoicePanel;
use crate::components::p2p::trade_action_panel::{TradeAction, TradeActionPanel};
use crate::components::p2p::trade_chat::{
    ChatMsg, TradeChat, load_chat_messages, save_chat_messages,
    decode_chat_content, encode_chat_content,
};
use crate::components::p2p::dispute_chat::{
    DisputeChat, DisputeChatMsg, load_dispute_chat_messages, save_dispute_chat_messages,
};
use crate::components::p2p::trade_status_badge::TradeStatusBadge;
use crate::components::p2p::trade_timeline::TradeTimeline;
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::social::mostro::{
    self, cant_do_message, trade_store::{Trade, TradeRole, TradeStatus},
};
use crate::stores::social::mostro::encrypted_attachment::{
    AttachmentMeta, encrypt_attachment, attachment_key_from_shared_secret, encode_nonce,
};
use crate::stores::social::mostro::client::{active_trade_filter, unwrap_mostro_response, ensure_node_relays_connected};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr::prelude::*;
use std::collections::HashSet;
use std::time::Duration;

type MostroAction = mostro_core::prelude::Action;
type MostroPayload = mostro_core::prelude::Payload;
use mostro_core::chat::SharedKey;

#[component]
pub fn P2PTradeDetail(order_id: String) -> Element {
    let mut trade_signal: Signal<Option<Trade>> = use_signal(|| mostro::find_by_order_id(&order_id));
    let mut chat_messages: Signal<Vec<ChatMsg>> = use_signal(|| load_chat_messages(&order_id));
    let mut dispute_chat_messages: Signal<Vec<DisputeChatMsg>> = use_signal(Vec::new);
    let mut action_busy = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let seen_events: Signal<HashSet<nostr::EventId>> = use_signal(HashSet::new);
    let nav = navigator();

    let trade = trade_signal.read().clone();

    // Derive trade keys and shared key for subscriptions
    let trade_keys = trade.as_ref().and_then(|t| {
        let keys = mostro::try_get()?;
        let idx = t.trade_index.unwrap_or(0);
        keys.get_trade_key_by_index(idx).ok()
    });
    let trade_pubkey = trade_keys.as_ref().map(|k| k.public_key());
    let trade_keys_for_proto = trade_keys.clone();
    let trade_keys_for_chat = trade_keys.clone();

    let node_relays = mostro::try_get_node_config()
        .map(|n| n.relays)
        .unwrap_or_default();

    let counterparty = trade.as_ref().and_then(|t| t.counterparty_pubkey.clone());
    let shared_key = match (&trade_keys, &counterparty) {
        (Some(tk), Some(cp_hex)) => PublicKey::from_hex(cp_hex)
            .ok()
            .and_then(|cp| SharedKey::derive(tk.secret_key(), &cp).ok()),
        _ => None,
    };
    let shared_key_for_chat = shared_key.clone();

    let solver_pubkey = trade.as_ref().and_then(|t| t.solver_pubkey.clone());
    let admin_shared_key = match (&trade_keys, &solver_pubkey) {
        (Some(tk), Some(sp_hex)) => PublicKey::from_hex(sp_hex)
            .ok()
            .and_then(|sp| SharedKey::derive(tk.secret_key(), &sp).ok()),
        _ => None,
    };
    let admin_shared_key_for_dispute = admin_shared_key.clone();
    let dispute_id = trade.as_ref().and_then(|t| t.dispute_id.clone());

    {
        if let Some(ref id) = dispute_id {
            dispute_chat_messages.set(load_dispute_chat_messages(id));
        }
    }

    // Ensure daemon relays are in the client pool before subscribing
    {
        let node_relays_clone = node_relays.clone();
        use_future(move || {
            let r = node_relays_clone.clone();
            async move {
                if !r.is_empty() {
                    ensure_node_relays_connected().await;
                }
            }
        });
    }

    // Clear the pending subscription from create_order if it matches this trade
    {
        let my_trade_pk = trade_pubkey.map(|p| p.to_hex());
        let pending = mostro::PENDING_CREATE_SUB.read().clone();
        if let (Some((_, pending_pk)), Some(ref my_pk)) = (pending, my_trade_pk) {
            if pending_pk.to_hex() == *my_pk {
                *mostro::PENDING_CREATE_SUB.write() = None;
            }
        }
    }

    // Backfill: one-shot fetch for missed GiftWraps when trade is still Pending
    {
        let tp = trade_pubkey;
        let trade_status = trade.as_ref().map(|t| t.status);
        let trade_created = trade.as_ref().map(|t| t.created_at);
        let relays = node_relays.clone();
        let mut ts = trade_signal;
        let order_id_for_bf = order_id.clone();
        use_future(move || {
            let relays = relays.clone();
            let order_id_for_bf = order_id_for_bf.clone();
            async move {
            let (pk, created) = match (tp, trade_status, trade_created) {
                (Some(pk), Some(TradeStatus::Pending), Some(c)) => (pk, c),
                _ => return,
            };
            let client = match crate::stores::nostr_client::get_client() {
                Some(c) => c,
                None => return,
            };
            let since = Timestamp::from(created.saturating_sub(300) as u64);
            let backfill_filter = crate::stores::social::mostro::client::active_trade_backfill_filter(pk, since);
            let urls: Vec<nostr::Url> = relays.iter().filter_map(|u| nostr::Url::parse(u).ok()).collect();
            if urls.is_empty() {
                return;
            }
            let events = match client.fetch_events_from(urls, backfill_filter, std::time::Duration::from_secs(10)).await {
                Ok(events) => events.into_iter().collect::<Vec<_>>(),
                Err(e) => {
                    log::warn!("Mostro backfill failed: {e}");
                    return;
                }
            };
            log::info!("Mostro backfill: received {} events", events.len());
            let keys = match mostro::try_get() {
                Some(k) => k,
                None => return,
            };
            let idx = mostro::find_by_order_id(&order_id_for_bf)
                .and_then(|t| t.trade_index)
                .unwrap_or(0);
            let trade_keys = match keys.get_trade_key_by_index(idx).ok() {
                Some(k) => k,
                None => return,
            };
            for event in events {
                let unwrapped = match unwrap_mostro_response(&event, &trade_keys).await {
                    Ok(Some(u)) => u,
                    _ => continue,
                };
                let action = unwrapped.message.inner_action().unwrap_or(MostroAction::CantDo);
                let mut current = match mostro::find_by_order_id(&order_id_for_bf) {
                    Some(t) => t,
                    None => continue,
                };
                match action {
                    MostroAction::NewOrder => {
                        if let Some(MostroPayload::Order(order)) = &unwrapped.message.get_inner_message_kind().payload {
                            if let Some(real_id) = order.id {
                                current.order_id = real_id.to_string();
                                let n = navigator();
                                n.replace(Route::P2PTradeDetail {
                                    order_id: real_id.to_string(),
                                });
                            }
                        }
                    }
                    MostroAction::CantDo => {
                        if let Some(MostroPayload::CantDo(reason)) = &unwrapped.message.get_inner_message_kind().payload {
                            let msg = reason
                                .as_ref()
                                .map(cant_do_message)
                                .unwrap_or_else(|| "Unknown reason".to_string());
                            let toast = consume_toast();
                            toast.error(
                                "Cannot proceed".to_string(),
                                ToastOptions::new()
                                    .description(msg)
                                    .duration(Duration::from_secs(5)),
                            );
                        }
                    }
                    _ => {}
                }
                mostro::upsert_trade(current.clone());
                ts.set(Some(current));
                let _ = mostro::publish_trades().await;
            }
        }
        });
    }

    // Periodic reconciliation: fetch missed GiftWraps for non-terminal trades
    {
        let tp = trade_pubkey;
        let trade_status = trade.as_ref().map(|t| t.status);
        let trade_updated = trade.as_ref().map(|t| t.updated_at);
        let relays = node_relays.clone();
        let mut ts = trade_signal;
        let oid = order_id.clone();
        use_future(move || {
            let relays = relays.clone();
            let oid = oid.clone();
            async move {
                let (pk, updated) = match (tp, trade_status, trade_updated) {
                    (Some(pk), Some(s), Some(u)) if !s.is_terminal() && s != TradeStatus::Pending => (pk, u),
                    _ => return,
                };
                loop {
                    crate::platform::timer::sleep(std::time::Duration::from_secs(300)).await;
                    let current = mostro::find_by_order_id(&oid);
                    let pk = match current {
                        Some(ref t) if !t.status.is_terminal() => pk,
                        _ => return,
                    };
                    let since = Timestamp::from(updated.saturating_sub(60) as u64);
                    let bf = crate::stores::social::mostro::client::active_trade_backfill_filter(pk, since);
                    let urls: Vec<nostr::Url> = relays.iter().filter_map(|u| nostr::Url::parse(u).ok()).collect();
                    if urls.is_empty() {
                        continue;
                    }
                    let client = match crate::stores::nostr_client::get_client() {
                        Some(c) => c,
                        None => continue,
                    };
                    let events = match client.fetch_events_from(urls, bf, std::time::Duration::from_secs(10)).await {
                        Ok(evs) => evs.into_iter().collect::<Vec<_>>(),
                        Err(e) => {
                            log::debug!("Mostro reconciliation fetch failed: {e}");
                            continue;
                        }
                    };
                    if events.is_empty() {
                        continue;
                    }
                    let keys = match mostro::try_get() {
                        Some(k) => k,
                        None => continue,
                    };
                    let idx = mostro::find_by_order_id(&oid)
                        .and_then(|t| t.trade_index)
                        .unwrap_or(0);
                    let trade_keys = match keys.get_trade_key_by_index(idx).ok() {
                        Some(k) => k,
                        None => continue,
                    };
                    for event in events {
                        let unwrapped = match unwrap_mostro_response(&event, &trade_keys).await {
                            Ok(Some(u)) => u,
                            _ => continue,
                        };
                        let action = unwrapped.message.inner_action().unwrap_or(MostroAction::CantDo);
                        let mut cur = match mostro::find_by_order_id(&oid) {
                            Some(t) => t,
                            None => continue,
                        };
                        let _kind = unwrapped.message.get_inner_message_kind();
                        if let Some(ns) = TradeStatus::from_action(action.clone(), cur.role) {
                            cur = mostro::apply_status(&cur, ns);
                        }
                        mostro::upsert_trade(cur.clone());
                        ts.set(Some(cur));
                        let _ = mostro::publish_trades().await;
                    }
                }
            }
        });
    }

    // Protocol subscription: GiftWraps to trade pubkey
    let proto_filter = trade_pubkey.map(|pk| active_trade_filter(&[pk]));
    let order_id_for_proto = order_id.clone();
    let trade_pubkey_for_proto = trade_pubkey.map(|p| p.to_hex());
    let relays_for_proto = node_relays.clone();
    crate::hooks::use_relay_subscription_to(
        proto_filter,
        None,
        relays_for_proto,
        move |event: &nostr_sdk::Event| {
            let event = event.clone();
            let keys = trade_keys_for_proto.clone();
            let oid = order_id_for_proto.clone();
            let my_pk = trade_pubkey_for_proto.clone();
            let mut seen = seen_events;
            spawn(async move {
                if seen.read().contains(&event.id) {
                    return;
                }
                seen.write().insert(event.id);

                let tk = match keys {
                    Some(k) => k,
                    None => return,
                };
                let unwrapped = match unwrap_mostro_response(&event, &tk).await {
                    Ok(Some(u)) => u,
                    Ok(None) => return,
                    Err(e) => {
                        log::debug!("mostro proto unwrap: {e}");
                        return;
                    }
                };

                let action = unwrapped.message.inner_action().unwrap_or(MostroAction::CantDo);
                let mut current = match mostro::find_by_order_id(&oid) {
                    Some(t) => t,
                    None => {
                        log::warn!("mostro: received {action:?} for unknown order {oid}, skipping");
                        return;
                    }
                };

                let kind = unwrapped.message.get_inner_message_kind();
                let new_status = match action {
                    MostroAction::AddBondInvoice => {
                        if let Some(MostroPayload::BondPayoutRequest(req)) = &kind.payload {
                            current.needs_bond_invoice = true;
                            current.bond_slashed_at = Some(req.slashed_at);
                            let claim_window_days = mostro::try_get_node_config()
                                .map(|c| c.bond_payout_claim_window_days)
                                .unwrap_or(30);
                            let claim_window = claim_window_days as i64 * 86400;
                            let deadline = req.slashed_at + claim_window;
                            current.bond_payout_deadline = Some(deadline);
                            let toast = consume_toast();
                            toast.warning(
                                "Bond payout claim".to_string(),
                                ToastOptions::new()
                                    .description(format!(
                                        "Counterparty's bond was slashed. Submit an invoice to claim your share. Deadline: {}",
                                        crate::utils::format::format_relative_time_or(deadline as u64, "unknown"),
                                    ))
                                    .duration(Duration::from_secs(10)),
                            );
                        } else {
                            current.needs_bond_invoice = true;
                        }
                        Some(TradeStatus::WaitingBond)
                    }
                    MostroAction::AddInvoice
                    | MostroAction::BuyerInvoiceAccepted => {
                        if let Some(MostroPayload::PaymentRequest(_, bolt11, _)) = &kind.payload {
                            current.pending_hold_invoice = Some(bolt11.clone());
                        }
                        Some(TradeStatus::WaitingBuyerInvoice)
                    }
                    MostroAction::PayInvoice => {
                        if let Some(MostroPayload::PaymentRequest(_, bolt11, _)) = &kind.payload {
                            current.pending_hold_invoice = Some(bolt11.clone());
                        }
                        Some(TradeStatus::WaitingSellerToPay)
                    }
                    MostroAction::PayBondInvoice => {
                        if let Some(MostroPayload::PaymentRequest(_, bolt11, _)) = &kind.payload {
                            current.pending_hold_invoice = Some(bolt11.clone());
                        }
                        current.is_bond_invoice = Some(true);
                        Some(TradeStatus::WaitingTakerBond)
                    }
                    MostroAction::WaitingSellerToPay => Some(TradeStatus::WaitingSellerToPay),
                    MostroAction::WaitingBuyerInvoice => Some(TradeStatus::WaitingBuyerInvoice),
                    MostroAction::HoldInvoicePaymentAccepted => {
                        if let Some(MostroPayload::Order(order)) = &kind.payload {
                            if current.counterparty_pubkey.is_none() {
                                let candidates = [
                                    order.buyer_trade_pubkey.as_deref(),
                                    order.seller_trade_pubkey.as_deref(),
                                ];
                                for pk in candidates.iter().flatten() {
                                    if my_pk.as_deref() != Some(pk) && !pk.is_empty() {
                                        current.counterparty_pubkey = Some(pk.to_string());
                                        break;
                                    }
                                }
                            }
                        }
                        Some(TradeStatus::Active)
                    }
                    MostroAction::BuyerTookOrder => {
                        if let Some(MostroPayload::Order(order)) = &kind.payload {
                            if let Some(buyer_pk) = &order.buyer_trade_pubkey {
                                if current.counterparty_pubkey.is_none() {
                                    current.counterparty_pubkey = Some(buyer_pk.clone());
                                }
                            }
                        }
                        Some(TradeStatus::Active)
                    }
                    MostroAction::FiatSentOk => {
                        if let Some(MostroPayload::Peer(peer)) = &kind.payload {
                            current.counterparty_pubkey = Some(peer.pubkey.clone());
                        }
                        current.fiat_was_sent = true;
                        Some(TradeStatus::FiatSent)
                    }
                    MostroAction::HoldInvoicePaymentSettled => Some(TradeStatus::Settled),
                    MostroAction::Released | MostroAction::PurchaseCompleted => {
                        Some(TradeStatus::Success)
                    }
                    MostroAction::Canceled | MostroAction::HoldInvoicePaymentCanceled => {
                        Some(TradeStatus::Canceled)
                    }
                    MostroAction::CooperativeCancelInitiatedByYou
                    | MostroAction::CooperativeCancelInitiatedByPeer => {
                        Some(TradeStatus::CancelPending)
                    }
                    MostroAction::CooperativeCancelAccepted => {
                        let toast = consume_toast();
                        toast.info(
                            "Trade canceled".to_string(),
                            ToastOptions::new()
                                .description("Both parties agreed to cancel the trade.".to_string())
                                .duration(Duration::from_secs(5)),
                        );
                        Some(TradeStatus::CooperativelyCanceled)
                    }
                    MostroAction::DisputeInitiatedByYou | MostroAction::DisputeInitiatedByPeer => {
                        if let Some(MostroPayload::Dispute(dispute_id, _)) = &kind.payload {
                            let toast = consume_toast();
                            let label = if action == MostroAction::DisputeInitiatedByYou {
                                "Dispute opened"
                            } else {
                                "Counterparty opened a dispute"
                            };
                            toast.info(
                                label.to_string(),
                                ToastOptions::new()
                                    .description(format!("Dispute ID: {dispute_id}"))
                                    .duration(Duration::from_secs(5)),
                            );
                        }
                        Some(TradeStatus::Dispute)
                    }
                    MostroAction::AdminTakeDispute | MostroAction::AdminTookDispute => {
                        current.solver_pubkey = Some(unwrapped.sender.to_hex());
                        let toast = consume_toast();
                        toast.info(
                            "Solver assigned".to_string(),
                            ToastOptions::new()
                                .description("A solver has been assigned to your dispute.")
                                .duration(Duration::from_secs(5)),
                        );
                        Some(TradeStatus::Dispute)
                    }
                    MostroAction::AdminCanceled => {
                        let toast = consume_toast();
                        toast.info(
                            "Admin canceled".to_string(),
                            ToastOptions::new()
                                .description("An admin has canceled this order.".to_string())
                                .duration(Duration::from_secs(5)),
                        );
                        Some(TradeStatus::CanceledByAdmin)
                    }
                    MostroAction::AdminSettled => {
                        let toast = consume_toast();
                        toast.info(
                            "Admin settled".to_string(),
                            ToastOptions::new()
                                .description("An admin has settled this order.".to_string())
                                .duration(Duration::from_secs(5)),
                        );
                        Some(TradeStatus::Success)
                    }
                    MostroAction::PaymentFailed => {
                        if let Some(MostroPayload::PaymentFailed(info)) = &kind.payload {
                            current.payment_failed_attempts = Some(info.payment_attempts);
                            current.payment_failed_retries_interval = Some(info.payment_retries_interval);
                            let toast = consume_toast();
                            toast.error(
                                "Payment failed".to_string(),
                                ToastOptions::new()
                                    .description(format!(
                                        "Up to {} retries, every {}s",
                                        info.payment_attempts,
                                        info.payment_retries_interval,
                                    ))
                                    .duration(Duration::from_secs(8)),
                            );
                        }
                        Some(TradeStatus::PaymentFailed)
                    }
                    MostroAction::BondSlashed => {
                        let toast = consume_toast();
                        toast.warning(
                            "Bond slashed".to_string(),
                            ToastOptions::new()
                                .description("Your anti-abuse bond has been slashed.".to_string())
                                .duration(Duration::from_secs(5)),
                        );
                        None
                    }
                    MostroAction::BondInvoiceAccepted => {
                        current.needs_bond_invoice = false;
                        let toast = consume_toast();
                        toast.info(
                            "Bond accepted".to_string(),
                            ToastOptions::new()
                                .description("Your bond invoice has been accepted.".to_string())
                                .duration(Duration::from_secs(3)),
                        );
                        None
                    }
                    MostroAction::BondPayoutCompleted => {
                        let toast = consume_toast();
                        toast.info(
                            "Bond update".to_string(),
                            ToastOptions::new()
                                .description(format!("{action:?}"))
                                .duration(Duration::from_secs(3)),
                        );
                        None
                    }
                    MostroAction::InvoiceUpdated => {
                        if let Some(MostroPayload::PaymentRequest(_, bolt11, _)) = &kind.payload {
                            current.pending_hold_invoice = Some(bolt11.clone());
                        }
                        None
                    }
                    MostroAction::Rate => {
                        let toast = consume_toast();
                        toast.info(
                            "Rate your counterparty".to_string(),
                            ToastOptions::new()
                                .description("The trade is complete. Please rate.".to_string())
                                .duration(Duration::from_secs(5)),
                        );
                        None
                    }
                    MostroAction::RateReceived => {
                        let toast = consume_toast();
                        toast.info(
                            "Rating received".to_string(),
                            ToastOptions::new()
                                .description("Thank you for rating.".to_string())
                                .duration(Duration::from_secs(3)),
                        );
                        None
                    }
                    MostroAction::NewOrder => {
                        if let Some(MostroPayload::Order(order)) = &kind.payload {
                            if let Some(real_id) = order.id {
                                current.order_id = real_id.to_string();
                                nav.replace(Route::P2PTradeDetail {
                                    order_id: real_id.to_string(),
                                });
                            }
                        }
                        None
                    }
                    MostroAction::CantDo => {
                        if let Some(MostroPayload::CantDo(reason)) = &kind.payload {
                            let msg = reason
                                .as_ref()
                                .map(cant_do_message)
                                .unwrap_or_else(|| "Unknown reason".to_string());
                            let toast = consume_toast();
                            toast.error(
                                "Cannot proceed".to_string(),
                                ToastOptions::new()
                                    .description(msg)
                                    .duration(Duration::from_secs(5)),
                            );
                        }
                        None
                    }
                    _ => None,
                };

                if let Some(ns) = new_status {
                    current = mostro::apply_status(&current, ns);
                }
                mostro::upsert_trade(current.clone());
                trade_signal.set(Some(current));
                let _ = mostro::publish_trades().await;
            });
        },
    );

    // Chat subscription: GiftWraps to shared key pubkey
    let chat_filter = shared_key.as_ref().map(|sk| mostro_core::chat::chat_filter(sk.public_key()));
    let sk_for_chat = shared_key_for_chat.clone();
    let my_pubkey_for_chat = trade_pubkey.map(|p| p.to_hex()).unwrap_or_default();
    let relays_for_chat = node_relays.clone();
    let oid_for_chat = order_id.clone();
    crate::hooks::use_relay_subscription_to(
        chat_filter,
        None,
        relays_for_chat,
        move |event: &nostr_sdk::Event| {
            let event = event.clone();
            let sk = sk_for_chat.clone();
            let my_pk = my_pubkey_for_chat.clone();
            let oid = oid_for_chat.clone();
            spawn(async move {
                let shared = match sk {
                    Some(s) => s,
                    None => return,
                };
                match mostro_core::chat::unwrap_chat_message(shared.keys(), &event).await {
                    Ok(chat_msg) => {
                        let is_me = chat_msg.sender.to_hex() == my_pk;
                        let (text, attachments) = decode_chat_content(&chat_msg.content);
                        chat_messages.write().push(ChatMsg {
                            content: text,
                            sender_hex: chat_msg.sender.to_hex(),
                            is_me,
                            timestamp: chat_msg.created_at.as_secs() as i64,
                            attachments,
                        });
                        save_chat_messages(&oid, &chat_messages.read());
                    }
                    Err(e) => {
                        log::debug!("chat unwrap: {e}");
                    }
                }
            });
        },
    );

    // Dispute chat subscription: GiftWraps to admin shared key pubkey
    let dispute_chat_filter = admin_shared_key
        .as_ref()
        .map(|sk| mostro_core::chat::chat_filter(sk.public_key()));
    let ask_for_dispute = admin_shared_key_for_dispute.clone();
    let my_pubkey_for_dispute = trade_pubkey.map(|p| p.to_hex()).unwrap_or_default();
    let relays_for_dispute = node_relays.clone();
    let did_for_dispute = dispute_id.clone();
    crate::hooks::use_relay_subscription_to(
        dispute_chat_filter,
        None,
        relays_for_dispute,
        move |event: &nostr_sdk::Event| {
            let event = event.clone();
            let ask = ask_for_dispute.clone();
            let my_pk = my_pubkey_for_dispute.clone();
            let did = did_for_dispute.clone();
            spawn(async move {
                let shared = match ask {
                    Some(s) => s,
                    None => return,
                };
                match mostro_core::chat::unwrap_chat_message(shared.keys(), &event).await {
                    Ok(chat_msg) => {
                        let is_me = chat_msg.sender.to_hex() == my_pk;
                        dispute_chat_messages.write().push(DisputeChatMsg {
                            content: chat_msg.content.clone(),
                            sender_hex: chat_msg.sender.to_hex(),
                            is_me,
                            timestamp: chat_msg.created_at.as_secs() as i64,
                            attachment: None,
                        });
                        if let Some(ref dispute_id) = did {
                            save_dispute_chat_messages(dispute_id, &dispute_chat_messages.read());
                        }
                    }
                    Err(e) => {
                        log::debug!("dispute chat unwrap: {e}");
                    }
                }
            });
        },
    );

    let on_action = {
        let order_id = order_id.clone();
        let node_relays = node_relays.clone();
        move |action: TradeAction| {
            let oid = order_id.clone();
            let relays = node_relays.clone();
            action_busy.set(true);
            action_error.set(None);
            spawn(async move {
                let mut t = match mostro::find_by_order_id(&oid) {
                    Some(t) => t,
                    None => {
                        action_error.set(Some("Trade not found".to_string()));
                        action_busy.set(false);
                        return;
                    }
                };
                let keys = match mostro::try_get() {
                    Some(k) => k,
                    None => {
                        action_error.set(Some("Mostro keys not ready".to_string()));
                        action_busy.set(false);
                        return;
                    }
                };
                let idx = t.trade_index.unwrap_or(0);
                let trade_keys = match keys.get_trade_key_by_index(idx) {
                    Ok(k) => k,
                    Err(e) => {
                        action_error.set(Some(format!("Key derivation failed: {e}")));
                        action_busy.set(false);
                        return;
                    }
                };
                let node = match mostro::try_get_node_config() {
                    Some(n) => n,
                    None => {
                        action_error.set(Some("Node not configured".to_string()));
                        action_busy.set(false);
                        return;
                    }
                };
                let node_pk = if node.pubkey.starts_with("npub1") {
                    match PublicKey::from_bech32(&node.pubkey) {
                        Ok(p) => p,
                        Err(e) => {
                            action_error.set(Some(format!("Bad node pubkey: {e}")));
                            action_busy.set(false);
                            return;
                        }
                    }
                } else {
                    match PublicKey::from_hex(&node.pubkey) {
                        Ok(p) => p,
                        Err(e) => {
                            action_error.set(Some(format!("Bad node pubkey: {e}")));
                            action_busy.set(false);
                            return;
                        }
                    }
                };

                let order_uuid = match uuid::Uuid::parse_str(&t.order_id) {
                    Ok(u) => u,
                    Err(_) => uuid::Uuid::new_v4(),
                };

                let next_trade = if t.should_send_next_trade() {
                    if let Some(mut mk) = mostro::try_get() {
                        mk.get_next_trade_key()
                            .ok()
                            .map(|k| {
                                let pk = k.public_key().to_hex();
                                let nidx = mk.trade_index.saturating_sub(1);
                                mostro::write_back_trade_index(mk.trade_index);
                                t.next_trade_pubkey = Some(pk.clone());
                                t.next_trade_index = Some(nidx);
                                (pk, nidx)
                            })
                    } else {
                        None
                    }
                } else {
                    None
                };

                if next_trade.is_some() {
                    mostro::upsert_trade(t.clone());
                }

                let message = match &action {
                    TradeAction::AddInvoice(bolt11) => {
                        if let Err(e) = mostro::validate_invoice(bolt11) {
                            action_error.set(Some(e));
                            action_busy.set(false);
                            return;
                        }
                        mostro::add_invoice(&keys, order_uuid, idx, bolt11.clone())
                    }
                    TradeAction::AddBondInvoice(bolt11) => {
                        if let Err(e) = mostro::validate_invoice(bolt11) {
                            action_error.set(Some(e));
                            action_busy.set(false);
                            return;
                        }
                        mostro::add_bond_invoice(&keys, order_uuid, idx, bolt11.clone())
                    }
                    TradeAction::FiatSent => mostro::fiat_sent(&keys, order_uuid, idx, next_trade),
                    TradeAction::Release => mostro::release(&keys, order_uuid, idx, next_trade),
                    TradeAction::Cancel => mostro::cancel(&keys, order_uuid, idx),
                    TradeAction::AcceptCancel => mostro::accept_cancel(&keys, order_uuid, idx),
                    TradeAction::Dispute => mostro::dispute(&keys, order_uuid, idx),
                    TradeAction::Rate(r) => match mostro::rate_user(&keys, order_uuid, idx, *r) {
                        Ok(m) => m,
                        Err(e) => {
                            action_error.set(Some(e));
                            action_busy.set(false);
                            return;
                        }
                    },
                };

                if let Err(e) = mostro::send_mostro_message(
                    &message,
                    &keys.identity_keys,
                    &trade_keys,
                    node_pk,
                    &relays,
                    node.pow,
                )
                .await
                {
                    let err_msg = if let Some(required_pow) =
                        mostro::client::fetch_daemon_pow(node_pk, &relays).await
                    {
                        if required_pow > node.pow {
                            format!(
                                "Send failed: daemon requires PoW of {required_pow} bits but we sent {pow}. Try again or switch daemons.",
                                pow = node.pow
                            )
                        } else {
                            format!("Send failed: {e}")
                        }
                    } else {
                        format!("Send failed: {e}")
                    };
                    action_error.set(Some(err_msg));
                    action_busy.set(false);
                    return;
                }

                let toast = consume_toast();
                let label = match &action {
                    TradeAction::AddInvoice(_) => "Invoice submitted",
                    TradeAction::AddBondInvoice(_) => "Bond invoice submitted",
                    TradeAction::FiatSent => "Fiat marked as sent",
                    TradeAction::Release => "Sats released",
                    TradeAction::Cancel => "Cancel requested",
                    TradeAction::AcceptCancel => "Cancel accepted",
                    TradeAction::Dispute => "Dispute opened",
                    TradeAction::Rate(_) => "Rating submitted",
                };
                toast.info(
                    label.to_string(),
                    ToastOptions::new().duration(Duration::from_secs(2)),
                );
                action_busy.set(false);
            });
        }
    };

    let on_chat_send = {
        let tk = trade_keys_for_chat.clone();
        let sk = shared_key.clone();
        let nr = node_relays.clone();
        let oid = order_id.clone();
        move |text: String| {
            let tk = tk.clone();
            let sk = sk.clone();
            let nr = nr.clone();
            let oid = oid.clone();
            spawn(async move {
                {
                    let now = crate::platform::timestamp::now_secs();
                    let my_hex = crate::stores::auth_store::get_pubkey()
                        .unwrap_or_default();
                    chat_messages.write().push(ChatMsg {
                        content: text.clone(),
                        sender_hex: my_hex,
                        is_me: true,
                        timestamp: now as i64,
                        attachments: Vec::new(),
                    });
                    save_chat_messages(&oid, &chat_messages.read());
                }
                let trade_k = match tk {
                    Some(k) => k,
                    None => return,
                };
                let shared = match sk {
                    Some(s) => s,
                    None => return,
                };
                match mostro_core::chat::wrap_chat_message(
                    &trade_k,
                    &shared.public_key(),
                    &text,
                )
                .await
                {
                    Ok(event) => {
                        use crate::stores::publish_queue::{self, types::QueueEventType};
                        publish_queue::enqueue(
                            event,
                            QueueEventType::DirectMessage,
                            Some(nr),
                            std::collections::HashMap::new(),
                        )
                        .await;
                    }
                    Err(e) => {
                        log::warn!("chat wrap failed: {e}");
                    }
                }
            });
        }
    };

    let on_dispute_chat_send = {
        let tk = trade_keys.clone();
        let ask = admin_shared_key.clone();
        let nr = node_relays.clone();
        let did = dispute_id.clone();
        move |text: String| {
            let tk = tk.clone();
            let ask = ask.clone();
            let nr = nr.clone();
            let did = did.clone();
            spawn(async move {
                let trade_k = match tk {
                    Some(k) => k,
                    None => return,
                };
                let admin_sk = match ask {
                    Some(s) => s,
                    None => return,
                };
                {
                    let now = crate::platform::timestamp::now_secs();
                    let my_hex = trade_k.public_key().to_hex();
                    dispute_chat_messages.write().push(DisputeChatMsg {
                        content: text.clone(),
                        sender_hex: my_hex,
                        is_me: true,
                        timestamp: now as i64,
                        attachment: None,
                    });
                    if let Some(ref dispute_id) = did {
                        save_dispute_chat_messages(dispute_id, &dispute_chat_messages.read());
                    }
                }
                match mostro_core::chat::wrap_chat_message(
                    &trade_k,
                    &admin_sk.public_key(),
                    &text,
                )
                .await
                {
                    Ok(event) => {
                        use crate::stores::publish_queue::{self, types::QueueEventType};
                        publish_queue::enqueue(
                            event,
                            QueueEventType::DirectMessage,
                            Some(nr),
                            std::collections::HashMap::new(),
                        )
                        .await;
                    }
                    Err(e) => {
                        log::warn!("dispute chat wrap failed: {e}");
                    }
                }
            });
        }
    };

    let on_upload_chat_file = {
        let tk = trade_keys_for_chat.clone();
        let sk = shared_key.clone();
        let nr = node_relays.clone();
        let oid = order_id.clone();
        move |(file_name, bytes, mime_type): (String, Vec<u8>, String)| {
            let tk = tk.clone();
            let sk = sk.clone();
            let nr = nr.clone();
            let oid = oid.clone();
            spawn(async move {
                let trade_k = match tk {
                    Some(k) => k,
                    None => return,
                };
                let shared = match sk {
                    Some(s) => s,
                    None => return,
                };
                let att_key = {
                    let raw = shared.secret_key().to_secret_bytes();
                    attachment_key_from_shared_secret(&raw)
                };
                let (encrypted, nonce) = match encrypt_attachment(&bytes, &att_key) {
                    Ok(r) => r,
                    Err(e) => {
                        log::warn!("chat attachment encrypt failed: {e}");
                        return;
                    }
                };
                let server = crate::stores::media::blossom_store::get_primary_server();
                let url = match crate::stores::media::blossom_store::upload_raw_blob(
                    encrypted,
                    "application/octet-stream".to_string(),
                    Some(server),
                )
                .await
                {
                    Ok(u) => u,
                    Err(e) => {
                        log::warn!("chat attachment upload failed: {e}");
                        return;
                    }
                };
                let meta = AttachmentMeta {
                    url,
                    nonce: encode_nonce(&nonce),
                    mime_type: mime_type.clone(),
                    size: bytes.len() as u64,
                };
                let content = encode_chat_content(
                    &format!("Sent a file: {file_name}"),
                    vec![meta.clone()],
                );
                {
                    let now = crate::platform::timestamp::now_secs();
                    let my_hex = crate::stores::auth_store::get_pubkey()
                        .unwrap_or_default();
                    chat_messages.write().push(ChatMsg {
                        content: format!("Sent a file: {file_name}"),
                        sender_hex: my_hex,
                        is_me: true,
                        timestamp: now as i64,
                        attachments: vec![meta],
                    });
                    save_chat_messages(&oid, &chat_messages.read());
                }
                match mostro_core::chat::wrap_chat_message(
                    &trade_k,
                    &shared.public_key(),
                    &content,
                )
                .await
                {
                    Ok(event) => {
                        use crate::stores::publish_queue::{self, types::QueueEventType};
                        publish_queue::enqueue(
                            event,
                            QueueEventType::DirectMessage,
                            Some(nr),
                            std::collections::HashMap::new(),
                        )
                        .await;
                    }
                    Err(e) => {
                        log::warn!("chat attachment wrap failed: {e}");
                    }
                }
            });
        }
    };

    let on_upload_dispute_file = {
        let tk = trade_keys.clone();
        let ask = admin_shared_key.clone();
        let nr = node_relays.clone();
        let did = dispute_id.clone();
        move |(file_name, bytes, mime_type): (String, Vec<u8>, String)| {
            let tk = tk.clone();
            let ask = ask.clone();
            let nr = nr.clone();
            let did = did.clone();
            spawn(async move {
                let trade_k = match tk {
                    Some(k) => k,
                    None => return,
                };
                let admin_sk = match ask {
                    Some(s) => s,
                    None => return,
                };
                let att_key = {
                    let raw = admin_sk.secret_key().to_secret_bytes();
                    attachment_key_from_shared_secret(&raw)
                };
                let (encrypted, nonce) = match encrypt_attachment(&bytes, &att_key) {
                    Ok(r) => r,
                    Err(e) => {
                        log::warn!("dispute attachment encrypt failed: {e}");
                        return;
                    }
                };
                let server = crate::stores::media::blossom_store::get_primary_server();
                let url = match crate::stores::media::blossom_store::upload_raw_blob(
                    encrypted,
                    "application/octet-stream".to_string(),
                    Some(server),
                )
                .await
                {
                    Ok(u) => u,
                    Err(e) => {
                        log::warn!("dispute attachment upload failed: {e}");
                        return;
                    }
                };
                let meta = AttachmentMeta {
                    url,
                    nonce: encode_nonce(&nonce),
                    mime_type: mime_type.clone(),
                    size: bytes.len() as u64,
                };
                let content = encode_chat_content(
                    &format!("Sent a file: {file_name}"),
                    vec![meta.clone()],
                );
                {
                    let now = crate::platform::timestamp::now_secs();
                    let my_hex = trade_k.public_key().to_hex();
                    dispute_chat_messages.write().push(DisputeChatMsg {
                        content: format!("Sent a file: {file_name}"),
                        sender_hex: my_hex,
                        is_me: true,
                        timestamp: now as i64,
                        attachment: Some(meta),
                    });
                    if let Some(ref dispute_id) = did {
                        save_dispute_chat_messages(dispute_id, &dispute_chat_messages.read());
                    }
                }
                match mostro_core::chat::wrap_chat_message(
                    &trade_k,
                    &admin_sk.public_key(),
                    &content,
                )
                .await
                {
                    Ok(event) => {
                        use crate::stores::publish_queue::{self, types::QueueEventType};
                        publish_queue::enqueue(
                            event,
                            QueueEventType::DirectMessage,
                            Some(nr),
                            std::collections::HashMap::new(),
                        )
                        .await;
                    }
                    Err(e) => {
                        log::warn!("dispute attachment wrap failed: {e}");
                    }
                }
            });
        }
    };

    rsx! {
        div { class: "min-h-screen p-4 max-w-3xl mx-auto",
            if !*crate::stores::nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else {
                div { class: "space-y-4",
                    // Header
                    div { class: "flex items-center gap-3",
                        button {
                            class: "p-2 hover:bg-accent rounded-lg",
                            title: "Back",
                            onclick: move |_| {
                                let _ = nav.push(Route::P2PMyTrades {});
                            },
                            crate::components::icons::ArrowLeftIcon { class: "w-5 h-5".to_string() }
                        }
                        h1 { class: "text-xl font-bold", "Trade Detail" }
                    }

                    if let Some(t) = trade {
                        // Summary card
                        div { class: "p-4 bg-card border border-border rounded-lg",
                            div { class: "flex items-center justify-between mb-2",
                                span { class: "text-sm text-muted-foreground", "Order" }
                                TradeStatusBadge { status: t.status }
                            }
                            div { class: "grid grid-cols-2 gap-2 text-sm",
                                div {
                                    span { class: "text-muted-foreground", "Kind: " }
                                    span { class: "font-medium", "{t.kind}" }
                                }
                                div {
                                    span { class: "text-muted-foreground", "Role: " }
                                    span { class: "font-medium", "{t.role.as_str()}" }
                                }
                                div {
                                    span { class: "text-muted-foreground", "Fiat: " }
                                    span { class: "font-medium", "{t.fiat_amount} {t.fiat_code}" }
                                }
                                if let Some(sats) = t.sats_amount {
                                    div {
                                        span { class: "text-muted-foreground", "Sats: " }
                                        span { class: "font-medium", "{sats}" }
                                    }
                                }
                            }
                            if !t.payment_methods.is_empty() {
                                div { class: "mt-2 text-xs text-muted-foreground",
                                    "Payment: {t.payment_methods.join(\", \")}"
                                }
                            }
                        }

                        // Timeline
                        TradeTimeline { trade: t.clone() }

                        // Maker waiting banner
                        if t.status == TradeStatus::Pending && t.role == TradeRole::Maker {
                            div { class: "p-3 bg-blue-500/10 border border-blue-500/20 rounded-lg",
                                p { class: "text-sm text-blue-600 dark:text-blue-400",
                                    "Your order is live on the order book. You'll be notified when a taker is found."
                                }
                            }
                        }

                        // Orphan trade warning (placeholder ID still pending after 30s)
                        if t.status == TradeStatus::Pending
                            && (t.order_id.starts_with("maker-") || t.order_id.starts_with("taker-"))
                        {
                            {
                                let elapsed = crate::platform::timestamp::now_secs() as i64 - t.created_at;
                                if elapsed > 30 {
                                    rsx! {
                                        div { class: "p-3 bg-amber-500/10 border border-amber-500/20 rounded-lg",
                                            p { class: "text-sm text-amber-600 dark:text-amber-400",
                                                "The daemon has not confirmed this trade yet. It may have been rejected or the relay connection is slow."
                                            }
                                        }
                                    }
                                } else {
                                    rsx! {}
                                }
                            }
                        }

                        // Range order info banner (maker only)
                        if t.is_range_order() && t.role == TradeRole::Maker {
                            {
                                let (min, max) = (t.min_fiat.unwrap_or(0.0), t.max_fiat.unwrap_or(0.0));
                                let taken: f64 = t.fiat_amount.parse().unwrap_or(0.0);
                                let remaining = max - taken;
                                let has_more = remaining >= min;
                                rsx! {
                                    div { class: "p-3 bg-blue-500/10 border border-blue-500/20 rounded-lg",
                                        p { class: "text-sm text-blue-600 dark:text-blue-400",
                                            "Range order: {min:.0} - {max:.0} {t.fiat_code}"
                                        }
                                        if has_more {
                                            p { class: "text-xs text-blue-500/80 mt-1",
                                                "Remaining capacity: {remaining:.0} {t.fiat_code}. A new trade key will be rotated automatically on next action."
                                            }
                                        } else {
                                            p { class: "text-xs text-blue-500/80 mt-1",
                                                "Fully filled ({taken:.0} {t.fiat_code} taken)."
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Hold invoice panel (when we have a pending invoice and seller needs to pay)
                        if let Some(ref invoice) = t.pending_hold_invoice {
                            if matches!(t.status, TradeStatus::WaitingSellerToPay | TradeStatus::WaitingTakerBond) {
                                HoldInvoicePanel {
                                    invoice: invoice.clone(),
                                    updated_at: Some(t.updated_at),
                                    is_bond: t.is_bond_invoice.unwrap_or(false),
                                    bond_payout_deadline: t.bond_payout_deadline,
                                }
                            }
                        }

                        // Action panel
                        TradeActionPanel {
                            trade: t.clone(),
                            on_action: on_action,
                        }

                        if *action_busy.read() {
                            div { class: "flex justify-center py-2",
                                span { class: "inline-block w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin text-muted-foreground" }
                            }
                        }
                        if let Some(ref err) = *action_error.read() {
                            p { class: "text-sm text-red-500", "{err}" }
                        }

                        // Chat (only when counterparty known)
                        {let chat_locked = t.counterparty_pubkey.is_none();
                        let my_pk = trade_pubkey.map(|p| p.to_hex()).unwrap_or_default();
                        rsx! {
                            TradeChat {
                                messages: chat_messages.read().clone(),
                                locked: chat_locked,
                                my_pubkey_hex: my_pk,
                                on_send: on_chat_send,
                                on_upload_file: on_upload_chat_file,
                            }
                        }}

                        // Dispute chat (only when solver assigned)
                        if t.solver_pubkey.is_some() {
                            {let dispute_locked = admin_shared_key.is_none();
                            let my_pk = trade_pubkey.map(|p| p.to_hex()).unwrap_or_default();
                            rsx! {
                                div { class: "mt-4",
                                    DisputeChat {
                                        messages: dispute_chat_messages.read().clone(),
                                        locked: dispute_locked,
                                        my_pubkey_hex: my_pk,
                                        on_send: on_dispute_chat_send,
                                        on_upload_file: on_upload_dispute_file,
                                    }
                                }
                            }}
                        }
                    } else {
                        div { class: "p-8 text-center",
                            div { class: "text-4xl mb-4", "?" }
                            h3 { class: "text-lg font-medium mb-2", "Trade not found" }
                            p { class: "text-muted-foreground mb-4",
                                "No local record exists for this trade."
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                onclick: move |_| {
                                    let _ = nav.push(Route::P2PHome {});
                                },
                                "Back to orders"
                            }
                        }
                    }
                }
            }
        }
    }
}
