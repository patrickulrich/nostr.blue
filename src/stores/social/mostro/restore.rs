//! Mostro session restore pipeline
//!
//! When the app starts (or the user re-authenticates), this module:
//!
//! 1. Sends a `RestoreSession` request to the configured Mostro daemon
//!    using the **identity key** as both seal signer and rumor author.
//! 2. Listens for GiftWraps addressed to the identity key.
//! 3. Parses `RestoreData(RestoreSessionInfo)` responses (bypassing
//!    `verify()` which fails on restore payloads).
//! 4. Re-derives trade keys for each restored order and upserts them
//!    into the trade store.
//! 5. Handles `LastTradeIndex` to sync the local monotonic counter.
//!
//! The daemon's `restore_session.rs` sends the response back to the
//! rumor-author pubkey (which is the identity key for restore requests).

use dioxus::prelude::*;
use mostro_core::prelude::*;
use nostr::nips::nip44;
use nostr::prelude::*;
use serde::{Deserialize, Serialize};
use std::result::Result;

use super::client::{send_mostro_message, unwrap_mostro_response};
use super::flow;
use super::keys;
use super::node_config;
use super::trade_store::{self, Trade, TradeRole, TradeStatus};

type MostroAction = mostro_core::prelude::Action;
type MostroPayload = mostro_core::prelude::Payload;

const RESTORE_STATE_KEY: &str = "mostro_restore_state";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestoreStage {
    Idle,
    SendingRequest,
    WaitingResponse,
    Done,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestoreState {
    pub stage: RestoreStage,
    pub restored_count: usize,
    pub last_error: Option<String>,
}

impl Default for RestoreState {
    fn default() -> Self {
        Self {
            stage: RestoreStage::Idle,
            restored_count: 0,
            last_error: None,
        }
    }
}

pub static RESTORE_STATE: GlobalSignal<RestoreState> = Signal::global(RestoreState::default);

pub fn init_from_cache() {
    if let Ok(state) = crate::platform::storage::get::<String>(RESTORE_STATE_KEY) {
        if let Ok(parsed) = serde_json::from_str(&state) {
            *RESTORE_STATE.write() = parsed;
        }
    }
}

pub fn reset() {
    let _ = crate::platform::storage::delete(RESTORE_STATE_KEY);
    *RESTORE_STATE.write() = RestoreState::default();
}

fn persist_state() {
    let json = serde_json::to_string(&*RESTORE_STATE.read()).unwrap_or_default();
    let _ = crate::platform::storage::set(RESTORE_STATE_KEY, &json);
}

fn status_from_daemon(s: &str) -> TradeStatus {
    match s {
        "pending" => TradeStatus::Pending,
        "waiting-buyer-invoice" => TradeStatus::WaitingBuyerInvoice,
        "waiting-payment" => TradeStatus::WaitingSellerToPay,
        "active" => TradeStatus::Active,
        "fiat-sent" => TradeStatus::FiatSent,
        "settled-hold-invoice" => TradeStatus::Settled,
        "success" | "completed-by-admin" | "settled-by-admin" => TradeStatus::Success,
        "canceled" => TradeStatus::Canceled,
        "cooperatively-canceled" => TradeStatus::CooperativelyCanceled,
        "canceled-by-admin" => TradeStatus::CanceledByAdmin,
        "expired" => TradeStatus::Expired,
        "dispute" => TradeStatus::Dispute,
        "payment-failed" => TradeStatus::PaymentFailed,
        "waiting-buyer-bond-invoice" | "waiting-seller-bond-invoice" => TradeStatus::WaitingBond,
        "waiting-taker-bond" => TradeStatus::WaitingTakerBond,
        "waiting-maker-bond" => TradeStatus::WaitingBond,
        "in-progress" => TradeStatus::Active,
        _ => TradeStatus::Pending,
    }
}

pub async fn request_restore() -> Result<(), String> {
    request_restore_with_retry(3).await
}

async fn request_restore_with_retry(max_retries: u32) -> Result<(), String> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match request_restore_inner().await {
            Ok(()) => return Ok(()),
            Err(e) => {
                if attempt >= max_retries {
                    *RESTORE_STATE.write() = RestoreState {
                        stage: RestoreStage::Failed,
                        restored_count: 0,
                        last_error: Some(format!("{e} (after {attempt} attempts)")),
                    };
                    persist_state();
                    return Err(e);
                }
                let delay = std::time::Duration::from_secs(2u64.pow(attempt));
                log::warn!(
                    "Restore attempt {attempt}/{max_retries} failed: {e} — retrying in {delay:?}"
                );
                crate::platform::timer::sleep(delay).await;
            }
        }
    }
}

