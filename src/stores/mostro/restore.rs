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
use nostr_relay_pool::relay::ReqExitPolicy;
use nostr_relay_pool::SubscribeAutoCloseOptions;
use serde::{Deserialize, Serialize};
use std::result::Result;

use super::client::{resolve_effective_pow, send_mostro_message, unwrap_mostro_response};
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
    /// Phase 3.2 (M6): Unix-secs timestamp of the last transition into
    /// `WaitingResponse`. Used by the timeout watchdog to detect races
    /// with subsequent retry attempts (the watchdog captures this value
    /// when spawned and only fails the restore if the timestamp still
    /// matches when the timeout fires).
    #[serde(default)]
    pub started_at: i64,
}

impl Default for RestoreState {
    fn default() -> Self {
        Self {
            stage: RestoreStage::Idle,
            restored_count: 0,
            last_error: None,
            started_at: 0,
        }
    }
}

pub static RESTORE_STATE: GlobalSignal<RestoreState> = Signal::global(RestoreState::default);

pub fn init_from_cache() {
    if let Ok(state) = crate::platform::storage::get::<String>(RESTORE_STATE_KEY) {
        if let Ok(mut parsed) = serde_json::from_str::<RestoreState>(&state) {
            // A restore that was interrupted mid-flight (SendingRequest /
            // WaitingResponse) when the app was closed can NEVER complete
            // on the next boot: the reply listener is live-only (it does
            // not backfill events published while we were closed), and the
            // 30s timeout watchdog died with the old process. Leaving the
            // stage as in-progress would keep `is_restore_in_progress()`
            // true forever, permanently disabling the Take/Create buttons
            // (take.rs:71, take_mostro_button.rs:31) and breaking the
            // auto-trigger (mod.rs/home.rs break on `!= Idle`). Coerce to
            // Idle so the boot poll re-fires a fresh restore.
            if matches!(
                parsed.stage,
                RestoreStage::SendingRequest | RestoreStage::WaitingResponse
            ) {
                log::warn!(
                    "Mostro restore was interrupted at {:?} — resetting to Idle for re-trigger",
                    parsed.stage
                );
                parsed = RestoreState {
                    stage: RestoreStage::Idle,
                    restored_count: parsed.restored_count,
                    last_error: parsed.last_error.take(),
                    started_at: 0,
                };
                persist_state();
            }
            *RESTORE_STATE.write() = parsed;
        }
    }
}

pub fn reset() {
    let _ = crate::platform::storage::delete(RESTORE_STATE_KEY);
    *RESTORE_STATE.write() = RestoreState::default();
}

/// E5: True if a restore is currently in flight (sending or waiting for
/// the daemon's reply). Used by `take_order` / `new_order` callers to
/// refuse creating a new trade while the trade-index counter might be
/// advanced by the restore sync. See `take.rs::take_order` for the
/// rationale.
#[allow(dead_code)]
pub fn is_restore_in_progress() -> bool {
    matches!(
        RESTORE_STATE.read().stage,
        RestoreStage::SendingRequest | RestoreStage::WaitingResponse
    )
}

fn persist_state() {
    let json = serde_json::to_string(&*RESTORE_STATE.read()).unwrap_or_default();
    let _ = crate::platform::storage::set(RESTORE_STATE_KEY, &json);
}

/// Map a daemon-reported status string to our internal `TradeStatus`.
///
/// Phase 2.2 (M1/M2/M3) corrections:
/// - `"in-progress"` now maps to `Active` (was incorrectly `Dispute`).
///   Per `mostro-core/src/order.rs`, `InProgress` is the live-trade state
///   BETWEEN `Active` and `FiatSent`, distinct from `Dispute`. The previous
///   mapping surfaced a false "in dispute" badge for active trades.
/// - `"settled-by-admin"` now maps to `Settled` (was `Success`) to stay
///   consistent with `apply_mostro_action`'s `AdminSettled → Settled`
///   mapping (which intentionally keeps the trade non-terminal so a
///   subsequent `PaymentFailed` can still apply).
/// - `"waiting-maker-bond"` now maps to `WaitingMakerBond` (was collapsed
///   to `WaitingBond`).
/// - Unknown statuses preserve the existing status (was hardcoded to
///   `Pending`, which could regress an active trade if the daemon adds a
///   new status we don't recognize yet).
fn status_from_daemon(s: &str, existing: Option<TradeStatus>) -> TradeStatus {
    match s {
        "pending" => TradeStatus::Pending,
        "waiting-buyer-invoice" => TradeStatus::WaitingBuyerInvoice,
        "waiting-payment" => TradeStatus::WaitingSellerToPay,
        "active" | "in-progress" => TradeStatus::Active,
        "fiat-sent" => TradeStatus::FiatSent,
        "settled-hold-invoice" | "settled-by-admin" => TradeStatus::Settled,
        "success" | "completed-by-admin" => TradeStatus::Success,
        "canceled" => TradeStatus::Canceled,
        "cooperatively-canceled" => TradeStatus::CooperativelyCanceled,
        "canceled-by-admin" => TradeStatus::CanceledByAdmin,
        "expired" => TradeStatus::Expired,
        "dispute" => TradeStatus::Dispute,
        "payment-failed" => TradeStatus::PaymentFailed,
        "waiting-taker-bond" => TradeStatus::WaitingTakerBond,
        "waiting-maker-bond" => TradeStatus::WaitingMakerBond,
        _ => {
            log::warn!("Unknown Mostro status from daemon: {s:?}; preserving existing");
            existing.unwrap_or(TradeStatus::Pending)
        }
    }
}

pub async fn request_restore() -> Result<(), String> {
    request_restore_with_retry(3).await
}

