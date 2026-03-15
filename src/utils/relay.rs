use crate::stores::relay::{self, DEFAULT_RELAYS};
use nostr_sdk::RelayUrl;
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
            true
        }
        Some(Host::Ipv6(ip)) => {
            if ip.is_loopback() || ip.is_unspecified() {
                return false;
            }
            let segments = ip.segments();
            let first = segments[0];
            (first & 0xfe00) != 0xfc00 && (first & 0xffc0) != 0xfe80
        }
        Some(Host::Domain(domain)) => {
            let domain = domain.to_ascii_lowercase();
            domain != "localhost"
                && !domain.ends_with(".localhost")
                && !domain.ends_with(".local")
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
