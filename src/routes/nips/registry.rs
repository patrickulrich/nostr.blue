//! Hardcoded registry of protocol specifications supported by nostr.blue.
//!
//! This is the single source of truth for the "Our NIPs" tab on the `/nips`
//! page. Each entry maps to a route at `/nips/<route_id>` rendered by
//! `nip_detail.rs`.
//!
//! To add rich per-spec implementation notes for a spec:
//!   1. Create `content/<route_id>.md` next to this file (e.g.
//!      `content/nip_01.md`).
//!   2. Flip the matching entry's `notes` field from `None` to
//!      `Some(include_str!("content/nip_01.md"))`.
//!
//! Until then, the detail page renders a stub with the upstream link.

/// The kind of protocol specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecType {
    Nip,
    Nut,
    Bud,
    Nkbip,
    Market,
}

impl SpecType {
    /// Lowercase prefix used in route IDs and card badges (e.g. `"nip"`, `"nut"`).
    pub const fn prefix(self) -> &'static str {
        match self {
            SpecType::Nip => "nip",
            SpecType::Nut => "nut",
            SpecType::Bud => "bud",
            SpecType::Nkbip => "nkbip",
            SpecType::Market => "market",
        }
    }

    /// Human-readable label shown in filter chips and card badges.
    pub const fn label(self) -> &'static str {
        match self {
            SpecType::Nip => "NIP",
            SpecType::Nut => "NUT",
            SpecType::Bud => "BUD",
            SpecType::Nkbip => "NKBIP",
            SpecType::Market => "Market Spec",
        }
    }

    /// Plural label for filter chips.
    pub const fn label_plural(self) -> &'static str {
        match self {
            SpecType::Nip => "NIPs",
            SpecType::Nut => "NUTs",
            SpecType::Bud => "BUDs",
            SpecType::Nkbip => "NKBIPs",
            SpecType::Market => "Market",
        }
    }

    /// Parse a route-ID prefix back into a `SpecType`.
    pub fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "nip" => Some(SpecType::Nip),
            "nut" => Some(SpecType::Nut),
            "bud" => Some(SpecType::Bud),
            "nkbip" => Some(SpecType::Nkbip),
            "market" => Some(SpecType::Market),
            _ => None,
        }
    }
}

/// One supported specification entry.
///
/// `notes` holds optional rich implementation notes embedded at compile time.
/// `None` means the detail page falls back to a stub with the upstream link.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SupportedSpec {
    pub spec_type: SpecType,
    /// Spec number without prefix (e.g. `"01"`, `"C7"`). Empty for Market.
    pub number: &'static str,
    pub title: &'static str,
    /// Optional related kind numbers (mostly NKBIPs).
    pub kinds: Option<&'static str>,
    /// Canonical upstream URL for the spec.
    pub upstream_url: &'static str,
    /// Optional rich implementation notes (embedded via `include_str!`).
    pub notes: Option<&'static str>,
}

impl SupportedSpec {
    /// The route ID used in `Route::NipDetail { nip_id }`.
    pub fn route_id(&self) -> String {
        match self.spec_type {
            SpecType::Market => "market-spec".to_string(),
            _ => format!("{}-{}", self.spec_type.prefix(), self.number),
        }
    }

    /// Badge text shown on a card (e.g. `"NIP-01"`, `"Market Spec"`).
    pub fn badge(&self) -> String {
        match self.spec_type {
            SpecType::Market => "Market Spec".to_string(),
            _ => format!("{}-{}", self.spec_type.label(), self.number),
        }
    }
}

const NIP_URL_BASE: &str = "https://github.com/nostr-protocol/nips/blob/master/";
const NUT_URL_BASE: &str = "https://github.com/cashubtc/nuts/blob/main/";
const BUD_URL_BASE: &str = "https://github.com/hzrd149/blossom/blob/master/buds/";

const fn nip_entry(num: &'static str, title: &'static str) -> SupportedSpec {
    SupportedSpec {
        spec_type: SpecType::Nip,
        number: num,
        title,
        kinds: None,
        upstream_url: NIP_URL_BASE,
        notes: None,
    }
}

const fn nut_entry(num: &'static str, title: &'static str) -> SupportedSpec {
    SupportedSpec {
        spec_type: SpecType::Nut,
        number: num,
        title,
        kinds: None,
        upstream_url: NUT_URL_BASE,
        notes: None,
    }
}

const fn bud_entry(num: &'static str, title: &'static str) -> SupportedSpec {
    SupportedSpec {
        spec_type: SpecType::Bud,
        number: num,
        title,
        kinds: None,
        upstream_url: BUD_URL_BASE,
        notes: None,
    }
}

