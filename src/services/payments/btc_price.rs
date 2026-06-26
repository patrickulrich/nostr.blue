//! Bitcoin Price Service
//!
//! Fetches BTC prices from CoinGecko API for fiat currency conversions.
//! CoinGecko supports CORS and provides free API access.
//!
//! C1: also ingests Mostro daemon rates from kind 30078 events
//! (`d = "mostro-rates"`, content = Yadio-format JSON
//! `{"BTC": {"USD": ...}}`). Per Mobile's `nostr_exchange_service.dart`
//! cascade pattern, Mostro rates take precedence when fresh — they
//! represent the daemon's actual quote (more relevant for trades on
//! that daemon than broad-market CoinGecko).
use crate::platform::http::http_client;
use dioxus::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
/// CoinGecko simple price response
/// Returns: { "bitcoin": { "usd": 12345.67, "eur": 11234.56, ... } }
#[derive(Debug, Clone, Deserialize)]
pub struct CoinGeckoResponse {
    pub bitcoin: HashMap<String, f64>,
}
/// Cached BTC prices by currency (USD, EUR, GBP, etc.)
pub static BTC_PRICES: GlobalSignal<HashMap<String, f64>> = Signal::global(HashMap::new);
/// Last fetch timestamp (unix seconds) for the CoinGecko/HTTP source.
pub static PRICE_LAST_FETCH: GlobalSignal<u64> = Signal::global(|| 0);

/// C1: Mostro-rates freshness. Timestamp of the last successful
/// `ingest_mostro_rates` call. 0 means no Mostro rates have been ingested.
/// Polled by `mostro_rates_are_fresh` and consulted by `get_btc_price`
/// to decide whether Mostro or CoinGecko/Yadio wins for a given currency.
pub static MOSTRO_RATES_FETCHED_AT: GlobalSignal<u64> = Signal::global(|| 0);

/// C1: currencies most recently ingested from the Mostro daemon's kind
/// 30078 event. Lets `get_btc_price` prefer the daemon's quote over
/// broad-market rates. Separate from `BTC_PRICES` so a CoinGecko refresh
/// doesn't silently override the daemon's quote.
pub static MOSTRO_RATES: GlobalSignal<HashMap<String, f64>> = Signal::global(HashMap::new);

/// 5-minute freshness window for Mostro rates (matches Mobile's
/// `_maxCacheAge = Duration(hours: 1)` minus tolerance for daemon's
/// 5-min publish interval).
const MOSTRO_RATES_FRESH_SECS: u64 = 5 * 60;

/// Supported fiat currencies for CoinGecko
const SUPPORTED_CURRENCIES: &str = "usd,eur,gbp,brl,try,ars,aud,mxn,cop";
/// Fetch BTC prices from CoinGecko (CORS-friendly)
pub async fn fetch_btc_prices() -> Result<(), String> {
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies={}",
        SUPPORTED_CURRENCIES,
    );
    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch prices: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("CoinGecko API error: {}", response.status()));
    }
    let data: CoinGeckoResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse prices: {}", e))?;
    let mut prices = BTC_PRICES.write();
    for (currency, price) in data.bitcoin {
        prices.insert(currency.to_uppercase(), price);
    }
    *PRICE_LAST_FETCH.write() = crate::platform::timestamp::now_secs();
    log::debug!("Updated BTC prices: {} currencies", prices.len());
    Ok(())
}

/// C1: ingest Mostro rates from a kind 30078 event's content. The
/// expected format matches Yadio: `{"BTC": {"USD": 50000.0, "EUR": ...}}`.
/// Replaces the previous Mostro-rates map entirely (matches Mobile's
/// `_cachedRates = rates` semantics — first successful source wins, no
/// merge). On parse error, leaves the existing map untouched so a single
/// malformed event doesn't wipe good data.
pub fn ingest_mostro_rates(content: &str) -> Result<usize, String> {
    let rates = parse_mostro_rates(content)?;
    let count = rates.len();
    *MOSTRO_RATES.write() = rates;
    *MOSTRO_RATES_FETCHED_AT.write() = crate::platform::timestamp::now_secs();
    log::debug!("Ingested {count} Mostro rates from kind 30078 event");
    Ok(count)
}

/// C1: pure parser for Mostro rates content. Extracted so tests can
/// exercise the parsing without a Dioxus runtime (the GlobalSignal
/// mutation in `ingest_mostro_rates` requires a runtime).
fn parse_mostro_rates(content: &str) -> Result<HashMap<String, f64>, String> {
    let parsed: HashMap<String, HashMap<String, f64>> = serde_json::from_str(content)
        .map_err(|e| format!("Mostro rates parse error: {e}"))?;
    let btc_rates = parsed
        .get("BTC")
        .ok_or_else(|| "Missing 'BTC' key in Mostro rates".to_string())?;
    let mut rates = HashMap::new();
    for (currency, price) in btc_rates {
        // Skip BTC→BTC (= 1) — matches Mobile's behavior.
        if currency.eq_ignore_ascii_case("BTC") {
            continue;
        }
        if *price > 0.0 && price.is_finite() {
            rates.insert(currency.to_uppercase(), *price);
        }
    }
    if rates.is_empty() {
        return Err("No usable rates in Mostro rates event".to_string());
    }
    Ok(rates)
}

