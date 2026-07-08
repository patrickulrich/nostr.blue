//! Background cleanup for Mostro trades
//!
//! Two responsibilities:
//!
//! 1. **Terminal GC**: removes trades that have reached a terminal state
//!    (Success, Canceled, Expired, etc.) more than 30 days ago. Matches
//!    the daemon's order expiry window so we don't accumulate stale
//!    records indefinitely.
//!
//! 2. **Orphan cleanup**: removes trades that were created locally but
//!    never received any daemon reply. This happens when the daemon is
//!    unreachable, silently discards the message (e.g. PoW mismatch), or
//!    the GiftWrap ACK is lost due to relay downtime.
//!
//! Both loops run as `spawn_forever` background tasks started after login
//! (see `auth_store::run_post_login_init`).
//!
//! ## Orphan detection heuristic
//!
//! A trade is considered orphaned when ALL of these hold:
//! - Status is `Pending` (no transition has ever been observed).
//! - `updated_at == created_at` (no daemon message has ever advanced it).
//! - Enough time has elapsed that the daemon would definitely have
//!   replied if it received the message (default 120s).
//!
//! Maker listings with a real UUID `order_id` are exempt — they're
//! legitimate open listings that may sit Pending for hours or days.
//! Maker trades with a placeholder `maker-{N}` id (the daemon never
//! ACKed the NewOrder) ARE eligible for orphan cleanup.
//!
//! Trades with `is_bond_invoice == Some(true)` get an extended grace
//! window (default 180s) to allow trailing `BondSlashed` notices.

use crate::platform::timestamp;
use crate::stores::mostro::trade_store::{self, CancelInitiator, TradeRole, TradeStatus};
use crate::stores::mostro::creation_ledger;

/// Default terminal-trade age limit (30 days). Phase 7.5: overridden at
/// runtime by `MostroSettings::trade_history_expiration_days` (see
/// `cleanup_expired` which reads the live setting).
#[allow(dead_code)]
const DEFAULT_MAX_AGE_SECS: u64 = 720 * 3600;
/// Terminal-GC loop tick interval.
#[allow(dead_code)]
const INTERVAL_SECS: u64 = 30 * 60;

/// Phase 7.5: returns the effective `MAX_AGE_SECS` — either from the
/// user's settings (0 = never expire) or the 30-day default.
fn effective_max_age_secs() -> u64 {
    let days = crate::stores::ui::p2p_settings::trade_history_expiration_days();
    if days == 0 {
        // Never expire — use a very large value so the age check never passes.
        u64::MAX
    } else {
        days as u64 * 86_400
    }
}

/// A Pending trade with no daemon reply older than this is an orphan.
const ORPHAN_THRESHOLD_SECS: i64 = 600;
/// Extended threshold when the daemon started the bond flow.
const ORPHAN_BOND_GRACE_SECS: i64 = 900;
/// Orphan-cleanup loop tick interval.
const ORPHAN_INTERVAL_SECS: u64 = 30;

/// Phase 3.5 (F15): grace window for admin-canceled trades when the daemon
/// has bonds enabled. A trailing `Action::BondSlashed` may follow within
/// this window if the solver directed a slash.
const ADMIN_CANCEL_BOND_GRACE_SECS: i64 = 60;

/// Run both cleanup loops forever. Designed for `spawn_forever`.
///
/// Each loop runs on its own interval so a slow orphan sweep can't delay
/// the terminal GC (and vice versa).
#[allow(dead_code)]
pub async fn run_all_cleanup_loops() {
    loop {
        let _ = tokio::join!(
            run_terminal_gc_once(),
            run_orphan_cleanup_once(),
            crate::platform::timer::sleep(std::time::Duration::from_secs(
                ORPHAN_INTERVAL_SECS.min(INTERVAL_SECS),
            )),
        );
    }
}

