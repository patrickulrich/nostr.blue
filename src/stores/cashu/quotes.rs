//! Quote Expiry Management
//!
//! Handles quote lifecycle, expiry detection, and cleanup.
//! Implements proactive quote management to prevent stale quotes.

// Allow dead_code for planned features not yet wired to UI
#![allow(dead_code)]

use dioxus::prelude::*;

use super::signals::{PENDING_MELT_QUOTES, PENDING_MINT_QUOTES};
use super::types::{
    MeltQuoteInfo, MintQuoteInfo, PendingMeltQuotesStoreStoreExt, PendingMintQuotesStoreStoreExt,
};

// =============================================================================
// Quote Expiry Types
// =============================================================================

/// Quote validity status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteValidity {
    /// Quote is still valid
    Valid,
    /// Quote is expiring soon (within warning threshold)
    ExpiringSoon,
    /// Quote has expired
    Expired,
    /// Quote has no expiry (or unknown)
    NoExpiry,
}

/// Quote expiry thresholds (in seconds)
pub mod thresholds {
    /// Warning threshold - consider quote expiring soon
    pub const WARNING_SECS: u64 = 60;
    /// Critical threshold - quote about to expire
    pub const CRITICAL_SECS: u64 = 10;
    /// Default quote TTL if not specified
    pub const DEFAULT_TTL_SECS: u64 = 600;
}

// =============================================================================
// Quote Expiry Checking
// =============================================================================

/// Get current timestamp in seconds
fn now_secs() -> u64 {
    js_sys::Date::now() as u64 / 1000
}

/// Check if a quote is expired
pub fn is_quote_expired(expiry: Option<u64>) -> bool {
    match expiry {
        Some(exp) => now_secs() >= exp,
        None => false, // No expiry means valid forever
    }
}

/// Get quote validity status
pub fn check_quote_validity(expiry: Option<u64>) -> QuoteValidity {
    match expiry {
        Some(exp) => {
            let now = now_secs();
            if now >= exp {
                QuoteValidity::Expired
            } else if exp - now <= thresholds::WARNING_SECS {
                QuoteValidity::ExpiringSoon
            } else {
                QuoteValidity::Valid
            }
        }
        None => QuoteValidity::NoExpiry,
    }
}

/// Get seconds until quote expires (None if expired or no expiry)
pub fn seconds_until_expiry(expiry: Option<u64>) -> Option<u64> {
    expiry.and_then(|exp| {
        let now = now_secs();
        if now >= exp {
            None
        } else {
            Some(exp - now)
        }
    })
}

/// Format time until expiry as human-readable string
pub fn format_expiry(expiry: Option<u64>) -> String {
    match seconds_until_expiry(expiry) {
        Some(secs) if secs >= 60 => format!("{}m {}s", secs / 60, secs % 60),
        Some(secs) => format!("{}s", secs),
        None => {
            if expiry.is_some() {
                "Expired".to_string()
            } else {
                "No expiry".to_string()
            }
        }
    }
}

// =============================================================================
// Quote Management
// =============================================================================

/// Get all pending mint quotes
pub fn get_pending_mint_quotes() -> Vec<MintQuoteInfo> {
    PENDING_MINT_QUOTES.read().data().read().clone()
}

/// Get all pending melt quotes
pub fn get_pending_melt_quotes() -> Vec<MeltQuoteInfo> {
    PENDING_MELT_QUOTES.read().data().read().clone()
}

/// Get expired mint quotes
pub fn get_expired_mint_quotes() -> Vec<MintQuoteInfo> {
    PENDING_MINT_QUOTES
        .read()
        .data()
        .read()
        .iter()
        .filter(|q| is_quote_expired(q.expiry))
        .cloned()
        .collect()
}

/// Get expired melt quotes
pub fn get_expired_melt_quotes() -> Vec<MeltQuoteInfo> {
    PENDING_MELT_QUOTES
        .read()
        .data()
        .read()
        .iter()
        .filter(|q| is_quote_expired(q.expiry))
        .cloned()
        .collect()
}

/// Get quotes expiring soon
pub fn get_expiring_mint_quotes() -> Vec<MintQuoteInfo> {
    PENDING_MINT_QUOTES
        .read()
        .data()
        .read()
        .iter()
        .filter(|q| check_quote_validity(q.expiry) == QuoteValidity::ExpiringSoon)
        .cloned()
        .collect()
}

/// Get quotes expiring soon
pub fn get_expiring_melt_quotes() -> Vec<MeltQuoteInfo> {
    PENDING_MELT_QUOTES
        .read()
        .data()
        .read()
        .iter()
        .filter(|q| check_quote_validity(q.expiry) == QuoteValidity::ExpiringSoon)
        .cloned()
        .collect()
}

// =============================================================================
// Quote Cleanup
// =============================================================================

/// Remove expired mint quotes from pending list
pub fn cleanup_expired_mint_quotes() -> usize {
    let expired_ids: Vec<String> = PENDING_MINT_QUOTES
        .read()
        .data()
        .read()
        .iter()
        .filter(|q| is_quote_expired(q.expiry))
        .map(|q| q.quote_id.clone())
        .collect();

    if expired_ids.is_empty() {
        return 0;
    }

    let count = expired_ids.len();
    log::info!("Cleaning up {} expired mint quotes", count);

    let mut data = PENDING_MINT_QUOTES.read().data();
    data.write()
        .retain(|q| !expired_ids.contains(&q.quote_id));

    count
}