/// All protocol specifications supported by nostr.blue.
///
/// Seeded from the README support tables. This is the source of truth —
/// update this list (and the README tables) when support changes.
pub const SUPPORTED_SPECS: &[SupportedSpec] = &[
    // --- NIPs ---
    nip_entry("01", "Basic protocol"),
    nip_entry("02", "Follow List"),
    nip_entry("04", "Encrypted DM (legacy)"),
    nip_entry("05", "DNS Identifiers"),
    nip_entry("06", "Key derivation from mnemonic"),
    nip_entry("07", "Browser extension signing"),
    nip_entry("09", "Event Deletion"),
    nip_entry("10", "Text Notes and Threads"),
    nip_entry("11", "Relay Information Document"),
    nip_entry("17", "Private Direct Messages"),
    nip_entry("18", "Reposts"),
    nip_entry("19", "bech32 identifiers"),
    nip_entry("21", "nostr: URI scheme"),
    nip_entry("22", "Comments"),
    nip_entry("23", "Long-form Content"),
    nip_entry("24", "Extra metadata fields"),
    nip_entry("25", "Reactions"),
    nip_entry("27", "Text Note References"),
    nip_entry("28", "Public Chat"),
    nip_entry("30", "Custom Emoji"),
    nip_entry("34", "Git stuff"),
    nip_entry("36", "Sensitive Content"),
    nip_entry("37", "Draft Events"),
    nip_entry("38", "User Statuses"),
    nip_entry("39", "External Identities"),
    nip_entry("40", "Expiration Timestamp"),
    nip_entry("41", "Editable Short Notes"),
    nip_entry("42", "Client Auth to Relays"),
    nip_entry("44", "Encrypted Payloads"),
    nip_entry("45", "Counting results"),
    nip_entry("46", "Remote Signing"),
    nip_entry("47", "Wallet Connect"),
    nip_entry("48", "Proxy Tags"),
    nip_entry("49", "Private Key Encryption"),
    nip_entry("50", "Search Capability"),
    nip_entry("51", "Lists"),
    nip_entry("52", "Calendar Events"),
    nip_entry("53", "Live Activities"),
    nip_entry("54", "Wiki"),
    nip_entry("55", "Android Signer Application"),
    nip_entry("56", "Reporting"),
    nip_entry("57", "Lightning Zaps"),
    nip_entry("58", "Badges"),
    nip_entry("59", "Gift Wrap"),
    nip_entry("60", "Cashu Wallet"),
    nip_entry("61", "Nutzaps"),
    nip_entry("62", "Request to Vanish"),
    nip_entry("64", "Chess (PGN)"),
    nip_entry("65", "Relay List Metadata"),
    nip_entry("66", "Relay Discovery"),
    nip_entry("68", "Picture-first feeds"),
    nip_entry("69", "P2P Order events"),
    nip_entry("70", "Protected Events"),
    nip_entry("71", "Video Events"),
    nip_entry("72", "Moderated Communities"),
    nip_entry("73", "External Content IDs"),
    nip_entry("75", "Zap Goals"),
    nip_entry("77", "Negentropy Syncing"),
    nip_entry("78", "App-specific data"),
    nip_entry("84", "Highlights"),
    nip_entry("87", "Mint Discoverability"),
    nip_entry("88", "Polls"),
    nip_entry("89", "App Handlers"),
    nip_entry("90", "Data Vending Machines"),
    nip_entry("92", "Media Attachments"),
    nip_entry("94", "File Metadata"),
    nip_entry("96", "HTTP File Storage"),
    nip_entry("98", "HTTP Auth"),
    nip_entry("99", "Classified Listings"),
    nip_entry("A0", "Voice Messages"),
    nip_entry("B0", "Web Bookmarks"),
    nip_entry("B7", "Blossom"),
    nip_entry("C0", "Code Snippets"),
    // --- NUTs (Cashu) ---
    nut_entry("00", "Notation and Encoding"),
    nut_entry("01", "Mint public keys"),
    nut_entry("02", "Keysets and fees"),
    nut_entry("03", "Swapping tokens"),
    nut_entry("04", "Minting tokens"),
    nut_entry("05", "Melting tokens"),
    nut_entry("06", "Mint info"),
    nut_entry("07", "Token state check"),
    nut_entry("08", "Overpaid fees"),
    nut_entry("09", "Signature restore"),
    nut_entry("10", "Spending conditions"),
    nut_entry("11", "P2PK"),
    nut_entry("12", "DLEQ proofs"),
    nut_entry("13", "Deterministic secrets"),
    nut_entry("14", "HTLCs"),
    nut_entry("15", "Multi-path payments"),
    nut_entry("17", "WebSocket subscriptions"),
    nut_entry("18", "Payment requests"),
    nut_entry("19", "Cached responses"),
    nut_entry("20", "Signature on mint quote"),
    nut_entry("21", "Clear authentication"),
    nut_entry("22", "Blind authentication"),
    // --- BUDs (Blossom) ---
    bud_entry("01", "Server requirements"),
    bud_entry("02", "Blob upload/management"),
    bud_entry("03", "User Server List"),
    bud_entry("04", "Mirroring blobs"),
    // --- NKBIPs ---
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "01",
        title: "Curated Publications",
        kinds: Some("30040, 30041"),
        upstream_url: "https://nostr.blue/wiki/nkbip-01",
        notes: None,
    },
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "02",
        title: "Vector Embeddings",
        kinds: Some("1987"),
        upstream_url: "https://nostr.blue/wiki/nkbip-02",
        notes: None,
    },
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "03",
        title: "Citations",
        kinds: Some("30, 31, 32, 33"),
        upstream_url: "https://nostr.blue/wiki/nkbip-03",
        notes: None,
    },
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "04",
        title: "Directory System",
        kinds: Some("30042-30045"),
        upstream_url: "https://nostr.blue/wiki/nkbip-04",
        notes: None,
    },
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "06",
        title: "Nostr MIME Types",
        kinds: Some("M tag"),
        upstream_url: "https://nostr.blue/wiki/nkbip-06",
        notes: None,
    },
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "08",
        title: "Book Wikilinks",
        kinds: Some("book:: macro"),
        upstream_url: "https://nostr.blue/wiki/nkbip-08",
        notes: None,
    },
    // --- Market spec ---
    SupportedSpec {
        spec_type: SpecType::Market,
        number: "",
        title: "NIP-99 Marketplace Specification",
        kinds: None,
        upstream_url: "https://github.com/GammaMarkets/market-spec",
        notes: None,
    },
];