async fn request_restore_inner() -> Result<(), String> {
    let mostro_keys = keys::try_get().ok_or("Mostro keys not initialized")?;
    let node = node_config::try_get().ok_or("Mostro node not configured")?;
    let node_pk = PublicKey::from_hex(&node.pubkey)
        .map_err(|e| format!("Invalid node pubkey: {e}"))?;

    *RESTORE_STATE.write() = RestoreState {
        stage: RestoreStage::SendingRequest,
        restored_count: 0,
        last_error: None,
    };
    persist_state();

    let message = flow::restore_session();

    send_mostro_message(
        &message,
        &mostro_keys.identity_keys,
        &mostro_keys.identity_keys,
        node_pk,
        &node.relays,
        node.pow,
    )
    .await?;

    let lti_message = flow::last_trade_index();
    let _ = send_mostro_message(
        &lti_message,
        &mostro_keys.identity_keys,
        &mostro_keys.identity_keys,
        node_pk,
        &node.relays,
        node.pow,
    )
    .await;

    *RESTORE_STATE.write() = RestoreState {
        stage: RestoreStage::WaitingResponse,
        restored_count: 0,
        last_error: None,
    };
    persist_state();

    Ok(())
}

pub async fn handle_restore_event(
    event: &nostr::Event,
    identity_keys: &nostr::Keys,
) -> bool {
    let mostro_keys = match keys::try_get() {
        Some(k) => k,
        None => return false,
    };

    let message = match unwrap_mostro_response(event, identity_keys).await {
        Ok(Some(u)) => u.message,
        Ok(None) => return false,
        Err(_) => {
            match try_parse_rumor_direct(event, identity_keys) {
                Some(msg) => msg,
                None => return false,
            }
        }
    };

    let action = message.inner_action().unwrap_or(MostroAction::CantDo);
    handle_restore_message(action, &message, &mostro_keys)
}

fn handle_restore_message(
    action: MostroAction,
    message: &Message,
    keys: &keys::MostroKeys,
) -> bool {
    match action {
        MostroAction::RestoreSession => {
            handle_restore_data(message, keys);
            true
        }
        MostroAction::LastTradeIndex => {
            handle_last_trade_index(message);
            true
        }
        MostroAction::Orders => {
            handle_orders_response(message);
            true
        }
        _ => false,
    }
}

fn try_parse_rumor_direct(
    event: &nostr::Event,
    receiver_keys: &nostr::Keys,
) -> Option<Message> {
    let content = &event.content;

    let seal_plaintext = nip44::decrypt(
        receiver_keys.secret_key(),
        &event.pubkey,
        content,
    )
    .ok()?;

    let seal: serde_json::Value = serde_json::from_str(&seal_plaintext).ok()?;
    let rumor_content = seal.get("content")?.as_str()?;
    let rumor: serde_json::Value = serde_json::from_str(rumor_content).ok()?;
    let msg_content = rumor.get("content")?.as_str()?;
    Message::from_json(msg_content).ok()
}

