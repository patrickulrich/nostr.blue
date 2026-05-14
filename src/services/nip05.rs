use dioxus::prelude::*;
use instant::Instant;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Nip05Status {
    Unknown,
    Verifying,
    Verified,
    Impersonator,
    Error,
}

#[derive(Clone, Debug)]
pub(crate) struct CacheEntry {
    status: Nip05Status,
    checked_at: Instant,
}

const CACHE_TTL_SECS: u64 = 8 * 60 * 60;

pub(super) static NIP05_CACHE: GlobalSignal<HashMap<String, CacheEntry>> =
    Signal::global(HashMap::new);

fn cache_key(pubkey: &str, nip05: &str) -> String {
    format!("{}:{}", pubkey, nip05.to_lowercase())
}

pub fn get_nip05_status(pubkey: &str, nip05: &str) -> Nip05Status {
    let key = cache_key(pubkey, nip05);
    let cache = NIP05_CACHE.peek();
    if let Some(entry) = cache.get(&key) {
        match &entry.status {
            Nip05Status::Verified | Nip05Status::Impersonator => return entry.status.clone(),
            Nip05Status::Error => {
                if entry.checked_at.elapsed().as_secs() < CACHE_TTL_SECS {
                    return entry.status.clone();
                }
            }
            Nip05Status::Verifying => return entry.status.clone(),
            Nip05Status::Unknown => {}
        }
    }
    Nip05Status::Unknown
}

pub fn verify_nip05(pubkey: &str, nip05: &str) {
    let key = cache_key(pubkey, nip05);
    {
        let cache = NIP05_CACHE.peek();
        if let Some(entry) = cache.get(&key) {
            match &entry.status {
                Nip05Status::Verified | Nip05Status::Impersonator => return,
                Nip05Status::Verifying => return,
                Nip05Status::Error => {
                    if entry.checked_at.elapsed().as_secs() < CACHE_TTL_SECS {
                        return;
                    }
                }
                Nip05Status::Unknown => {}
            }
        }
    }

    NIP05_CACHE.write().insert(
        key.clone(),
        CacheEntry {
            status: Nip05Status::Verifying,
            checked_at: Instant::now(),
        },
    );

    let pubkey_hex = pubkey.to_string();
    let nip05_str = nip05.to_string();
    let key_clone = key.clone();
    spawn(async move {
        let result = do_verify(&pubkey_hex, &nip05_str).await;
        NIP05_CACHE.write().insert(
            key_clone,
            CacheEntry {
                status: result,
                checked_at: Instant::now(),
            },
        );
    });
}

async fn do_verify(pubkey_hex: &str, nip05: &str) -> Nip05Status {
    let (name, domain) = match nip05.split_once('@') {
        Some((n, d)) => (n, d),
        None => return Nip05Status::Error,
    };

    if name.is_empty() || domain.is_empty() {
        return Nip05Status::Error;
    }

    let url = format!(
        "https://{}/.well-known/nostr.json?name={}",
        domain,
        urlencoding::encode(name)
    );

    let client = match crate::platform::http::http_client() {
        Ok(c) => c,
        Err(_) => return Nip05Status::Error,
    };

    let resp = match client
        .get(&url)
        .header("Accept", "application/json")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::debug!("NIP-05 fetch failed for {}: {}", nip05, e);
            return Nip05Status::Error;
        }
    };

    if !resp.status().is_success() {
        log::debug!("NIP-05 returned {} for {}", resp.status(), nip05);
        return Nip05Status::Error;
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            log::debug!("NIP-05 parse failed for {}: {}", nip05, e);
            return Nip05Status::Error;
        }
    };

    let names = match body.get("names").and_then(|n| n.as_object()) {
        Some(m) => m,
        None => return Nip05Status::Error,
    };

    let name_lower = name.to_lowercase();
    let returned_pubkey = match names.get(&name_lower).and_then(|v| v.as_str()) {
        Some(pk) => pk,
        None => return Nip05Status::Impersonator,
    };

    if returned_pubkey.to_lowercase() == pubkey_hex.to_lowercase() {
        log::debug!("NIP-05 verified: {} matches {}", nip05, pubkey_hex);
        Nip05Status::Verified
    } else {
        log::debug!(
            "NIP-05 impersonator: {} returned {} expected {}",
            nip05,
            returned_pubkey,
            pubkey_hex
        );
        Nip05Status::Impersonator
    }
}

pub fn retry_nip05(pubkey: &str, nip05: &str) {
    let key = cache_key(pubkey, nip05);
    NIP05_CACHE.write().remove(&key);
    verify_nip05(pubkey, nip05);
}
