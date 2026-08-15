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
use std::sync::atomic::{AtomicBool, Ordering};

use crate::stores::publish_queue::{self, types::QueueEventType};
use crate::stores::nostr_client;

/// Send a Mostro `Message` to a daemon.
///
/// The message is wrapped (NIP-59 gift-wrap for v1 daemons, NIP-44 direct
/// for v2 daemons) using `mostro_core::transport::wrap_message_with`, which
/// dispatches on the resolved [`Transport`]. The transport is looked up from
/// the persisted `MOSTRO_NODE_CONFIG` for the given `node_pubkey`, defaulting
/// to gift-wrap when the daemon's `protocol_version` is unknown or 1 — this
/// preserves the pre-v2 behaviour for all existing daemons.
///
/// For NIP-44 direct (v2), a NIP-40 `expiration` tag is mandatory on the
/// wire (mostro `TRANSPORT_V2_SPEC`); when the caller passes `None` we
/// stamp a 30-day default (`dm_days`), matching mostrix and the daemon.
///
/// The wrapped event is enqueued in the publish queue with
/// `QueueEventType::Mostro` (labeled "Mostro Sync" in the queue UI — it is
/// daemon protocol traffic, not a user-authored DM). In privacy mode, the
/// caller passes the SAME `Keys` for both `identity_keys` and `trade_keys`.
#[allow(dead_code)]
pub async fn send_mostro_message(
    message: &Message,
    identity_keys: &nostr::Keys,
    trade_keys: &nostr::Keys,
    node_pubkey: PublicKey,
    node_relays: &[String],
    pow: u8,
) -> Result<(), String> {
    // Resolve the wire transport + per-action PoW from the current node
    // config. The active daemon's pubkey must match `node_pubkey`; if it
    // doesn't (e.g. mid switch), fall back to gift-wrap + the caller's `pow`
    // so we never silently mis-pair a v2 daemon onto the v1 path.
    let (transport, effective_pow) = match super::node_config::try_get() {
        Some(cfg)
            if matches_pubkey(&cfg.pubkey, &node_pubkey) =>
        {
            // Phase 2d: per-action PoW policy. On a v2 transport, first-contact
            // actions (NewOrder/TakeBuy/TakeSell) use max(pow, pow_first_contact)
            // — matching the daemon's spam-gate lanes (mostro spam_gate.rs,
            // mostrix nostr_pow_for_protocol_dm). The daemon doesn't advertise
            // pow_first_contact today, so this falls back to `pow` in practice.
            let action = &message.get_inner_message_kind().action;
            (cfg.transport(), cfg.effective_pow_for_action(action))
        }
        _ => (Transport::GiftWrap, pow),
    };
    // The caller-resolved `pow` is the trusted baseline (it may have just
    // fetched a fresh value via resolve_effective_pow). Use the action-aware
    // value only when it exceeds the baseline — never grind less than the
    // daemon's advertised base `pow`.
    let pow = effective_pow.max(pow);

    // For NIP-44 direct, NIP-40 expiration is mandatory. Default to 30 days
    // (mostro `dm_days`) when the caller didn't specify one. Gift-wrap has
    // no such requirement; leave it `None` to preserve existing behaviour.
    let expiration = if transport == Transport::Nip44Direct {
        Some(default_dm_expiration())
    } else {
        None
    };

    let opts = WrapOptions {
        pow,
        expiration,
        signed: true,
    };

    let event = mostro_core::transport::wrap_message_with(
        transport,
        message,
        identity_keys,
        trade_keys,
        node_pubkey,
        opts,
    )
    .await
    .map_err(|e| format!("mostro {transport} wrap failed: {e}"))?;

    publish_queue::enqueue(
        event,
        QueueEventType::Mostro,
        Some(node_relays.to_vec()),
        HashMap::new(),
    )
    .await;
    Ok(())
}

/// Default NIP-40 expiration stamped on v2 (kind-14) direct messages when
/// the caller doesn't supply one: 30 days, matching mostro's `dm_days` and
/// mostrix's `default_dm_expiration`. Keeps kind-14 events from lingering on
/// relays forever.
fn default_dm_expiration() -> Timestamp {
    Timestamp::from_secs(crate::platform::timestamp::now_secs().saturating_add(30 * 86_400))
}

/// True if `cfg_pubkey_str` (hex or npub) refers to the same key as
/// `node_pubkey`. Used to confirm the active config matches the daemon we're
/// about to address before trusting its `protocol_version`.
fn matches_pubkey(cfg_pubkey_str: &str, node_pubkey: &PublicKey) -> bool {
    if cfg_pubkey_str.is_empty() {
        return false;
    }
    PublicKey::from_hex(cfg_pubkey_str)
        .or_else(|_| PublicKey::from_bech32(cfg_pubkey_str))
        .map(|pk| &pk == node_pubkey)
        .unwrap_or(false)
}

/// Try to unwrap an incoming Mostro envelope addressed to `receiver_keys`.
///
/// Dispatches on the event kind via `mostro_core::transport::unwrap_incoming`:
/// kind 1059 → NIP-59 gift-wrap unwrap; kind 14 → NIP-44 direct unwrap (the
/// v2 transport). Both yield the same transport-agnostic `UnwrappedMessage`.
/// Other kinds return `Ok(None)` so callers polling multiple candidate keys
/// can skip non-Mostro traffic (e.g. NIP-17 peer chat that also uses kind 14)
/// without logging spurious errors.
///
/// Returns `Ok(None)` when the envelope is not addressed to this key (NIP-44
/// decrypt fails) — the expected "not for me" signal when polling multiple
/// candidate trade keys. Returns `Err(_)` on structural problems (invalid
/// JSON, signature mismatch, malformed tuple, etc.).
///
/// Logs an informational note if the incoming message carries a protocol
/// version newer than the one we've audited; the wire types are additive so
/// processing continues regardless. See the Bug #12 checklist above.
#[allow(dead_code)]
pub async fn unwrap_mostro_response(
    event: &Event,
    receiver_keys: &nostr::Keys,
) -> Result<Option<UnwrappedMessage>, String> {
    // Only attempt Mostro transports (1059 gift-wrap, 14 nip44-direct). Other
    // kinds — including kind 14 events from NIP-17 peer chat, which must be
    // handled by the dedicated chat path — are "not for us" here.
    if event.kind != NostrKind::GiftWrap && event.kind != NostrKind::PrivateDirectMessage {
        return Ok(None);
    }
    let unwrapped = mostro_core::transport::unwrap_incoming(event, receiver_keys)
        .await
        .map_err(|e| format!("mostro unwrap failed: {e}"))?;

    // Protocol version check. mostro-core's PROTOCOL_VER is pub(crate) so we
    // can't import it — hardcode the current expected version here. Update
    // this constant when bumping the mostro-core dependency.
    //
    // Bug #12 fix: when bumping mostro-core, verify:
    //   1. EXPECTED_PROTOCOL_VER in this file matches the new PROTOCOL_VER.
    //   2. New Action variants are handled in apply_mostro_action (client.rs).
    //   3. New Payload variants are handled in apply_mostro_action.
    //   4. New CantDoReason variants are translated in helpers::cant_do_message.
    //   5. Run `cargo test stores::mostro::client::tests::test_conformance_every_action_status_matches_golden_table`
    //      and update the golden table if new actions were added.
    // Protocol version check. mostro-core's PROTOCOL_VER is pub(crate) so we
    // can't import it — track the current expected version here. Update this
    // constant when bumping the mostro-core dependency.
    //
    // Both `1` and `2` are legitimate inbound: gift-wrap (v1) daemons downgrade
    // their replies to version 1 server-side (`stamp_protocol_version`), while
    // NIP-44-direct (v2) daemons and our own outbound stamp version 2. So this
    // is informational only — the wire types are additive and we process
    // regardless. The constant exists to detect a future mostro-core bump that
    // stamps a version we haven't audited.
    //
    // Bug #12 fix: when bumping mostro-core, verify:
    //   1. EXPECTED_PROTOCOL_VER in this file tracks the new PROTOCOL_VER.
    //   2. New Action variants are handled in apply_mostro_action (client.rs).
    //   3. New Payload variants are handled in apply_mostro_action.
    //   4. New CantDoReason variants are translated in helpers::cant_do_message.
    //   5. Run `cargo test stores::mostro::client::tests::test_conformance_every_action_status_matches_golden_table`
    //      and update the golden table if new actions were added.
    if let Some(ref msg) = unwrapped {
        const EXPECTED_PROTOCOL_VER: u8 = 2;
        let incoming_ver = msg.message.get_inner_message_kind().version;
        // v1 (gift-wrap daemon replies) and v2 (nip44-direct + our own) are
        // both expected. Only flag genuinely unknown future versions.
        if incoming_ver > EXPECTED_PROTOCOL_VER {
            log::info!(
                "Mostro protocol version newer than expected: incoming message \
                 version {} > known {}. Processing anyway (wire types are \
                 additive); audit new variants per Bug #12 checklist.",
                incoming_ver,
                EXPECTED_PROTOCOL_VER
            );
        }
    }

    Ok(unwrapped)
}

/// Resolve the current daemon's wire transport + pubkey from the persisted
/// `MOSTRO_NODE_CONFIG`. Returns `(Transport::GiftWrap, None)` when no config
/// is set yet (pre-login / first run), which keeps every filter on the v1
/// path — safe for all existing daemons.
///
/// Read reactively in component bodies so subscriptions rebuild when the
/// daemon's `protocol_version` changes (transport flip). The background
/// monitor reads it once at startup inside its spawned task.
fn current_transport_and_daemon() -> (Transport, Option<PublicKey>) {
    match super::node_config::try_get() {
        Some(cfg) => {
            let transport = cfg.transport();
            let daemon_pk = PublicKey::from_hex(&cfg.pubkey)
                .or_else(|_| PublicKey::from_bech32(&cfg.pubkey))
                .ok();
            (transport, daemon_pk)
        }
        None => (Transport::GiftWrap, None),
    }
}

