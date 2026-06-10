//! Trade reconciliation via kind 38383 public order events
//!
//! Subscribes to kind 38383 events for active trades' order IDs (d-tags)
//! to detect status changes visible on the public order board. This
//! supplements the GiftWrap message channel for cases where the daemon
//! updates the NIP-33 event before the GiftWrap arrives (or if the
//! GiftWrap is lost due to relay downtime).
//!
//! The reconciliation is intentionally coarse: kind 38383 maps daemon
//! statuses to 4 wire buckets (`pending`, `in-progress`, `success`,
//! `canceled`), so we can only detect bucket transitions, not granular
//! status changes within a bucket.

use crate::stores::social::mostro::trade_store::{self, Trade, TradeStatus};
use crate::utils::nip69;

#[allow(dead_code)]
pub fn reconcile_order_event(event: &nostr::Event) {
    let order = match nip69::parse_p2p_order(event) {
        Ok(o) => o,
        Err(_) => return,
    };

    let Some(mut trade) = trade_store::find_by_order_id(&order.order_id) else {
        return;
    };

    if trade.status.is_terminal() {
        return;
    }

    let wire_status = extract_wire_status(event);
    let new_status = map_wire_status(&wire_status, &trade);

    if new_status != trade.status {
        trade = trade_store::apply_status(&trade, new_status);
        trade_store::upsert(trade);
        log::info!(
            "Reconciled trade {} from public event: {:?} → {:?}",
            order.order_id,
            wire_status,
            new_status,
        );
    }
}

fn extract_wire_status(event: &nostr::Event) -> Option<String> {
    event
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some("s"))
        .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
}

fn map_wire_status(wire: &Option<String>, trade: &Trade) -> TradeStatus {
    let Some(status) = wire else {
        return trade.status;
    };

    match status.as_str() {
        "pending" => TradeStatus::Pending,
        "in-progress" => {
            if trade.status == TradeStatus::Pending
                || trade.status == TradeStatus::WaitingBond
                || trade.status == TradeStatus::WaitingTakerBond
                || trade.status == TradeStatus::WaitingBuyerInvoice
                || trade.status == TradeStatus::WaitingSellerToPay
            {
                TradeStatus::Active
            } else {
                trade.status
            }
        }
        "success" => TradeStatus::Success,
        "canceled" => TradeStatus::Canceled,
        _ => trade.status,
    }
}

#[allow(dead_code)]
pub fn build_reconciliation_filter(trades: &[Trade]) -> Option<nostr::Filter> {
    let order_ids: Vec<String> = trades
        .iter()
        .filter(|t| !t.status.is_terminal() && !t.order_id.is_empty())
        .filter_map(|t| {
            if uuid::Uuid::parse_str(&t.order_id).is_ok() {
                Some(t.order_id.clone())
            } else {
                None
            }
        })
        .collect();

    if order_ids.is_empty() {
        return None;
    }

    let mut filter = nostr::Filter::new()
        .kind(nostr::Kind::Custom(38383))
        .limit(order_ids.len());

    for oid in &order_ids {
        filter = filter.identifier(oid.as_str());
    }

    Some(filter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::social::mostro::trade_store::tests::default_test_trade;

    #[test]
    fn test_map_wire_pending() {
        let trade = default_test_trade(TradeStatus::Active);
        assert_eq!(map_wire_status(&Some("pending".to_string()), &trade), TradeStatus::Pending);
    }

    #[test]
    fn test_map_wire_in_progress_from_pending() {
        let trade = default_test_trade(TradeStatus::Pending);
        assert_eq!(map_wire_status(&Some("in-progress".to_string()), &trade), TradeStatus::Active);
    }

    #[test]
    fn test_map_wire_in_progress_preserves_fiat_sent() {
        let trade = default_test_trade(TradeStatus::FiatSent);
        assert_eq!(map_wire_status(&Some("in-progress".to_string()), &trade), TradeStatus::FiatSent);
    }

    #[test]
    fn test_map_wire_success() {
        let trade = default_test_trade(TradeStatus::FiatSent);
        assert_eq!(map_wire_status(&Some("success".to_string()), &trade), TradeStatus::Success);
    }

    #[test]
    fn test_map_wire_canceled() {
        let trade = default_test_trade(TradeStatus::Active);
        assert_eq!(map_wire_status(&Some("canceled".to_string()), &trade), TradeStatus::Canceled);
    }

    #[test]
    fn test_map_wire_unknown_preserves() {
        let trade = default_test_trade(TradeStatus::Dispute);
        assert_eq!(map_wire_status(&Some("weird".to_string()), &trade), TradeStatus::Dispute);
    }

    #[test]
    fn test_map_wire_none_preserves() {
        let trade = default_test_trade(TradeStatus::Settled);
        assert_eq!(map_wire_status(&None, &trade), TradeStatus::Settled);
    }

    #[test]
    fn test_build_filter_skips_terminal() {
        let trades = vec![default_test_trade(TradeStatus::Success)];
        assert!(build_reconciliation_filter(&trades).is_none());
    }
}
