use crate::stores::relay::RelayDisplayInfo;
use crate::stores::relay::{self, DEFAULT_RELAYS};
use crate::stores::relay::signals::{RelayPoolStoreStoreExt, RELAY_POOL};
use dioxus::prelude::ReadableExt;
use nostr::JsonUtil;
use nostr_sdk::RelayUrl;
use std::collections::HashSet;
use url::{Host, Url};

pub fn default_relay_urls() -> Vec<RelayUrl> {
    DEFAULT_RELAYS
        .iter()
        .filter_map(|url| RelayUrl::parse(url).ok())
        .collect()
}

pub fn is_public_relay_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };

    match parsed.host() {
        Some(Host::Ipv4(ip)) => {
            if ip.is_unspecified() {
                return false;
            }
            let octets = ip.octets();
            if octets[0] == 127 || octets[0] == 10 {
                return false;
            }
            if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                return false;
            }
            if octets[0] == 192 && octets[1] == 168 {
                return false;
            }
            if octets[0] == 169 && octets[1] == 254 {
                return false;
            }
            if octets[0] == 100 && (64..=127).contains(&octets[1]) {
                return false;
            }
            if octets[0] == 198 && (octets[1] == 18 || octets[1] == 19) {
                return false;
            }
            if octets[0] == 192 && octets[1] == 0 && octets[2] == 2 {
                return false;
            }
            if octets[0] == 198 && octets[1] == 51 && octets[2] == 100 {
                return false;
            }
            if octets[0] == 203 && octets[1] == 0 && octets[2] == 113 {
                return false;
            }
            if octets[0] >= 224 {
                return false;
            }
            if octets == [255, 255, 255, 255] {
                return false;
            }
            true
        }
        Some(Host::Ipv6(ip)) => {
            if ip.is_loopback() || ip.is_unspecified() {
                return false;
            }
            let segments = ip.segments();
            let first = segments[0];
            (first & 0xfe00) != 0xfc00 && (first & 0xffc0) != 0xfe80 && (first & 0xff00) != 0xff00
        }
        Some(Host::Domain(domain)) => {
            let domain = domain.to_ascii_lowercase();
            domain != "localhost" && !domain.ends_with(".localhost") && !domain.ends_with(".local")
        }
        None => false,
    }
}

pub fn configured_write_relay_urls() -> Vec<RelayUrl> {
    let filter_relay_urls = |relay_urls: Vec<String>| -> Vec<RelayUrl> {
        let mut relay_urls: Vec<RelayUrl> = relay_urls
            .into_iter()
            .filter(|url| is_public_relay_url(url) && !relay::is_relay_blocked(url))
            .filter_map(|url| RelayUrl::parse(&url).ok())
            .collect();
        relay_urls.truncate(5);
        relay_urls
    };

    let relay_urls = filter_relay_urls(relay::get_write_relays());
    if relay_urls.is_empty() {
        let mut relay_urls = default_relay_urls()
            .into_iter()
            .filter(|relay_url| !relay::is_relay_blocked(relay_url.as_str()))
            .collect::<Vec<_>>();
        relay_urls.truncate(5);
        return relay_urls;
    }

    relay_urls
}

pub fn encode_relay_route_id(url: &str) -> String {
    urlencoding::encode(url).into_owned()
}

pub fn decode_relay_route_id(id: &str) -> Result<String, String> {
    let decoded = urlencoding::decode(id)
        .map_err(|e| format!("Invalid relay route id: {}", e))?
        .into_owned();
    RelayUrl::parse(&decoded)
        .map_err(|e| format!("Invalid relay URL: {}", e))
        .map(|url| url.to_string())
}

pub fn relay_http_url(relay_url: &str) -> Result<String, String> {
    let mut url = Url::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    match url.scheme() {
        "wss" => url
            .set_scheme("https")
            .map_err(|_| "Failed to convert relay URL to HTTPS".to_string())?,
        "ws" => url
            .set_scheme("http")
            .map_err(|_| "Failed to convert relay URL to HTTP".to_string())?,
        scheme => return Err(format!("Unsupported relay scheme: {}", scheme)),
    }
    Ok(url.to_string())
}