pub async fn request_last_trade_index() -> Result<(), String> {
    let mostro_keys = keys::try_get().ok_or("Mostro keys not initialized")?;
    let node = node_config::try_get().ok_or("Mostro node not configured")?;
    let node_pk = PublicKey::from_hex(&node.pubkey)
        .map_err(|e| format!("Invalid node pubkey: {e}"))?;
    let message = flow::last_trade_index();
    let pow = resolve_effective_pow(&node, node_pk).await;
    send_mostro_message(
        &message,
        &mostro_keys.identity_keys,
        &mostro_keys.identity_keys,
        node_pk,
        &node.relays,
        pow,
    )
    .await
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
                        started_at: 0,
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
        started_at: 0,
    };
    persist_state();

    let message = flow::restore_session();

    let pow = resolve_effective_pow(&node, node_pk).await;

    send_mostro_message(
        &message,
        &mostro_keys.identity_keys,
        &mostro_keys.identity_keys,
        node_pk,
        &node.relays,
        pow,
    )
    .await?;

    // M5: defer LastTradeIndex until after RestoreSession has had time to
    // process. The daemon processes messages in order, but if both arrive
    // simultaneously the LTI response may reflect a stale trade index.
    // A 3-second delay gives the daemon time to process the restore first.
    crate::platform::timer::sleep(std::time::Duration::from_secs(3)).await;

    let lti_message = flow::last_trade_index();
    let _ = send_mostro_message(
        &lti_message,
        &mostro_keys.identity_keys,
        &mostro_keys.identity_keys,
        node_pk,
        &node.relays,
        pow,
    )
    .await;

    // Phase 3.2 (M6): capture the started_at timestamp so the watchdog can
    // distinguish this attempt from a subsequent retry (the watchdog
    // captures the value when spawned and only fails the restore if it
    // still matches when the timeout fires).
    let now = crate::platform::timestamp::now_secs() as i64;
    *RESTORE_STATE.write() = RestoreState {
        stage: RestoreStage::WaitingResponse,
        restored_count: 0,
        last_error: None,
        started_at: now,
    };
    persist_state();

    // Spawn a 30s watchdog. If the daemon never replies, the user gets a
    // retry CTA instead of an indefinite "Restoring..." spinner.
    spawn_restore_timeout_watchdog(now);

    Ok(())
}

/// Bounded `limit` for the one-shot restore-reply fetch fallback. Gift-wrap
/// `created_at` is randomized (NIP-59), so we fetch by recency-count rather
/// than a time window.
const RESTORE_FALLBACK_LIMIT: usize = 50;

/// One-shot fetch fallback for the restore reply, invoked by the watchdog
/// when the live session subscription hasn't delivered the daemon's reply
/// within 30s. The notification dispatcher rides a bounded broadcast
/// channel and logs-and-continues on `Lagged` with **no recovery** — under
/// burst load (e.g. initial feed backfill at boot, when restore auto-fires)
/// it can silently drop the daemon's reply even though the event is stored
/// on the relay. This fetches it explicitly.
///
/// Gift-wrap `created_at` is randomized (NIP-59), so we deliberately do NOT
/// use `.since(...)` — a bounded `limit` retrieves the most recent daemon
/// replies addressed to the identity key, and we process any
/// restore/orders/last-trade-index payload among them. Processing is
/// idempotent: re-handling `RestoreData` re-upserts the same trades and
/// flips the stage to `Done`.
async fn fetch_restore_reply_fallback(identity_pk: PublicKey) -> bool {
    let Some(client) = crate::stores::nostr_client::get_client() else {
        return false;
    };
    let Some(node) = node_config::try_get() else {
        return false;
    };
    let Ok(daemon_pk) = super::helpers::parse_node_pubkey(&node.pubkey) else {
        return false;
    };
    let urls: Vec<nostr::Url> = node
        .relays
        .iter()
        .filter_map(|u| nostr::Url::parse(u).ok())
        .collect();
    if urls.is_empty() {
        return false;
    }
    let transport = node.transport();
    let filter = match transport {
        Transport::GiftWrap => Filter::new()
            .kind(nostr::Kind::GiftWrap)
            .pubkey(identity_pk)
            .limit(RESTORE_FALLBACK_LIMIT),
        Transport::Nip44Direct => Filter::new()
            .kind(nostr::Kind::PrivateDirectMessage)
            .author(daemon_pk)
            .pubkey(identity_pk)
            .limit(RESTORE_FALLBACK_LIMIT),
    };
    let identity_keys = match keys::try_get() {
        Some(k) => k.identity_keys.clone(),
        None => return false,
    };
    match client
        .fetch_events_from(urls, filter, std::time::Duration::from_secs(10))
        .await
    {
        Ok(events) => {
            let mut processed = false;
            for event in events.into_iter() {
                // Early-exit: once restore has reached `Done`, skip the
                // remaining (CPU-intensive NIP-44 gift-wrap decryptions).
                // The reply has been found; later events can't advance the
                // stage further and only burn WASM CPU.
                if RESTORE_STATE.read().stage == RestoreStage::Done {
                    break;
                }
                if handle_restore_event(&event, &identity_keys).await {
                    processed = true;
                }
            }
            if processed {
                log::info!("Mostro restore reply recovered via fetch fallback");
            }
            processed
        }
        Err(e) => {
            log::warn!("Mostro restore fetch fallback failed: {e}");
            false
        }
    }
}

