//! Mostro trade execution orchestrator
//!
//! Top-level "take this order" flow: wires the keys, the node config, the
//! flow builders, the publish queue, and the trade store together.
//!
//! Called from `P2POrderCard::Take` and from the future `/p2p/create` page.
//!
//! Returns the `order_id` of the resulting trade so the caller can
//! navigate to `/p2p/trade/:order_id`.

use mostro_core::prelude::*;
use nostr::prelude::*;
use nostr_sdk::ToBech32;
use uuid::Uuid;

use super::client::{ensure_node_relays_connected, resolve_effective_pow, send_mostro_message};
use super::flow;
use super::helpers::parse_node_pubkey;
use super::keys;
use super::node_config::{self, MostroNodeConfig};
use super::source_tag::parse_source_tag;
use super::trade_store::{self, Trade, TradeRole};
use crate::stores::auth_store;
use crate::utils::nip69::P2POrder;

/// What the user provided when they hit "Take".
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct TakeRequest {
    pub order: P2POrder,
    pub buyer_invoice: Option<String>,
    pub fiat_amount_override: Option<f64>,
    pub pow: u8,
}

/// Result of a successful take.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct TakeResult {
    /// Stable identifier of the trade record we created.
    pub order_id: String,
    /// The derived trade key (hex pubkey) — exposed so the UI can
    /// display the trade's identity if useful.
    pub trade_pubkey: String,
    /// The on-wire `Message` we sent, for debug logging.
    pub sent_action: Action,
}

