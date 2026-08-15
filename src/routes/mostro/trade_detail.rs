//! Mostro trade detail screen — full live view
//!
//! `/p2p/trade/:order_id` — the primary trade interaction page.
//!
//! Two relay subscriptions run concurrently:
//! 1. **Protocol**: GiftWraps addressed to the user's trade pubkey → daemon messages
//! 2. **Chat**: GiftWraps addressed to the SharedKey's public key → P2P chat

use crate::components::mostro::hold_invoice_panel::HoldInvoicePanel;
use crate::components::mostro::trade_action_panel::{TradeAction, TradeActionPanel};
use crate::components::mostro::trade_chat::{
    ChatMsg, TradeChat, load_chat_messages, save_chat_messages,
    decode_chat_content, encode_chat_content, is_dup_chat_msg,
};
use crate::components::mostro::dispute_chat::{
    DisputeChat, DisputeChatMsg, load_dispute_chat_messages, save_dispute_chat_messages,
    is_dup_dispute_msg,
};
use crate::components::mostro::trade_status_badge::TradeStatusBadge;
use crate::components::mostro::trade_timeline::TradeTimeline;
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::mostro::{
    self, cant_do_message, trade_store::{Trade, TradeRole, TradeStatus},
};
use crate::stores::mostro::encrypted_attachment::{
    AttachmentMeta, encrypt_attachment, attachment_key_from_shared_secret, encode_nonce,
};
use crate::stores::mostro::client::{active_trade_filter, unwrap_mostro_response, ensure_node_relays_connected};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr::prelude::*;
use std::time::Duration;

type MostroAction = mostro_core::prelude::Action;
type MostroPayload = mostro_core::prelude::Payload;
use mostro_core::chat::SharedKey;