/// Phase 3.2 (M6): spawn a 30-second watchdog that transitions the restore
/// state to `Failed` if it's still `WaitingResponse` with the same
/// `started_at` value when the timeout fires.
///
/// The `started_at` check prevents the watchdog from incorrectly failing
/// a subsequent retry attempt (which would have a fresh `started_at`).
///
/// Before declaring `Failed`, the watchdog attempts a one-shot fetch for
/// the reply (see `fetch_restore_reply_fallback`) to recover from a
/// broadcast-lag drop in the notification dispatcher.
fn spawn_restore_timeout_watchdog(expected_started_at: i64) {
    dioxus_core::spawn_forever(async move {
        crate::platform::timer::sleep(std::time::Duration::from_secs(30)).await;
        let current = RESTORE_STATE.read().clone();
        if current.stage == RestoreStage::WaitingResponse
            && current.started_at == expected_started_at
        {
            // Try to recover the reply via a direct fetch before giving up.
            // If found, `handle_restore_data` has already flipped the stage
            // to `Done`.
            if let Some(pk) = keys::try_get().map(|k| k.identity_keys.public_key()) {
                let _ = fetch_restore_reply_fallback(pk).await;
            }
            let after = RESTORE_STATE.read().clone();
            if after.stage == RestoreStage::WaitingResponse
                && after.started_at == expected_started_at
            {
                log::warn!(
                    "Restore response timed out after 30s (+fallback); transitioning to Failed"
                );
                *RESTORE_STATE.write() = RestoreState {
                    stage: RestoreStage::Failed,
                    restored_count: 0,
                    last_error: Some(
                        "Restore timed out — the daemon did not reply within 30s. Tap retry."
                            .to_string(),
                    ),
                    started_at: expected_started_at,
                };
                persist_state();
            }
        }
    });
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
        // Phase 3.3 (Correction 3): bounds-check the i64 trade index
        // before casting to u32. The daemon's `RestoredOrdersInfo`
        // (`mostro-core/src/message.rs:485-493`) carries `trade_index: i64`,
        // which in practice is always a small positive integer, but a
        // negative value (sentinel or bug) would silently become a huge
        // `u32` via `as u32` — corrupting the derived trade key and
        // producing a gift-wrap subscription to a wrong pubkey.
        if order_info.trade_index < 0 {
            log::warn!(
                "Skipping restored order {}: negative trade_index {} from daemon",
                order_info.order_id,
                order_info.trade_index
            );
            continue;
        }
        let idx_u32 = order_info.trade_index as u32;

        let trade_key = match keys.get_trade_key_by_index(idx_u32) {
            Ok(k) => k,
            Err(e) => {
                log::warn!(
                    "Failed to derive trade key for index {}: {e}",
                    order_info.trade_index
                );
                continue;
            }
        };

        let existing = trade_store::find_by_order_id(&order_info.order_id.to_string());
        // Phase 2.2 (M3): pass the existing status as a fallback for unknown
        // daemon statuses (was hardcoded to Pending, which could regress an
        // active trade if the daemon adds a status we don't recognize).
        let status = status_from_daemon(
            &order_info.status,
            existing.as_ref().map(|t| t.status),
        );

        // Phase 3.3 (M9/M10): the daemon's RestoreSessionInfo doesn't
        // carry `payment_failed_attempts`/`payment_failed_retries_interval`
        // or `bond_slashed_at`/`bond_payout_deadline` — those come only
        // from live `PaymentFailed`/`BondSlashed`/`AddBondInvoice` events.
        // The existing-trade path preserves them automatically (it only
        // updates `status`/`updated_at`/`trade_index`). The fresh-Trade
        // path hardcodes them to `None`, which is correct because we have
        // no source for them yet. They'll be populated by subsequent live
        // events as the background monitor catches up.
        //
        // Phase 3.3: also populate `my_trade_pubkey` from the derived trade
        // key so the Phase 1.5 background monitor picks up these restored
        // trades (it routes gift wraps via `my_trade_pubkey` first, then
        // falls back to deriving from `trade_index`). Previously the
        // restore path derived the key but discarded it (assigned to
        // `_trade_key`), so restored trades lacked `my_trade_pubkey` and
        // were unreachable in privacy mode (where `trade_index` is None).
        let restored_trade_pubkey = trade_key.public_key().to_hex();

        let trade = if let Some(mut t) = existing {
            let updated = trade_store::apply_status(&t, status);
            t.status = updated.status;
            t.updated_at = updated.updated_at;
            t.trade_index = Some(idx_u32);
            // Backfill my_trade_pubkey if missing (Phase 3.3).
            if t.my_trade_pubkey.is_none() {
                t.my_trade_pubkey = Some(restored_trade_pubkey.clone());
            }
            t
        } else {
            Trade {
                order_id: order_info.order_id.to_string(),
                d_tag: String::new(),
                maker_pubkey: String::new(),
                my_trade_pubkey: Some(restored_trade_pubkey.clone()),
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
                trade_index: Some(idx_u32),
                pending_hold_invoice: None,
                my_payout_invoice: None,
                needs_bond_invoice: false,
                needs_bond_payout: false,
                note: None,
                min_fiat: None,
                max_fiat: None,
                dispute_id: None,
                // Phase 3.3 (M9/M10): not in RestoreSessionInfo; populated
                // by subsequent live events as the monitor catches up.
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
                daemon_pubkey: node_config::try_get()
                    .map(|c| c.pubkey)
                    .unwrap_or_default(),
                expires_at: None,
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

    // Bounds-check the i64 trade index before casting to u32 (mirrors the
    // per-order guard above and the reference daemon's own check at
    // `mostro/src/db.rs:934`). A negative value from the daemon would wrap to
    // a huge `u32` via `as u32`, and `max()` would pick it — corrupting
    // `sync_trade_index` and every subsequently-derived trade key.
    if let Some(max_idx) = payload
        .restore_orders
        .iter()
        .filter(|o| o.trade_index >= 0)
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
        started_at: 0,
    };
    persist_state();

    log::info!("Restored {count} Mostro trades");

    enrich_from_order_events();
    request_order_details(payload.restore_orders.iter().map(|o| o.order_id).collect());
}

fn handle_last_trade_index(message: &Message) {
    let kind = message.get_inner_message_kind();
    let remote_raw = kind.trade_index();
    // Guard against a negative index before casting (the daemon's
    // `users.last_trade_index` is `i64`; a negative sentinel/bug would wrap
    // to `u32::MAX` and poison trade-key allocation). Mirrors the per-order
    // guard in `apply_restore_payload` and the daemon's own `db.rs:934` check.
    if remote_raw < 0 {
        log::warn!(
            "Ignoring negative LastTradeIndex {} from daemon",
            remote_raw
        );
        return;
    }
    let remote_index = remote_raw as u32;

    if let Some(mut k) = keys::try_get() {
        if let Err(e) = k.sync_trade_index(remote_index) {
            log::warn!("Failed to sync trade index from LastTradeIndex: {e}");
        } else {
            keys::write_back_trade_index(k.trade_index);
            log::info!(
                "Synced trade index to {} (remote was {remote_index})",
                k.trade_index
            );
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
    let node_clone = node.clone();

    spawn(async move {
        let pow = resolve_effective_pow(&node_clone, node_pk).await;
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
            // Surface to the user via the background toast queue so they
            // know enrichment failed and their restored trades may be
            // missing fields (sats amount, payment methods, etc.).
            super::enqueue_background_toast(
                "Mostro restore".to_string(),
                format!(
                    "Could not fetch full order details: {e}. \
                     Trades restored with limited info."
                ),
            );
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

    let enriched = merge_small_orders(orders);
    if enriched > 0 {
        spawn(async move {
            let _ = trade_store::publish().await;
        });
        log::info!("Stage 2 enriched {enriched} trades from Orders response");
    }
}

/// C5: merge a list of `SmallOrder` records (from a daemon `Orders`
/// response) into the local `TRADES` cache. Public so `apply_mostro_action`
/// can route inbound `Action::Orders` payloads through the same logic the
/// restore pipeline uses — letting a future "refresh my trades" button
/// work outside the restore context.
///
/// For each SmallOrder, finds the matching local trade by `order_id` and
/// fills in fields the daemon knows but the local cache may lack
/// (kind, fiat_code, fiat_amount, sats, payment_methods, premium, role,
/// counterparty_pubkey). Local-only fields (my_trade_pubkey, dispute_id,
/// etc.) are preserved. Unknown orders are silently skipped.
///
/// Returns the count of trades enriched. Caller is responsible for
/// triggering a NIP-78 publish if the return is > 0.
#[allow(dead_code)]
pub fn merge_small_orders(orders: &[mostro_core::prelude::SmallOrder]) -> usize {
    let mostro_keys = match keys::try_get() {
        Some(k) => k,
        None => return 0,
    };

    let mut enriched = 0;
    for small_order in orders {
        let order_id = match small_order.id {
            Some(id) => id.to_string(),
            None => continue,
        };
        let mut trade = match trade_store::find_by_order_id(&order_id) {
            Some(t) => t,
            // Create-on-miss: the daemon returned an order we own (Orders is
            // identity-gated, restore is master-key-gated) but have no local
            // record for — the record was lost (e.g. a client-side data bug)
            // or this is a targeted recovery. Reconstruct it from the
            // SmallOrder, deriving our trade_index by matching our derived
            // trade keys against the daemon-reported pubkeys. The field-
            // fill logic below is idempotent for pre-filled fields.
            None => create_trade_from_small_order(&mostro_keys, small_order),
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

        let role = derive_role(&mostro_keys, trade.trade_index, small_order);
        if role != trade.role {
            trade.role = role;
        }

        if trade.counterparty_pubkey.is_none() {
            trade.counterparty_pubkey = derive_counterparty(&role, small_order);
        }

        trade_store::upsert(trade);
        enriched += 1;
    }
    enriched
}

fn derive_role(
    mostro_keys: &keys::MostroKeys,
    trade_index: Option<u32>,
    order: &mostro_core::prelude::SmallOrder,
) -> TradeRole {
    // Derive OUR trade pubkey for this trade. `get_trade_key_by_index`
    // returns the identity key in privacy mode (which is exactly what the
    // daemon recorded as the buyer/seller trade pubkey in that mode), so a
    // single code path handles both modes. When the index is unknown we
    // fall back to the identity key.
    //
    // Bug fix: this previously compared the daemon-returned
    // `buyer_trade_pubkey` (a per-trade derived key, NIP-06 index >= 1)
    // against the IDENTITY key (index 0). Those never match in normal
    // mode, so every buy-order maker was mislabeled `Taker` on restore
    // Stage 2 enrichment (`is_buyer` was always false → Buy branch →
    // Taker). Sell-order makers were correct only by coincidence (the
    // not-buyer arm). Deriving the real trade key fixes both.
    let my_pk = match trade_index {
        Some(idx) => match mostro_keys.get_trade_key_by_index(idx) {
            Ok(k) => k.public_key().to_hex(),
            Err(_) => mostro_keys.identity_keys.public_key().to_hex(),
        },
        None => mostro_keys.identity_keys.public_key().to_hex(),
    };
    let is_buyer = order
        .buyer_trade_pubkey
        .as_deref()
        .is_some_and(|pk| pk == my_pk);

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

fn derive_counterparty(
    role: &TradeRole,
    order: &mostro_core::prelude::SmallOrder,
) -> Option<String> {
    let buyer_pk = order.buyer_trade_pubkey.as_ref();
    let seller_pk = order.seller_trade_pubkey.as_ref();

    let is_buy_order = matches!(order.kind, Some(mostro_core::order::Kind::Buy));

    match role {
        TradeRole::Maker => {
            if is_buy_order {
                seller_pk.cloned()
            } else {
                buyer_pk.cloned()
            }
        }
        TradeRole::Taker => {
            if is_buy_order {
                buyer_pk.cloned()
            } else {
                seller_pk.cloned()
            }
        }
    }
}

/// Determine which trade-index WE used for `order` by scanning our derived
/// trade keys (indices `1..trade_index`; index 0 is the identity key) and
/// matching their pubkeys against the daemon-reported buyer/seller trade
/// pubkeys. Returns `None` in privacy mode (identity key IS the trade key,
/// no per-trade index) or when no derived key matches (e.g. the mnemonic
/// changed, or we don't actually own this order).
fn derive_my_trade_index(
    mostro_keys: &keys::MostroKeys,
    order: &mostro_core::prelude::SmallOrder,
) -> Option<u32> {
    if mostro_keys.privacy_mode {
        return None;
    }
    let buyer = order.buyer_trade_pubkey.as_deref();
    let seller = order.seller_trade_pubkey.as_deref();
    for idx in 1..mostro_keys.trade_index {
        if let Ok(tk) = mostro_keys.get_trade_key_by_index(idx) {
            let pk = tk.public_key().to_hex();
            if Some(pk.as_str()) == buyer || Some(pk.as_str()) == seller {
                return Some(idx);
            }
        }
    }
    None
}

/// Reconstruct a fresh `Trade` from a daemon `SmallOrder` for the
/// create-on-miss path. Used by [`merge_small_orders`] (restore Stage 2 +
/// inbound `Action::Orders`) and by [`recover_order_by_id`]. Derives
/// `trade_index` and role so the record is immediately actionable (e.g.
/// cancel via the recovered maker trade key).
fn create_trade_from_small_order(
    mostro_keys: &keys::MostroKeys,
    small_order: &mostro_core::prelude::SmallOrder,
) -> Trade {
    let my_idx = derive_my_trade_index(mostro_keys, small_order);
    let role = derive_role(mostro_keys, my_idx, small_order);
    let my_trade_pubkey = match my_idx {
        Some(idx) => mostro_keys
            .get_trade_key_by_index(idx)
            .ok()
            .map(|k| k.public_key().to_hex()),
        None if mostro_keys.privacy_mode => {
            Some(mostro_keys.identity_keys.public_key().to_hex())
        }
        None => None,
    };
    let now = crate::platform::timestamp::now_secs() as i64;
    let status_str = small_order
        .status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "pending".to_string());
    let mut trade = Trade {
        order_id: small_order.id.map(|i| i.to_string()).unwrap_or_default(),
        d_tag: String::new(),
        maker_pubkey: String::new(),
        my_trade_pubkey,
        counterparty_pubkey: derive_counterparty(&role, small_order),
        solver_pubkey: None,
        last_request_id: None,
        role,
        kind: small_order.kind.map(|k| k.to_string()).unwrap_or_default(),
        fiat_amount: small_order.fiat_amount.to_string(),
        fiat_code: small_order.fiat_code.clone(),
        sats_amount: if small_order.amount > 0 {
            Some(small_order.amount)
        } else {
            None
        },
        premium: if small_order.premium != 0 {
            small_order.premium as f64 / 100.0
        } else {
            0.0
        },
        payment_methods: vec![small_order.payment_method.clone()],
        status: status_from_daemon(&status_str, None),
        created_at: now,
        updated_at: now,
        trade_index: my_idx,
        pending_hold_invoice: None,
        my_payout_invoice: None,
        needs_bond_invoice: false,
        needs_bond_payout: false,
        note: None,
        min_fiat: small_order.min_amount.map(|m| m as f64),
        max_fiat: small_order.max_amount.map(|m| m as f64),
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
        daemon_pubkey: node_config::try_get()
            .map(|c| c.pubkey)
            .unwrap_or_default(),
        expires_at: None,
    };
    // maker_pubkey: the daemon records the maker's trade pubkey in
    // buyer_trade_pubkey (buy order) or seller_trade_pubkey (sell order).
    if let (Some(ref buyer_pk), Some(ref seller_pk)) = (
        &small_order.buyer_trade_pubkey,
        &small_order.seller_trade_pubkey,
    ) {
        trade.maker_pubkey = if small_order.kind == Some(mostro_core::order::Kind::Sell) {
            seller_pk.clone()
        } else {
            buyer_pk.clone()
        };
    }
    trade
}

/// Recover a single order by ID via the daemon's `Action::Orders`
/// (identity-gated, NO status filter — returns the order in ANY state as
/// long as the requester's master identity is on the order). Reconstructs a
/// local `Trade` record that was lost (e.g. after a client-side data bug),
/// letting the user view/cancel a listing they own but can no longer see.
///
/// Uses the proven request/reply pattern from `create_order.rs` +
/// mostro-cli's `wait_for_dm`: **subscribe before send** (so the
/// subscription is open when the reply lands), then wait on the
/// notification stream racing a timeout. The daemon replies to
/// `event.sender` (the rumor author = the identity key here), so the
/// filter is `#p=identity`. Surfaces `CantDo::NotFound` distinctly from a
/// timeout/relay problem.
#[allow(dead_code)]
pub async fn recover_order_by_id(order_id: uuid::Uuid) -> Result<usize, String> {
    let mostro_keys = keys::try_get().ok_or("Mostro keys not initialized")?;
    let node = node_config::try_get().ok_or("Mostro node not configured")?;
    let daemon_pk = super::helpers::parse_node_pubkey(&node.pubkey)
        .map_err(|e| format!("Invalid daemon pubkey: {e}"))?;
    let Some(client) = crate::stores::nostr_client::get_client() else {
        return Err("Nostr client not ready".to_string());
    };
    let urls: Vec<nostr::Url> = node
        .relays
        .iter()
        .filter_map(|u| nostr::Url::parse(u).ok())
        .collect();
    if urls.is_empty() {
        return Err("No daemon relays configured".to_string());
    }

    // Daemon relays must be pool members + connected for subscribe/send.
    super::client::ensure_node_relays_connected().await;

    let identity_pk = mostro_keys.identity_keys.public_key();
    let transport = node.transport();
    // Reply filter: transport-aware, #p=identity (matches the rumor author).
    // NO authors pin on GiftWrap (the outer wrap is signed by a one-off
    // ephemeral key, never the daemon); authors=daemon only for v2 kind-14
    // (load-bearing — disambiguates from NIP-17 peer chat).
    let reply_filter = match transport {
        Transport::GiftWrap => Filter::new()
            .kind(nostr::Kind::GiftWrap)
            .pubkey(identity_pk)
            .limit(0),
        Transport::Nip44Direct => Filter::new()
            .kind(nostr::Kind::PrivateDirectMessage)
            .author(daemon_pk)
            .pubkey(identity_pk)
            .limit(0),
    };

    // 1. Subscribe BEFORE send (race-free). WaitForEventsAfterEOSE keeps the
    //    sub open for the first live event after EOSE (the reply).
    let auto_close = SubscribeAutoCloseOptions::default()
        .exit_policy(ReqExitPolicy::WaitForEventsAfterEOSE(1))
        .timeout(Some(std::time::Duration::from_secs(20)));
    let sub_id = client
        .subscribe_to(urls, reply_filter, Some(auto_close))
        .await
        .map_err(|e| format!("subscribe failed: {e}"))?;

    // 2. Send Orders-by-ID (identity as rumor author → reply gift-wrapped to
    //    the identity key).
    let message = flow::request_orders(vec![order_id]);
    let pow = resolve_effective_pow(&node, daemon_pk).await;
    if let Err(e) = send_mostro_message(
        &message,
        &mostro_keys.identity_keys,
        &mostro_keys.identity_keys,
        daemon_pk,
        &node.relays,
        pow,
    )
    .await
    {
        let _ = client.unsubscribe(&sub_id).await;
        return Err(e);
    }

    // 3. Wait for the reply on the notification stream (race vs 20s timeout).
    let identity_keys = mostro_keys.identity_keys.clone();
    let mut notifications = client.notifications();
    let listen_fut = async {
        loop {
            match notifications.recv().await {
                Ok(nostr_sdk::RelayPoolNotification::Event { event, .. }) => {
                    if event.kind != nostr::Kind::GiftWrap
                        && event.kind != nostr::Kind::PrivateDirectMessage
                    {
                        continue;
                    }
                    if !event.tags.public_keys().any(|pk| *pk == identity_pk) {
                        continue;
                    }
                    let unwrapped = match unwrap_mostro_response(&event, &identity_keys).await {
                        Ok(Some(u)) => u,
                        _ => continue,
                    };
                    let action = unwrapped
                        .message
                        .inner_action()
                        .unwrap_or(MostroAction::CantDo);
                    log::info!("recover_order_by_id: received daemon reply action={action:?}");
                    return Some(unwrapped);
                }
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    };
    let timeout_fut = crate::platform::timer::sleep(std::time::Duration::from_secs(20));
    futures::pin_mut!(listen_fut, timeout_fut);
    let outcome = futures::future::select(listen_fut, timeout_fut).await;
    // Always clean up the subscription.
    let _ = client.unsubscribe(&sub_id).await;

    let unwrapped = match outcome {
        futures::future::Either::Left((Some(u), _)) => u,
        futures::future::Either::Left((None, _)) => {
            return Err(
                "Notification stream closed before the daemon replied — retry.".to_string(),
            );
        }
        futures::future::Either::Right(_) => {
            return Err(
                "Daemon did not reply within 20s — the request may not have reached it \
                 (relay/PoW/transport). Retry, or verify your Mostro daemon connection."
                    .to_string(),
            );
        }
    };

    // 4. Dispatch on action.
    let action = unwrapped
        .message
        .inner_action()
        .unwrap_or(MostroAction::CantDo);
    match action {
        MostroAction::Orders => {
            // create-on-miss rebuilds the trade from the SmallOrder.
            handle_orders_response(&unwrapped.message);
            if trade_store::find_by_order_id(&order_id.to_string()).is_some() {
                log::info!("Recovered Mostro order {order_id} by ID");
                Ok(1)
            } else {
                Err(
                    "Daemon replied Orders but it did not contain this order id.".to_string(),
                )
            }
        }
        MostroAction::CantDo => {
            let kind = unwrapped.message.get_inner_message_kind();
            let reason = match &kind.payload {
                Some(MostroPayload::CantDo(r)) => r.clone(),
                _ => None,
            };
            match reason {
                Some(CantDoReason::NotFound) => Err(
                    "The daemon doesn't recognize your Mostro identity as the owner of this \
                     order. Your Mostro mnemonic may have changed (site data cleared / different \
                     browser) — the order was created under a different identity."
                        .to_string(),
                ),
                Some(other) => Err(format!("Daemon refused: {other:?}")),
                None => Err("Daemon refused (CantDo, no reason).".to_string()),
            }
        }
        other => Err(format!("Unexpected daemon reply: {other:?}")),
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

        // Phase 3.1 (C5): use `identifiers(vec)` (plural) instead of a loop
        // of `.identifier(oid)` (singular). The nostr-sdk `Filter` builder
        // returns a new `Self` on each call, so the loop form silently
        // overwrites the d-tag value on each iteration — only the LAST
        // order id survived in the filter, and multi-trade restores only
        // enriched one trade per batch.
        //
        // Verified API surface: `Filter::identifiers<I, S>(self, I)` at
        // `/home/patrick/nostr/crates/nostr/src/filter.rs:690`.
        let filter = Filter::new()
            .kind(nostr::Kind::Custom(38383))
            .identifiers(ids_for_spawn.clone())
            .limit(ids_for_spawn.len());

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
                    if trade.expires_at.is_none() {
                        trade.expires_at = order.expires_at.map(|t| t as i64);
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

    /// Build a `MostroKeys` from the canonical test mnemonic, with a
    /// trade_index high enough that indices 1..=3 are "used".
    fn test_keys(privacy_mode: bool) -> keys::MostroKeys {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let id = crate::utils::nip06::derive_at(words, None, keys::MOSTRO_ACCOUNT, 0, 0).unwrap();
        keys::MostroKeys {
            mnemonic: crate::utils::zeroize_string::ZeroizeString(words.to_string()),
            trade_index: 5,
            identity_keys: id,
            privacy_mode,
        }
    }

    #[test]
    fn test_derive_role_buy_order_maker() {
        // Regression: the OLD derive_role compared buyer_trade_pubkey (a
        // per-trade derived key, NIP-06 index >= 1) against the IDENTITY key
        // (index 0), which never matches → buy-order makers were mislabeled
        // Taker on restore Stage 2 enrichment.
        let mk = test_keys(false);
        let maker_pk = mk.get_trade_key_by_index(1).unwrap().public_key().to_hex();
        let order = mostro_core::prelude::SmallOrder {
            kind: Some(mostro_core::order::Kind::Buy),
            buyer_trade_pubkey: Some(maker_pk),
            ..Default::default()
        };
        assert_eq!(
            derive_role(&mk, Some(1), &order),
            TradeRole::Maker,
            "buy-order maker (we are the buyer) must derive Maker"
        );
    }

    #[test]
    fn test_derive_role_sell_order_maker() {
        let mk = test_keys(false);
        let maker_pk = mk.get_trade_key_by_index(1).unwrap().public_key().to_hex();
        let order = mostro_core::prelude::SmallOrder {
            kind: Some(mostro_core::order::Kind::Sell),
            seller_trade_pubkey: Some(maker_pk),
            ..Default::default()
        };
        assert_eq!(
            derive_role(&mk, Some(1), &order),
            TradeRole::Maker,
            "sell-order maker (we are the seller) must derive Maker"
        );
    }

    #[test]
    fn test_derive_role_taker_of_sell() {
        let mk = test_keys(false);
        let our_pk = mk.get_trade_key_by_index(2).unwrap().public_key().to_hex();
        let order = mostro_core::prelude::SmallOrder {
            kind: Some(mostro_core::order::Kind::Sell),
            buyer_trade_pubkey: Some(our_pk), // we're the buyer → taker
            seller_trade_pubkey: Some("a".repeat(64)),
            ..Default::default()
        };
        assert_eq!(
            derive_role(&mk, Some(2), &order),
            TradeRole::Taker,
            "taking a sell order (we are the buyer) must derive Taker"
        );
    }

    #[test]
    fn test_derive_role_privacy_mode_uses_identity_key() {
        // Privacy mode: trade_index is None on the wire; the identity key IS
        // the trade key. derive_role must fall back to the identity key and
        // still derive the correct role.
        let mk = test_keys(true);
        let id_pk = mk.identity_keys.public_key().to_hex();
        let order = mostro_core::prelude::SmallOrder {
            kind: Some(mostro_core::order::Kind::Buy),
            buyer_trade_pubkey: Some(id_pk),
            ..Default::default()
        };
        assert_eq!(derive_role(&mk, None, &order), TradeRole::Maker);
    }

    #[test]
    fn test_derive_my_trade_index_finds_matching_key() {
        let mk = test_keys(false);
        let pk_at_3 = mk.get_trade_key_by_index(3).unwrap().public_key().to_hex();
        let order = mostro_core::prelude::SmallOrder {
            seller_trade_pubkey: Some(pk_at_3),
            ..Default::default()
        };
        assert_eq!(derive_my_trade_index(&mk, &order), Some(3));
    }

    #[test]
    fn test_derive_my_trade_index_none_when_not_ours() {
        let mk = test_keys(false);
        let order = mostro_core::prelude::SmallOrder {
            buyer_trade_pubkey: Some("9".repeat(64)),
            ..Default::default()
        };
        assert_eq!(derive_my_trade_index(&mk, &order), None);
    }

    #[test]
    fn test_derive_my_trade_index_privacy_mode_none() {
        let mk = test_keys(true);
        let id_pk = mk.identity_keys.public_key().to_hex();
        let order = mostro_core::prelude::SmallOrder {
            buyer_trade_pubkey: Some(id_pk),
            ..Default::default()
        };
        assert_eq!(derive_my_trade_index(&mk, &order), None);
    }

    #[test]
    fn test_status_from_daemon_pending() {
        assert_eq!(status_from_daemon("pending", None), TradeStatus::Pending);
    }

    #[test]
    fn test_status_from_daemon_active() {
        assert_eq!(status_from_daemon("active", None), TradeStatus::Active);
    }

    #[test]
    fn test_status_from_daemon_fiat_sent() {
        assert_eq!(
            status_from_daemon("fiat-sent", None),
            TradeStatus::FiatSent
        );
    }

    #[test]
    fn test_status_from_daemon_success() {
        assert_eq!(status_from_daemon("success", None), TradeStatus::Success);
    }

    #[test]
    fn test_status_from_daemon_canceled() {
        assert_eq!(status_from_daemon("canceled", None), TradeStatus::Canceled);
    }

    #[test]
    fn test_status_from_daemon_dispute() {
        assert_eq!(status_from_daemon("dispute", None), TradeStatus::Dispute);
    }

    #[test]
    fn test_status_from_daemon_payment_failed() {
        assert_eq!(
            status_from_daemon("payment-failed", None),
            TradeStatus::PaymentFailed
        );
    }

    #[test]
    fn test_status_from_daemon_waiting_buyer_invoice() {
        assert_eq!(
            status_from_daemon("waiting-buyer-invoice", None),
            TradeStatus::WaitingBuyerInvoice
        );
    }

    #[test]
    fn test_status_from_daemon_waiting_payment() {
        assert_eq!(
            status_from_daemon("waiting-payment", None),
            TradeStatus::WaitingSellerToPay
        );
    }

    /// Phase 2.2 (M3): unknown statuses preserve the existing trade status
    /// rather than regressing to Pending. This avoids surfacing false
    /// "Pending" badges if the daemon adds a new status we don't recognize.
    #[test]
    fn test_status_from_daemon_unknown_preserves_existing() {
        assert_eq!(
            status_from_daemon("something-unknown", Some(TradeStatus::Active)),
            TradeStatus::Active,
            "unknown status with existing must preserve existing"
        );
        assert_eq!(
            status_from_daemon("something-unknown", None),
            TradeStatus::Pending,
            "unknown status with no existing falls back to Pending"
        );
    }

    #[test]
    fn test_status_from_daemon_cooperatively_canceled() {
        assert_eq!(
            status_from_daemon("cooperatively-canceled", None),
            TradeStatus::CooperativelyCanceled
        );
    }

    #[test]
    fn test_status_from_daemon_canceled_by_admin() {
        assert_eq!(
            status_from_daemon("canceled-by-admin", None),
            TradeStatus::CanceledByAdmin
        );
    }

    #[test]
    fn test_status_from_daemon_waiting_taker_bond() {
        assert_eq!(
            status_from_daemon("waiting-taker-bond", None),
            TradeStatus::WaitingTakerBond
        );
    }

    /// Phase 2.2 (M1): `"in-progress"` is the live-trade state BETWEEN
    /// Active and FiatSent, NOT Dispute. The previous mapping surfaced a
    /// false "in dispute" badge for active trades on restore.
    #[test]
    fn test_status_from_daemon_in_progress_maps_to_active() {
        assert_eq!(
            status_from_daemon("in-progress", None),
            TradeStatus::Active,
            "in-progress must map to Active, not Dispute"
        );
    }

    /// Phase 2.2: `"completed-by-admin"` continues to map to `Success`
    /// (admin completed a successful trade — terminal).
    #[test]
    fn test_status_from_daemon_completed_by_admin_maps_to_success() {
        assert_eq!(
            status_from_daemon("completed-by-admin", None),
            TradeStatus::Success
        );
    }

    /// Phase 2.2 (M2): `"settled-by-admin"` now maps to `Settled` (was
    /// `Success`) to stay consistent with `apply_mostro_action`'s
    /// `AdminSettled → Settled` mapping. `Settled` is intentionally
    /// non-terminal so a subsequent `PaymentFailed` from the daemon's
    /// `do_payment` step can still transition the trade correctly. If we
    /// jumped straight to terminal `Success` here, the action-driven
    /// update and the restore-driven update would land on different
    /// statuses.
    #[test]
    fn test_status_from_daemon_settled_by_admin_maps_to_settled() {
        assert_eq!(
            status_from_daemon("settled-by-admin", None),
            TradeStatus::Settled,
            "settled-by-admin must map to Settled, not Success"
        );
    }

    /// Phase 2.2: `"settled-hold-invoice"` (the canonical "seller released,
    /// payout in flight" status) continues to map to `Settled`.
    #[test]
    fn test_status_from_daemon_settled_hold_invoice() {
        assert_eq!(
            status_from_daemon("settled-hold-invoice", None),
            TradeStatus::Settled
        );
    }

    /// Phase 2.2: `"waiting-maker-bond"` now maps to `WaitingMakerBond`
    /// (was collapsed to `WaitingBond`, losing the maker/taker
    /// distinction).
    #[test]
    fn test_status_from_daemon_waiting_maker_bond() {
        assert_eq!(
            status_from_daemon("waiting-maker-bond", None),
            TradeStatus::WaitingMakerBond,
            "waiting-maker-bond must preserve the maker distinction"
        );
    }

    #[test]
    fn test_restore_state_default() {
        let state = RestoreState::default();
        assert_eq!(state.stage, RestoreStage::Idle);
        assert_eq!(state.restored_count, 0);
        assert!(state.last_error.is_none());
        assert_eq!(state.started_at, 0);
    }

    #[test]
    fn test_restore_state_serde_roundtrip() {
        let state = RestoreState {
            stage: RestoreStage::Done,
            restored_count: 5,
            last_error: Some("timeout".to_string()),
            started_at: 1_700_000_000,
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: RestoreState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.stage, RestoreStage::Done);
        assert_eq!(parsed.restored_count, 5);
        assert_eq!(parsed.last_error, Some("timeout".to_string()));
        assert_eq!(parsed.started_at, 1_700_000_000);
    }
}
