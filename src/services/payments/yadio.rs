use crate::platform::http::http_client;
use dioxus::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
struct YadioResponse {
    #[serde(default)]
    btc: HashMap<String, f64>,
}

pub static YADIO_RATES: GlobalSignal<HashMap<String, f64>> = Signal::global(HashMap::new);
pub static YADIO_LAST_FETCH: GlobalSignal<u64> = Signal::global(|| 0);

pub async fn fetch_yadio_rates() -> Result<(), String> {
    let url = "https://api.yadio.io/exrates/BTC";
    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {e}"))?
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Yadio fetch failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Yadio API error: {}", response.status()));
    }
    let data: YadioResponse = response
        .json()
        .await
        .map_err(|e| format!("Yadio parse error: {e}"))?;
    let mut rates = YADIO_RATES.write();
    rates.clear();
    for (currency, rate) in data.btc {
        rates.insert(currency.to_uppercase(), rate);
    }
    *YADIO_LAST_FETCH.write() = crate::platform::timestamp::now_secs();
    log::debug!("Updated Yadio rates: {} currencies", rates.len());
    Ok(())
}

pub fn is_currency_supported(code: &str) -> bool {
    let rates = YADIO_RATES.read();
    rates.contains_key(&code.to_uppercase())
}

#[allow(dead_code)]
pub fn supported_currencies() -> Vec<String> {
    let rates = YADIO_RATES.read();
    let mut keys: Vec<String> = rates.keys().cloned().collect();
    keys.sort();
    keys
}

pub fn rates_are_stale() -> bool {
    let last = *YADIO_LAST_FETCH.read();
    if last == 0 {
        return true;
    }
    let now = crate::platform::timestamp::now_secs();
    now.saturating_sub(last) > 600
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_yadio_module_compiles() {
        assert!(true);
    }
}