pub fn upgrade_to_secure_relay_url(url: &str) -> String {
    if let Some(stripped) = url.strip_prefix("ws://") {
        format!("wss://{}", stripped)
    } else {
        url.to_string()
    }
}

pub fn normalize_known_relay_url(url: &str) -> String {
    nostr::Url::parse(url)
        .map(|parsed| parsed.to_string())
        .unwrap_or_else(|_| url.to_string())
}

/// Compact display label for a relay URL: host (with port, path), scheme
/// stripped. Falls back to the raw string when unparseable.
pub fn display_relay_url(url: &str) -> String {
    if let Ok(parsed) = nostr::Url::parse(url) {
        let host = parsed.host_str().unwrap_or(url);
        let host_with_port = match parsed.port() {
            Some(port) => format!("{}:{}", host, port),
            None => host.to_string(),
        };
        if (parsed.scheme() == "wss" || parsed.scheme() == "ws") && parsed.path() == "/" {
            host_with_port
        } else {
            format!("{}{}", host_with_port, parsed.path())
        }
    } else {
        url.to_string()
    }
}

fn collect_vanish_relay_urls_from_sources(
    general_relays: impl IntoIterator<Item = String>,
    dm_relays: impl IntoIterator<Item = String>,
    search_relays: impl IntoIterator<Item = String>,
    local_relays: impl IntoIterator<Item = String>,
    broadcast_relays: impl IntoIterator<Item = String>,
    blocked_relays: impl IntoIterator<Item = String>,
) -> Vec<String> {
    let blocked_relays: HashSet<String> = blocked_relays
        .into_iter()
        .map(|url| normalize_known_relay_url(&url))
        .collect();
    let mut seen = HashSet::new();
    let mut collected = Vec::new();
    let mut extend_unique = |urls: Vec<String>| {
        for url in urls {
            let normalized = normalize_known_relay_url(&url);
            if blocked_relays.contains(&normalized) || !seen.insert(normalized.clone()) {
                continue;
            }
            collected.push(normalized);
        }
    };

    extend_unique(general_relays.into_iter().collect());
    extend_unique(dm_relays.into_iter().collect());
    extend_unique(search_relays.into_iter().collect());
    extend_unique(local_relays.into_iter().collect());
    extend_unique(broadcast_relays.into_iter().collect());

    collected
}