/// Pure transport-aware builder for the live DM subscription filter.
///
/// - v1 (GiftWrap): `kinds=[1059]`, `#p=trade_pubkeys`. No authors pin —
///   the outer gift wrap is signed by a throwaway ephemeral key, so the
///   daemon's pubkey is not a useful pre-filter.
/// - v2 (NIP-44 direct): `kinds=[14]`, `authors=[daemon]`, `#p=trade_pubkeys`.
///   The authors pin is LOAD-BEARING: kind 14 is shared with NIP-17 peer chat,
///   so without it nostr.blue's own DM machinery would misparse Mostro replies
///   as peer chat. Mirrors mostrix `filter_protocol_dm_from_mostro` and mobile's
///   `SubscriptionManager`.
///
/// Uses `.limit(0)` for live-only semantics. No `.since()` — gift-wrap
/// `created_at` is randomized (NIP-59), so a cursor would drop new events.
fn dm_live_filter(
    transport: Transport,
    daemon_pubkey: Option<PublicKey>,
    trade_pubkeys: &[PublicKey],
) -> Filter {
    match transport {
        Transport::GiftWrap => Filter::new()
            .kind(NostrKind::GiftWrap)
            .pubkeys(trade_pubkeys.iter().copied())
            .limit(0),
        Transport::Nip44Direct => {
            // The authors pin requires the daemon's pubkey. If it's missing
            // (config not yet loaded), fall back to gift-wrap shape rather
            // than emitting an un-pinned kind-14 filter that would collide
            // with NIP-17 chat.
            match daemon_pubkey {
                Some(daemon) => Filter::new()
                    .kind(NostrKind::PrivateDirectMessage)
                    .author(daemon)
                    .pubkeys(trade_pubkeys.iter().copied())
                    .limit(0),
                None => Filter::new()
                    .kind(NostrKind::GiftWrap)
                    .pubkeys(trade_pubkeys.iter().copied())
                    .limit(0),
            }
        }
    }
}

/// Pure transport-aware builder for the batch backfill filter (adds
/// `.since(last_sync - 3d)` to the live shape; no limit so the one-shot
/// `fetch_events` returns everything in the window).
fn dm_backfill_filter(
    transport: Transport,
    daemon_pubkey: Option<PublicKey>,
    trade_pubkeys: &[PublicKey],
    last_sync_secs: i64,
) -> Filter {
    let since_secs = last_sync_secs.saturating_sub(BACKFILL_SLACK_SECS).max(0);
    dm_live_filter(transport, daemon_pubkey, trade_pubkeys)
        .since(nostr::Timestamp::from(since_secs as u64))
}

/// Build a filter that subscribes to all Mostro DMs addressed to any of the
/// given active trade pubkeys, on the current daemon's transport.
///
/// IMPORTANT: do NOT use `.since(...)` for live gift-wrap subscriptions — the
/// daemon randomizes gift-wrap `created_at` to defeat timing correlation, so
/// `since(now)` won't match new events. We use `.limit(0)` for "new only".
/// `.since()` is only safe on a one-shot `fetch_events` backfill (see
/// [`backfill_filter`]). This filter rebuilds reactively when the daemon's
/// `protocol_version` flips transport (via the `MOSTRO_NODE_CONFIG` signal
/// read inside [`current_transport_and_daemon`]).
#[allow(dead_code)]
pub fn active_trade_filter(trade_pubkeys: &[PublicKey]) -> Filter {
    let (transport, daemon_pk) = current_transport_and_daemon();
    dm_live_filter(transport, daemon_pk, trade_pubkeys)
}

/// Build a filter for kind 38386 dispute events from a daemon.
///
/// Used by the background monitor to pick up dispute status changes
/// (e.g. auto-close on cooperative cancel or release during dispute —
/// see `mostro/src/app/dispute.rs:252-334`). Without this, the local
/// `DISPUTES` cache goes stale after the initial `DisputeInitiatedBy*`
/// action, and the user never sees the dispute resolution status.
#[allow(dead_code)]
pub fn dispute_status_filter(daemon_pubkey: PublicKey) -> Filter {
    Filter::new()
        .kind(NostrKind::Custom(
            super::dispute_store::DISPUTE_EVENT_KIND,
        ))
        .author(daemon_pubkey)
}

/// Local-storage key for the high-water mark of the backfill cursor.
///
/// Updated after each successful backfill to `now_secs()`. Read at the start
/// of each backfill to compute `since = max(0, last_sync - 3 days)`. The 3-day
/// slack covers `nostr::nips::nip59::RANGE_RANDOM_TIMESTAMP_TWEAK = 0..172800`
/// (up to 2 days of `created_at` randomization on gift-wrap envelopes) plus
/// 1 day of margin for relay propagation delay.
const LAST_TRADE_BACKFILL_KEY: &str = "mostro_last_trade_backfill_secs";

/// 3-day slack window for the backfill `since` cursor.
///
/// See `LAST_TRADE_BACKFILL_KEY` doc for rationale.
const BACKFILL_SLACK_SECS: i64 = 3 * 86_400;

/// Build a one-shot backfill filter covering Mostro DMs addressed to any of
/// the given trade pubkeys since `last_sync - 3 days`, on the current
/// daemon's transport.
///
/// Per `active_trade_filter`'s doc, `.since()` is safe for one-shot
/// `fetch_events` calls (where we tolerate re-processing already-seen events
/// via the dedup LRU) but NOT for long-lived subscriptions (where it would
/// drop new events whose randomized `created_at` falls before the cursor).
#[allow(dead_code)]
pub fn backfill_filter(
    trade_pubkeys: &[PublicKey],
    last_sync_secs: i64,
) -> Filter {
    let (transport, daemon_pk) = current_transport_and_daemon();
    dm_backfill_filter(transport, daemon_pk, trade_pubkeys, last_sync_secs)
}

/// Read the persisted backfill cursor from `platform::storage`.
///
/// Returns 0 on first run (full 3-day backfill) or storage failure.
fn read_backfill_cursor() -> i64 {
    crate::platform::storage::get::<i64>(LAST_TRADE_BACKFILL_KEY).unwrap_or(0)
}

/// Persist the backfill cursor to `platform::storage`.
fn write_backfill_cursor(secs: i64) {
    let _ = crate::platform::storage::set(LAST_TRADE_BACKFILL_KEY, &secs);
}

/// Prevents overlapping `backfill_active_trades` invocations. The login
/// monitor (`start_background_trade_monitor`) and the toast-drainer periodic
/// poll (`mostro_toast_drainer`) can fire concurrently; without this guard
/// both would issue duplicate `fetch_events` round-trips for the same window.
static IS_BACKFILLING: AtomicBool = AtomicBool::new(false);

/// RAII guard that releases the backfill flag on drop — covering every return
/// path and panics — so `IS_BACKFILLING` can never get stuck set.
struct BackfillGuard;
impl Drop for BackfillGuard {
    fn drop(&mut self) {
        IS_BACKFILLING.store(false, Ordering::SeqCst);
    }
}

/// Phase 1.6 (M5) one-shot backfill: fetch gift wraps for all active trades
/// covering the window `[last_sync - 3 days, now]` and process them through
/// the same `apply_mostro_action` path used by the live subscription.
///
/// This catches events that arrived while the app was closed or the
/// subscription was unmounted. Without this, the user would miss trade
/// updates that happened between sessions.
///
/// The 3-day window is required because gift-wrap envelopes have randomized
/// `created_at` (see `nostr::nips::nip59::RANGE_RANDOM_TIMESTAMP_TWEAK =
/// 0..172800` — up to 2 days back). Re-processed events are deduped via
/// `dedup::SEEN_EVENTS`.
pub async fn backfill_active_trades() {
    // Short-circuit if a backfill is already running. `swap` returns the
    // previous value; `true` means another caller owns it. The guard resets
    // the flag when this function returns (or panics).
    if IS_BACKFILLING.swap(true, Ordering::SeqCst) {
        log::debug!("backfill_active_trades already in progress; skipping");
        return;
    }
    let _guard = BackfillGuard;

    let key_map = build_trade_key_map();
    if key_map.is_empty() {
        return;
    }
    let all_pks: Vec<PublicKey> = key_map.keys().cloned().collect();
    let last_sync = read_backfill_cursor();
    let filter = backfill_filter(&all_pks, last_sync);

    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return,
    };

    let events = match client
        .fetch_events(filter, std::time::Duration::from_secs(15))
        .await
    {
        Ok(events) => events,
        Err(e) => {
            log::debug!("backfill fetch failed (non-fatal): {e}");
            return;
        }
    };

    let now_secs = crate::platform::timestamp::now_secs() as i64;
    let mut applied = 0usize;
    for event in events.into_iter() {
        // Dedup against the global LRU so we don't double-apply events that
        // were already processed by a prior backfill or the live subscription.
        if super::dedup::is_seen(&event.id) {
            continue;
        }
        super::dedup::mark_seen(event.id);

        let recipient = match event.tags.public_keys().next().cloned() {
            Some(pk) => pk,
            None => continue,
        };
        let (trade_index, order_id) = match key_map.get(&recipient) {
            Some(entry) => entry,
            None => continue,
        };
        let keys_state = super::keys::try_get();
        let keys = match keys_state {
            Some(k) => k,
            None => continue,
        };
        let tk = match keys.get_trade_key_by_index(*trade_index) {
            Ok(k) => k,
            Err(_) => continue,
        };

        let unwrapped = match unwrap_mostro_response(&event, &tk).await {
            Ok(Some(u)) => u,
            Ok(None) => continue,
            Err(_) => continue,
        };
        let action = unwrapped
            .message
            .inner_action()
            .unwrap_or(mostro_core::prelude::Action::CantDo);
        let payload = unwrapped.message.get_inner_message_kind().payload.clone();
        let my_pk_hex = tk.public_key().to_hex();

        let mut trade = match super::trade_store::find_by_order_id(order_id) {
            Some(t) => t,
            None => continue,
        };
        let trade_before = trade.clone();
        let old_status = trade.status;
        let (new_status, toasts) =
            apply_mostro_action(&mut trade, action, &payload, unwrapped.sender, &my_pk_hex);
        // Phase 2.3 (M4/M8): surface toasts from the backfill path too
        // (e.g., a CantDo reason or BondSlashed that arrived while offline).
        for t in toasts {
            let body = t.body.unwrap_or_default();
            super::enqueue_background_toast(t.title, body);
        }
        if let Some(ns) = new_status {
            trade = super::trade_store::apply_status(&trade, ns);
        }
        if trade.status != old_status || trade != trade_before {
            log::info!(
                "backfill: trade {order_id} status {:?} → {:?}",
                old_status,
                trade.status
            );
            super::trade_store::upsert(trade.clone());
            applied += 1;
            if trade.status.is_terminal() {
                super::waiter::prune_waiters_for_order(&trade.order_id);
            }
        }
    }

    if applied > 0 {
        let _ = super::trade_store::publish().await;
    }
    write_backfill_cursor(now_secs);
    log::info!("backfill complete: {} trade(s) updated", applied);
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
    let events = client.fetch_events_from(&urls, filter, std::time::Duration::from_secs(15))
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

