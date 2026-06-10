//! Trusted Mostro community nodes
//!
//! Static list of known Mostro daemon instances, curated from the Mostro
//! community. Used to seed the default node configuration and power the
//! community selector UI.

use super::node_config::MostroNodeConfig;

/// A single trusted Mostro community with its daemon pubkey and relays.
#[derive(Clone, Debug, PartialEq)]
pub struct MostroCommunity {
    /// Daemon hex pubkey.
    pub pubkey: &'static str,
    /// Human-readable region label.
    pub region: &'static str,
    /// Relay URLs where this daemon can be reached.
    pub relays: &'static [&'static str],
}

/// All trusted communities. The first entry is used as the default when
/// the user has not explicitly configured a node.
pub const COMMUNITIES: &[MostroCommunity] = &[
    MostroCommunity {
        pubkey: "82fa8cb978b43c79b2156585bac2c011176a21d2aead6d9f7c575c005be88390",
        region: "Default",
        relays: &[
            "wss://mostro-p2p.tech",
            "wss://nos.lol",
            "wss://relay.mostro.network",
        ],
    },
    MostroCommunity {
        pubkey: "00000235a3e904cfe1213a8a54d6f1ec1bef7cc6bfaabd6193e82931ccf1366a",
        region: "Cuba",
        relays: &[
            "wss://relay.mostro.network",
            "wss://nos.lol",
        ],
    },
    MostroCommunity {
        pubkey: "0000cc02101ec29eea9ce623258752b9d7da66c27845ed26846dd0b0fc736b40",
        region: "Spain",
        relays: &[
            "wss://relay.mostro.network",
            "wss://nos.lol",
        ],
    },
    MostroCommunity {
        pubkey: "00000978acc594c506976c655b6decbf2d4af25ffdaa6680f2a9568b0a88441b",
        region: "Colombia",
        relays: &[
            "wss://relay.mostro.network",
            "wss://nos.lol",
        ],
    },
    MostroCommunity {
        pubkey: "00007cb3305fb972f5cc83f83a8fbca1e64e93c9d1369880a9fd62ef95d23f91",
        region: "Bolivia",
        relays: &[
            "wss://relay.mostro.network",
            "wss://nos.lol",
        ],
    },
    MostroCommunity {
        pubkey: "000009ee1e4b1dc7add19ab30e4ef854d7b562e208b62686fd9002b50b24dabb",
        region: "Venezuela",
        relays: &[
            "wss://relay.mostro.network",
            "wss://nos.lol",
        ],
    },
];

/// Return the default node config (first community).
#[allow(dead_code)]
pub fn default_node_config() -> Option<MostroNodeConfig> {
    let c = COMMUNITIES.first()?;
    MostroNodeConfig::new(
        c.pubkey.to_string(),
        c.relays.iter().map(|s| s.to_string()).collect(),
        Some(c.region.to_string()),
    )
    .ok()
}

/// Look up a community by its daemon pubkey.
#[allow(dead_code)]
pub fn find_by_pubkey(pubkey: &str) -> Option<&'static MostroCommunity> {
    COMMUNITIES.iter().find(|c| c.pubkey == pubkey)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_communities_non_empty() {
        assert!(!COMMUNITIES.is_empty());
    }

    #[test]
    fn test_default_node_config_is_valid() {
        let c = COMMUNITIES.first().unwrap();
        assert!(!c.pubkey.is_empty());
        assert!(!c.relays.is_empty());
        assert_eq!(c.pubkey.len(), 64);
    }

    #[test]
    fn test_find_by_pubkey() {
        let first = COMMUNITIES.first().unwrap();
        let found = find_by_pubkey(first.pubkey);
        assert!(found.is_some());
        assert_eq!(found.unwrap().region, first.region);
    }

    #[test]
    fn test_find_by_unknown_pubkey_returns_none() {
        assert!(find_by_pubkey("00dead").is_none());
    }
}