pub fn vanish_relay_urls() -> Vec<String> {
    let general_relays = relay::USER_RELAY_METADATA
        .read()
        .as_ref()
        .map(|metadata| {
            metadata
                .relays
                .iter()
                .map(|relay| relay.url.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let dm_relays = relay::USER_RELAY_METADATA
        .read()
        .as_ref()
        .map(|metadata| metadata.dm_relays.clone())
        .unwrap_or_default();

    collect_vanish_relay_urls_from_sources(
        general_relays,
        dm_relays,
        relay::SEARCH_RELAYS.read().clone(),
        relay::LOCAL_RELAYS.read().clone(),
        relay::BROADCAST_RELAYS.read().clone(),
        relay::BLOCKED_RELAYS.read().clone(),
    )
}

pub fn build_persisted_relay_set() -> HashSet<String> {
    let mut known_relays = HashSet::new();
    if let Some(metadata) = relay::USER_RELAY_METADATA.read().as_ref() {
        for relay in &metadata.relays {
            known_relays.insert(normalize_known_relay_url(&relay.url));
        }
        for relay in &metadata.dm_relays {
            known_relays.insert(normalize_known_relay_url(relay));
        }
    }
    for relay_url in relay::LOCAL_RELAYS.read().iter() {
        known_relays.insert(normalize_known_relay_url(relay_url));
    }
    for relay_url in relay::SEARCH_RELAYS.read().iter() {
        known_relays.insert(normalize_known_relay_url(relay_url));
    }
    for relay_url in relay::BROADCAST_RELAYS.read().iter() {
        known_relays.insert(normalize_known_relay_url(relay_url));
    }
    for relay_url in relay::BLOCKED_RELAYS.read().iter() {
        known_relays.insert(normalize_known_relay_url(relay_url));
    }
    known_relays
}

pub fn build_known_relay_set(connection_info: Option<&[RelayDisplayInfo]>) -> HashSet<String> {
    let mut known_relays = build_persisted_relay_set();
    if let Some(connection_info) = connection_info {
        for info in connection_info {
            known_relays.insert(normalize_known_relay_url(&info.url));
        }
    }
    known_relays
}

const MAX_NIP11_BYTES: usize = 1_048_576;

#[cfg(feature = "web")]
pub async fn fetch_nip11_body(url: &str) -> Result<String, String> {
    use futures::FutureExt;
    use js_sys::{Reflect, Uint8Array};
    use wasm_bindgen::JsCast;
    use web_sys::AbortController;
    use web_sys::{Request, RequestInit, RequestMode, RequestRedirect, Response};
    use wasm_bindgen_futures::JsFuture;

    let controller = AbortController::new()
        .map_err(|e| format!("Failed to create abort controller: {:?}", e))?;
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    opts.set_redirect(RequestRedirect::Error);
    opts.set_signal(Some(&controller.signal()));

    let request = Request::new_with_str_and_init(url, &opts)
        .map_err(|e| format!("Failed to create relay metadata request: {:?}", e))?;
    request
        .headers()
        .set("Accept", "application/nostr+json")
        .map_err(|e| format!("Failed to set relay metadata headers: {:?}", e))?;

    let window = web_sys::window().ok_or("No window object")?;
    let deadline = crate::platform::timer::sleep_ms(15_000).fuse();
    let request = JsFuture::from(window.fetch_with_request(&request)).fuse();
    futures::pin_mut!(request, deadline);
    let response = futures::select! {
        resp = request => resp,
        _ = deadline => {
            controller.abort();
            return Err("Request timeout".to_string());
        },
    }
    .map_err(|e| format!("Failed to fetch relay metadata: {:?}", e))?;

    let response: Response = response
        .dyn_into()
        .map_err(|_| "Failed to cast relay metadata response".to_string())?;
    if !response.ok() {
        return Err(format!(
            "Relay metadata request failed: {}",
            response.status()
        ));
    }

    let mut bytes = Vec::new();
    let mut total_bytes = 0usize;
    let body = response
        .body()
        .ok_or_else(|| "Relay metadata response body missing".to_string())?;
    let reader = body
        .get_reader()
        .dyn_into::<web_sys::ReadableStreamDefaultReader>()
        .map_err(|_| "Failed to create relay metadata stream reader".to_string())?;
    loop {
        let read = JsFuture::from(reader.read()).fuse();
        futures::pin_mut!(read);
        let chunk = futures::select! {
            read = read => read,
            _ = deadline => {
                controller.abort();
                return Err("Request timeout".to_string());
            },
        }
        .map_err(|e| format!("Failed to read relay metadata body: {:?}", e))?;
        let done = Reflect::get(&chunk, &"done".into())
            .map_err(|e| format!("Failed to inspect relay metadata stream state: {:?}", e))?
            .as_bool()
            .unwrap_or(false);
        if done {
            break;
        }
        let value = Reflect::get(&chunk, &"value".into())
            .map_err(|e| format!("Failed to read relay metadata stream chunk: {:?}", e))?;
        let chunk = Uint8Array::new(&value).to_vec();
        total_bytes = total_bytes
            .checked_add(chunk.len())
            .ok_or_else(|| format!("Relay metadata exceeds {} bytes", MAX_NIP11_BYTES))?;
        if total_bytes > MAX_NIP11_BYTES {
            controller.abort();
            return Err(format!("Relay metadata exceeds {} bytes", MAX_NIP11_BYTES));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|e| format!("Failed to decode relay metadata as UTF-8: {}", e))
}

#[cfg(not(feature = "web"))]
pub async fn fetch_nip11_body(url: &str) -> Result<String, String> {
    use crate::platform::http::http_client;
    use futures::StreamExt;

    let response = http_client()
        .map_err(|e| format!("HTTP client init failed: {}", e))?
        .get(url)
        .header("Accept", "application/nostr+json")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "Request timeout".to_string()
            } else {
                format!("Failed to fetch relay metadata: {}", e)
            }
        })?;

    if !response.status().is_success() {
        return Err(format!(
            "Relay metadata request failed: {}",
            response.status()
        ));
    }

    let mut stream = response.bytes_stream();
    let mut body = Vec::new();
    let mut total_bytes = 0usize;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Failed to stream relay metadata: {}", e))?;
        total_bytes = total_bytes
            .checked_add(chunk.len())
            .ok_or_else(|| format!("Relay metadata exceeds {} bytes", MAX_NIP11_BYTES))?;
        if total_bytes > MAX_NIP11_BYTES {
            return Err(format!("Relay metadata exceeds {} bytes", MAX_NIP11_BYTES));
        }
        body.extend_from_slice(&chunk);
    }

    String::from_utf8(body).map_err(|e| format!("Failed to decode relay metadata as UTF-8: {}", e))
}

