//! Global Mostro session subscription.
//!
//! Listens for Mostro daemon replies (GiftWrap kind 1059 / NIP-44 direct
//! kind 14) addressed to the user's active trade pubkeys AND the Mostro
//! identity key, on the configured daemon relays. This is the single
//! always-mounted listener that makes restore (`RestoreData`), order
//! enrichment (`Orders`), `LastTradeIndex`, and per-trade action updates
//! arrive regardless of the current route.
//!
//! Previously this subscription lived only inside the `/mostro` home route
//! (`routes/mostro/home.rs`). If the user navigated away (or arrived on a
//! different route), the daemon's replies were missed — the root cause of
//! "restore didn't bring my order back" and of silently-dropped trade
//! updates. Mounting it in the always-mounted `Layout` fixes that.
//!
//! ## Dioxus reactivity requirement
//!
//! This hook reads `TRADES` (via `build_trade_key_map` →
//! `active_trades_for_daemon`) and `MOSTRO_NODE_CONFIG` during render so
//! the host component (Layout) is subscribed to them. When a trade is
//! added/removed or the daemon config changes, Layout re-renders, the
//! filter is recomputed, and `use_relay_subscription_to` re-subscribes
//! with the updated `#p` set / relay list. (See the Dioxus verification:
//! `use_effect(use_reactive!(|filter, relay_urls| …))` re-runs on
//! dependency change only if the value is read in the reactive scope.)

use crate::stores::mostro::restore::handle_restore_event;
use crate::stores::mostro::{
    active_trade_filter, apply_mostro_action, apply_status, build_trade_key_map,
    cant_do_message, try_get as try_get_mostro_keys, try_get_node_config,
    unwrap_mostro_response, upsert_trade, publish_trades,
};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr::prelude::*;
use std::collections::HashSet;
use std::time::Duration;

/// Mount the global Mostro session subscription. Call once from the
/// always-mounted root `Layout` component (`routes/mod.rs`). Safe to call
/// before login / before Mostro keys exist — it no-ops (filter `None`)
/// until `try_get_mostro_keys()` and `try_get_node_config()` both return
/// `Some`.
#[allow(dead_code)]
pub fn use_mostro_session() {
    // Dedup buffer for events we've already processed. A persistent signal
    // (not a local) so it survives re-subscriptions triggered by filter
    // rebuilds without re-processing the backfill.
    let mut seen_events: Signal<HashSet<EventId>> = use_signal(HashSet::new);

    // Read keys + node config during render so Layout re-renders when they
    // change (node switch, keys init). These reads subscribe Layout to the
    // underlying GlobalSignals.
    let keys_state = try_get_mostro_keys();
    let node_cfg = try_get_node_config();
    let identity_keys = keys_state.as_ref().map(|k| k.identity_keys.clone());
    let identity_pk = identity_keys.as_ref().map(|k| k.public_key());
    let node_relays_for_sub = node_cfg.map(|n| n.relays).unwrap_or_default();

    // build_trade_key_map reads TRADES, subscribing Layout to trade changes.
    let key_map = build_trade_key_map();
    let mut all_pks: Vec<PublicKey> = key_map.keys().cloned().collect();
    if let Some(ipk) = identity_pk {
        if !all_pks.contains(&ipk) {
            all_pks.push(ipk);
        }
    }

    let session_filter = if all_pks.is_empty() {
        None
    } else {
        // Transport-aware: reads the current daemon's protocol_version so
        // the subscription rebuilds on a transport flip (v2 pins
        // authors=[daemon] to disambiguate kind-14 from NIP-17 peer chat).
        Some(active_trade_filter(&all_pks))
    };

    let id_keys_for_cb = identity_keys.clone();
    crate::hooks::use_relay_subscription_to(
        session_filter,
        None,
        node_relays_for_sub,
        move |event: &nostr_sdk::Event| {
            let event = event.clone();
            // Dedup: `insert` returns true if newly added. Skip entirely if
            // we've already seen this event id (the live sub and the
            // recovery fetch fallback can both deliver the same event).
            let is_new = seen_events.with_mut(|seen| seen.insert(event.id));
            if !is_new {
                return;
            }
            let id_keys = id_keys_for_cb.clone();
            spawn(async move {
                let recipient = event.tags.public_keys().next().cloned();

                // Route: if p-tag matches a known trade key → trade handler.
                if let Some(recipient_pk) = recipient {
                    let km = build_trade_key_map();
                    if let Some(&(trade_index, ref order_id)) = km.get(&recipient_pk) {
                        let keys_state = try_get_mostro_keys();
                        let keys = match keys_state {
                            Some(k) => k,
                            None => return,
                        };
                        let tk = match keys.get_trade_key_by_index(trade_index).ok() {
                            Some(k) => k,
                            None => return,
                        };
                        let unwrapped = match unwrap_mostro_response(&event, &tk).await {
                            Ok(Some(u)) => u,
                            Ok(None) => return,
                            Err(_) => return,
                        };

                        let action = unwrapped
                            .message
                            .inner_action()
                            .unwrap_or(mostro_core::prelude::Action::CantDo);
                        let payload = unwrapped.message.get_inner_message_kind().payload.clone();
                        let my_pk_hex = tk.public_key().to_hex();

                        let mut trade = match crate::stores::mostro::find_by_order_id(order_id) {
                            Some(t) => t,
                            None => return,
                        };

                        let old_status = trade.status;
                        let (new_status, _) = apply_mostro_action(
                            &mut trade,
                            action.clone(),
                            &payload,
                            unwrapped.sender,
                            &my_pk_hex,
                        );

                        if let Some(ns) = new_status {
                            trade = apply_status(&trade, ns);
                        }

                        if action == mostro_core::prelude::Action::CantDo {
                            if let Some(mostro_core::prelude::Payload::CantDo(reason)) = &payload {
                                let msg = reason
                                    .as_ref()
                                    .map(cant_do_message)
                                    .unwrap_or_else(|| "Unknown reason".to_string());
                                let event_age = crate::platform::timestamp::now_secs() as i64
                                    - event.created_at.as_secs() as i64;
                                if event_age < 60 {
                                    let toast = consume_toast();
                                    toast.error(
                                        "Cannot proceed".to_string(),
                                        ToastOptions::new()
                                            .description(msg)
                                            .duration(Duration::from_secs(5)),
                                    );
                                }
                            }
                        } else if trade.status != old_status {
                            let event_age = crate::platform::timestamp::now_secs() as i64
                                - event.created_at.as_secs() as i64;
                            if event_age < 60 {
                                let label = trade.status.label().to_string();
                                let toast = consume_toast();
                                toast.info(
                                    "Trade updated".to_string(),
                                    ToastOptions::new().description(format!(
                                        "{}: {}",
                                        order_id.chars().take(8).collect::<String>(),
                                        label
                                    )),
                                );
                            }
                        }

                        upsert_trade(trade.clone());
                        let _ = publish_trades().await;
                        return;
                    }
                }

                // Fallback: identity key → restore/orders/last-trade-index handler.
                if let Some(ref keys) = id_keys {
                    let _ = handle_restore_event(&event, keys).await;
                }
            });
        },
    );
}