/// Top-level entrypoint: take a Mostro order.
///
/// Steps:
/// 1. Resolve the daemon node config (read from cache or NIP-78).
/// 2. Make sure Mostro keys are initialized.
/// 3. Allocate a fresh trade key (increments the trade index).
/// 4. Build the `TakeSell` or `TakeBuy` message.
/// 5. Persist a new `Trade` record (status = `Pending`).
/// 6. Wrap + publish the message via `send_mostro_message`.
/// 7. Return the trade id for navigation.
#[allow(dead_code)]
pub async fn take_order(req: TakeRequest) -> Result<TakeResult, String> {
    // 1. Auth + client
    if !auth_store::is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    // E5: refuse to take a new order while a session restore is in flight.
    // The restore sync can advance `mostro_trade_index` past the slot we'd
    // allocate here, leaving the new trade unreachable by key derivation
    // (the background monitor wouldn't know to derive a key for the index
    // we used). The window is small but real — typically <5 seconds.
    if crate::stores::mostro::restore::is_restore_in_progress() {
        return Err(
            "Session restore in progress — please wait a few seconds and try again.".to_string(),
        );
    }

    // Phase 4.1 (M7): hard-error on unparseable order_id instead of
    // falling back to a random UUID. The previous `unwrap_or_else(|_| Uuid::new_v4())`
    // silently produced a doomed trade record referencing an order that
    // doesn't exist on the daemon — the daemon would reject with
    // `CantDo::NotFound`, but by then the trade-index slot was already
    // incremented and persisted (line 84), leaking a slot per typo.
    //
    // Also validate the buyer invoice BEFORE incrementing the trade index,
    // so a malformed invoice doesn't waste a slot.
    if Uuid::parse_str(&req.order.order_id).is_err() {
        return Err(format!(
            "Invalid order id: {:?} — expected a UUID. \
             The order may have been deleted or the relay returned a malformed event.",
            req.order.order_id
        ));
    }
    if let Some(ref inv) = req.buyer_invoice {
        if let Err(e) = flow::validate_invoice(inv) {
            return Err(format!("Invalid buyer invoice: {e}"));
        }
    }

    // Bug #5 fix: if YADIO_RATES is empty (cold start / offline), attempt a
    // sync refresh before checking. If still empty after refresh, warn but
    // proceed — the daemon will reject if truly unsupported. Previously
    // the check was skipped entirely when YADIO_RATES was empty, allowing
    // potentially-unsupported-currency takes through silently.
    if crate::services::payments::yadio::YADIO_RATES().is_empty() {
        log::info!("mostro: YADIO_RATES empty at take time, refreshing...");
        let _ = crate::services::payments::yadio::fetch_yadio_rates().await;
    }
    if !crate::services::payments::yadio::is_currency_supported(&req.order.currency)
        && !crate::services::payments::yadio::YADIO_RATES().is_empty()
    {
        return Err(format!(
            "Currency {} is not supported by the exchange rate source. \
             The daemon may reject this order.",
            req.order.currency
        ));
    } else if crate::services::payments::yadio::YADIO_RATES().is_empty() {
        log::warn!(
            "mostro: YADIO_RATES still empty after refresh; skipping \
             currency check for {}",
            req.order.currency
        );
    }

    let node = node_config::try_get()
        .ok_or_else(|| "Mostro node not configured. Visit /settings/mostro to pick a daemon.".to_string())?;

    // Auto-configure from source tag
    let node = match auto_config_from_source(&node, &req.order).await {
        Some((cfg, false)) => cfg,
        Some((cfg, true)) => {
            let _ = node_config::save_config(cfg.clone()).await;
            cfg
        }
        None => node,
    };

    // 2. Keys
    keys::init();
    let mut k = keys::try_get().ok_or_else(|| "Mostro keys not initialized".to_string())?;
    let trade_keys = k
        .next_protocol_trade_keys()
        .map_err(|e| format!("failed to derive trade key: {e}"))?;
    keys::write_back_trade_index(k.trade_index);
    let trade_pubkey = trade_keys.public_key().to_hex();
    let trade_index = if k.privacy_mode {
        None
    } else {
        Some(k.trade_index.saturating_sub(1))
    };

    // 3. Resolve order_id: prefer the canonical UUID from `d` tag; fall back
    // to the kind-38383 event id so we never lose the trade record.
    let order_uuid = Uuid::parse_str(&req.order.order_id)
        .map_err(|e| format!("order id UUID parse failed (should be pre-validated): {e}"))?;
    let stable_id = if Uuid::parse_str(&req.order.order_id).is_ok() {
        req.order.order_id.clone()
    } else {
        req.order.event_id.clone()
    };

    // Self-take guard: refuse to take an order we created. The daemon does
    // NOT reject this (it only checks trade-pubkey collision, and we always
    // rotate to a fresh trade key per take), so without this guard a maker
    // can take their own order — which corrupts local state (the taker
    // record collides with the maker record on order_id) and has no possible
    // benefit. Detected via the local maker trade. (Role-aware upsert now
    // prevents the clobber even if this is bypassed, but blocking here is
    // the correct UX.)
    if trade_store::find_by_order_id(&stable_id)
        .is_some_and(|t| t.role == TradeRole::Maker)
    {
        return Err("You can't take your own order.".to_string());
    }

    // 4. Build the wire message
    let kind_str = req.order.order_type.as_str();

    let sats_from_override = req
        .fiat_amount_override
        .and_then(|f| crate::services::btc_price::fiat_to_sats(f, &req.order.currency))
        .map(|s| s.min(i64::MAX as u64) as i64);

    // The PaymentRequest's third field is the **fiat amount** for range
    // orders (partial take), or `None` to let the daemon compute sats.
    // We must NOT pass sats here — see `mostro-cli/src/cli/take_order.rs:22-43`
    // for the reference: it sends `None` unless the user passed a fiat
    // amount for a range-order partial take.
    let range_fiat_amount = req.fiat_amount_override.map(|f| f as i64);

    let (message, sent_action) = match req.order.order_type {
        crate::utils::nip69::OrderType::Sell => {
            let m = flow::take_sell(
                &k,
                order_uuid,
                k.trade_index.saturating_sub(1),
                req.buyer_invoice.clone(),
                range_fiat_amount,
            );
            (m, Action::TakeSell)
        }
        crate::utils::nip69::OrderType::Buy => {
            let m = flow::take_buy(
                &k,
                order_uuid,
                k.trade_index.saturating_sub(1),
                sats_from_override,
            );
            (m, Action::TakeBuy)
        }
    };
    drop(k);

    // 5. Persist a new trade record
    let role = TradeRole::Taker;
    let fiat_amount_str = req
        .fiat_amount_override
        .map(|f| format!("{f}"))
        .unwrap_or_else(|| match &req.order.fiat_amount {
            crate::utils::nip69::FiatAmount::Fixed(amt) => format!("{amt}"),
            crate::utils::nip69::FiatAmount::Range { min, max } => {
                if (min - max).abs() < f64::EPSILON {
                    format!("{min}")
                } else {
                    format!("{min}-{max}")
                }
            }
        });
    let sats_amount = sats_from_override.or({
        if req.order.amount_sats > 0 {
            Some(req.order.amount_sats as i64)
        } else {
            None
        }
    });
    let mut trade = Trade::new_pending(
        stable_id.clone(),
        req.order.order_id.clone(),
        req.order.pubkey.clone(),
        role,
        kind_str.to_string(),
        fiat_amount_str,
        req.order.currency.clone(),
        sats_amount,
        req.order.premium.unwrap_or(0.0),
        req.order.payment_methods.clone(),
        trade_index,
    );
    // Stash the trade pubkey we used on the record so we can derive the
    // SharedKey for chat later.
    trade.my_trade_pubkey = Some(trade_pubkey.clone());
    if let crate::utils::nip69::FiatAmount::Range { min, max } = &req.order.fiat_amount {
        trade.min_fiat = Some(*min);
        trade.max_fiat = Some(*max);
    }
    let trade = trade;
    trade_store::upsert(trade);
    // Record in the durable creation ledger (taker side).
    crate::stores::mostro::creation_ledger::append(
        crate::stores::mostro::creation_ledger::CreationLedgerEntry {
            order_id: stable_id.clone(),
            role: TradeRole::Taker,
            kind: kind_str.to_string(),
            trade_index,
            my_trade_pubkey: Some(trade_pubkey.clone()),
            daemon_pubkey: node.pubkey.clone(),
            created_at: crate::platform::timestamp::now_secs() as i64,
            confirmed: true,
        },
    );
    let _ = trade_store::publish().await;

    // 6. Ensure relays + wrap + send
    let identity_keys = {
        let k2 = keys::try_get().ok_or_else(|| "Mostro keys vanished mid-flight".to_string())?;
        k2.identity_keys.clone()
    };
    ensure_node_relays_connected().await;
    let node_pubkey = parse_node_pubkey(&node.pubkey)?;
    let pow = resolve_effective_pow(&node, node_pubkey).await;
    if let Err(e) = send_mostro_message(
        &message,
        &identity_keys,
        &trade_keys,
        node_pubkey,
        &node.relays,
        pow,
    )
    .await
    {
        trade_store::remove(&stable_id);
        let _ = trade_store::publish().await;
        return Err(e);
    }

    Ok(TakeResult {
        order_id: stable_id,
        trade_pubkey,
        sent_action,
    })
}

