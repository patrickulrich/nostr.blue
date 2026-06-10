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

use super::client::{ensure_node_relays_connected, send_mostro_message};
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
    let node = node_config::try_get()
        .ok_or_else(|| "Mostro node not configured. Visit /settings/p2p to pick a daemon.".to_string())?;

    // Auto-configure from source tag when using default node
    let node = auto_config_from_source(&node, &req.order).await.unwrap_or(node);

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
    let order_uuid = Uuid::parse_str(&req.order.order_id).unwrap_or_else(|_| Uuid::new_v4());
    let stable_id = if Uuid::parse_str(&req.order.order_id).is_ok() {
        req.order.order_id.clone()
    } else {
        req.order.event_id.clone()
    };

    // 4. Build the wire message
    let kind_str = req.order.order_type.as_str();

    let sats_from_override = req
        .fiat_amount_override
        .and_then(|f| crate::services::btc_price::fiat_to_sats(f, &req.order.currency))
        .map(|s| s.min(i64::MAX as u64) as i64);

    let (message, sent_action) = match req.order.order_type {
        crate::utils::nip69::OrderType::Sell => {
            let invoice = req.buyer_invoice.as_ref().map(|s| {
                (
                    s.clone(),
                    sats_from_override.unwrap_or(
                        req.order
                            .amount_sats
                            .min(i64::MAX as u64)
                            .try_into()
                            .unwrap_or(0),
                    ),
                )
            });
            let m = flow::take_sell(&k, order_uuid, k.trade_index.saturating_sub(1), invoice);
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
    trade.maker_trade_pubkey = Some(trade_pubkey.clone());
    if let crate::utils::nip69::FiatAmount::Range { min, max } = &req.order.fiat_amount {
        trade.min_fiat = Some(*min);
        trade.max_fiat = Some(*max);
    }
    let trade = trade;
    trade_store::upsert(trade);
    let _ = trade_store::publish().await;

    // 6. Ensure relays + wrap + send
    let identity_keys = {
        let k2 = keys::try_get().ok_or_else(|| "Mostro keys vanished mid-flight".to_string())?;
        k2.identity_keys.clone()
    };
    ensure_node_relays_connected().await;
    let node_pubkey = parse_node_pubkey(&node.pubkey)?;
    send_mostro_message(
        &message,
        &identity_keys,
        &trade_keys,
        node_pubkey,
        &node.relays,
        req.pow,
    )
    .await?;

    Ok(TakeResult {
        order_id: stable_id,
        trade_pubkey,
        sent_action,
    })
}

#[allow(dead_code)]
pub fn trade_pubkey_npub(trade: &super::trade_store::Trade) -> Option<String> {
    trade
        .maker_trade_pubkey
        .as_ref()
        .and_then(|h| PublicKey::from_hex(h).ok())
        .and_then(|pk| pk.to_bech32().ok())
}

/// Try to auto-configure the node from the order's source tag. Only
/// overwrites the in-memory config when the current config is still the
/// default community pubkey AND the source tag carries a different pubkey.
async fn auto_config_from_source(
    current: &MostroNodeConfig,
    order: &P2POrder,
) -> Option<MostroNodeConfig> {
    let source_str = order.source.as_deref()?;
    let parsed = parse_source_tag(source_str)?;
    let is_default = super::communities::COMMUNITIES
        .iter()
        .any(|c| c.pubkey == current.pubkey);
    if !is_default {
        return None;
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
    let _ = node_config::save_config(new_cfg.clone()).await;
    Some(new_cfg)
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
}
