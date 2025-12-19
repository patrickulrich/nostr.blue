//! Bitcoin Price Service
//!
//! Fetches BTC prices from CoinGecko API for fiat currency conversions.
//! CoinGecko supports CORS and provides free API access.

use gloo_net::http::Request;
use serde::Deserialize;
use dioxus::prelude::*;
use std::collections::HashMap;

/// CoinGecko simple price response
/// Returns: { "bitcoin": { "usd": 12345.67, "eur": 11234.56, ... } }
#[derive(Debug, Clone, Deserialize)]
pub struct CoinGeckoResponse {
    pub bitcoin: HashMap<String, f64>,
}

/// Cached BTC prices by currency (USD, EUR, GBP, etc.)
pub static BTC_PRICES: GlobalSignal<HashMap<String, f64>> =
    Signal::global(|| HashMap::new());

/// Last fetch timestamp (unix seconds)
pub static PRICE_LAST_FETCH: GlobalSignal<u64> = Signal::global(|| 0);

/// Supported fiat currencies for CoinGecko
const SUPPORTED_CURRENCIES: &str = "usd,eur,gbp,brl,try,ars,aud,mxn,cop";

/// Fetch BTC prices from CoinGecko (CORS-friendly)
pub async fn fetch_btc_prices() -> Result<(), String> {
    let url = format!(
        "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies={}",
        SUPPORTED_CURRENCIES
    );

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch prices: {}", e))?;

    if !response.ok() {
        return Err(format!("CoinGecko API error: {}", response.status()));
    }

    let data: CoinGeckoResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse prices: {}", e))?;

    // Cache prices (convert keys to uppercase for consistency)
    let mut prices = BTC_PRICES.write();
    for (currency, price) in data.bitcoin {
        prices.insert(currency.to_uppercase(), price);
    }

    // Update timestamp
    *PRICE_LAST_FETCH.write() = js_sys::Date::now() as u64 / 1000;

    log::debug!("Updated BTC prices: {} currencies", prices.len());

    Ok(())
}

/// Get cached price for currency
/// Falls back to USD for unknown currencies
pub fn get_btc_price(currency: &str) -> Option<f64> {
    let prices = BTC_PRICES.read();
    prices.get(&currency.to_uppercase())
        .or_else(|| prices.get("USD"))  // Fallback to USD
        .copied()
}