#[component]
pub fn MostroTradeDetail(order_id: String) -> Element {
    let mut trade_signal: Signal<Option<Trade>> = use_signal(|| mostro::find_by_order_id(&order_id));
    let mut chat_messages: Signal<Vec<ChatMsg>> = use_signal(|| load_chat_messages(&order_id));
    let mut dispute_chat_messages: Signal<Vec<DisputeChatMsg>> = use_signal(Vec::new);
    let mut action_busy = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let countdown_tick = use_signal(|| 0u64);
    let recovering = use_signal(|| false);
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

    // Clear the pending subscription from create_order if it matches this trade.
    // Unsubscribe from the relay to avoid leaking the dead subscription.
    {
        let my_trade_pk = trade_pubkey.map(|p| p.to_hex());
        let pending = mostro::PENDING_CREATE_SUB.write().take();
        if let (Some((old_sub_id, pending_pk)), Some(ref my_pk)) = (pending, my_trade_pk) {
            if pending_pk.to_hex() == *my_pk {
                spawn(async move {
                    if let Some(client) = crate::stores::nostr_client::get_client() {
                        let _ = client.unsubscribe(&old_sub_id).await;
                    }
                });
            } else {
                *mostro::PENDING_CREATE_SUB.write() = Some((old_sub_id, pending_pk));
            }
        }
    }

    // Backfill: poll for missed GiftWraps when trade is still Pending.
    // Retries every 5 seconds for up to 30 seconds to handle the case where
    // the daemon ACK was missed (e.g. timeout during order creation).
    {
        let tp = trade_pubkey;
        let trade_status = trade.as_ref().map(|t| t.status);
        let trade_created = trade.as_ref().map(|t| t.created_at);
        let relays = node_relays.clone();
        let mut ts = trade_signal;
        let order_id_for_bf = order_id.clone();
        let nav_for_bf = navigator();
        use_future(move || {
            let relays = relays.clone();
            let order_id_for_bf = order_id_for_bf.clone();
            async move {
            let (pk, created) = match (tp, trade_status, trade_created) {
                (Some(pk), Some(TradeStatus::Pending), Some(c)) => (pk, c),
                _ => return,
            };
            let urls: Vec<nostr::Url> = relays.iter().filter_map(|u| nostr::Url::parse(u).ok()).collect();
            if urls.is_empty() {
                return;
            }
            let deadline = created as u64 + 30;
            let mut attempts = 0u32;
            loop {
                let current_trade = mostro::find_by_order_id(&order_id_for_bf);
                let still_pending = current_trade
                    .as_ref()
                    .map(|t| t.status == TradeStatus::Pending)
                    .unwrap_or(false);
                if !still_pending {
                    return;
                }
                let now = crate::platform::timestamp::now_secs();
                if now >= deadline {
                    let is_placeholder = current_trade
                        .as_ref()
                        .map(|t| t.is_placeholder())
                        .unwrap_or(true);
                    if is_placeholder {
                        let node_cfg = mostro::try_get_node_config();
                        let node_pk_hex = node_cfg.as_ref().map(|c| c.pubkey.clone());
                        let node_pow = node_cfg.as_ref().map(|c| c.pow).unwrap_or(0);
                        let relay_urls = relays.clone();
                        let order_id_for_remove = order_id_for_bf.clone();
                        spawn(async move {
                            if let Some(pk_hex) = node_pk_hex {
                                if let Ok(npk_parsed) = nostr::PublicKey::from_hex(&pk_hex) {
                                    if let Some(required_pow) = mostro::client::fetch_daemon_pow(npk_parsed, &relay_urls).await {
                                        if required_pow > node_pow {
                                            let toast = consume_toast();
                                            toast.error(
                                                "PoW too low".to_string(),
                                                ToastOptions::new()
                                                    .description(format!(
                                                        "Daemon requires PoW of {required_pow}, we sent {node_pow}. Update daemon settings."
                                                    ))
                                                    .duration(Duration::from_secs(10)),
                                            );
                                        }
                                    }
                                }
                            }
                        });

                        let toast = consume_toast();
                        toast.error(
                            "Trade did not confirm".to_string(),
                            ToastOptions::new()
                                .description(
                                    "The daemon did not acknowledge this trade within 30 seconds. \
                                     It may have been rejected or the relays may be slow. \
                                     Please try again.".to_string(),
                                )
                                .duration(Duration::from_secs(8)),
                        );
                        mostro::remove_trade(&order_id_for_remove);
                        nav_for_bf.replace(Route::MostroHome {});
                        return;
                    } else {
                        // Real-UUID Pending trade: stop aggressive polling but
                        // keep the trade. The always-mounted use_mostro_session
                        // hook and the periodic reconciler (every 300s) will
                        // continue catching daemon events at a slower cadence.
                        log::info!(
                            "Trade {} still Pending after 30s with real UUID — transitioning to slow reconcile",
                            order_id_for_bf
                        );
                        return;
                    }
                }

                let client = match crate::stores::nostr_client::get_client() {
                    Some(c) => c,
                    None => {
                        crate::platform::timer::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                };

                if attempts > 0 {
                    crate::platform::timer::sleep(std::time::Duration::from_secs(5)).await;
                }
                attempts += 1;

                // Phase 3.4 (F14): widen the backfill `since` window from
                // 5 minutes (`created - 300`) to 3 DAYS, to account for
                // NIP-59's gift-wrap envelope `created_at` randomization
                // (`nostr::nips::nip59::RANGE_RANDOM_TIMESTAMP_TWEAK =
                // 0..172800`, i.e. up to 2 days back), plus 1 day of
                // margin for relay propagation delay.
                //
                // Without this, the backfill silently missed most gift
                // wraps because their randomized `created_at` falls before
                // the 5-min window — leaving the trade-detail page stuck
                // on an older step than the daemon actually is in. The
                // wider window will re-fetch some already-seen events,
                // which the `dedup::SEEN_EVENTS` LRU handles transparently.
                let now_secs = crate::platform::timestamp::now_secs() as i64;
                let since_secs = now_secs.saturating_sub(3 * 86_400).max(created.saturating_sub(3 * 86_400));
                let since = Timestamp::from(since_secs as u64);
                let backfill_filter = crate::stores::mostro::client::active_trade_backfill_filter(pk, since);
                let events = match client.fetch_events_from(urls.clone(), backfill_filter, std::time::Duration::from_secs(10)).await {
                    Ok(events) => events.into_iter().collect::<Vec<_>>(),
                    Err(e) => {
                        log::debug!("Mostro backfill attempt {attempts} failed: {e}");
                        continue;
                    }
                };
                if events.is_empty() {
                    continue;
                }
                log::info!("Mostro backfill attempt {attempts}: received {} events", events.len());
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
                                    nav_for_bf.replace(Route::MostroTradeDetail {
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
                let after = mostro::find_by_order_id(&order_id_for_bf);
                if after.as_ref().map(|t| t.status != TradeStatus::Pending).unwrap_or(false) {
                    return;
                }
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
                    // Phase 3.4 (F14): same 3-day skew-tolerant window as
                    // the initial backfill. Reconciliation runs every 5 min,
                    // so most calls will return zero new events; the wider
                    // window only matters on the first call after app start
                    // (covers events that arrived while the page was
                    // unmounted).
                    let now_secs = crate::platform::timestamp::now_secs() as i64;
                    let since_secs = now_secs.saturating_sub(3 * 86_400).max(updated.saturating_sub(3 * 86_400));
                    let since = Timestamp::from(since_secs as u64);
                    let bf = crate::stores::mostro::client::active_trade_backfill_filter(pk, since);
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
                        let kind = unwrapped.message.get_inner_message_kind();
                        let my_pk_hex = trade_keys.public_key().to_hex();
                        let (new_status, _toast) = mostro::apply_mostro_action(
                            &mut cur,
                            action,
                            &kind.payload,
                            unwrapped.sender,
                            &my_pk_hex,
                        );
                        if let Some(ns) = new_status {
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

    // Auto-tick for countdown timer (1-second interval)
    {
        let mut tick = countdown_tick;
        use_future(move || async move {
            loop {
                crate::platform::timer::sleep(std::time::Duration::from_secs(1)).await;
                let now = tick.read().wrapping_add(1);
                tick.set(now);
            }
        });
    }

    // Phase 8.3: fetch counterparty reputation from kind 38384 when the
    // counterparty pubkey becomes available. One-shot fetch (ratings
    // don't change in real-time during a trade).
    {
        let trade_signal_for_rating = trade_signal;
        let node_relays_for_rating = node_relays.clone();
        let daemon_pk_hex = mostro::try_get_node_config().map(|n| n.pubkey);
        use_future(move || {
            let node_relays = node_relays_for_rating.clone();
            let daemon_hex = daemon_pk_hex.clone();
            let trade_signal = trade_signal_for_rating;
            async move {
                loop {
                    let trade = trade_signal.read().clone();
                    let cpk = match trade.and_then(|t| t.counterparty_pubkey) {
                        Some(pk) => pk,
                        None => {
                            crate::platform::timer::sleep(std::time::Duration::from_secs(5)).await;
                            continue;
                        }
                    };
                    let daemon_hex = match &daemon_hex {
                        Some(pk) => pk.clone(),
                        None => return,
                    };
                    let daemon_pk = match PublicKey::from_hex(&daemon_hex) {
                        Ok(pk) => pk,
                        Err(_) => return,
                    };
                    let filter = crate::stores::mostro::ratings::rating_filter(
                        daemon_pk,
                        &cpk,
                    );
                    let urls: Vec<nostr::Url> = node_relays
                        .iter()
                        .filter_map(|u| nostr::Url::parse(u).ok())
                        .collect();
                    if urls.is_empty() {
                        return;
                    }
                    let client = match crate::stores::nostr_client::get_client() {
                        Some(c) => c,
                        None => return,
                    };
                    match client
                        .fetch_events_from(&urls, filter, std::time::Duration::from_secs(10))
                        .await
                    {
                        Ok(events) => {
                            for event in events.into_iter() {
                                if let Some((pubkey, rating)) =
                                    crate::stores::mostro::ratings::parse_rating_event(&event)
                                {
                                    crate::stores::mostro::ratings::upsert_rating(
                                        pubkey, rating,
                                    );
                                }
                            }
                            return; // One-shot — don't poll.
                        }
                        Err(e) => {
                            log::debug!("Rating fetch failed: {e}");
                            crate::platform::timer::sleep(std::time::Duration::from_secs(15)).await;
                        }
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
            spawn(async move {
                if mostro::is_seen(&event.id) {
                    return;
                }
                mostro::mark_seen(event.id);

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
                let kind = unwrapped.message.get_inner_message_kind();
                let request_id = kind.request_id;
                mostro::waiter::try_satisfy_waiter(
                    &oid,
                    request_id,
                    unwrapped.message.clone(),
                );
                let mut current = match mostro::find_by_order_id(&oid) {
                    Some(t) => t,
                    None => {
                        log::warn!("mostro: received {action:?} for unknown order {oid}, skipping");
                        return;
                    }
                };

                // Single source of truth: delegate field mutations + toast
                // production to `apply_mostro_action`. The caller retains
                // only side effects that require async/nav/trade_signal
                // access (NewOrder navigation, CantDo trade removal).
                let original_order_id = current.order_id.clone();
                let payload = unwrapped.message.get_inner_message_kind().payload.clone();
                let my_pk_hex = my_pk.as_deref().unwrap_or("");
                let action_for_side_effects = action.clone();
                let (new_status, toasts) = mostro::apply_mostro_action(
                    &mut current,
                    action,
                    &payload,
                    unwrapped.sender,
                    my_pk_hex,
                );
                mostro::emit_toasts(&toasts);

                // Caller-specific side effects that cannot live in the pure
                // state machine (require nav, trade_signal, async, etc.).
                match action_for_side_effects {
                    MostroAction::NewOrder => {
                        if let Some(MostroPayload::Order(order)) = &payload {
                            if let Some(real_id) = order.id {
                                let was_placeholder = original_order_id.starts_with("maker-")
                                    || original_order_id.starts_with("taker-");
                                if was_placeholder {
                                    nav.replace(Route::MostroTradeDetail {
                                        order_id: real_id.to_string(),
                                    });
                                } else if current.is_range_order()
                                    && current.child_order_id.is_none()
                                {
                                    current.child_order_id = Some(real_id.to_string());
                                    let mut child = Trade::new_pending(
                                        real_id.to_string(),
                                        real_id.to_string(),
                                        current.maker_pubkey.clone(),
                                        TradeRole::Maker,
                                        current.kind.clone(),
                                        String::new(),
                                        current.fiat_code.clone(),
                                        None,
                                        current.premium,
                                        current.payment_methods.clone(),
                                        current.next_trade_index,
                                    );
                                    child.parent_order_id = Some(current.order_id.clone());
                                    if let Some(nxt_pk) = &current.next_trade_pubkey {
                                        child.my_trade_pubkey = Some(nxt_pk.clone());
                                    }
                                    mostro::upsert_trade(child);
                                    let toast = consume_toast();
                                    toast.info(
                                        "Range order continued".to_string(),
                                        ToastOptions::new()
                                            .description(format!("Child order created: {real_id}"))
                                            .duration(Duration::from_secs(5)),
                                    );
                                }
                            }
                        }
                    }
                    MostroAction::CantDo => {
                        if let Some(MostroPayload::CantDo(Some(r))) = &payload {
                            use mostro_core::prelude::CantDoReason;
                            match r {
                                CantDoReason::PendingOrderExists | CantDoReason::NotFound => {
                                    mostro::remove_trade(&current.order_id);
                                    trade_signal.set(None);
                                    let _ = mostro::publish_trades().await;
                                    return;
                                }
                                CantDoReason::InvalidTradeIndex => {
                                    let oid = oid.clone();
                                    spawn(async move {
                                        log::info!("Triggering trade index sync for {oid}");
                                        let _ = mostro::request_restore().await;
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }

                if let Some(ns) = new_status {
                    if mostro::is_status_transition_allowed(&current.status, &ns) {
                        current = mostro::apply_status(&current, ns);
                        if current.status.is_terminal() {
                            mostro::waiter::prune_waiters_for_order(&current.order_id);
                        }
                    } else {
                        log::debug!(
                            "Blocked status transition {:?} → {:?} on trade {}",
                            current.status, ns, current.order_id
                        );
                    }
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
            if crate::stores::mostro::dedup::is_seen(&event.id) {
                return;
            }
            crate::stores::mostro::dedup::mark_seen(event.id);
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
                        let msg = ChatMsg {
                            content: text,
                            sender_hex: chat_msg.sender.to_hex(),
                            is_me,
                            timestamp: chat_msg.created_at.as_secs() as i64,
                            attachments,
                        };
                        if !is_dup_chat_msg(&chat_messages.read(), &msg) {
                            chat_messages.write().push(msg);
                            save_chat_messages(&oid, &chat_messages.read());
                        }
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
            if crate::stores::mostro::dedup::is_seen(&event.id) {
                return;
            }
            crate::stores::mostro::dedup::mark_seen(event.id);
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
                        // Phase 5.3 (M15): decode attachments from the chat
                        // content using the same decode_chat_content helper
                        // as the trade chat. Previously this was hardcoded to
                        // `attachment: None`, so inbound dispute file messages
                        // appeared as raw JSON text instead of a file bubble.
                        let (text, attachments) = decode_chat_content(&chat_msg.content);
                        let attachment = attachments.into_iter().next();
                        let msg = DisputeChatMsg {
                            content: text,
                            sender_hex: chat_msg.sender.to_hex(),
                            is_me,
                            timestamp: chat_msg.created_at.as_secs() as i64,
                            attachment,
                        };
                        if !is_dup_dispute_msg(&dispute_chat_messages.read(), &msg) {
                            dispute_chat_messages.write().push(msg);
                            if let Some(ref dispute_id) = did {
                                save_dispute_chat_messages(dispute_id, &dispute_chat_messages.read());
                            }
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
            if *action_busy.read() {
                return;
            }
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

                // Bug #11 fix: short-circuit ALL outbound actions when
                // the order_id is still a placeholder (`maker-N` / `taker-N`).
                // Only Cancel/Discard are valid on placeholders. Previously
                // a non-UUID order_id would silently fall through to
                // `Uuid::new_v4()`, sending a random UUID to the daemon
                // which would `CantDo::NotFound`.
                if t.is_placeholder()
                    && !matches!(action, TradeAction::Cancel | TradeAction::Discard)
                {
                    let toast = consume_toast();
                    toast.warning(
                        "Order not yet confirmed".to_string(),
                        ToastOptions::new()
                            .description(
                                "The daemon hasn't acknowledged this order yet. \
                                 Wait for the NewOrder confirmation before performing actions."
                                    .to_string(),
                            )
                            .duration(Duration::from_secs(5)),
                    );
                    action_busy.set(false);
                    return;
                }

                if t.is_placeholder()
                    && matches!(action, TradeAction::Cancel | TradeAction::Discard)
                {
                    mostro::remove_trade(&t.order_id);
                    let _ = mostro::publish_trades().await;
                    let toast = consume_toast();
                    toast.info(
                        "Trade discarded".to_string(),
                        ToastOptions::new()
                            .description(
                                "The unconfirmed trade has been removed from your list."
                                    .to_string(),
                            )
                            .duration(Duration::from_secs(3)),
                    );
                    let _ = nav.push(Route::MostroMyTrades {});
                    action_busy.set(false);
                    return;
                }

                let order_uuid = match uuid::Uuid::parse_str(&t.order_id) {
                    Ok(u) => u,
                    Err(_) => {
                        // Non-placeholder but non-UUID — corrupted local
                        // state. Log and abort rather than sending a random
                        // UUID to the daemon.
                        log::error!(
                            "mostro: order_id is neither placeholder nor UUID: {}",
                            t.order_id
                        );
                        action_busy.set(false);
                        return;
                    }
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
                    // C6: pre-register a placeholder trade for the next
                    // slice of this range order. The daemon will create
                    // a child order and address its `NewOrder` ACK to
                    // `next_trade.0`. Without a placeholder, the
                    // background trade monitor wouldn't know to derive
                    // a key for that pubkey and route the ACK. When the
                    // ACK arrives, `trade_store::upsert`'s
                    // `my_trade_pubkey` match replaces the placeholder
                    // order_id with the real one (placeholder→UUID
                    // migration path at trade_store.rs:610-611).
                    if let Some((ref next_pk, next_idx)) = next_trade {
                        crate::stores::mostro::trade_store::insert_range_child_placeholder(
                            &t,
                            next_pk.clone(),
                            next_idx,
                        );
                    }
                }

                let message = match &action {
                    TradeAction::AddInvoice(input) => {
                        let bolt11 = {
                            let lower = input.trim().to_ascii_lowercase();
                            if lower.contains('@') && lower.contains('.') {
                                let sats = t.sats_amount.unwrap_or(0) as u64;
                                if sats == 0 {
                                    action_error.set(Some("Cannot resolve Lightning Address: sats amount unknown".to_string()));
                                    action_busy.set(false);
                                    return;
                                }
                                if let Err(e) = crate::services::payments::lnurl::check_lud16_reachable(input.trim()).await {
                                    action_error.set(Some(format!(
                                        "Lightning Address {} appears unreachable: {e}. Try a direct invoice instead.",
                                        input.trim()
                                    )));
                                    action_busy.set(false);
                                    return;
                                }
                                match crate::services::payments::lnurl::get_invoice_from_lud16(
                                    input.trim(),
                                    sats,
                                    None,
                                ).await {
                                    Ok(invoice) => invoice,
                                    Err(e) => {
                                        action_error.set(Some(format!("Failed to resolve Lightning Address: {e}")));
                                        action_busy.set(false);
                                        return;
                                    }
                                }
                            } else {
                                input.clone()
                            }
                        };
                        if let Err(e) = mostro::validate_invoice_with_amount(
                            &bolt11,
                            t.sats_amount.map(|s| s as u64),
                        ) {
                            action_error.set(Some(e));
                            action_busy.set(false);
                            return;
                        }
                        mostro::add_invoice(&keys, order_uuid, idx, bolt11)
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
                    TradeAction::Cancel | TradeAction::AcceptCancel => {
                        // Phase 3.5 (F15): stamp User-initiated cancel so
                        // cleanup_expired can delete the trade quickly (no
                        // slash is expected from a cancel the user
                        // themselves triggered or accepted from the peer).
                        if let Some(mut trade) = mostro::find_by_order_id(&order_uuid.to_string()) {
                            if trade.cancel_initiator.is_none() {
                                trade.cancel_initiator =
                                    Some(crate::stores::mostro::trade_store::CancelInitiator::User);
                                mostro::upsert_trade(trade);
                            }
                        }
                        if action == TradeAction::Cancel {
                            mostro::cancel(&keys, order_uuid, idx)
                        } else {
                            mostro::accept_cancel(&keys, order_uuid, idx)
                        }
                    }
                    TradeAction::Dispute => mostro::dispute(&keys, order_uuid, idx),
                    TradeAction::Rate(r) => match mostro::rate_user(&keys, order_uuid, idx, *r) {
                        Ok(m) => m,
                        Err(e) => {
                            action_error.set(Some(e));
                            action_busy.set(false);
                            return;
                        }
                    },
                    TradeAction::Discard => unreachable!("Discard handled above"),
                };

                let mut waiter_registered = false;
                {
                    let rid = message.get_inner_message_kind().request_id;
                    if let Some(rid) = rid {
                        t.last_request_id = Some(rid);
                        mostro::upsert_trade(t.clone());
                        // Wire the waiter: keep the action button busy until
                        // the daemon acks (any action with matching
                        // request_id), a CantDo arrives, or a 15s timeout
                        // expires. The live event handler at ~line 497 calls
                        // try_satisfy_waiter, which resolves the channel.
                        let rx = mostro::waiter::register_waiter(oid.clone(), rid);
                        waiter_registered = true;
                        let mut busy_signal = action_busy;
                        let log_oid = oid.clone();
                        spawn(async move {
                            match tokio::time::timeout(
                                std::time::Duration::from_secs(15),
                                rx,
                            )
                            .await
                            {
                                Ok(Ok(_response)) => {
                                    // Ack received. The live handler already
                                    // applied apply_mostro_action and surfaced
                                    // any CantDo toast.
                                    busy_signal.set(false);
                                }
                                Ok(Err(_)) => {
                                    // Channel dropped (pruned via
                                    // prune_waiters_for_order on terminal
                                    // status).
                                    busy_signal.set(false);
                                }
                                Err(_) => {
                                    log::warn!(
                                        "mostro: timed out after 15s waiting \
                                         for ack on order {log_oid} \
                                         (request_id {rid}), clearing busy"
                                    );
                                    // Remove the leaked waiter entry so the
                                    // WAITERS HashMap doesn't grow unboundedly.
                                    mostro::waiter::prune_waiter(&log_oid, rid);
                                    busy_signal.set(false);
                                }
                            }
                        });
                    }
                }

                let pow = mostro::resolve_effective_pow(&node, node_pk).await;
                if let Err(e) = mostro::send_mostro_message(
                    &message,
                    &keys.identity_keys,
                    &trade_keys,
                    node_pk,
                    &relays,
                    pow,
                )
                .await
                {
                    let err_msg = if let Some(required_pow) =
                        mostro::client::fetch_daemon_pow(node_pk, &relays).await
                    {
                        if required_pow > pow {
                            format!(
                                "Send failed: daemon requires PoW of {required_pow} bits but we sent {pow}. Try again or switch daemons.",
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
                    TradeAction::Discard => "Trade discarded",
                };
                toast.info(
                    label.to_string(),
                    ToastOptions::new().duration(Duration::from_secs(2)),
                );
                // Only clear busy immediately if no waiter was registered
                // (i.e., the action had no request_id). When a waiter IS
                // registered, busy clears when the spawned task resolves
                // (on ack, CantDo, or 15s timeout).
                if !waiter_registered {
                    action_busy.set(false);
                }
            });
        }
    };

    let on_chat_send = {
        let tk = trade_keys_for_chat.clone();
        let sk = shared_key.clone();
        let nr = node_relays.clone();
        let oid = order_id.clone();
        let cpk_for_notify = counterparty.clone();
        move |text: String| {
            let tk = tk.clone();
            let sk = sk.clone();
            let nr = nr.clone();
            let oid = oid.clone();
            let cpk_notify = cpk_for_notify.clone();
            spawn(async move {
                let trade_k = match tk {
                    Some(k) => k,
                    None => return,
                };
                let shared = match sk {
                    Some(s) => s,
                    None => return,
                };
                {
                     let now = crate::platform::timestamp::now_secs();
                     let my_hex = trade_k.public_key().to_hex();
                     let msg = ChatMsg {
                         content: text.clone(),
                         sender_hex: my_hex,
                         is_me: true,
                         timestamp: now as i64,
                         attachments: Vec::new(),
                     };
                     if !is_dup_chat_msg(&chat_messages.read(), &msg) {
                         chat_messages.write().push(msg);
                         save_chat_messages(&oid, &chat_messages.read());
                     }
                 }
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
                        // Phase 10.4: fire-and-forget peer wake via the
                        // Mostro push server. The push server looks up
                        // the peer's trade pubkey in its registered-tokens
                        // database and sends a push notification to wake
                        // their device.
                        if let Some(ref peer_pk) = cpk_notify {
                            crate::services::mostro_push::notify_peer(peer_pk).await;
                        }
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
                     let msg = DisputeChatMsg {
                         content: text.clone(),
                         sender_hex: my_hex,
                         is_me: true,
                         timestamp: now as i64,
                         attachment: None,
                     };
                     if !is_dup_dispute_msg(&dispute_chat_messages.read(), &msg) {
                         dispute_chat_messages.write().push(msg);
                         if let Some(ref dispute_id) = did {
                             save_dispute_chat_messages(dispute_id, &dispute_chat_messages.read());
                         }
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
                // Phase 5.5 (M17): sign the Blossom upload with the trade
                // key, not the user's primary identity. This preserves the
                // per-trade key separation that the Mostro protocol enforces.
                let url = match crate::stores::media::blossom_store::upload_raw_blob_with_signer(
                    encrypted,
                    "application/octet-stream".to_string(),
                    Some(server),
                    &trade_k,
                )
                .await
                {
                    Ok(u) => u,
                    Err(e) => {
                        log::warn!("chat attachment upload failed: {e}");
                        return;
                    }
                };
                // Phase 5.1 (C7): spec-compatible AttachmentMeta with all
                // fields for cross-client interop.
                let meta = AttachmentMeta {
                    kind: AttachmentMeta::classify(&mime_type),
                    blossom_url: url,
                    nonce: encode_nonce(&nonce),
                    mime_type: mime_type.clone(),
                    original_size: bytes.len() as u64,
                    filename: Some(file_name.clone()),
                    encrypted_size: Some((bytes.len() + 12 + 16) as u64),
                    file_type: Some(AttachmentMeta::file_type_label(&mime_type).to_string()),
                    width: None,
                    height: None,
                };
                let content = encode_chat_content(
                    &format!("Sent a file: {file_name}"),
                    vec![meta.clone()],
                );
                {
                     let now = crate::platform::timestamp::now_secs();
                     let my_hex = trade_k.public_key().to_hex();
                     let msg = ChatMsg {
                         content: format!("Sent a file: {file_name}"),
                         sender_hex: my_hex,
                         is_me: true,
                         timestamp: now as i64,
                         attachments: vec![meta],
                     };
                     if !is_dup_chat_msg(&chat_messages.read(), &msg) {
                         chat_messages.write().push(msg);
                         save_chat_messages(&oid, &chat_messages.read());
                     }
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
                // Phase 5.5 (M17): sign the dispute-chat Blossom upload
                // with the trade key (not the primary identity).
                let url = match crate::stores::media::blossom_store::upload_raw_blob_with_signer(
                    encrypted,
                    "application/octet-stream".to_string(),
                    Some(server),
                    &trade_k,
                )
                .await
                {
                    Ok(u) => u,
                    Err(e) => {
                        log::warn!("dispute attachment upload failed: {e}");
                        return;
                    }
                };
                // Phase 5.1 (C7): spec-compatible AttachmentMeta.
                let meta = AttachmentMeta {
                    kind: AttachmentMeta::classify(&mime_type),
                    blossom_url: url,
                    nonce: encode_nonce(&nonce),
                    mime_type: mime_type.clone(),
                    original_size: bytes.len() as u64,
                    filename: Some(file_name.clone()),
                    encrypted_size: Some((bytes.len() + 12 + 16) as u64),
                    file_type: Some(AttachmentMeta::file_type_label(&mime_type).to_string()),
                    width: None,
                    height: None,
                };
                let content = encode_chat_content(
                    &format!("Sent a file: {file_name}"),
                    vec![meta.clone()],
                );
                {
                     let now = crate::platform::timestamp::now_secs();
                     let my_hex = trade_k.public_key().to_hex();
                     let msg = DisputeChatMsg {
                         content: format!("Sent a file: {file_name}"),
                         sender_hex: my_hex,
                         is_me: true,
                         timestamp: now as i64,
                         attachment: Some(meta),
                     };
                     if !is_dup_dispute_msg(&dispute_chat_messages.read(), &msg) {
                         dispute_chat_messages.write().push(msg);
                         if let Some(ref dispute_id) = did {
                             save_dispute_chat_messages(dispute_id, &dispute_chat_messages.read());
                         }
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
                                let _ = nav.push(Route::MostroMyTrades {});
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
                            if let Some(ref cpk) = t.counterparty_pubkey {
                                {
                                    let profile = crate::stores::profiles::get_profile(cpk);
                                    if profile.is_none() {
                                        crate::stores::profiles::queue_profile_request(cpk.clone());
                                    }
                                    let display_name = profile
                                        .as_ref()
                                        .and_then(crate::stores::profiles::display_name_or_name)
                                        .unwrap_or_else(|| {
                                            let hex = cpk.as_str();
                                            format!("{}...", &hex[..hex.len().min(12)])
                                        });
                                    rsx! {
                                        div { class: "mt-2 pt-2 border-t border-border",
                                            span { class: "text-xs text-muted-foreground", "Counterparty: " }
                                            span { class: "text-xs font-medium", "{display_name}" }
                                            // Phase 8.5: counterparty reputation card.
                                            div { class: "mt-1",
                                                crate::components::mostro::reputation_card::ReputationCard {
                                                    pubkey_hex: cpk.clone(),
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(ref child_id) = t.child_order_id {
                                div { class: "mt-2 pt-2 border-t border-border",
                                    span { class: "text-xs text-muted-foreground", "Child order: " }
                                    Link {
                                        to: Route::MostroTradeDetail { order_id: child_id.clone() },
                                        class: "text-xs text-blue-500 hover:underline",
                                        "{child_id}"
                                    }
                                }
                            }
                            if let Some(ref parent_id) = t.parent_order_id {
                                div { class: "mt-2 pt-2 border-t border-border",
                                    span { class: "text-xs text-muted-foreground", "Parent order: " }
                                    Link {
                                        to: Route::MostroTradeDetail { order_id: parent_id.clone() },
                                        class: "text-xs text-blue-500 hover:underline",
                                        "{parent_id}"
                                    }
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

                        // Cancel pending banner
                        if t.status == TradeStatus::CancelPending {
                            div { class: "p-3 bg-amber-500/10 border border-amber-500/20 rounded-lg",
                                p { class: "text-sm text-amber-600 dark:text-amber-400",
                                    if t.fiat_was_sent {
                                        "A cancel request is pending. Fiat was already sent — accepting will NOT reverse the transfer."
                                    } else {
                                        "A cancel request is pending. The trade will be canceled once both parties agree."
                                    }
                                }
                            }
                        }

                        // Order expiration countdown
                        if let Some(expires) = t.expires_at {
                            if !t.status.is_terminal() {
                                {
                                    let _ = *countdown_tick.read();
                                    let now = crate::platform::timestamp::now_secs() as i64;
                                    let remaining = (expires - now).max(0);
                                    let total = expires - t.created_at;
                                    let fraction = if total > 0 {
                                        remaining as f64 / total as f64
                                    } else {
                                        0.0
                                    };
                                    let (color_cls, bg_cls) = if remaining == 0 || fraction < 0.25 {
                                        ("text-red-500", "bg-red-500/10 border-red-500/20")
                                    } else if fraction < 0.5 {
                                        ("text-amber-600 dark:text-amber-400", "bg-amber-500/10 border-amber-500/20")
                                    } else {
                                        ("text-green-600 dark:text-green-400", "bg-green-500/10 border-green-500/20")
                                    };
                                    let label = if remaining == 0 {
                                        "Order expired".to_string()
                                    } else {
                                        let d = remaining / 86400;
                                        let h = (remaining % 86400) / 3600;
                                        let m = (remaining % 3600) / 60;
                                        let s = remaining % 60;
                                        if d > 0 {
                                            format!("Expires in {d}d {h}h {m}m {s}s")
                                        } else if h > 0 {
                                            format!("Expires in {h}h {m}m {s}s")
                                        } else {
                                            format!("Expires in {m}m {s}s")
                                        }
                                    };
                                    rsx! {
                                        div { class: "p-3 {bg_cls} border rounded-lg",
                                            p { class: "text-sm {color_cls}", "{label}" }
                                        }
                                    }
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
                                            button {
                                                class: "mt-2 px-3 py-1.5 text-xs font-medium border border-amber-500/40 text-amber-600 dark:text-amber-400 rounded-lg hover:bg-amber-500/20 transition",
                                                onclick: {
                                                    let mut on_action = on_action.clone();
                                                    move |_| (on_action)(TradeAction::Discard)
                                                },
                                                "Discard Trade"
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
                            countdown_tick: *countdown_tick.read(),
                            busy: *action_busy.read(),
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
                        let shared_key_hex = shared_key_for_chat.as_ref().map(|sk| sk.to_hex());
                        rsx! {
                            TradeChat {
                                messages: chat_messages.read().clone(),
                                locked: chat_locked,
                                my_pubkey_hex: my_pk,
                                on_send: on_chat_send,
                                on_upload_file: on_upload_chat_file,
                                shared_key_hex: shared_key_hex,
                            }
                        }}

                        // Dispute chat — visible whenever a solver has been
                        // assigned and the trade is not terminal. The input
                        // is only editable while the dispute is active
                        // (status == Dispute); after resolution the chat
                        // history remains visible but read-only.
                        if t.solver_pubkey.is_some()
                            && !t.status.is_terminal()
                        {
                            {let dispute_locked = admin_shared_key.is_none()
                                || !matches!(t.status, TradeStatus::Dispute);
                            let my_pk = trade_pubkey.map(|p| p.to_hex()).unwrap_or_default();
                            let admin_sk_hex = admin_shared_key_for_dispute.as_ref().map(|sk| sk.to_hex());
                            rsx! {
                                div { class: "mt-4",
                                    DisputeChat {
                                        messages: dispute_chat_messages.read().clone(),
                                        locked: dispute_locked,
                                        my_pubkey_hex: my_pk,
                                        on_send: on_dispute_chat_send,
                                        on_upload_file: on_upload_dispute_file,
                                        shared_key_hex: admin_sk_hex,
                                    }
                                }
                            }}
                        }
                    } else {
                        {let mut recovering = recovering;
                        let oid_recover = order_id.clone();
                        rsx! {
                            div { class: "p-8 text-center",
                                div { class: "text-4xl mb-4", "?" }
                                h3 { class: "text-lg font-medium mb-2", "Trade not found" }
                                p { class: "text-muted-foreground mb-4",
                                    "No local record exists for this trade."
                                }
                                div { class: "flex flex-col gap-2 items-center",
                                    button {
                                        class: "px-4 py-2 border border-border rounded-lg hover:bg-accent transition disabled:opacity-50 text-sm",
                                        disabled: *recovering.read()
                                            || crate::stores::mostro::is_restore_in_progress(),
                                        title: "Fetch this order from the daemon and rebuild your local trade record",
                                        onclick: move |_| {
                                            let oid = oid_recover.clone();
                                            let mut ts = trade_signal;
                                            spawn(async move {
                                                let uuid = match uuid::Uuid::parse_str(&oid) {
                                                    Ok(u) => u,
                                                    Err(_) => {
                                                        let toast = consume_toast();
                                                        toast.error(
                                                            "Cannot recover".to_string(),
                                                            ToastOptions::new()
                                                                .description("Order id is not a valid UUID.")
                                                                .duration(Duration::from_secs(4)),
                                                        );
                                                        return;
                                                    }
                                                };
                                                recovering.set(true);
                                                match mostro::recover_order_by_id(uuid).await {
                                                    Ok(1) => {
                                                        let toast = consume_toast();
                                                        toast.success(
                                                            "Order recovered".to_string(),
                                                            ToastOptions::new()
                                                                .description("Restored your trade record from the daemon.")
                                                                .duration(Duration::from_secs(3)),
                                                        );
                                                        ts.set(mostro::find_by_order_id(&oid));
                                                    }
                                                    Ok(_) => {
                                                        let toast = consume_toast();
                                                        toast.error(
                                                            "Not found".to_string(),
                                                            ToastOptions::new()
                                                                .description("The daemon has no record of you owning this order.")
                                                                .duration(Duration::from_secs(5)),
                                                        );
                                                    }
                                                    Err(e) => {
                                                        let toast = consume_toast();
                                                        toast.error(
                                                            "Recovery failed".to_string(),
                                                            ToastOptions::new()
                                                                .description(e)
                                                                .duration(Duration::from_secs(6)),
                                                        );
                                                    }
                                                }
                                                recovering.set(false);
                                            });
                                        },
                                        { if *recovering.read() { "Recovering…" } else { "Recover from daemon" } }
                                    }
                                    button {
                                        class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                        onclick: move |_| {
                                            let _ = nav.push(Route::MostroHome {});
                                        },
                                        "Back to orders"
                                    }
                                }
                            }
                        }}
                    }
                }
            }
        }
    }
}
