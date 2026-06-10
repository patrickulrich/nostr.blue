//! Terminal trade cleanup
//!
//! Periodically removes terminal trades older than 720 hours (30 days)
//! from the local cache and publishes the updated trade list to NIP-78.
//!
//! Designed to run as a background `use_future` loop with a 30-minute
//! interval. The cleanup threshold matches the daemon's order expiry
//! window so we don't accumulate stale trade records indefinitely.

use crate::platform::timestamp;
use crate::stores::social::mostro::trade_store;

#[allow(dead_code)]
const MAX_AGE_SECS: u64 = 720 * 3600;
#[allow(dead_code)]
const INTERVAL_SECS: u64 = 30 * 60;

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

fn cleanup_expired() -> usize {
    let now = timestamp::now_secs() as i64;
    let trades = trade_store::TRADES();
    let expired_ids: Vec<String> = trades
        .iter()
        .filter(|t| {
            if !t.status.is_terminal() {
                return false;
            }
            let age = now - t.updated_at;
            age >= MAX_AGE_SECS as i64
        })
        .map(|t| t.order_id.clone())
        .collect();

    let removed = expired_ids.len();
    for id in &expired_ids {
        trade_store::remove(id);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_age_is_30_days() {
        assert_eq!(MAX_AGE_SECS, 720 * 3600);
    }

    #[test]
    fn test_interval_is_30_minutes() {
        assert_eq!(INTERVAL_SECS, 30 * 60);
    }
}