fn handle_restore_data(message: &Message, keys: &keys::MostroKeys) {
    let kind = message.get_inner_message_kind();
    let payload = match &kind.payload {
        Some(MostroPayload::RestoreData(info)) => info,
        _ => {
            log::debug!("RestoreSession response missing RestoreData payload");
            return;
        }
    };

    let mut count = 0;
    for order_info in &payload.restore_orders {
        let _trade_key = match keys.get_trade_key_by_index(order_info.trade_index as u32) {
            Ok(k) => k,
            Err(e) => {
                log::warn!(
                    "Failed to derive trade key for index {}: {e}",
                    order_info.trade_index
                );
                continue;
            }
        };

        let status = status_from_daemon(&order_info.status);

        let existing = trade_store::find_by_order_id(&order_info.order_id.to_string());

        let trade = if let Some(mut t) = existing {
            let updated = trade_store::apply_status(&t, status);
            t.status = updated.status;
            t.updated_at = updated.updated_at;
            t.trade_index = Some(order_info.trade_index as u32);
            t
        } else {
            Trade {
                order_id: order_info.order_id.to_string(),
                d_tag: String::new(),
                maker_pubkey: String::new(),
                maker_trade_pubkey: None,
                counterparty_pubkey: None,
                solver_pubkey: None,
                last_request_id: None,
                role: TradeRole::Taker,
                kind: String::new(),
                fiat_amount: String::new(),
                fiat_code: String::new(),
                sats_amount: None,
                premium: 0.0,
                payment_methods: vec![],
                status,
                created_at: crate::platform::timestamp::now_secs() as i64,
                updated_at: crate::platform::timestamp::now_secs() as i64,
                trade_index: Some(order_info.trade_index as u32),
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
        };

        trade_store::upsert(trade);
        count += 1;
    }

    for dispute_info in &payload.restore_disputes {
        if let Some(mut t) = trade_store::find_by_order_id(&dispute_info.order_id.to_string()) {
            t.dispute_id = Some(dispute_info.dispute_id.to_string());
            if let Some(ref solver) = dispute_info.solver_pubkey {
                t.solver_pubkey = Some(solver.clone());
            }
            t.status = TradeStatus::Dispute;
            t.updated_at = crate::platform::timestamp::now_secs() as i64;
            trade_store::upsert(t);
            count += 1;
        }
    }

    if let Some(max_idx) = payload
        .restore_orders
        .iter()
        .map(|o| o.trade_index as u32)
        .max()
    {
        if let Some(mut k) = keys::try_get() {
            let _ = k.sync_trade_index(max_idx);
            keys::write_back_trade_index(k.trade_index);
        }
    }

    *RESTORE_STATE.write() = RestoreState {
        stage: RestoreStage::Done,
        restored_count: count,
        last_error: None,
    };
    persist_state();

    log::info!("Restored {count} Mostro trades");

    enrich_from_order_events();
    request_order_details(payload.restore_orders.iter().map(|o| o.order_id).collect());
}

fn handle_last_trade_index(message: &Message) {
    let kind = message.get_inner_message_kind();
    let remote_index = kind.trade_index() as u32;
    if remote_index == 0 {
        return;
    }

    if let Some(mut k) = keys::try_get() {
        if let Err(e) = k.sync_trade_index(remote_index) {
            log::warn!("Failed to sync trade index from LastTradeIndex: {e}");
        } else {
            keys::write_back_trade_index(k.trade_index);
            log::info!("Synced trade index to {remote_index} from daemon");
        }
    }
}

fn request_order_details(order_ids: Vec<uuid::Uuid>) {
    if order_ids.is_empty() {
        return;
    }
    let mostro_keys = match keys::try_get() {
        Some(k) => k,
        None => return,
    };
    let node = match node_config::try_get() {
        Some(n) => n,
        None => return,
    };
    let node_pk = match PublicKey::from_hex(&node.pubkey) {
        Ok(pk) => pk,
        Err(_) => return,
    };

    let message = flow::request_orders(order_ids);
    let identity = mostro_keys.identity_keys.clone();
    let relays = node.relays.clone();
    let pow = node.pow;

    spawn(async move {
        if let Err(e) = send_mostro_message(
            &message,
            &identity,
            &identity,
            node_pk,
            &relays,
            pow,
        )
        .await
        {
            log::warn!("Failed to send Stage 2 Orders request: {e}");
        }
    });
}

#[allow(dead_code)]
pub async fn handle_orders_event(
    event: &nostr::Event,
    identity_keys: &nostr::Keys,
) -> bool {
    let message = match unwrap_mostro_response(event, identity_keys).await {
        Ok(Some(u)) => u.message,
        _ => return false,
    };

    let action = message.inner_action().unwrap_or(MostroAction::CantDo);
    if action != MostroAction::Orders {
        return false;
    }

    handle_orders_response(&message);
    true
}

fn handle_orders_response(message: &Message) {
    let kind = message.get_inner_message_kind();
    let orders = match &kind.payload {
        Some(MostroPayload::Orders(small_orders)) => small_orders,
        _ => {
            log::debug!("Orders response missing Orders payload");
            return;
        }
    };

    let mostro_keys = match keys::try_get() {
        Some(k) => k,
        None => return,
    };
    let identity_pk = mostro_keys.identity_keys.public_key().to_hex();

    let mut enriched = 0;
    for small_order in orders {
        let order_id = match small_order.id {
            Some(id) => id.to_string(),
            None => continue,
        };
        let Some(mut trade) = trade_store::find_by_order_id(&order_id) else {
            continue;
        };

        if trade.kind.is_empty() {
            if let Some(ref k) = small_order.kind {
                trade.kind = k.to_string();
            }
        }
        if trade.fiat_code.is_empty() {
            trade.fiat_code = small_order.fiat_code.clone();
        }
        if trade.fiat_amount.is_empty() {
            trade.fiat_amount = small_order.fiat_amount.to_string();
        }
        if trade.sats_amount.is_none() && small_order.amount > 0 {
            trade.sats_amount = Some(small_order.amount);
        }
        if trade.maker_pubkey.is_empty() {
            if let Some(ref buyer_pk) = small_order.buyer_trade_pubkey {
                if let Some(ref seller_pk) = small_order.seller_trade_pubkey {
                    trade.maker_pubkey = if small_order.kind == Some(mostro_core::order::Kind::Sell) {
                        seller_pk.clone()
                    } else {
                        buyer_pk.clone()
                    };
                }
            }
        }
        if trade.payment_methods.is_empty() {
            trade.payment_methods = vec![small_order.payment_method.clone()];
        }
        if trade.premium == 0.0 && small_order.premium != 0 {
            trade.premium = small_order.premium as f64 / 100.0;
        }
        if let Some(min) = small_order.min_amount {
            trade.min_fiat = Some(min as f64);
        }
        if let Some(max) = small_order.max_amount {
            trade.max_fiat = Some(max as f64);
        }

        let role = derive_role(&identity_pk, small_order);
        if role != trade.role {
            trade.role = role;
        }

        trade_store::upsert(trade);
        enriched += 1;
    }

    if enriched > 0 {
        spawn(async move {
            let _ = trade_store::publish().await;
        });
        log::info!("Stage 2 enriched {enriched} trades from Orders response");
    }
}

fn derive_role(identity_pk: &str, order: &mostro_core::prelude::SmallOrder) -> TradeRole {
    let is_buyer = order
        .buyer_trade_pubkey
        .as_ref()
        .is_some_and(|pk| pk == identity_pk);

    match order.kind {
        Some(mostro_core::order::Kind::Buy) => {
            if is_buyer {
                TradeRole::Maker
            } else {
                TradeRole::Taker
            }
        }
        Some(mostro_core::order::Kind::Sell) => {
            if is_buyer {
                TradeRole::Taker
            } else {
                TradeRole::Maker
            }
        }
        _ => TradeRole::Taker,
    }
}

/// After restoring trades from the daemon (which returns minimal data),
/// fetch the corresponding kind 38383 events from relays to fill in
/// missing details (kind, fiat_amount, currency, payment_methods, etc.).
fn enrich_from_order_events() {
    let trades = trade_store::TRADES.read().clone();
    let incomplete: Vec<&Trade> = trades
        .iter()
        .filter(|t| t.kind.is_empty() || t.fiat_amount.is_empty())
        .collect();
    if incomplete.is_empty() {
        return;
    }

    let order_ids: Vec<String> = incomplete
        .iter()
        .filter_map(|t| {
            if uuid::Uuid::parse_str(&t.order_id).is_ok() {
                Some(t.order_id.clone())
            } else {
                None
            }
        })
        .collect();
    if order_ids.is_empty() {
        return;
    }

    let ids_for_spawn = order_ids;
    spawn(async move {
        let mut enriched = 0;

        let mut filter = Filter::new()
            .kind(nostr::Kind::Custom(38383))
            .limit(ids_for_spawn.len());
        for oid in &ids_for_spawn {
            filter = filter.identifier(oid);
        }

        let events = match crate::stores::nostr_client::fetch_events_from_relays(
            filter,
            std::time::Duration::from_secs(10),
        )
        .await
        {
            Ok(events) => events,
            Err(e) => {
                log::warn!("Failed to batch-fetch order events for enrichment: {e}");
                return;
            }
        };

        for event in events.iter() {
            if let Ok(order) = crate::utils::nip69::parse_p2p_order(event) {
                if let Some(mut trade) = trade_store::find_by_order_id(&order.order_id) {
                    if trade.kind.is_empty() {
                        trade.kind = order.order_type.as_str().to_string();
                    }
                    if trade.fiat_amount.is_empty() {
                        trade.fiat_amount = match &order.fiat_amount {
                            crate::utils::nip69::FiatAmount::Fixed(amt) => format!("{amt}"),
                            crate::utils::nip69::FiatAmount::Range { min, max } => {
                                format!("{min}-{max}")
                            }
                        };
                    }
                    if trade.fiat_code.is_empty() {
                        trade.fiat_code = order.currency.clone();
                    }
                    if trade.maker_pubkey.is_empty() {
                        trade.maker_pubkey = order.pubkey.clone();
                    }
                    if trade.payment_methods.is_empty() {
                        trade.payment_methods = order.payment_methods.clone();
                    }
                    if trade.sats_amount.is_none() && order.amount_sats > 0 {
                        trade.sats_amount = Some(order.amount_sats as i64);
                    }
                    if trade.d_tag.is_empty() {
                        trade.d_tag = order.order_id.clone();
                    }
                    trade_store::upsert(trade);
                    enriched += 1;
                }
            }
        }

        if enriched > 0 {
            let _ = trade_store::publish().await;
            log::info!("Enriched {enriched} restored trades from order events");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_from_daemon_pending() {
        assert_eq!(status_from_daemon("pending"), TradeStatus::Pending);
    }

    #[test]
    fn test_status_from_daemon_active() {
        assert_eq!(status_from_daemon("active"), TradeStatus::Active);
    }

    #[test]
    fn test_status_from_daemon_fiat_sent() {
        assert_eq!(status_from_daemon("fiat-sent"), TradeStatus::FiatSent);
    }

    #[test]
    fn test_status_from_daemon_success() {
        assert_eq!(status_from_daemon("success"), TradeStatus::Success);
    }

    #[test]
    fn test_status_from_daemon_canceled() {
        assert_eq!(status_from_daemon("canceled"), TradeStatus::Canceled);
    }

    #[test]
    fn test_status_from_daemon_dispute() {
        assert_eq!(status_from_daemon("dispute"), TradeStatus::Dispute);
    }

    #[test]
    fn test_status_from_daemon_payment_failed() {
        assert_eq!(
            status_from_daemon("payment-failed"),
            TradeStatus::PaymentFailed
        );
    }

    #[test]
    fn test_status_from_daemon_waiting_buyer_invoice() {
        assert_eq!(
            status_from_daemon("waiting-buyer-invoice"),
            TradeStatus::WaitingBuyerInvoice
        );
    }

    #[test]
    fn test_status_from_daemon_waiting_payment() {
        assert_eq!(
            status_from_daemon("waiting-payment"),
            TradeStatus::WaitingSellerToPay
        );
    }

    #[test]
    fn test_status_from_daemon_unknown_falls_back_to_initiated() {
        assert_eq!(status_from_daemon("something-unknown"), TradeStatus::Pending);
    }

    #[test]
    fn test_status_from_daemon_cooperatively_canceled() {
        assert_eq!(
            status_from_daemon("cooperatively-canceled"),
            TradeStatus::CooperativelyCanceled
        );
    }

    #[test]
    fn test_status_from_daemon_canceled_by_admin() {
        assert_eq!(
            status_from_daemon("canceled-by-admin"),
            TradeStatus::CanceledByAdmin
        );
    }

    #[test]
    fn test_status_from_daemon_waiting_taker_bond() {
        assert_eq!(
            status_from_daemon("waiting-taker-bond"),
            TradeStatus::WaitingTakerBond
        );
    }

    #[test]
    fn test_status_from_daemon_in_progress_maps_to_active() {
        assert_eq!(status_from_daemon("in-progress"), TradeStatus::Active);
    }

    #[test]
    fn test_status_from_daemon_completed_by_admin_maps_to_success() {
        assert_eq!(
            status_from_daemon("completed-by-admin"),
            TradeStatus::Success
        );
    }

    #[test]
    fn test_status_from_daemon_settled_by_admin_maps_to_success() {
        assert_eq!(
            status_from_daemon("settled-by-admin"),
            TradeStatus::Success
        );
    }

    #[test]
    fn test_restore_state_default() {
        let state = RestoreState::default();
        assert_eq!(state.stage, RestoreStage::Idle);
        assert_eq!(state.restored_count, 0);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn test_restore_state_serde_roundtrip() {
        let state = RestoreState {
            stage: RestoreStage::Done,
            restored_count: 5,
            last_error: Some("timeout".to_string()),
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: RestoreState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.stage, RestoreStage::Done);
        assert_eq!(parsed.restored_count, 5);
        assert_eq!(parsed.last_error, Some("timeout".to_string()));
    }
}