#[allow(dead_code)]
pub fn trade_pubkey_npub(trade: &super::trade_store::Trade) -> Option<String> {
    trade
        .my_trade_pubkey
        .as_ref()
        .and_then(|h| PublicKey::from_hex(h).ok())
        .and_then(|pk| pk.to_bech32().ok())
}

/// Try to auto-configure the node from the order's source tag.
///
/// Returns:
/// - `None` if no source tag or it can't be parsed
/// - `Some((config, true))` if the source tag specifies a *different* daemon
///   (caller should confirm the switch — see `check_source_tag_daemon_switch`)
/// - `Some((config, false))` if the source tag matches the current daemon
///   (apply silently)
async fn auto_config_from_source(
    current: &MostroNodeConfig,
    order: &P2POrder,
) -> Option<(MostroNodeConfig, bool)> {
    let source_str = order.source.as_deref()?;
    let parsed = parse_source_tag(source_str)?;
    let is_same = parsed.mostro_pubkey.to_hex() == current.pubkey;
    if is_same {
        return Some((current.clone(), false));
    }
    let new_cfg = MostroNodeConfig::new(
        parsed.mostro_pubkey.to_hex(),
        if parsed.relays.is_empty() {
            current.relays.clone()
        } else {
            parsed.relays
        },
        Some("auto (source tag)".to_string()),
    )
    .ok()?;
    Some((new_cfg, true))
}

