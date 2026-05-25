use nostr_sdk::prelude::*;

pub const KIND_RELAY_DISCOVERY: u16 = 30166;
#[allow(dead_code)]
pub const KIND_RELAY_MONITOR: u16 = 10166;

#[derive(Clone, Debug, PartialEq)]
pub struct RelayDiscoveryData {
    pub relay_url: String,
    pub rtt_open: Option<u64>,
    pub rtt_read: Option<u64>,
    pub rtt_write: Option<u64>,
    pub network_type: Option<String>,
    pub relay_types: Vec<String>,
    pub supported_nips: Vec<u32>,
    pub requirements: Vec<(String, bool)>,
    pub topics: Vec<String>,
    pub accepted_kinds: Vec<(u64, bool)>,
    pub geohashes: Vec<String>,
    pub nip11_json: Option<String>,
    pub monitor_pubkey: PublicKey,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub struct RelayMonitorAnnouncement {
    pub pubkey: PublicKey,
    pub frequency: Option<u64>,
    pub checks: Vec<String>,
    pub geohashes: Vec<String>,
}

fn parse_negated(value: &str) -> (String, bool) {
    if let Some(stripped) = value.strip_prefix('!') {
        (stripped.to_string(), true)
    } else {
        (value.to_string(), false)
    }
}

pub fn parse_relay_discovery(event: &Event) -> Option<RelayDiscoveryData> {
    let relay_url = event.tags.identifier()?.to_string();

    let rtt_open = event
        .tags
        .find(TagKind::custom("rtt-open"))
        .and_then(|t| t.content())
        .and_then(|v| v.parse::<u64>().ok());

    let rtt_read = event
        .tags
        .find(TagKind::custom("rtt-read"))
        .and_then(|t| t.content())
        .and_then(|v| v.parse::<u64>().ok());

    let rtt_write = event
        .tags
        .find(TagKind::custom("rtt-write"))
        .and_then(|t| t.content())
        .and_then(|v| v.parse::<u64>().ok());

    let network_type = event
        .tags
        .find(TagKind::single_letter(Alphabet::N, false))
        .and_then(|t| t.content())
        .map(|s| s.to_string());

    let relay_types: Vec<String> = event
        .tags
        .filter(TagKind::single_letter(Alphabet::T, true))
        .filter_map(|t| t.content().map(|s| s.to_string()))
        .collect();

    let supported_nips: Vec<u32> = event
        .tags
        .filter(TagKind::single_letter(Alphabet::N, true))
        .filter_map(|t| t.content().and_then(|v| v.parse::<u32>().ok()))
        .collect();

    let requirements: Vec<(String, bool)> = event
        .tags
        .filter(TagKind::single_letter(Alphabet::R, true))
        .filter_map(|t| t.content().map(parse_negated))
        .collect();

    let topics: Vec<String> = event
        .tags
        .hashtags()
        .map(|s| s.to_string())
        .collect();

    let accepted_kinds: Vec<(u64, bool)> = event
        .tags
        .filter(TagKind::single_letter(Alphabet::K, true))
        .filter_map(|t| {
            t.content().map(|v| {
                let (val, neg) = parse_negated(v);
                (val.parse::<u64>().unwrap_or(0), neg)
            })
        })
        .collect();

    let geohashes: Vec<String> = event
        .tags
        .filter(TagKind::single_letter(Alphabet::G, false))
        .filter_map(|t| t.content().map(|s| s.to_string()))
        .collect();

    let nip11_json = {
        let content = &event.content;
        if content.is_empty() {
            None
        } else {
            Some(content.to_string())
        }
    };

    Some(RelayDiscoveryData {
        relay_url,
        rtt_open,
        rtt_read,
        rtt_write,
        network_type,
        relay_types,
        supported_nips,
        requirements,
        topics,
        accepted_kinds,
        geohashes,
        nip11_json,
        monitor_pubkey: event.pubkey,
        created_at: event.created_at,
    })
}

#[allow(dead_code)]
pub fn parse_monitor_announcement(event: &Event) -> RelayMonitorAnnouncement {
    let frequency = event
        .tags
        .find(TagKind::custom("frequency"))
        .and_then(|t| t.content())
        .and_then(|v| v.parse::<u64>().ok());

    let checks: Vec<String> = event
        .tags
        .filter(TagKind::single_letter(Alphabet::C, false))
        .filter_map(|t| t.content().map(|s| s.to_string()))
        .collect();

    let geohashes: Vec<String> = event
        .tags
        .filter(TagKind::single_letter(Alphabet::G, false))
        .filter_map(|t| t.content().map(|s| s.to_string()))
        .collect();

    RelayMonitorAnnouncement {
        pubkey: event.pubkey,
        frequency,
        checks,
        geohashes,
    }
}

pub fn discovery_filter(limit: usize) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_RELAY_DISCOVERY))
        .limit(limit)
}

pub fn discovery_filter_for_relay(relay_url: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_RELAY_DISCOVERY))
        .identifier(relay_url)
        .limit(10)
}

#[allow(dead_code)]
pub fn monitor_announcement_filter() -> Filter {
    Filter::new()
        .kind(Kind::Custom(KIND_RELAY_MONITOR))
        .limit(200)
}

pub fn aggregate_discoveries(discoveries: &[RelayDiscoveryData]) -> Vec<RelayDiscoveryData> {
    let mut best: std::collections::HashMap<String, RelayDiscoveryData> =
        std::collections::HashMap::new();
    for d in discoveries {
        let key = d.relay_url.clone();
        let dominated = match best.get(&key) {
            Some(existing) => d.created_at > existing.created_at,
            None => true,
        };
        if dominated {
            best.insert(key, d.clone());
        }
    }
    let mut result: Vec<RelayDiscoveryData> = best.into_values().collect();
    result.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    result
}
