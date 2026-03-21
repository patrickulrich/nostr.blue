use crate::stores::relay::RelayDisplayInfo;
use crate::stores::relay::{self, DEFAULT_RELAYS};
use dioxus::prelude::ReadableExt;
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

pub fn normalize_known_relay_url(url: &str) -> String {
    nostr::Url::parse(url)
        .map(|parsed| parsed.to_string())
        .unwrap_or_else(|_| url.to_string())
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
}