pub async fn check_nip42_support() -> bool {
    let write_relays: Vec<String> = {
        let pool = RELAY_POOL.read();
        pool.data()
            .read()
            .iter()
            .filter(|r| {
                r.has_write
                    && !matches!(
                        r.status,
                        nostr_relay_pool::RelayStatus::Disconnected
                    )
            })
            .map(|r| r.url.clone())
            .take(5)
            .collect()
    };

    for url in &write_relays {
        if let Ok(http_url) = relay_http_url(url) {
            if let Ok(body) = fetch_nip11_body(&http_url).await {
                if let Ok(doc) =
                    nostr_sdk::nips::nip11::RelayInformationDocument::from_json(&body)
                {
                    if doc
                        .supported_nips
                        .as_ref()
                        .is_some_and(|nips| nips.contains(&42))
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_route_id_round_trips() {
        let url = "wss://relay.example.com/path?q=1";
        let encoded = encode_relay_route_id(url);
        let decoded = decode_relay_route_id(&encoded).expect("route id should decode");
        assert_eq!(decoded, url);
    }

    #[test]
    fn relay_route_id_rejects_invalid_url() {
        let encoded = encode_relay_route_id("not-a-relay-url");
        assert!(decode_relay_route_id(&encoded).is_err());
    }

    #[test]
    fn relay_http_url_converts_secure_ws() {
        let http = relay_http_url("wss://relay.example.com/path?q=1").expect("should convert");
        assert_eq!(http, "https://relay.example.com/path?q=1");
    }

    #[test]
    fn relay_http_url_converts_insecure_ws() {
        let http = relay_http_url("ws://relay.example.com:8080").expect("should convert");
        assert_eq!(http, "http://relay.example.com:8080/");
    }

    #[test]
    fn display_relay_url_strips_scheme_and_root_path() {
        assert_eq!(display_relay_url("wss://relay.example.com"), "relay.example.com");
        assert_eq!(
            display_relay_url("wss://relay.example.com:8080"),
            "relay.example.com:8080"
        );
        assert_eq!(
            display_relay_url("wss://relay.example.com/path"),
            "relay.example.com/path"
        );
        assert_eq!(display_relay_url("not a url"), "not a url");
    }

    #[test]
    fn collect_vanish_relay_urls_deduplicates_and_excludes_blocked_relays() {
        let urls = collect_vanish_relay_urls_from_sources(
            vec![
                "wss://relay.one/".to_string(),
                "wss://relay.two/".to_string(),
                "wss://relay.one".to_string(),
            ],
            vec!["wss://relay.dm/".to_string()],
            vec!["wss://relay.search/".to_string()],
            vec!["ws://localhost:8080/".to_string()],
            vec!["wss://relay.broadcast/".to_string()],
            vec![
                "wss://relay.two".to_string(),
                "wss://relay.broadcast".to_string(),
            ],
        );

        assert_eq!(
            urls,
            vec![
                "wss://relay.one/".to_string(),
                "wss://relay.dm/".to_string(),
                "wss://relay.search/".to_string(),
                "ws://localhost:8080/".to_string(),
            ]
        );
    }
}