/// All supported specs.
pub fn all() -> &'static [SupportedSpec] {
    SUPPORTED_SPECS
}

/// Resolve a route ID (e.g. `"nip-01"`, `"nut-00"`, `"market-spec"`) to a spec.
///
/// Returns `None` for unknown specs and for custom NIP IDs (`naddr1…`),
/// signalling the caller to fall through to the relay-fetch path.
pub fn find(id: &str) -> Option<&'static SupportedSpec> {
    if id == "market-spec" {
        return SUPPORTED_SPECS.iter().find(|s| s.spec_type == SpecType::Market);
    }
    let (prefix, number) = id.split_once('-')?;
    let spec_type = SpecType::from_prefix(prefix)?;
    SUPPORTED_SPECS
        .iter()
        .find(|s| s.spec_type == spec_type && s.number.eq_ignore_ascii_case(number))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_nip() {
        let s = find("nip-01").expect("NIP-01 should be registered");
        assert_eq!(s.spec_type, SpecType::Nip);
        assert_eq!(s.number, "01");
        assert_eq!(s.title, "Basic protocol");
        assert!(s.upstream_url.contains("nostr-protocol/nips"));
    }

    #[test]
    fn test_find_nip_hex_case_insensitive() {
        // Route IDs are lowercase by convention but lookup should tolerate either casing.
        assert!(find("nip-c0").is_some());
        assert!(find("nip-C0").is_some());
    }

    #[test]
    fn test_find_nut_and_bud() {
        assert_eq!(find("nut-00").map(|s| s.title), Some("Notation and Encoding"));
        assert_eq!(find("bud-01").map(|s| s.title), Some("Server requirements"));
    }

    #[test]
    fn test_find_nkbip_carries_kinds() {
        let s = find("nkbip-01").expect("NKBIP-01 registered");
        assert_eq!(s.kinds, Some("30040, 30041"));
    }

    #[test]
    fn test_find_market_spec() {
        let s = find("market-spec").expect("market-spec registered");
        assert_eq!(s.spec_type, SpecType::Market);
        assert!(s.badge().contains("Market"));
    }

    #[test]
    fn test_find_unknown_returns_none() {
        // NIP-03 (OpenTimestamps) and NIP-13 (PoW) are not supported by nostr.blue.
        assert!(find("nip-03").is_none());
        assert!(find("nip-13").is_none());
        assert!(find("nonsense").is_none());
        assert!(find("").is_none());
    }

    #[test]
    fn test_find_naddr_returns_none() {
        // Custom NIPs are handled by the relay-fetch path, not the registry.
        assert!(find("naddr1xyz").is_none());
    }

    #[test]
    fn test_route_id_round_trip() {
        for spec in SUPPORTED_SPECS.iter() {
            let id = spec.route_id();
            let back = find(&id).expect("route_id should round-trip through find");
            assert_eq!(back.spec_type, spec.spec_type);
            assert_eq!(back.number, spec.number);
        }
    }

    #[test]
    fn test_no_duplicate_route_ids() {
        let mut ids: Vec<String> = SUPPORTED_SPECS.iter().map(|s| s.route_id()).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate route IDs in registry");
    }

    #[test]
    fn test_registry_is_populated() {
        assert!(
            SUPPORTED_SPECS.len() >= 80,
            "expected a broad registry, got {}",
            SUPPORTED_SPECS.len()
        );
    }
}