/// C1: true if Mostro rates were ingested within the last 5 minutes.
pub fn mostro_rates_are_fresh() -> bool {
    let fetched_at = *MOSTRO_RATES_FETCHED_AT.read();
    if fetched_at == 0 {
        return false;
    }
    let now = crate::platform::timestamp::now_secs();
    now.saturating_sub(fetched_at) <= MOSTRO_RATES_FRESH_SECS
}

/// Get cached price for currency.
///
/// C1 cascade (per Mobile's `nostr_exchange_service.dart`): prefer
/// Mostro rates when fresh (they reflect the daemon's actual quote),
/// fall back to CoinGecko `BTC_PRICES`, then USD as a last resort.
pub fn get_btc_price(currency: &str) -> Option<f64> {
    let upper = currency.to_uppercase();
    if mostro_rates_are_fresh() {
        if let Some(price) = MOSTRO_RATES.read().get(&upper).copied() {
            return Some(price);
        }
    }
    let prices = BTC_PRICES.read();
    prices
        .get(&upper)
        .or_else(|| prices.get("USD"))
        .copied()
}
/// Convert fiat amount to satoshis using cached BTC price
/// Returns None if no price is available, amount is invalid, or result would overflow
///
/// # Arguments
/// * `amount` - The fiat amount to convert (must be non-negative and finite)
/// * `currency` - The fiat currency code (e.g., "USD", "EUR")
///
/// # Examples
/// ```
/// // If BTC = $100,000 USD
/// let sats = fiat_to_sats(100.0, "USD");
/// // Returns Some(100_000) (100 USD = 0.001 BTC = 100,000 sats)
/// ```
pub fn fiat_to_sats(amount: f64, currency: &str) -> Option<u64> {
    if amount < 0.0 || !amount.is_finite() {
        return None;
    }
    let btc_price = get_btc_price(currency)?;
    if btc_price <= 0.0 || !btc_price.is_finite() {
        return None;
    }
    let sats = (amount / btc_price) * 100_000_000.0;
    if sats < 0.0 || !sats.is_finite() || sats > u64::MAX as f64 {
        return None;
    }
    Some(sats as u64)
}
/// Check if CoinGecko/HTTP prices are stale (older than 5 minutes).
///
/// Note: this only reflects the HTTP source. Mostro rates have their own
/// freshness check via `mostro_rates_are_fresh`.
pub fn prices_are_stale() -> bool {
    let last_fetch = *PRICE_LAST_FETCH.read();
    if last_fetch == 0 {
        return true;
    }
    let now = crate::platform::timestamp::now_secs();
    now.saturating_sub(last_fetch) > 300
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_mostro_rates_parses_yadio_format() {
        let content = r#"{"BTC": {"USD": 50000.0, "EUR": 45000.5, "ARS": 50000000.0}}"#;
        let rates = parse_mostro_rates(content).unwrap();
        assert_eq!(rates.len(), 3);
        assert_eq!(rates.get("USD"), Some(&50000.0));
        assert_eq!(rates.get("EUR"), Some(&45000.5));
    }

    #[test]
    fn ingest_mostro_rates_skips_btc_to_btc() {
        let content = r#"{"BTC": {"BTC": 1.0, "USD": 50000.0}}"#;
        let rates = parse_mostro_rates(content).unwrap();
        assert_eq!(rates.len(), 1, "BTC→BTC should be skipped");
        assert!(rates.contains_key("USD"));
    }

    #[test]
    fn ingest_mostro_rates_rejects_missing_btc_key() {
        let content = r#"{"USD": 50000.0}"#;
        assert!(parse_mostro_rates(content).is_err());
    }

    #[test]
    fn ingest_mostro_rates_rejects_malformed_json() {
        let content = "not json";
        assert!(parse_mostro_rates(content).is_err());
    }

    #[test]
    fn ingest_mostro_rates_rejects_empty_rate_set() {
        // All-zero rates are dropped as invalid.
        let content = r#"{"BTC": {"USD": 0.0, "EUR": -1.0}}"#;
        assert!(parse_mostro_rates(content).is_err());
    }

    #[test]
    fn ingest_mostro_rates_uppercases_currency_codes() {
        let content = r#"{"BTC": {"usd": 50000.0, "eur": 45000.0}}"#;
        let rates = parse_mostro_rates(content).unwrap();
        assert!(rates.contains_key("USD"));
        assert!(rates.contains_key("EUR"));
        assert!(!rates.contains_key("usd"));
    }
}