/// Loop that periodically removes terminal trades older than `MAX_AGE_SECS`.
/// Kept as a standalone function for backwards compat with the original
/// `run_cleanup_loop` API and for callers that want only terminal GC.
#[allow(dead_code)]
pub async fn run_cleanup_loop() {
    loop {
        crate::platform::timer::sleep(std::time::Duration::from_secs(INTERVAL_SECS)).await;
        let removed = cleanup_expired();
        if removed > 0 {
            log::info!("Cleaned up {removed} expired terminal trades");
            let _ = trade_store::publish().await;
        }
    }
}

#[allow(dead_code)]
async fn run_terminal_gc_once() {
    // Terminal GC runs on the longer 30-min cadence; this helper is a
    // no-op on most ticks and only fires when the interval has elapsed.
    // Implementation: we use a state-free approach based on the trade
    // timestamps themselves, so just calling `cleanup_expired` on every
    // shared tick is correct and cheap (filters by `is_terminal()` first).
    // To avoid running every 30s, we throttle via the longer sleep in
    // `run_all_cleanup_loops` — but since that sleeps the shorter of the
    // two intervals, we add a simple counter-based throttle here.
    use std::sync::atomic::{AtomicU64, Ordering};
    static TICKS_SINCE_LAST_GC: AtomicU64 = AtomicU64::new(0);
    let ticks = TICKS_SINCE_LAST_GC.fetch_add(1, Ordering::Relaxed);
    let gc_period_ticks = INTERVAL_SECS / ORPHAN_INTERVAL_SECS;
    if ticks < gc_period_ticks.max(1) {
        return;
    }
    TICKS_SINCE_LAST_GC.store(0, Ordering::Relaxed);
    let removed = cleanup_expired();
    if removed > 0 {
        log::info!("Cleaned up {removed} expired terminal trades");
        let _ = trade_store::publish().await;
    }
}

#[allow(dead_code)]
async fn run_orphan_cleanup_once() {
    let removed = cleanup_orphans();
    if removed > 0 {
        log::warn!("Removed {removed} orphan Mostro trade(s) (no daemon reply)");
        let _ = trade_store::publish().await;
    }
}

fn cleanup_expired() -> usize {
    let now = timestamp::now_secs() as i64;
    // Phase 3.5 (F15): check whether the daemon has bonds enabled so we
    // can apply the admin-cancel grace window only when relevant. Reading
    // `MOSTRO_NODE_INFO` is best-effort — if it's None (e.g., the user
    // hasn't picked a daemon yet), we default to false (no grace) which
    // matches the original "delete on the 30-day sweep" behavior.
    let bonds_enabled = crate::stores::mostro::node_config::MOSTRO_NODE_INFO
        ()
        .as_ref()
        .map(|info| info.bond_enabled)
        .unwrap_or(false);

    let trades = trade_store::TRADES();
    let expired_ids: Vec<String> = trades
        .iter()
        .filter(|t| {
            if !t.status.is_terminal() {
                return false;
            }
            let age = now - t.updated_at;

            // Phase 3.5 (F15): user/peer-initiated cancels are deleted
            // instantly (no slash expected from a cancel the user
            // themselves triggered or accepted from the peer). The 30-day
            // sweep would otherwise leave stale "CooperativelyCanceled"
            // entries cluttering the history view.
            if matches!(
                t.cancel_initiator,
                Some(CancelInitiator::User) | Some(CancelInitiator::Peer)
            ) && age >= ADMIN_CANCEL_BOND_GRACE_SECS
            {
                return true;
            }

            // Admin/Daemon cancels with bonds enabled: keep for the grace
            // window so a trailing `Action::BondSlashed` can arrive. After
            // the grace window, fall through to the MAX_AGE_SECS check
            // below (which they won't pass for ~30 days).
            if matches!(
                t.cancel_initiator,
                Some(CancelInitiator::Admin) | Some(CancelInitiator::Daemon)
            ) && bonds_enabled
                && age < ADMIN_CANCEL_BOND_GRACE_SECS
            {
                return false;
            }

            // Default: keep terminal trades for the user-configured history window.
            age >= effective_max_age_secs() as i64
        })
        .map(|t| t.order_id.clone())
        .collect();

    let removed = expired_ids.len();
    for id in &expired_ids {
        trade_store::remove(id);
    }
    removed
}