/// Phase 1.4 (C8) pre-check: determine whether taking `order` would require
/// switching to a different Mostro daemon (because the order's `source` tag
/// names a daemon other than the currently-selected one).
///
/// Returns:
/// - `Ok(None)` if no switch is needed (same daemon, or no source tag).
/// - `Ok(Some(new_cfg))` if the source tag names a DIFFERENT daemon. The
///   caller MUST show a confirmation prompt before proceeding — silently
///   switching daemons is a phishing vector (a malicious order with a
///   crafted source tag could redirect trades to an attacker's daemon).
/// - `Err(...)` if Mostro is not yet configured.
///
/// On user confirmation of a switch, call `node_config::save_config(new_cfg)`
/// and then `take_order` — `take_order`'s internal `auto_config_from_source`
/// will see the configs now match and proceed without another prompt.
#[allow(dead_code)]
pub fn check_source_tag_daemon_switch(
    order: &P2POrder,
) -> Result<Option<MostroNodeConfig>, String> {
    let current = node_config::try_get()
        .ok_or_else(|| "Mostro node not configured. Visit /settings/p2p to pick a daemon.".to_string())?;
    // Mirror auto_config_from_source's logic synchronously so callers can
    // decide whether to prompt before any async work begins.
    let source_str = match order.source.as_deref() {
        Some(s) => s,
        None => return Ok(None),
    };
    let parsed = match parse_source_tag(source_str) {
        Some(p) => p,
        None => return Ok(None),
    };
    if parsed.mostro_pubkey.to_hex() == current.pubkey {
        return Ok(None);
    }
    let new_cfg = MostroNodeConfig::new(
        parsed.mostro_pubkey.to_hex(),
        if parsed.relays.is_empty() {
            current.relays.clone()
        } else {
            parsed.relays
        },
        Some("auto (source tag)".to_string()),
    )
    .map_err(|e| format!("Failed to build switched config: {e}"))?;
    Ok(Some(new_cfg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::nip69::{FiatAmount, OrderType};

    fn sample_order_sell() -> P2POrder {
        P2POrder {
            order_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            event_id: "abc123def456".to_string(),
            pubkey: "1111111111111111111111111111111111111111111111111111111111111111".to_string(),
            naddr: "naddr1...".to_string(),
            coordinate: "38383:1111:order".to_string(),
            created_at: 1_700_000_000,
            order_type: OrderType::Sell,
            currency: "EUR".to_string(),
            status: crate::utils::nip69::OrderStatus::Pending,
            amount_sats: 100_000,
            fiat_amount: FiatAmount::Fixed(50.0),
            premium: Some(1.0),
            payment_methods: vec!["SEPA".to_string()],
            network: crate::utils::nip69::Network::Mainnet,
            layer: crate::utils::nip69::Layer::Lightning,
            platform: Some("mostro".to_string()),
            source: None,
            maker_name: None,
            rating: None,
            geohash: None,
            bond: None,
            expires_at: None,
            expiration: None,
        }
    }

    #[test]
    fn test_parse_node_pubkey_accepts_hex() {
        let hex = "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390";
        let result = super::super::helpers::parse_node_pubkey(hex);
        assert!(result.is_ok());
    }

    #[test]
    fn test_take_request_constructs() {
        let order = sample_order_sell();
        let req = TakeRequest {
            order,
            buyer_invoice: None,
            fiat_amount_override: None,
            pow: 0,
        };
        assert_eq!(req.order.order_type, OrderType::Sell);
        assert!(req.buyer_invoice.is_none());
    }

    #[test]
    fn test_trade_pubkey_npub_handles_missing() {
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
        assert!(trade_pubkey_npub(&trade).is_none());
    }

    /// Phase 4.1 (M7) regression: a malformed order_id (not a UUID) must
    /// be detected BEFORE incrementing the trade index. Verifies the
    /// UUID-parse check directly (we can't exercise the full async
    /// `take_order` without a Dioxus runtime + auth state).
    #[test]
    fn test_malformed_order_id_fails_uuid_parse() {
        let bad_ids = &["not-a-uuid", "", "12345", "abc-def-ghi"];
        for id in bad_ids {
            assert!(
                Uuid::parse_str(id).is_err(),
                "expected {id:?} to fail UUID parse"
            );
        }
        // Sanity: the canonical form parses cleanly.
        assert!(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    /// Phase 4.1 (M7) regression: a malformed buyer invoice must be
    /// rejected by `flow::validate_invoice` before `take_order` increments
    /// the trade index.
    #[test]
    fn test_malformed_invoice_fails_validation() {
        let bad_invoices = &[
            "not-an-invoice",
            "",
            "lightning:abc",
            "lnbcxinvalid",
        ];
        for inv in bad_invoices {
            // validate_invoice may accept Lightning Addresses (contains @)
            // and try to fetch them — restrict the test to clearly-invalid
            // strings that don't look like either form.
            if !inv.contains('@') {
                assert!(
                    super::flow::validate_invoice(inv).is_err(),
                    "expected {inv:?} to fail invoice validation"
                );
            }
        }
    }
}