/// Remove expired melt quotes from pending list
pub fn cleanup_expired_melt_quotes() -> usize {
    let expired_ids: Vec<String> = PENDING_MELT_QUOTES
        .read()
        .data()
        .read()
        .iter()
        .filter(|q| is_quote_expired(q.expiry))
        .map(|q| q.quote_id.clone())
        .collect();

    if expired_ids.is_empty() {
        return 0;
    }

    let count = expired_ids.len();
    log::info!("Cleaning up {} expired melt quotes", count);

    let mut data = PENDING_MELT_QUOTES.read().data();
    data.write()
        .retain(|q| !expired_ids.contains(&q.quote_id));

    count
}

/// Cleanup all expired quotes
pub fn cleanup_all_expired_quotes() -> (usize, usize) {
    let mint_cleaned = cleanup_expired_mint_quotes();
    let melt_cleaned = cleanup_expired_melt_quotes();
    (mint_cleaned, melt_cleaned)
}

/// Remove a specific mint quote by ID
pub fn remove_mint_quote(quote_id: &str) {
    let mut data = PENDING_MINT_QUOTES.read().data();
    data.write().retain(|q| q.quote_id != quote_id);
}

/// Remove a specific melt quote by ID
pub fn remove_melt_quote(quote_id: &str) {
    let mut data = PENDING_MELT_QUOTES.read().data();
    data.write().retain(|q| q.quote_id != quote_id);
}

// =============================================================================
// Quote Lookup
// =============================================================================

/// Find a mint quote by ID
pub fn find_mint_quote(quote_id: &str) -> Option<MintQuoteInfo> {
    PENDING_MINT_QUOTES
        .read()
        .data()
        .read()
        .iter()
        .find(|q| q.quote_id == quote_id)
        .cloned()
}

/// Find a melt quote by ID
pub fn find_melt_quote(quote_id: &str) -> Option<MeltQuoteInfo> {
    PENDING_MELT_QUOTES
        .read()
        .data()
        .read()
        .iter()
        .find(|q| q.quote_id == quote_id)
        .cloned()
}

/// Check if a mint quote exists and is valid
pub fn is_mint_quote_valid(quote_id: &str) -> bool {
    find_mint_quote(quote_id)
        .map(|q| !is_quote_expired(q.expiry))
        .unwrap_or(false)
}

/// Check if a melt quote exists and is valid
pub fn is_melt_quote_valid(quote_id: &str) -> bool {
    find_melt_quote(quote_id)
        .map(|q| !is_quote_expired(q.expiry))
        .unwrap_or(false)
}

// =============================================================================
// Quote Statistics
// =============================================================================

/// Quote stats for display
#[derive(Debug, Clone, Default)]
pub struct QuoteStats {
    pub pending_mint: usize,
    pub pending_melt: usize,
    pub expired_mint: usize,
    pub expired_melt: usize,
    pub expiring_soon_mint: usize,
    pub expiring_soon_melt: usize,
}

/// Get current quote statistics
pub fn get_quote_stats() -> QuoteStats {
    let mint_quotes = PENDING_MINT_QUOTES.read().data().read().clone();
    let melt_quotes = PENDING_MELT_QUOTES.read().data().read().clone();

    let expired_mint = mint_quotes.iter().filter(|q| is_quote_expired(q.expiry)).count();
    let expired_melt = melt_quotes.iter().filter(|q| is_quote_expired(q.expiry)).count();

    let expiring_mint = mint_quotes
        .iter()
        .filter(|q| check_quote_validity(q.expiry) == QuoteValidity::ExpiringSoon)
        .count();
    let expiring_melt = melt_quotes
        .iter()
        .filter(|q| check_quote_validity(q.expiry) == QuoteValidity::ExpiringSoon)
        .count();

    QuoteStats {
        pending_mint: mint_quotes.len(),
        pending_melt: melt_quotes.len(),
        expired_mint,
        expired_melt,
        expiring_soon_mint: expiring_mint,
        expiring_soon_melt: expiring_melt,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_quote_validity() {
        let now = now_secs();

        // Expired
        assert_eq!(check_quote_validity(Some(now - 100)), QuoteValidity::Expired);

        // Expiring soon
        assert_eq!(
            check_quote_validity(Some(now + 30)),
            QuoteValidity::ExpiringSoon
        );

        // Valid
        assert_eq!(check_quote_validity(Some(now + 300)), QuoteValidity::Valid);

        // No expiry
        assert_eq!(check_quote_validity(None), QuoteValidity::NoExpiry);
    }

    #[test]
    fn test_format_expiry() {
        let now = now_secs();

        assert_eq!(format_expiry(Some(now + 90)), "1m 30s");
        assert_eq!(format_expiry(Some(now + 45)), "45s");
        assert_eq!(format_expiry(Some(now - 10)), "Expired");
        assert_eq!(format_expiry(None), "No expiry");
    }
}