/// Predicate: is this trade an orphan that should be cleaned up?
fn is_orphan(trade: &trade_store::Trade, now: i64, in_ledger: bool) -> bool {
    // Only Pending trades can be orphans.
    if trade.status != TradeStatus::Pending {
        return false;
    }
    // "No daemon reply" signal: `updated_at` never advanced past `created_at`.
    // Any status-changing GiftWrap from the daemon triggers `apply_status`
    // which bumps `updated_at`.
    if trade.updated_at > trade.created_at {
        return false;
    }
    // Durable ledger exemption: trades recorded in the creation ledger were
    // intentionally created by the user. They survive the orphan sweep so
    // they can be recovered via `recover_order_by_id` or `request_restore`.
    if in_ledger {
        return false;
    }
    // Maker listings with a real UUID order_id are legitimate open listings.
    // Maker trades with a placeholder `maker-{N}` id were never ACKed by
    // the daemon and ARE eligible for cleanup.
    if trade.role == TradeRole::Maker && !is_placeholder_maker_id(&trade.order_id) {
        return false;
    }
    // Defense-in-depth: if the order is demonstrably still live on the
    // public P2P board (any non-terminal status), don't orphan it — the
    // daemon clearly has it and a missed reply (now caught by the global
    // session listener) is the only reason `updated_at` hasn't advanced.
    if is_live_on_board(&trade.order_id) {
        return false;
    }
    let threshold = if trade.is_bond_invoice == Some(true) {
        ORPHAN_BOND_GRACE_SECS
    } else {
        ORPHAN_THRESHOLD_SECS
    };
    (now - trade.created_at) >= threshold
}

/// True if `order_id` matches an order on the public P2P board that is in a
/// non-terminal state (Pending, InProgress — NOT Canceled/Success/Expired).
/// Only real UUIDs are published to the board (placeholders like `maker-{N}`
/// aren't until ACK'd), so non-UUID ids short-circuit to false.
fn is_live_on_board(order_id: &str) -> bool {
    if uuid::Uuid::parse_str(order_id).is_err() {
        return false;
    }
    crate::stores::social::p2p_store::get_all_cached_orders()
        .iter()
        .any(|o| {
            o.order_id == order_id
                && !matches!(
                    o.status,
                    crate::utils::nip69::OrderStatus::Canceled
                        | crate::utils::nip69::OrderStatus::Success
                        | crate::utils::nip69::OrderStatus::Expired
                )
        })
}

/// Returns true if `id` is a `maker-{N}` placeholder assigned locally
/// when the daemon's NewOrder ACK never arrived. See `create_order.rs:440-442`.
fn is_placeholder_maker_id(id: &str) -> bool {
    id.starts_with("maker-") && id["maker-".len()..].parse::<u32>().is_ok()
}