/// Determine the effective PoW difficulty to use when sending a Mostro message.
///
/// If the cached `node.pow` is already non-zero we trust it (the live
/// subscription on the P2P home page keeps it fresh via
/// [`super::node_config::update_pow_from_event`]).
///
/// When `node.pow` is `0` (never initialised, e.g. the user went straight
/// to "Create Order" without visiting the P2P home page), we proactively
/// fetch the daemon's kind-38385 info event and read its `pow` tag.
/// On success the cached config is updated so subsequent sends skip the
/// fetch.
///
/// Falls back to `0` on any error (client missing, relay timeout, etc.).
pub async fn resolve_effective_pow(
    node: &super::node_config::MostroNodeConfig,
    node_pk: PublicKey,
) -> u8 {
    if node.pow > 0 {
        return node.pow;
    }
    // Slow path: PoW is unknown (cold start). Ensure daemon relays are in
    // the pool and connected before fetching the kind-38385 info event.
    // Without this, callers like `request_restore_inner` that don't
    // explicitly call `ensure_node_relays_connected` will fail silently —
    // `fetch_events_from` returns `RelayNotFound` for relays not in the pool.
    ensure_node_relays_connected().await;
    match fetch_daemon_pow(node_pk, &node.relays).await {
        Some(fetched) if fetched > 0 => {
            if let Some(mut cfg) = super::node_config::try_get() {
                cfg.pow = fetched;
                let _ = super::save_node_config(cfg).await;
            }
            fetched
        }
        _ => 0,
    }
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

/// Build a one-shot backfill filter for Mostro DMs addressed to a single
/// trade pubkey, on the current daemon's transport. Uses `.since()` +
/// `.limit(200)` to fetch historical events that may have been missed before
/// the live subscription was active.
pub fn active_trade_backfill_filter(
    trade_pubkey: PublicKey,
    since: Timestamp,
) -> Filter {
    let (transport, daemon_pk) = current_transport_and_daemon();
    dm_live_filter(transport, daemon_pk, std::slice::from_ref(&trade_pubkey))
        .since(since)
        .limit(200)
}

/// Build a map from trade pubkey → (trade_index, order_id) for all active trades.
/// Used for O(1) routing of incoming GiftWraps: read the outer `p` tag,
/// look up which trade it belongs to, then unwrap with the correct key.
///
/// Phase 1.5 (C3): previously this skipped trades with `trade_index == None`,
/// which silently disabled background monitoring for ALL privacy-mode trades
/// (in privacy mode, identity == trade key, so `trade_index` is set to `None`
/// by `flow::maybe_trade_index`). The fix is to look up the trade pubkey from
/// the persisted `my_trade_pubkey` field first (set at take/create time),
/// then fall back to deriving from `trade_index` when present.
pub fn build_trade_key_map() -> std::collections::HashMap<PublicKey, (u32, String)> {
    let keys_state = super::keys::try_get();
    let keys = match keys_state {
        Some(k) => k,
        None => return std::collections::HashMap::new(),
    };
    let mut map = std::collections::HashMap::new();
    for trade in super::trade_store::active_trades_for_daemon() {
        // Preferred path: use the persisted trade pubkey (works for both
        // privacy-mode and normal-mode trades, doesn't require derivation).
        if let Some(pk_hex) = trade.my_trade_pubkey.as_ref() {
            if let Ok(pk) = super::helpers::parse_node_pubkey(pk_hex) {
                let idx = trade.trade_index.unwrap_or(0);
                map.insert(pk, (idx, trade.order_id.clone()));
                continue;
            }
        }
        // Fallback: derive from trade_index. Required for restored trades
        // where `my_trade_pubkey` isn't populated yet (Phase 3.3 will fix
        // the restore path to populate it).
        if let Some(idx) = trade.trade_index {
            if let Ok(tk) = keys.get_trade_key_by_index(idx) {
                map.insert(tk.public_key(), (idx, trade.order_id.clone()));
            }
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
///
/// If the monotonicity guard rejects the status transition, all side-effect
/// mutations (pending_hold_invoice, bond_slashed_at, counterparty_pubkey, etc.)
/// are rolled back from the snapshot taken at function entry. This mirrors
/// mostrix's pattern at `dm_utils/mod.rs:876-886` and prevents rejected
/// transitions from leaking field mutations into the persisted trade.
#[allow(clippy::type_complexity)]
pub fn apply_mostro_action(
    trade: &mut super::trade_store::Trade,
    action: mostro_core::prelude::Action,
    payload: &Option<mostro_core::prelude::Payload>,
    _sender: PublicKey,
    my_pk_hex: &str,
) -> (
    Option<super::trade_store::TradeStatus>,
    Vec<super::toast::MostroToast>,
) {
    use mostro_core::prelude::{Action as A, Payload as P};
    use super::toast::MostroToast;
    use super::trade_store::TradeStatus as S;

    // Snapshot for rollback. If the monotonicity guard below rejects the
    // transition, we restore side-effect fields from this snapshot so that
    // rejected transitions don't leak mutations.
    let snapshot = trade.clone();

    let kind = match payload {
        Some(p) => p,
        None => &P::Amount(0),
    };

    let mut toasts: Vec<MostroToast> = Vec::new();

    let status = match action {
        A::AddBondInvoice => {
            let is_payout = matches!(kind, P::BondPayoutRequest(_));
            if let P::BondPayoutRequest(bpr) = kind {
                trade.bond_slashed_at = Some(bpr.slashed_at);
                let window_days = super::node_config::try_get()
                    .map(|n| n.bond_payout_claim_window_days)
                    .unwrap_or(30);
                let deadline = bpr.slashed_at + (window_days as i64 * 86400);
                trade.bond_payout_deadline = Some(deadline);
                trade.needs_bond_payout = true;
                toasts.push(
                    MostroToast::warning("Bond payout claim").body(format!(
                        "Counterparty's bond was slashed. Submit an invoice to claim your share. Deadline: {}",
                        crate::utils::format::format_relative_time_or(deadline as u64, "unknown"),
                    )).duration(std::time::Duration::from_secs(10)),
                );
            }
            trade.needs_bond_invoice = true;
            if is_payout {
                None
            } else {
                // Phase 2.1 (C1, Bug #1 fix): role-aware bond status —
                // single source of truth. Previously `trade_detail.rs`
                // returned the generic `WaitingBond` here, causing the
                // displayed status to differ depending on which
                // subscription (page vs background monitor) delivered
                // the event first.
                match trade.role {
                    super::trade_store::TradeRole::Maker => Some(S::WaitingMakerBond),
                    super::trade_store::TradeRole::Taker => Some(S::WaitingTakerBond),
                }
            }
        }
        A::AddInvoice => {
            match kind {
                P::Order(small_order) => {
                    // Daemon sends Order payload (both initial take-sell flow
                    // and post-payment-retry-exhaustion). Extract the sats
                    // amount so the buyer knows how much to invoice.
                    if small_order.amount > 0 {
                        trade.sats_amount = Some(small_order.amount);
                    }
                    // When the trade is at PaymentFailed (retries exhausted),
                    // keep it there for UI consistency — the TradeActionPanel
                    // already shows an invoice input at PaymentFailed status.
                    // Returning None skips the monotonicity guard entirely,
                    // preserving the sats_amount mutation without rollback.
                    if trade.status == S::PaymentFailed {
                        None
                    } else {
                        Some(S::WaitingBuyerInvoice)
                    }
                }
                P::PaymentRequest(_, bolt11, _) => {
                    trade.pending_hold_invoice = Some(bolt11.clone());
                    Some(S::WaitingBuyerInvoice)
                }
                _ if trade.status == S::PaymentFailed => None,
                _ => Some(S::WaitingBuyerInvoice),
            }
        }
        // `BuyerInvoiceAccepted` is a pure acknowledgment from the daemon
        // that the buyer's payout invoice was accepted. It is currently
        // never emitted by the daemon (no handler in `mostro/src/app/`),
        // but is reserved for future use. It must NOT transition status
        // or stash a payment request.
        A::BuyerInvoiceAccepted => {
            toasts.push(
                MostroToast::info("Invoice accepted")
                    .body("Your payout invoice has been accepted.")
                    .duration(std::time::Duration::from_secs(3)),
            );
            None
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
            match trade.role {
                super::trade_store::TradeRole::Maker => Some(S::WaitingMakerBond),
                super::trade_store::TradeRole::Taker => Some(S::WaitingTakerBond),
            }
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
        A::HoldInvoicePaymentSettled | A::Released => {
            // C2: defense-in-depth dispute auto-close. If this trade had
            // an open dispute, the daemon will republish the kind 38386
            // event with status `settled` (per
            // `docs/DISPUTE_AUTO_CLOSE_ON_USER_RESOLUTION.md`). The
            // dispute monitor at client.rs:1699-1751 picks up the
            // republish eventually, but we advance the local dispute
            // store immediately to close the latency window.
            if let Some(ref did) = trade.dispute_id {
                super::dispute_store::mark_auto_closed_by_release(did);
            }
            Some(S::Settled)
        }
        A::PurchaseCompleted => Some(S::Success),
        A::Canceled | A::HoldInvoicePaymentCanceled => {
            // C2: same defense-in-depth for cancel path.
            if let Some(ref did) = trade.dispute_id {
                super::dispute_store::mark_auto_closed_by_cancel(did);
            }
            if trade.cancel_initiator.is_none() {
                trade.cancel_initiator = Some(super::trade_store::CancelInitiator::Daemon);
            }
            Some(S::Canceled)
        }
        A::CooperativeCancelInitiatedByYou => {
            if trade.cancel_initiator.is_none() {
                trade.cancel_initiator = Some(super::trade_store::CancelInitiator::User);
            }
            toasts.push(
                MostroToast::info("Cancel requested")
                    .body("Waiting for counterparty to accept your cancel request.")
                    .duration(std::time::Duration::from_secs(5)),
            );
            Some(S::CancelPending)
        }
        A::CooperativeCancelInitiatedByPeer => {
            if trade.cancel_initiator.is_none() {
                trade.cancel_initiator = Some(super::trade_store::CancelInitiator::Peer);
            }
            // Caller has the fiat_was_sent context via the trade snapshot
            // — but we can read it from `trade` here since it's already
            // a field on the struct.
            let desc = if trade.fiat_was_sent {
                "Counterparty wants to cancel. Fiat was already sent — accepting will NOT reverse the transfer.".to_string()
            } else {
                "Counterparty wants to cancel the trade. Accept or wait for expiry.".to_string()
            };
            toasts.push(
                MostroToast::warning("Cancel requested")
                    .body(desc)
                    .duration(std::time::Duration::from_secs(7)),
            );
            Some(S::CancelPending)
        }
        A::CooperativeCancelAccepted => {
            // C2: defense-in-depth dispute auto-close on cooperative cancel
            // completion. Mirrors the `Canceled` arm above.
            if let Some(ref did) = trade.dispute_id {
                super::dispute_store::mark_auto_closed_by_cancel(did);
            }
            toasts.push(
                MostroToast::info("Trade canceled")
                    .body("Both parties agreed to cancel the trade.")
                    .duration(std::time::Duration::from_secs(5)),
            );
            Some(S::CooperativelyCanceled)
        }
        A::DisputeInitiatedByYou | A::DisputeInitiatedByPeer => {
            let label = if action == A::DisputeInitiatedByYou {
                "Dispute opened"
            } else {
                "Counterparty opened a dispute"
            };
            if let P::Dispute(dispute_id, _) = kind {
                trade.dispute_id = Some(dispute_id.to_string());
                toasts.push(
                    MostroToast::info(label)
                        .body(format!("Dispute ID: {dispute_id}"))
                        .duration(std::time::Duration::from_secs(5)),
                );
            }
            Some(S::Dispute)
        }
        A::AdminTakeDispute | A::AdminTookDispute => {
            // Solver pubkey arrives inside `Payload::Peer.pubkey` (per
            // mostro-core/src/message.rs:34-40 and the daemon's
            // admin_take_dispute.rs:266-284). The gift-wrap `sender` is the
            // daemon itself, NOT the solver — using it would derive the
            // admin-shared-key against the wrong pubkey and the dispute chat
            // would never decrypt. Fall back to leaving solver_pubkey unset
            // when the Peer payload is absent (safer than a wrong key).
            if let Some(solver_pk) = super::helpers::extract_peer_pubkey(payload) {
                trade.solver_pubkey = Some(solver_pk.to_hex());
            } else {
                log::warn!(
                    "AdminTookDispute for trade {} missing Payload::Peer; \
                     solver_pubkey left unset (was {:#?})",
                    trade.order_id,
                    payload
                );
            }
            toasts.push(
                MostroToast::info("Solver assigned")
                    .body("A solver has been assigned to your dispute.")
                    .duration(std::time::Duration::from_secs(5)),
            );
            Some(S::Dispute)
        }
        A::AdminCanceled => {
            // Phase 3.5 (F15): admin/solver canceled. If bonds are enabled
            // on the daemon, a trailing `Action::BondSlashed` may follow
            // within ~60s; `cleanup_expired` honors `cancel_initiator ==
            // Admin` to defer deletion past that window.
            trade.cancel_initiator = Some(super::trade_store::CancelInitiator::Admin);
            toasts.push(
                MostroToast::info("Admin canceled")
                    .body("An admin has canceled this order.")
                    .duration(std::time::Duration::from_secs(5)),
            );
            Some(S::CanceledByAdmin)
        }
        // Per `mostro/src/app/admin_settle.rs:94-210`: when the daemon sends
        // `AdminSettled`, the order is in `SettledHoldInvoice` (intermediate);
        // `do_payment` runs afterward and either transitions to `Success`
        // (via `PurchaseCompleted`) or emits `PaymentFailed`. Mapping directly
        // to `Success` here would make `Success` (terminal) block the
        // subsequent `PaymentFailed` from being applied.
        A::AdminSettled => {
            toasts.push(
                MostroToast::info("Admin settled")
                    .body("An admin settled the dispute. Payout is in progress — the trade completes once the buyer's invoice is paid.")
                    .duration(std::time::Duration::from_secs(8)),
            );
            Some(S::Settled)
        }
        A::PaymentFailed => {
            if let P::PaymentFailed(info) = kind {
                trade.payment_failed_attempts = Some(info.payment_attempts);
                trade.payment_failed_retries_interval = Some(info.payment_retries_interval);
                toasts.push(
                    MostroToast::error("Payment failed")
                        .body(format!(
                            "Up to {} retries, every {}s",
                            info.payment_attempts, info.payment_retries_interval,
                        ))
                        .duration(std::time::Duration::from_secs(8)),
                );
            }
            Some(S::PaymentFailed)
        }
        A::BondInvoiceAccepted => {
            trade.needs_bond_invoice = false;
            toasts.push(
                MostroToast::info("Bond accepted")
                    .body("Your bond invoice has been accepted.")
                    .duration(std::time::Duration::from_secs(3)),
            );
            None
        }
        A::InvoiceUpdated => {
            if let P::PaymentRequest(_, bolt11, _) = kind {
                trade.pending_hold_invoice = Some(bolt11.clone());
            }
            toasts.push(
                MostroToast::info("Invoice updated")
                    .body("The payment invoice has been updated.")
                    .duration(std::time::Duration::from_secs(5)),
            );
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
        // Phase 2.3 (M4): surface the daemon's refusal to the user via the
        // toast tuple. Previously the reason was silently dropped in the
        // background monitor, leaving the user with no feedback when the
        // daemon rejected an action while the trade-detail page wasn't
        // mounted.
        A::CantDo => {
            let body = if let P::CantDo(Some(reason)) = kind {
                super::helpers::cant_do_message(reason)
            } else {
                "The daemon refused the request.".to_string()
            };
            toasts.push(MostroToast::error("Mostro refused").body(body));
            None
        }
        A::Rate => {
            // The daemon sends `Rate` to the seller AFTER
            // `HoldInvoicePaymentSettled` (release.rs:262-268) but never
            // sends `PurchaseCompleted` to the seller (only to the buyer,
            // release.rs:557-565). Without this inference, the seller's
            // trade would be stuck at `Settled` (non-terminal) forever.
            // The buyer also receives `Rate` (release.rs:592-600) but is
            // already at `Success` via `PurchaseCompleted`, so the
            // `Settled` guard makes this safe for both roles.
            if trade.status == S::Settled {
                toasts.push(
                    MostroToast::info("Rate your counterparty")
                        .body("The trade is complete. Please rate.")
                        .duration(std::time::Duration::from_secs(5)),
                );
                Some(S::Success)
            } else {
                None
            }
        }
        A::RateReceived => {
            // Surface the rating as a background toast so the user knows
            // they were rated by their counterparty. The mobile client maps
            // RateReceived to a notification; we use the in-app toast queue
            // (surfaced by mostro_toast_drainer at the root level).
            if let P::RatingUser(stars) = kind {
                let title = super::i18n::tr("mostro.rate_received_title");
                let body = super::i18n::tr("mostro.rate_received_body")
                    .replace("{stars}", &stars.to_string());
                toasts.push(MostroToast::info(title).body(body));
            }
            None
        }
        // Phase 2.3 (M8) + Phase 3b: record the bond slash and surface a
        // CAUSE-AWARE toast. A `BondSlashed` action means the user's
        // anti-abuse bond was forfeited — either by a solver's dispute
        // decision OR by a waiting-state timeout. The daemon sends an
        // identical payload for both causes (mostro `ANTI_ABUSE_BOND.md`), so
        // we infer the cause from the trade's dispute history, mirroring
        // mobile's `bondSlashCause` helper: if a dispute was opened
        // (`dispute_id` set / status is Dispute), it's a dispute slash;
        // otherwise a timeout slash. The distinction matters for UX —
        // timeout slashes can be prevented by the user acting in time,
        // dispute slashes cannot.
        A::BondSlashed => {
            let now = crate::platform::timestamp::now_secs() as i64;
            trade.bond_slashed_at = Some(now);
            let window_days = super::node_config::try_get()
                .map(|n| n.bond_payout_claim_window_days)
                .unwrap_or(30);
            trade.bond_payout_deadline = Some(now + (window_days as i64 * 86400));
            trade.needs_bond_payout = true;
            trade.needs_bond_invoice = true;
            let amount_hint = if let P::Order(slashed_order) = kind {
                format!("{} sats", slashed_order.amount)
            } else {
                "see trade detail".to_string()
            };
            // Infer cause: dispute if a dispute was involved, else timeout.
            let cause_is_dispute = trade.dispute_id.is_some()
                || matches!(trade.status, S::Dispute);
            let (title, detail) = if cause_is_dispute {
                (
                    "Bond slashed (dispute)",
                    "A solver decided the dispute against you.".to_string(),
                )
            } else {
                (
                    "Bond slashed (timeout)",
                    "You missed a waiting-state deadline.".to_string(),
                )
            };
            toasts.push(
                MostroToast::warning(title).body(format!(
                    "{detail} Your anti-abuse bond ({amount_hint}) was \
                     forfeited. Submit a payout invoice before the claim \
                     window expires (in {window_days} days)."
                )).duration(std::time::Duration::from_secs(8)),
            );
            None
        }
        A::BondPayoutCompleted => {
            trade.needs_bond_payout = false;
            trade.needs_bond_invoice = false;
            toasts.push(
                MostroToast::info("Bond payout complete")
                    .body("Your bond payout has been processed.")
                    .duration(std::time::Duration::from_secs(5)),
            );
            None
        }
        A::TradePubkey => {
            if let P::Peer(peer) = kind {
                if trade.counterparty_pubkey.is_none() {
                    trade.counterparty_pubkey = Some(peer.pubkey.clone());
                }
            }
            None
        }
        // C5: handle `Action::Orders(Payload::Orders(Vec<SmallOrder>))`
        // arriving outside the restore path. The daemon supports this as
        // a general "query my orders" primitive (`app/orders::orders_action`),
        // so a future "refresh my trades" button could trigger it. Routes
        // through the same merge logic the restore pipeline uses
        // (`restore::merge_small_orders`).
        A::Orders => {
            if let P::Orders(small_orders) = kind {
                let count = super::restore::merge_small_orders(small_orders);
                if count > 0 {
                    log::info!("Action::Orders enriched {count} trades");
                    // Publish in the background — fire and forget.
                    let order_id_for_publish = trade.order_id.clone();
                    dioxus::prelude::spawn(async move {
                        let _ = super::trade_store::publish().await;
                        log::debug!(
                            "Published trades after Orders merge for {order_id_for_publish}"
                        );
                    });
                }
            }
            None
        }
        _ => {
            // Unknown Action variant — likely a new mostro-core release (e.g.
            // the Cashu-escrow actions AddCashuEscrow/CashuEscrowLocked/
            // CashuPmSignature). Bumped from debug to info so these are
            // visible without being noisy; the action is silently no-op'd
            // (no status change, no toast) until an explicit handler is added.
            log::info!(
                "Unhandled Mostro action in apply_mostro_action (no-op): {action:?}"
            );
            None
        }
    };

    if let Some(ref new_status) = status {
        // Phase 2.1 (C2): consult the canonical monotonicity predicate
        // (`is_status_transition_allowed`) rather than duplicating the
        // guard here. The predicate has the exception list
        // (Dispute / CancelPending / PaymentFailed / CooperativelyCanceled /
        // CanceledByAdmin) that permits legitimate cross-rank transitions
        // the previous local guard silently dropped — e.g., a
        // `CooperativeCancelInitiatedByPeer` arriving while the trade is
        // `FiatSent` (rank 3 → CancelPending rank 2), or a `Dispute`
        // opened from `FiatSent`.
        //
        // Uses the pure predicate rather than `apply_status` so this code
        // is testable without a Dioxus/wasm-bindgen timestamp context.
        if !super::trade_store::is_status_transition_allowed(&trade.status, new_status) {
            log::debug!(
                "apply_status rejected transition {:?} → {:?} from {action:?} on trade {}",
                trade.status,
                new_status,
                trade.order_id
            );
            // Rollback side-effect mutations from the snapshot so that
            // rejected transitions don't leak field changes (e.g., a stale
            // AddInvoice arriving after Success would otherwise set
            // pending_hold_invoice on a terminal trade). Mirrors mostrix's
            // rollback write at dm_utils/mod.rs:876-886.
            *trade = snapshot;
            return (None, Vec::new());
        }
    }

    (status, toasts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::mostro::trade_store::tests::default_test_trade;
    use crate::stores::mostro::trade_store::TradeStatus;
    use mostro_core::prelude::{Action as A, Payload as P};

    const TEST_PK_HEX: &str = "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390";

    fn test_sender() -> PublicKey {
        PublicKey::from_hex(TEST_PK_HEX).unwrap()
    }

    #[test]
    fn test_node_info_filter_shape() {
        let f = node_info_filter();
        assert!(f.kinds.is_some());
    }

    #[test]
    fn test_order_live_filter_shape() {
        let pk = PublicKey::from_hex(TEST_PK_HEX).unwrap();
        let f = order_live_filter(pk, "test-d-tag");
        assert!(f.kinds.is_some());
    }

    /// Phase 1.6 (M5): `backfill_filter` must include a `since` cursor
    /// (unlike `active_trade_filter` which intentionally omits it — gift
    /// wrap subscriptions need `.limit(0)` instead because of NIP-59's
    /// `created_at` randomization).
    ///
    /// Phase 2d: exercises the pure `dm_backfill_filter` builder directly
    /// (the public `backfill_filter` wrapper reads the GlobalSignal, which
    /// isn't available in a plain `#[test]`).
    #[test]
    fn test_backfill_filter_has_since_cursor() {
        let pk = PublicKey::from_hex(TEST_PK_HEX).unwrap();
        let last_sync = 1_700_000_000_i64; // arbitrary non-zero timestamp
        let f = dm_backfill_filter(Transport::GiftWrap, None, &[pk], last_sync);
        assert!(f.since.is_some(), "backfill_filter must set a since cursor");
        let since = f.since.unwrap().as_u64();
        let expected_min = (last_sync.saturating_sub(BACKFILL_SLACK_SECS)).max(0) as u64;
        assert!(
            since <= expected_min,
            "since ({since}) must be at most last_sync - BACKFILL_SLACK_SECS ({expected_min})"
        );
    }

    /// Phase 1.6 (M5): `active_trade_filter` must NOT include a `since`
    /// cursor (would drop new events whose randomized `created_at` falls
    /// before the cursor). It uses `.limit(0)` for live-only semantics.
    ///
    /// Phase 2d: exercises the pure `dm_live_filter` builder directly.
    #[test]
    fn test_active_trade_filter_omits_since() {
        let pk = PublicKey::from_hex(TEST_PK_HEX).unwrap();
        let f = dm_live_filter(Transport::GiftWrap, None, &[pk]);
        assert!(
            f.since.is_none(),
            "active_trade_filter must NOT set since (gift-wrap created_at is randomized)"
        );
    }

    /// Phase 2d: v2 (NIP-44 direct) live filter must be `kinds=[14]` with an
    /// `authors=[daemon]` pin (load-bearing NIP-17 disambiguation) and the
    /// `#p` trade-pubkey tags.
    #[test]
    fn test_dm_live_filter_v2_pins_daemon_author() {
        let trade = PublicKey::from_hex(TEST_PK_HEX).unwrap();
        let daemon = PublicKey::from_hex(
            "1111111111111111111111111111111111111111111111111111111111111111",
        )
        .unwrap();
        let f = dm_live_filter(Transport::Nip44Direct, Some(daemon), &[trade]);
        let kinds = f.kinds.as_ref().expect("v2 filter must set kinds");
        assert!(
            kinds.iter().any(|k| *k == NostrKind::PrivateDirectMessage),
            "v2 filter must subscribe to kind 14"
        );
        let authors = f.authors.as_ref().expect("v2 filter must pin daemon author");
        assert!(
            authors.iter().any(|a| *a == daemon),
            "v2 filter must authors-pin the daemon pubkey (NIP-17 disambiguation)"
        );
        assert!(f.since.is_none(), "live filter must not set since");
    }

    /// Phase 2d: when v2 is requested but the daemon pubkey isn't known yet
    /// (config still loading), the builder must fall back to the gift-wrap
    /// shape rather than emit an un-pinned kind-14 filter that would collide
    /// with NIP-17 peer chat.
    #[test]
    fn test_dm_live_filter_v2_without_daemon_falls_back_to_giftwrap() {
        let trade = PublicKey::from_hex(TEST_PK_HEX).unwrap();
        let f = dm_live_filter(Transport::Nip44Direct, None, &[trade]);
        let kinds = f.kinds.as_ref().expect("filter must set kinds");
        assert!(
            kinds.iter().any(|k| *k == NostrKind::GiftWrap),
            "v2-without-daemon must fall back to kind 1059 (no un-pinned kind-14)"
        );
        assert!(f.authors.is_none(), "fallback must not set an authors pin");
    }

    /// Fix 2 regression test: `AdminSettled` must map to `Settled` (not
    /// `Success`), so that a subsequent `PaymentFailed` from the daemon's
    /// `do_payment` step can still transition the trade correctly. If we
    /// jumped straight to terminal `Success`, the `PaymentFailed` would be
    /// silently dropped by the terminal-status guard.
    #[test]
    fn test_admin_settled_maps_to_settled_not_success() {
        let mut trade = default_test_trade(TradeStatus::Dispute);
        let my_pk = "deadbeef".to_string();

        let (status, _) =
            apply_mostro_action(&mut trade, A::AdminSettled, &None, test_sender(), &my_pk);
        assert_eq!(status, Some(TradeStatus::Settled));
        assert!(!trade.status.is_terminal(), "Settled must not be terminal");

        // Now feed a PaymentFailed payload — it must still be applied.
        let pf_info = mostro_core::prelude::PaymentFailedInfo {
            payment_attempts: 2,
            payment_retries_interval: 60,
        };
        let (status2, _) = apply_mostro_action(
            &mut trade,
            A::PaymentFailed,
            &Some(P::PaymentFailed(pf_info)),
            test_sender(),
            &my_pk,
        );
        assert_eq!(status2, Some(TradeStatus::PaymentFailed));
    }

    /// Regression test: the daemon sends `Rate` to the seller after
    /// `HoldInvoicePaymentSettled` but never sends `PurchaseCompleted`
    /// to the seller. `Rate` arriving while `Settled` must infer
    /// `Success` so the seller's trade doesn't get stuck.
    #[test]
    fn test_rate_advances_settled_to_success() {
        let mut trade = default_test_trade(TradeStatus::Settled);
        let my_pk = "deadbeef".to_string();

        let (status, _) =
            apply_mostro_action(&mut trade, A::Rate, &None, test_sender(), &my_pk);
        assert_eq!(status, Some(TradeStatus::Success));
    }

    /// Regression test: `Rate` arriving while already at `Success`
    /// (buyer path via `PurchaseCompleted`) must be a status no-op.
    #[test]
    fn test_rate_does_not_regress_from_success() {
        let mut trade = default_test_trade(TradeStatus::Success);
        let my_pk = "deadbeef".to_string();

        let (status, _) =
            apply_mostro_action(&mut trade, A::Rate, &None, test_sender(), &my_pk);
        assert_eq!(status, None);
    }

    /// Fix 3 regression test: `BuyerInvoiceAccepted` is a pure ack and
    /// must not change status or stash a hold invoice, even if a stray
    /// `PaymentRequest` payload arrives with it.
    #[test]
    fn test_buyer_invoice_accepted_is_no_op() {
        let mut trade = default_test_trade(TradeStatus::Active);
        let my_pk = "deadbeef".to_string();
        let payload = Some(P::PaymentRequest(None, "lnbc1stub".to_string(), None));

        let original_status = trade.status;
        let (status, _) = apply_mostro_action(
            &mut trade,
            A::BuyerInvoiceAccepted,
            &payload,
            test_sender(),
            &my_pk,
        );
        assert!(status.is_none(), "BuyerInvoiceAccepted must not change status");
        assert_eq!(trade.status, original_status);
        assert!(
            trade.pending_hold_invoice.is_none(),
            "BuyerInvoiceAccepted must not stash a hold invoice"
        );
    }

    /// `AddInvoice` (server-push) should still transition to
    /// `WaitingBuyerInvoice` and stash the bolt11 for the buyer to act on.
    /// Starting from `Pending` since `Active → WaitingBuyerInvoice` would
    /// be a status regression blocked by the monotonicity guard.
    #[test]
    fn test_add_invoice_still_transitions() {
        let mut trade = default_test_trade(TradeStatus::Pending);
        let my_pk = "deadbeef".to_string();
        let payload = Some(P::PaymentRequest(None, "lnbc1stub".to_string(), None));

        let (status, _) =
            apply_mostro_action(&mut trade, A::AddInvoice, &payload, test_sender(), &my_pk);
        assert_eq!(status, Some(TradeStatus::WaitingBuyerInvoice));
        assert_eq!(trade.pending_hold_invoice.as_deref(), Some("lnbc1stub"));
    }

    /// Phase 1.1 (C1) regression: `AdminTookDispute` must source the solver
    /// pubkey from `Payload::Peer.pubkey`, NOT from the gift-wrap `sender`
    /// (which is the daemon). Using the daemon's pubkey would derive the
    /// admin-shared-key against the wrong pubkey and the dispute chat would
    /// never decrypt.
    #[test]
    fn test_admin_took_dispute_extracts_solver_from_peer_payload() {
        let mut trade = default_test_trade(TradeStatus::Dispute);
        let my_pk = "deadbeef".to_string();

        // Daemon sender pubkey (what was previously used incorrectly).
        let daemon_sender = test_sender();

        // Solver pubkey is DIFFERENT from the daemon sender.
        let solver_hex = "1111111111111111111111111111111111111111111111111111111111111111";
        let solver_pk = PublicKey::from_hex(solver_hex).unwrap();
        let peer_payload = Some(P::Peer(mostro_core::prelude::Peer::new(
            solver_hex.to_string(),
            None,
        )));

        let (status, _) = apply_mostro_action(
            &mut trade,
            A::AdminTookDispute,
            &peer_payload,
            daemon_sender,
            &my_pk,
        );

        assert_eq!(status, Some(TradeStatus::Dispute));
        assert_eq!(
            trade.solver_pubkey.as_deref(),
            Some(solver_pk.to_hex().as_str()),
            "solver_pubkey must come from Payload::Peer, not from gift-wrap sender"
        );
        assert_ne!(
            trade.solver_pubkey.as_deref(),
            Some(daemon_sender.to_hex().as_str()),
            "solver_pubkey must NOT equal the daemon gift-wrap sender"
        );
    }

    /// Phase 1.1 (C1) regression: when the `Payload::Peer` is absent (e.g.
    /// older daemon or malformed event), `solver_pubkey` should be left
    /// untouched rather than filled with the daemon's pubkey.
    #[test]
    fn test_admin_took_dispute_without_peer_payload_leaves_solver_unset() {
        let mut trade = default_test_trade(TradeStatus::Dispute);
        // Pre-set solver to verify it isn't overwritten with daemon's pubkey.
        let pre_existing = "2222222222222222222222222222222222222222222222222222222222222222";
        trade.solver_pubkey = Some(pre_existing.to_string());
        let my_pk = "deadbeef".to_string();

        let (status, _) =
            apply_mostro_action(&mut trade, A::AdminTookDispute, &None, test_sender(), &my_pk);

        assert_eq!(status, Some(TradeStatus::Dispute));
        assert_eq!(
            trade.solver_pubkey.as_deref(),
            Some(pre_existing),
            "solver_pubkey must be preserved (not overwritten with daemon sender)"
        );
    }

    /// Phase 2.1 (C2) regression: cross-rank transitions that ARE on the
    /// exception list (Dispute, CancelPending, PaymentFailed,
    /// CooperativelyCanceled, CanceledByAdmin) must NOT be silently
    /// dropped when arriving from a higher-rank state.
    ///
    /// The previous local guard in `apply_mostro_action` lacked the
    /// exception list and blocked e.g. `FiatSent` (rank 3) → `Dispute`
    /// (rank 2), leaving the user stuck in `FiatSent` after they opened a
    /// dispute.
    #[test]
    fn test_dispute_from_fiat_sent_is_allowed() {
        let mut trade = default_test_trade(TradeStatus::FiatSent);
        let my_pk = "deadbeef".to_string();

        // Daemon sends `DisputeInitiatedByPeer` while we're in FiatSent.
        let (status, _) = apply_mostro_action(
            &mut trade,
            A::DisputeInitiatedByPeer,
            &None,
            test_sender(),
            &my_pk,
        );
        assert_eq!(
            status,
            Some(TradeStatus::Dispute),
            "Dispute must be reachable from FiatSent (exception list)"
        );
    }

    /// Phase 2.1 (C2) regression: `CooperativeCancelInitiatedByPeer`
    /// arriving while the trade is `FiatSent` must transition to
    /// `CancelPending`. Previously the local guard blocked this because
    /// rank 3 (FiatSent) → rank 2 (CancelPending) was treated as a
    /// regression.
    #[test]
    fn test_cooperative_cancel_from_fiat_sent_is_allowed() {
        let mut trade = default_test_trade(TradeStatus::FiatSent);
        let my_pk = "deadbeef".to_string();

        let (status, _) = apply_mostro_action(
            &mut trade,
            A::CooperativeCancelInitiatedByPeer,
            &None,
            test_sender(),
            &my_pk,
        );
        assert_eq!(
            status,
            Some(TradeStatus::CancelPending),
            "CancelPending must be reachable from FiatSent (exception list)"
        );
    }

    /// Phase 2.1 (C2) regression: transitions NOT on the exception list
    /// still get blocked. E.g., a stale relay replay of `Pending` (rank 0)
    /// after the trade is already `Active` (rank 2) should be dropped.
    #[test]
    fn test_backward_regression_still_blocked() {
        let mut trade = default_test_trade(TradeStatus::Active);
        let my_pk = "deadbeef".to_string();

        // Pending is NOT on the exception list and rank 0 < Active rank 2.
        // We don't have a direct Action that maps to Pending in
        // apply_mostro_action, but we can verify the predicate directly.
        use super::super::trade_store::is_status_transition_allowed;
        assert!(
            !is_status_transition_allowed(&TradeStatus::Active, &TradeStatus::Pending),
            "non-exception regression must still be blocked"
        );
        // Sanity: same transition with CancelPending target IS allowed.
        assert!(
            is_status_transition_allowed(&TradeStatus::Active, &TradeStatus::CancelPending),
            "CancelPending is on the exception list and must be allowed"
        );
    }

    /// Fix 1 regression: InvoiceUpdated mutates pending_hold_invoice
    /// without changing status. The caller must detect the mutation
    /// via trade comparison, not just status comparison.
    #[test]
    fn test_invoice_updated_mutates_without_status_change() {
        let mut trade = default_test_trade(TradeStatus::WaitingBuyerInvoice);
        let original = trade.clone();
        let payload = Some(P::PaymentRequest(None, "lnbc1new".to_string(), None));
        let (status, toast) = apply_mostro_action(
            &mut trade,
            A::InvoiceUpdated,
            &payload,
            test_sender(),
            "deadbeef",
        );
        assert!(status.is_none());
        assert!(!toast.is_empty(), "InvoiceUpdated now produces a toast");
        assert_ne!(trade, original, "trade must differ after mutation");
        assert_eq!(trade.pending_hold_invoice.as_deref(), Some("lnbc1new"));
    }

    /// Fix 3 regression: AddBondInvoice WITHOUT BondPayoutRequest returns
    /// role-specific bond status. A taker gets WaitingTakerBond.
    #[test]
    fn test_add_bond_invoice_regular_returns_waiting_bond() {
        let mut trade = default_test_trade(TradeStatus::Pending);
        let (status, _) =
            apply_mostro_action(&mut trade, A::AddBondInvoice, &None, test_sender(), "deadbeef");
        // default_test_trade uses TradeRole::Taker, so we expect WaitingTakerBond.
        assert_eq!(status, Some(TradeStatus::WaitingTakerBond));
        assert!(trade.needs_bond_invoice);
        assert!(
            !trade.needs_bond_payout,
            "regular bond must not set payout flag"
        );
    }

    /// Fix 3 regression: BondPayoutCompleted clears both flags.
    #[test]
    fn test_bond_payout_completed_clears_flags() {
        let mut trade = default_test_trade(TradeStatus::CanceledByAdmin);
        trade.needs_bond_payout = true;
        trade.needs_bond_invoice = true;
        let (status, _) = apply_mostro_action(
            &mut trade,
            A::BondPayoutCompleted,
            &None,
            test_sender(),
            "deadbeef",
        );
        assert!(status.is_none());
        assert!(!trade.needs_bond_payout);
        assert!(!trade.needs_bond_invoice);
    }

    // Fix 3: BondSlashed and AddBondInvoice(BondPayoutRequest) set
    // needs_bond_payout + needs_bond_invoice. These code paths call
    // `platform::timestamp::now_secs()` and `node_config::try_get()`,
    // which require a Dioxus/WASM runtime and cannot be unit-tested on
    // the host target. The logic is verified by the inverse test
    // (BondPayoutCompleted clears the flags) and by the integration with
    // Fix 1's `trade != trade_before` persistence check.

    // ── Mutation leak regression tests ──────────────────────────────
    // These verify that rejected transitions (where the monotonicity
    // guard returns false) roll back ALL side-effect mutations via the
    // snapshot/restore mechanism, not just the status change.

    /// Regression: `AddInvoice` arriving on a terminal (`Success`)
    /// trade must NOT stash `pending_hold_invoice`. Before the fix,
    /// the mutation leaked even though the status transition was
    /// rejected by the guard.
    #[test]
    fn rejected_addinvoice_on_terminal_restores_pending_hold_invoice() {
        let mut trade = default_test_trade(TradeStatus::Success);
        assert!(trade.pending_hold_invoice.is_none());

        let payload = Some(P::PaymentRequest(None, "lnbc1stale".to_string(), None));
        let (status, _) =
            apply_mostro_action(&mut trade, A::AddInvoice, &payload, test_sender(), "deadbeef");

        assert!(status.is_none(), "status must be None (rejected by guard)");
        assert_eq!(
            trade.status,
            TradeStatus::Success,
            "terminal status must be unchanged"
        );
        assert!(
            trade.pending_hold_invoice.is_none(),
            "pending_hold_invoice must be rolled back, not leaked"
        );
    }

    /// Regression: `PayInvoice` arriving on a terminal trade must NOT
    /// stash `pending_hold_invoice`.
    #[test]
    fn rejected_payinvoice_on_terminal_restores_pending_hold_invoice() {
        let mut trade = default_test_trade(TradeStatus::Success);
        let payload = Some(P::PaymentRequest(None, "lnbc1stale".to_string(), None));

        let (status, _) =
            apply_mostro_action(&mut trade, A::PayInvoice, &payload, test_sender(), "deadbeef");

        assert!(status.is_none());
        assert!(
            trade.pending_hold_invoice.is_none(),
            "pending_hold_invoice must be rolled back"
        );
    }

    /// Accepted transitions (where the guard allows the new status)
    /// must still apply side-effect mutations. This is the positive
    /// counterpart to the rollback tests — ensuring the snapshot
    /// mechanism doesn't suppress valid mutations.
    #[test]
    fn accepted_addinvoice_preserves_pending_hold_invoice() {
        let mut trade = default_test_trade(TradeStatus::WaitingBuyerInvoice);
        let payload = Some(P::PaymentRequest(None, "lnbc1valid".to_string(), None));

        let (status, _) =
            apply_mostro_action(&mut trade, A::AddInvoice, &payload, test_sender(), "deadbeef");

        assert_eq!(status, Some(TradeStatus::WaitingBuyerInvoice));
        assert_eq!(
            trade.pending_hold_invoice.as_deref(),
            Some("lnbc1valid"),
            "accepted transition must preserve the mutation"
        );
    }

    /// Regression: `FiatSentOk` arriving on a terminal trade must NOT
    /// set `counterparty_pubkey` or `fiat_was_sent`.
    #[test]
    fn rejected_fiatsentok_on_terminal_restores_counterparty() {
        let mut trade = default_test_trade(TradeStatus::Canceled);
        assert!(trade.counterparty_pubkey.is_none());
        assert!(!trade.fiat_was_sent);

        let peer = mostro_core::prelude::Peer::new("abc123".to_string(), None);
        let payload = Some(P::Peer(peer));

        let (status, _) =
            apply_mostro_action(&mut trade, A::FiatSentOk, &payload, test_sender(), "deadbeef");

        assert!(status.is_none(), "Canceled is terminal — must reject");
        assert!(
            trade.counterparty_pubkey.is_none(),
            "counterparty_pubkey must be rolled back"
        );
        assert!(
            !trade.fiat_was_sent,
            "fiat_was_sent must be rolled back"
        );
    }

    /// Regression for Bug #1: `AddBondInvoice` (non-payout) must return
    /// a role-specific bond status — `WaitingMakerBond` for makers and
    /// `WaitingTakerBond` for takers — NOT the generic `WaitingBond`.
    /// Previously the inline `trade_detail.rs` handler returned
    /// `WaitingBond`, causing the displayed status to differ depending
    /// on which subscription (page vs background monitor) delivered
    /// the event first.
    #[test]
    fn test_add_bond_invoice_regular_returns_role_specific_bond_status() {
        use crate::stores::mostro::trade_store::TradeRole;

        // Taker role: expect WaitingTakerBond.
        let mut trade_taker = default_test_trade(TradeStatus::Pending);
        trade_taker.role = TradeRole::Taker;
        let (status_taker, _) = apply_mostro_action(
            &mut trade_taker,
            A::AddBondInvoice,
            &None,
            test_sender(),
            "deadbeef",
        );
        assert_eq!(
            status_taker,
            Some(TradeStatus::WaitingTakerBond),
            "taker must get WaitingTakerBond, not generic WaitingBond"
        );

        // Maker role: expect WaitingMakerBond.
        let mut trade_maker = default_test_trade(TradeStatus::Pending);
        trade_maker.role = TradeRole::Maker;
        let (status_maker, _) = apply_mostro_action(
            &mut trade_maker,
            A::AddBondInvoice,
            &None,
            test_sender(),
            "deadbeef",
        );
        assert_eq!(
            status_maker,
            Some(TradeStatus::WaitingMakerBond),
            "maker must get WaitingMakerBond, not generic WaitingBond"
        );
    }

    /// Regression for Bug #3: `AdminSettled` must map to non-terminal
    /// `Settled` (NOT `Success`), so that a subsequent `PaymentFailed`
    /// from the daemon's `do_payment` step can still transition the
    /// trade correctly. If we jumped straight to terminal `Success`,
    /// the `PaymentFailed` would be silently dropped by the
    /// terminal-status guard.
    #[test]
    fn test_payment_failed_after_admin_settled_keeps_non_terminal() {
        let mut trade = default_test_trade(TradeStatus::Dispute);
        let my_pk = "deadbeef".to_string();

        let (status, _) = apply_mostro_action(
            &mut trade,
            A::AdminSettled,
            &None,
            test_sender(),
            &my_pk,
        );
        assert_eq!(status, Some(TradeStatus::Settled));
        assert!(!trade.status.is_terminal(), "Settled must not be terminal");

        // Now feed a PaymentFailed payload — it must still be applied.
        let pf_info = mostro_core::prelude::PaymentFailedInfo {
            payment_attempts: 2,
            payment_retries_interval: 60,
        };
        let (status2, _) = apply_mostro_action(
            &mut trade,
            A::PaymentFailed,
            &Some(P::PaymentFailed(pf_info)),
            test_sender(),
            &my_pk,
        );
        assert_eq!(status2, Some(TradeStatus::PaymentFailed));
    }

    /// Conformance: every inbound `Action` that `apply_mostro_action`
    /// handles must produce a deterministic `Option<TradeStatus>` for a
    /// given starting status. This locks down the migration from the
    /// duplicated inline match in `trade_detail.rs`; future drift
    /// fails this test.
    #[test]
    fn test_conformance_every_action_status_matches_golden_table() {
        // We exercise each action from a `Pending` starting status and
        // assert the resulting status. Actions that return `None` from
        // `apply_mostro_action` (pure side-effects, no status transition)
        // are listed as `None`.
        //
        // Note: actions that read GlobalSignals (RateReceived via i18n,
        // CantDo via helpers::cant_do_message, BondSlashed via
        // node_config) are excluded — they require a Dioxus runtime
        // and are tested separately.
        let cases: Vec<(A, Option<P>, Option<TradeStatus>)> = vec![
            // Actions that don't transition from Pending (return None).
            (A::BuyerInvoiceAccepted, None, None),
            (A::TradePubkey, None, None),
            // Actions that DO transition from Pending.
            (A::PayInvoice, Some(P::PaymentRequest(None, "lnbc1x".into(), None)), Some(TradeStatus::WaitingSellerToPay)),
            (A::AddInvoice, Some(P::PaymentRequest(None, "lnbc1x".into(), None)), Some(TradeStatus::WaitingBuyerInvoice)),
            (A::WaitingSellerToPay, None, Some(TradeStatus::WaitingSellerToPay)),
            (A::WaitingBuyerInvoice, None, Some(TradeStatus::WaitingBuyerInvoice)),
        ];

        for (action, payload, expected) in cases {
            let mut trade = default_test_trade(TradeStatus::Pending);
            let label = format!("{action:?}");
            let (actual, _toasts) =
                apply_mostro_action(&mut trade, action, &payload, test_sender(), "deadbeef");
            assert_eq!(
                actual, expected,
                "conformance violation for action {label} from Pending with payload {payload:?}"
            );
        }
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

/// Start a background subscription that monitors all active trades'
/// GiftWraps.  Unlike the page-level subscriptions on the P2P home and
/// trade-detail pages, this listener is **always active** (as long as the
/// user is logged in) and processes status updates regardless of which
/// page the user is currently viewing.
///
/// This mirrors the architecture of the Mostrix TUI client and the
/// Flutter mobile client, both of which maintain a global, page-agnostic
/// listener for Mostro daemon messages.
///
/// In addition to incrementing `TRADE_UNREAD`, the listener unwraps each
/// GiftWrap, applies the Mostro action via [`apply_mostro_action`], and
/// upserts the trade so that reactive UIs (e.g. "My Trades") update in
/// real time even when no page-level subscription is mounted.
///
/// Should be called once after login.  Uses a persistent relay
/// subscription (no auto-close) and the `NotificationDispatcher` for
/// efficient event fan-out.
///
/// Phase 1.6 (M5): also runs a one-shot backfill covering the 3-day window
/// before the subscription starts, so events that arrived while the app was
/// closed are processed. The backfill uses `.since(last_sync - 3 days)` on
/// a `fetch_events` call (NOT a subscription) — see `backfill_filter`'s doc
/// for the gift-wrap randomization rationale.
pub async fn start_background_trade_monitor() {
    // Phase 1.6 (M5): one-shot backfill FIRST so historical events land
    // before the live subscription starts streaming new ones. The dedup
    // LRU prevents double-application when the live subscription sees the
    // same events.
    backfill_active_trades().await;

    let key_map = build_trade_key_map();
    if key_map.is_empty() {
        return;
    }
    let all_pks: Vec<PublicKey> = key_map.keys().cloned().collect();
    let filter = active_trade_filter(&all_pks);
    let client = match crate::stores::nostr_client::get_client() {
        Some(c) => c,
        None => return,
    };
    crate::stores::relay::specialty::ensure_p2p_relays_connected(&client).await;

    let sub_id = {
        let mut attempts = 0u8;
        loop {
            match client.subscribe(filter.clone(), None).await {
                Ok(output) => break output.val,
                Err(e) => {
                    attempts += 1;
                    if attempts >= 3 {
                        log::error!(
                            "Failed to start trade monitor after {attempts} attempts: {e}. \
                             Retrying in 60s."
                        );
                        crate::platform::timer::sleep(std::time::Duration::from_secs(60)).await;
                        attempts = 0;
                    } else {
                        log::warn!(
                            "Trade monitor subscribe attempt {attempts} failed: {e}; \
                             retrying in 5s."
                        );
                        crate::platform::timer::sleep(std::time::Duration::from_secs(5)).await;
                    }
                }
            }
        }
    };
    let dispatcher =
        match crate::stores::notification_dispatcher::DispatcherHandle::create(sub_id) {
            Some(handle) => handle,
            None => return,
        };
    let (_handle, mut rx) = dispatcher;
    dioxus::prelude::spawn(async move {
        // Dedup against the global LRU (`dedup::SEEN_EVENTS`) rather than a
        // local HashSet. This bounds memory across long sessions and shares
        // dedup state with the one-shot backfill path. The LRU is sized to
        // cover the 3-day gift-wrap envelope randomization window (see
        // `nostr::nips::nip59::RANGE_RANDOM_TIMESTAMP_TWEAK`).
        while let Some(event) = rx.recv().await {
            // Phase 1.8 (M12): check dedup BEFORE incrementing TRADE_UNREAD,
            // so duplicate deliveries don't inflate the unread counter.
            if super::dedup::is_seen(&event.id) {
                continue;
            }
            super::dedup::mark_seen(event.id);
            *super::TRADE_UNREAD.write() += 1;

            let recipient = match event.tags.public_keys().next().cloned() {
                Some(pk) => pk,
                None => continue,
            };

            let km = build_trade_key_map();
            let (trade_index, order_id) = match km.get(&recipient) {
                Some(entry) => entry,
                None => continue,
            };

            let keys_state = super::keys::try_get();
            let keys = match keys_state {
                Some(k) => k,
                None => continue,
            };
            let tk = match keys.get_trade_key_by_index(*trade_index) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let unwrapped = match unwrap_mostro_response(&event, &tk).await {
                Ok(Some(u)) => u,
                Ok(None) => continue,
                Err(e) => {
                    log::debug!("background monitor: unwrap failed for {order_id}: {e}");
                    continue;
                }
            };

            let action = unwrapped
                .message
                .inner_action()
                .unwrap_or(mostro_core::prelude::Action::CantDo);
            let payload = unwrapped.message.get_inner_message_kind().payload.clone();
            let my_pk_hex = tk.public_key().to_hex();

            let mut trade = match super::trade_store::find_by_order_id(order_id) {
                Some(t) => t,
                None => continue,
            };
            let trade_before = trade.clone();

            let old_status = trade.status;
            let action_for_notify = action.clone();
            let payload_for_notify = payload.clone();
            let (new_status, toasts) =
                apply_mostro_action(&mut trade, action, &payload, unwrapped.sender, &my_pk_hex);

            // Phase 2.3 (M4/M8): surface daemon refusals (CantDo) and bond
            // slash notifications (BondSlashed) arriving in the background.
            // Without this the user gets no feedback when the daemon rejects
            // an action while the trade-detail page isn't mounted.
            for t in toasts {
                let body = t.body.unwrap_or_default();
                super::enqueue_background_toast(t.title, body);
            }

            if let Some(ns) = new_status {
                trade = super::trade_store::apply_status(&trade, ns);
            }

            if trade.status != old_status || trade != trade_before {
                log::info!(
                    "background monitor: trade {order_id} status {:?} → {:?}",
                    old_status,
                    trade.status
                );
                super::trade_store::upsert(trade.clone());
                let _ = super::trade_store::publish().await;

                // Phase 9: dispatch a local notification for the status
                // change. The mapper checks p2p_settings::should_notify
                // per-category toggles and returns None for actions that
                // don't warrant a notification.
                if crate::stores::ui::p2p_settings::should_notify(&action_for_notify) {
                    if let Some((title, body)) =
                        super::notifications::map_action_to_notification(
                            action_for_notify.clone(),
                            &trade,
                            payload_for_notify.as_ref(),
                        )
                    {
                        super::notifications::show_notification(&title, &body);
                    }
                }

                if trade.status.is_terminal() {
                    super::waiter::prune_waiters_for_order(&trade.order_id);
                }
            }

            // B2 (notification history): persist a notification record
            // independent of whether status changed. This catches chat
            // messages (`SendDm`), disputes opened by peer, and other
            // actions that carry useful context but don't transition the
            // trade FSM. Respects the same `should_notify` per-category
            // toggle as the OS-level notification above.
            if crate::stores::ui::p2p_settings::should_notify(&action_for_notify) {
                if let Some(n) = super::notifications::build_notification(
                    action_for_notify,
                    &trade,
                    payload_for_notify.as_ref(),
                ) {
                    super::notification_store::push(n);
                }
            }
        }
    });

    // Dispute status monitor: subscribe to kind 38386 events from the
    // daemon so the local `DISPUTES` cache stays up-to-date. Without
    // this, dispute auto-close on cooperative cancel or release during
    // dispute (mostro/src/app/dispute.rs:252-334) goes unnoticed — the
    // user only sees the initial `DisputeInitiatedBy*` action and never
    // the resolution status.
    start_background_dispute_monitor(&client).await;
}

/// Start a background subscription for kind 38386 dispute events from
/// the current daemon. Parses and upserts each dispute event into the
/// global `DISPUTES` cache so that reactive UIs update in real time.
async fn start_background_dispute_monitor(client: &nostr_sdk::Client) {
    let node_cfg = match super::node_config::try_get() {
        Some(c) => c,
        None => return,
    };
    // Bug #2 fix: use `parse_node_pubkey` (accepts both hex and npub
    // formats) rather than `PublicKey::from_str` which only accepts hex.
    // Previously, if the user configured an npub-format daemon pubkey in
    // Settings, the dispute monitor would silently fail to start —
    // dispute events would never be received.
    let daemon_pk: PublicKey = match super::helpers::parse_node_pubkey(&node_cfg.pubkey) {
        Ok(pk) => pk,
        Err(e) => {
            log::warn!(
                "mostro: invalid daemon pubkey for dispute monitor: {e} (value: {})",
                node_cfg.pubkey
            );
            return;
        }
    };

    let filter = dispute_status_filter(daemon_pk);
    let sub_id = match client.subscribe(filter, None).await {
        Ok(output) => output.val,
        Err(e) => {
            log::warn!("Failed to start dispute monitor: {e}");
            return;
        }
    };
    let dispatcher =
        match crate::stores::notification_dispatcher::DispatcherHandle::create(sub_id) {
            Some(handle) => handle,
            None => return,
        };
    let (_dispute_handle, mut dispute_rx) = dispatcher;
    dioxus::prelude::spawn(async move {
        while let Some(event) = dispute_rx.recv().await {
            if super::dedup::is_seen(&event.id) {
                continue;
            }
            super::dedup::mark_seen(event.id);

            if let Some(dispute) = super::dispute_store::parse_dispute_event(&event) {
                log::debug!(
                    "dispute monitor: upserting dispute {} status {:?}",
                    dispute.dispute_id,
                    dispute.status
                );
                super::dispute_store::upsert(dispute);
            }
        }
    });
}
