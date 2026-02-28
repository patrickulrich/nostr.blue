//! Bitcoin Price Service
//!
//! Fetches BTC prices from CoinGecko API for fiat currency conversions.
//! CoinGecko supports CORS and provides free API access.
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
/// Last fetch timestamp (unix seconds)
pub static PRICE_LAST_FETCH: GlobalSignal<u64> = Signal::global(|| 0);
/// Supported fiat currencies for CoinGecko
const SUPPORTED_CURRENCIES: &str = "usd,eur,gbp,brl,try,ars,aud,mxn,cop";
/// Fetch BTC prices from CoinGecko (CORS-friendly)
pub async fn fetch_btc_prices() -> Result<(), String> {
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies={}",
        SUPPORTED_CURRENCIES,
    );
    #[cfg(not(feature = "web"))]
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;
    #[cfg(feature = "web")]
    let client = reqwest::Client::new();
    let response = client.get(&url).send()
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
/// Get cached price for currency
/// Falls back to USD for unknown currencies
pub fn get_btc_price(currency: &str) -> Option<f64> {
    let prices = BTC_PRICES.read();
    prices.get(&currency.to_uppercase()).or_else(|| prices.get("USD")).copied()
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
/// Check if prices are stale (older than 5 minutes)
pub fn prices_are_stale() -> bool {
    let last_fetch = *PRICE_LAST_FETCH.read();
    if last_fetch == 0 {
        return true;
    }
    let now = crate::platform::timestamp::now_secs();
    now.saturating_sub(last_fetch) > 300
}