fn cleanup_orphans() -> usize {
    let now = timestamp::now_secs() as i64;
    let trades = trade_store::TRADES();
    let ledger_ids: std::collections::HashSet<String> = creation_ledger::CREATION_LEDGER()
        .iter()
        .map(|e| e.order_id.clone())
        .collect();
    let orphan_ids: Vec<String> = trades
        .iter()
        .filter(|t| is_orphan(t, now, ledger_ids.contains(&t.order_id)))
        .map(|t| t.order_id.clone())
        .collect();

    let removed = orphan_ids.len();
    for id in &orphan_ids {
        trade_store::remove(id);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::mostro::trade_store::{Trade, TradeRole};

    /// Fixed `now` for deterministic tests (avoids `timestamp::now_secs()`
    /// which panics off-wasm when called from host-target `cargo test`).
    const NOW: i64 = 1_700_000_000;

    fn build_trade(status: TradeStatus, role: TradeRole, age_secs: i64, updated: bool) -> Trade {
        let created = NOW - age_secs;
        let mut t = Trade::new_pending_at(
            created,
            "order-1".to_string(),
            "d-1".to_string(),
            "maker-hex".to_string(),
            role,
            "sell".to_string(),
            "100".to_string(),
            "USD".to_string(),
            Some(50_000),
            0.0,
            vec![],
            Some(0),
        );
        t.status = status;
        t.updated_at = if updated { created + 30 } else { created };
        t
    }

    #[test]
    fn test_max_age_is_30_days() {
        assert_eq!(DEFAULT_MAX_AGE_SECS, 720 * 3600);
    }

    #[test]
    fn test_interval_is_30_minutes() {
        assert_eq!(INTERVAL_SECS, 30 * 60);
    }

    #[test]
    fn test_orphan_thresholds_are_distinct() {
        assert!(ORPHAN_BOND_GRACE_SECS > ORPHAN_THRESHOLD_SECS);
        assert!(ORPHAN_THRESHOLD_SECS >= 60);
    }

    #[test]
    fn test_orphan_taker_old_no_reply() {
        let t = build_trade(TradeStatus::Pending, TradeRole::Taker, 650, false);
        assert!(is_orphan(&t, NOW, false), "old Pending taker with no reply is orphan");
    }

    #[test]
    fn test_not_orphan_taker_young() {
        let t = build_trade(TradeStatus::Pending, TradeRole::Taker, 30, false);
        assert!(!is_orphan(&t, NOW, false), "young Pending trade is not orphan");
    }

    #[test]
    fn test_not_orphan_daemon_replied() {
        let t = build_trade(TradeStatus::Pending, TradeRole::Taker, 200, true);
        assert!(
            !is_orphan(&t, NOW, false),
            "trade whose updated_at advanced is not orphan"
        );
    }

    #[test]
    fn test_not_orphan_maker_with_real_uuid() {
        let mut t = build_trade(TradeStatus::Pending, TradeRole::Maker, 600, false);
        // Real UUID order id → legitimate open listing
        t.order_id = "550e8400-e29b-41d4-a716-446655440000".to_string();
        assert!(!is_orphan(&t, NOW, false), "maker listing with real UUID is not orphan");
    }

    #[test]
    fn test_orphan_maker_with_placeholder_id() {
        let mut t = build_trade(TradeStatus::Pending, TradeRole::Maker, 650, false);
        t.order_id = "maker-5".to_string();
        assert!(
            is_orphan(&t, NOW, false),
            "maker with placeholder id and no reply is orphan"
        );
    }

    #[test]
    fn test_not_orphan_maker_placeholder_young() {
        let mut t = build_trade(TradeStatus::Pending, TradeRole::Maker, 30, false);
        t.order_id = "maker-5".to_string();
        assert!(!is_orphan(&t, NOW, false), "young placeholder maker is not orphan");
    }

    #[test]
    fn test_orphan_within_bond_grace_is_kept() {
        let mut t = build_trade(TradeStatus::Pending, TradeRole::Taker, 150, false);
        t.is_bond_invoice = Some(true);
        assert!(
            !is_orphan(&t, NOW, false),
            "bond-flow trade within grace window is not orphan"
        );
    }

    #[test]
    fn test_orphan_past_bond_grace_is_removed() {
        let mut t = build_trade(TradeStatus::Pending, TradeRole::Taker, 950, false);
        t.is_bond_invoice = Some(true);
        assert!(
            is_orphan(&t, NOW, false),
            "bond-flow trade past grace window is orphan"
        );
    }

    #[test]
    fn test_not_orphan_when_status_advanced() {
        let t = build_trade(TradeStatus::Active, TradeRole::Taker, 9999, false);
        assert!(!is_orphan(&t, NOW, false), "non-Pending trade is never orphan");
    }

    #[test]
    fn test_placeholder_maker_id_detection() {
        assert!(is_placeholder_maker_id("maker-0"));
        assert!(is_placeholder_maker_id("maker-42"));
        assert!(!is_placeholder_maker_id("maker-"));
        assert!(!is_placeholder_maker_id("maker-abc"));
        assert!(!is_placeholder_maker_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_placeholder_maker_id(""));
    }
}
