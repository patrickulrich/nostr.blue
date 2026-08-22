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
    Dip,
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
            SpecType::Dip => "dip",
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
            SpecType::Dip => "DIP",
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
            SpecType::Dip => "DIPs",
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
            "dip" => Some(SpecType::Dip),
            _ => None,
        }
    }
}

/// One supported specification entry.
///
/// `notes` holds optional rich implementation notes embedded at compile time.
/// `None` means the detail page falls back to a stub with the upstream link.
///
/// `naddr` optionally marks an entry whose content lives in a Nostr addressable
/// event (a "custom NIP" promoted to the supported grid). When set, `route_id`
/// returns the naddr so the detail page fetches live content from relays instead
/// of using embedded `notes`.
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
    /// Optional naddr of a relay-hosted custom NIP promoted to the supported grid.
    /// When `Some`, the detail page renders live relay content instead of `notes`.
    pub naddr: Option<&'static str>,
}

impl SupportedSpec {
    /// The route ID used in `Route::NipDetail { nip_id }`.
    pub fn route_id(&self) -> String {
        if let Some(naddr) = self.naddr {
            return naddr.to_string();
        }
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
const DIP_URL_BASE: &str = "https://github.com/damus-io/dips/blob/master/";

const fn nip_entry(num: &'static str, title: &'static str, notes: Option<&'static str>) -> SupportedSpec {
    SupportedSpec {
        spec_type: SpecType::Nip,
        number: num,
        title,
        kinds: None,
        upstream_url: NIP_URL_BASE,
        notes,
        naddr: None,
    }
}

const fn nut_entry(num: &'static str, title: &'static str, notes: Option<&'static str>) -> SupportedSpec {
    SupportedSpec {
        spec_type: SpecType::Nut,
        number: num,
        title,
        kinds: None,
        upstream_url: NUT_URL_BASE,
        notes,
        naddr: None,
    }
}

const fn bud_entry(num: &'static str, title: &'static str, notes: Option<&'static str>) -> SupportedSpec {
    SupportedSpec {
        spec_type: SpecType::Bud,
        number: num,
        title,
        kinds: None,
        upstream_url: BUD_URL_BASE,
        notes,
        naddr: None,
    }
}

const fn dip_entry(num: &'static str, title: &'static str, notes: Option<&'static str>) -> SupportedSpec {
    SupportedSpec {
        spec_type: SpecType::Dip,
        number: num,
        title,
        kinds: None,
        upstream_url: DIP_URL_BASE,
        notes,
        naddr: None,
    }
}

/// All protocol specifications supported by nostr.blue.
///
/// Seeded from the README support tables. This is the source of truth —
/// update this list (and the README tables) when support changes.
pub const SUPPORTED_SPECS: &[SupportedSpec] = &[
    // --- NIPs ---
    nip_entry("01", "Basic protocol", Some(include_str!("content/nip_01.md"))),
    nip_entry("02", "Follow List", Some(include_str!("content/nip_02.md"))),
    nip_entry("04", "Encrypted DM (legacy)", Some(include_str!("content/nip_04.md"))),
    nip_entry("05", "DNS Identifiers", Some(include_str!("content/nip_05.md"))),
    nip_entry("06", "Key derivation from mnemonic", Some(include_str!("content/nip_06.md"))),
    nip_entry("07", "Browser extension signing", Some(include_str!("content/nip_07.md"))),
    nip_entry("09", "Event Deletion", Some(include_str!("content/nip_09.md"))),
    nip_entry("10", "Text Notes and Threads", Some(include_str!("content/nip_10.md"))),
    nip_entry("11", "Relay Information Document", Some(include_str!("content/nip_11.md"))),
    nip_entry("17", "Private Direct Messages", Some(include_str!("content/nip_17.md"))),
    nip_entry("18", "Reposts", Some(include_str!("content/nip_18.md"))),
    nip_entry("19", "bech32 identifiers", Some(include_str!("content/nip_19.md"))),
    nip_entry("21", "nostr: URI scheme", Some(include_str!("content/nip_21.md"))),
    nip_entry("22", "Comments", Some(include_str!("content/nip_22.md"))),
    nip_entry("23", "Long-form Content", Some(include_str!("content/nip_23.md"))),
    nip_entry("24", "Extra metadata fields", Some(include_str!("content/nip_24.md"))),
    nip_entry("25", "Reactions", Some(include_str!("content/nip_25.md"))),
    nip_entry("27", "Text Note References", Some(include_str!("content/nip_27.md"))),
    nip_entry("28", "Public Chat", Some(include_str!("content/nip_28.md"))),
    nip_entry("29", "Relay-based Groups", Some(include_str!("content/nip_29.md"))),
    nip_entry("30", "Custom Emoji", Some(include_str!("content/nip_30.md"))),
    nip_entry("34", "Git stuff", Some(include_str!("content/nip_34.md"))),
    nip_entry("36", "Sensitive Content", Some(include_str!("content/nip_36.md"))),
    nip_entry("37", "Draft Events", Some(include_str!("content/nip_37.md"))),
    nip_entry("38", "User Statuses", Some(include_str!("content/nip_38.md"))),
    nip_entry("39", "External Identities", Some(include_str!("content/nip_39.md"))),
    nip_entry("40", "Expiration Timestamp", Some(include_str!("content/nip_40.md"))),
    nip_entry("41", "Editable Short Notes", None),
    nip_entry("42", "Client Auth to Relays", Some(include_str!("content/nip_42.md"))),
    nip_entry("44", "Encrypted Payloads", Some(include_str!("content/nip_44.md"))),
    nip_entry("45", "Counting results", Some(include_str!("content/nip_45.md"))),
    nip_entry("46", "Remote Signing", Some(include_str!("content/nip_46.md"))),
    nip_entry("47", "Wallet Connect", Some(include_str!("content/nip_47.md"))),
    nip_entry("48", "Proxy Tags", Some(include_str!("content/nip_48.md"))),
    nip_entry("49", "Private Key Encryption", Some(include_str!("content/nip_49.md"))),
    nip_entry("50", "Search Capability", Some(include_str!("content/nip_50.md"))),
    nip_entry("51", "Lists", Some(include_str!("content/nip_51.md"))),
    nip_entry("52", "Calendar Events", Some(include_str!("content/nip_52.md"))),
    nip_entry("53", "Live Activities", Some(include_str!("content/nip_53.md"))),
    nip_entry("54", "Wiki", Some(include_str!("content/nip_54.md"))),
    nip_entry("55", "Android Signer Application", Some(include_str!("content/nip_55.md"))),
    nip_entry("56", "Reporting", Some(include_str!("content/nip_56.md"))),
    nip_entry("57", "Lightning Zaps", Some(include_str!("content/nip_57.md"))),
    nip_entry("58", "Badges", Some(include_str!("content/nip_58.md"))),
    nip_entry("59", "Gift Wrap", Some(include_str!("content/nip_59.md"))),
    nip_entry("60", "Cashu Wallet", Some(include_str!("content/nip_60.md"))),
    nip_entry("61", "Nutzaps", Some(include_str!("content/nip_61.md"))),
    nip_entry("62", "Request to Vanish", Some(include_str!("content/nip_62.md"))),
    nip_entry("64", "Chess (PGN)", Some(include_str!("content/nip_64.md"))),
    nip_entry("65", "Relay List Metadata", Some(include_str!("content/nip_65.md"))),
    nip_entry("66", "Relay Discovery", Some(include_str!("content/nip_66.md"))),
    nip_entry("68", "Picture-first feeds", Some(include_str!("content/nip_68.md"))),
    nip_entry("69", "P2P Order events", Some(include_str!("content/nip_69.md"))),
    nip_entry("70", "Protected Events", Some(include_str!("content/nip_70.md"))),
    nip_entry("71", "Video Events", Some(include_str!("content/nip_71.md"))),
    nip_entry("72", "Moderated Communities", Some(include_str!("content/nip_72.md"))),
    nip_entry("73", "External Content IDs", Some(include_str!("content/nip_73.md"))),
    nip_entry("75", "Zap Goals", Some(include_str!("content/nip_75.md"))),
    nip_entry("77", "Negentropy Syncing", Some(include_str!("content/nip_77.md"))),
    nip_entry("78", "App-specific data", Some(include_str!("content/nip_78.md"))),
    nip_entry("84", "Highlights", Some(include_str!("content/nip_84.md"))),
    nip_entry("87", "Mint Discoverability", Some(include_str!("content/nip_87.md"))),
    nip_entry("88", "Polls", Some(include_str!("content/nip_88.md"))),
    nip_entry("89", "App Handlers", Some(include_str!("content/nip_89.md"))),
    nip_entry("90", "Data Vending Machines", Some(include_str!("content/nip_90.md"))),
    nip_entry("92", "Media Attachments", Some(include_str!("content/nip_92.md"))),
    nip_entry("94", "File Metadata", Some(include_str!("content/nip_94.md"))),
    nip_entry("96", "HTTP File Storage", Some(include_str!("content/nip_96.md"))),
    nip_entry("98", "HTTP Auth", Some(include_str!("content/nip_98.md"))),
    nip_entry("99", "Classified Listings", Some(include_str!("content/nip_99.md"))),
    nip_entry("A0", "Voice Messages", Some(include_str!("content/nip_A0.md"))),
    nip_entry("B0", "Web Bookmarks", Some(include_str!("content/nip_B0.md"))),
    nip_entry("B7", "Blossom", Some(include_str!("content/nip_B7.md"))),
    nip_entry("C0", "Code Snippets", Some(include_str!("content/nip_C0.md"))),
    SupportedSpec {
        spec_type: SpecType::Nip,
        number: "5A",
        title: "Static Websites (nsites)",
        kinds: Some("15128, 35128"),
        upstream_url: NIP_URL_BASE,
        notes: Some(include_str!("content/nip_5A.md")),
        naddr: None,
    },
    // NIP-F4 Podcasts. nostr.blue reads both the official NIP-F4 kinds and the
    // legacy custom-NIP scheme ("podcast-episodes-and-trailers", kinds 30054 +
    // 30078), which is superseded by NIP-F4. Subscriptions remain on NIP-51
    // kind 30003 (d="podcast-subscriptions").
    SupportedSpec {
        spec_type: SpecType::Nip,
        number: "F4",
        title: "Podcasts",
        kinds: Some("54, 10154, 10164, 10054"),
        upstream_url: NIP_URL_BASE,
        notes: Some(include_str!("content/nip_F4.md")),
        naddr: None,
    },
    // Custom NIP promoted to the supported grid: content is fetched live from
    // the naddr event (kind 30067/39067 pinboard system), so no embedded notes.
    SupportedSpec {
        spec_type: SpecType::Nip,
        number: "XX",
        title: "Pinboards",
        kinds: Some("30067, 39067"),
        upstream_url: "",
        notes: None,
        naddr: Some(
            "naddr1qqyhq6twvfhkzunywvpzqr6k8l3vlhccpjcsgkrtjkrnx7dqc87ul0psr2qvsf2lx0g47quaqvzqqqrcvy6s9gd8",
        ),
    },
    // The meta-spec for custom NIPs itself (kind 30817 "NIPs on Nostr").
    // Defines the community-authored NIP format; content is fetched live.
    SupportedSpec {
        spec_type: SpecType::Nip,
        number: "YY",
        title: "NIPs on Nostr",
        kinds: Some("30817"),
        upstream_url: "",
        notes: None,
        naddr: Some(
            "naddr1qvzqqqrcvypzqprpljlvcnpnw3pejvkkhrc3y6wvmd7vjuad0fg2ud3dky66gaxaqqxku6tswvkk7m3ddehhxarjqk4nmy",
        ),
    },
    // nostr.blue music specs (community NIPs defining the music kinds below).
    SupportedSpec {
        spec_type: SpecType::Nip,
        number: "ZZ",
        title: "Music Tracks",
        kinds: Some("36787"),
        upstream_url: "",
        notes: None,
        naddr: Some(
            "naddr1qqxx6atnd93j6arjv93kkuczyqduwzspfzelx9k6x0lrez0j8cl8rtz0lxvqylk8z2ustnfy76jpzqcyqqq8scgyxv0z4",
        ),
    },
    SupportedSpec {
        spec_type: SpecType::Nip,
        number: "WW",
        title: "Music Playlists",
        kinds: Some("34139"),
        upstream_url: "",
        notes: None,
        naddr: Some(
            "naddr1qqsx6atnd93j6urvv9ukc6tnw3ej6amfw35z6urr95erqtt8w45kguczyrmey2s2mvl6fhd9am92vtm00mnpt8ml2hsgqdngd35wpquzcdrcsqcyqqq8scg50w9jj",
        ),
    },
    // --- NUTs (Cashu) ---
    nut_entry("00", "Notation and Encoding", Some(include_str!("content/nut_00.md"))),
    nut_entry("01", "Mint public keys", Some(include_str!("content/nut_01.md"))),
    nut_entry("02", "Keysets and fees", Some(include_str!("content/nut_02.md"))),
    nut_entry("03", "Swapping tokens", Some(include_str!("content/nut_03.md"))),
    nut_entry("04", "Minting tokens", Some(include_str!("content/nut_04.md"))),
    nut_entry("05", "Melting tokens", Some(include_str!("content/nut_05.md"))),
    nut_entry("06", "Mint info", Some(include_str!("content/nut_06.md"))),
    nut_entry("07", "Token state check", Some(include_str!("content/nut_07.md"))),
    nut_entry("08", "Overpaid fees", Some(include_str!("content/nut_08.md"))),
    nut_entry("09", "Signature restore", Some(include_str!("content/nut_09.md"))),
    nut_entry("10", "Spending conditions", Some(include_str!("content/nut_10.md"))),
    nut_entry("11", "P2PK", Some(include_str!("content/nut_11.md"))),
    nut_entry("12", "DLEQ proofs", Some(include_str!("content/nut_12.md"))),
    nut_entry("13", "Deterministic secrets", Some(include_str!("content/nut_13.md"))),
    nut_entry("14", "HTLCs", Some(include_str!("content/nut_14.md"))),
    nut_entry("15", "Multi-path payments", Some(include_str!("content/nut_15.md"))),
    nut_entry("17", "WebSocket subscriptions", Some(include_str!("content/nut_17.md"))),
    nut_entry("18", "Payment requests", Some(include_str!("content/nut_18.md"))),
    nut_entry("19", "Cached responses", Some(include_str!("content/nut_19.md"))),
    nut_entry("20", "Signature on mint quote", Some(include_str!("content/nut_20.md"))),
    nut_entry("21", "Clear authentication", Some(include_str!("content/nut_21.md"))),
    nut_entry("22", "Blind authentication", Some(include_str!("content/nut_22.md"))),
    // --- BUDs (Blossom) ---
    bud_entry("01", "Server requirements", Some(include_str!("content/bud_01.md"))),
    bud_entry("02", "Blob upload/management", Some(include_str!("content/bud_02.md"))),
    bud_entry("03", "User Server List", Some(include_str!("content/bud_03.md"))),
    bud_entry("04", "Mirroring blobs", Some(include_str!("content/bud_04.md"))),
    // --- NKBIPs ---
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "01",
        title: "Curated Publications",
        kinds: Some("30040, 30041"),
        upstream_url: "https://nostr.blue/wiki/nkbip-01",
        notes: Some(include_str!("content/nkbip_01.md")),
        naddr: None,
    },
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "02",
        title: "Vector Embeddings",
        kinds: Some("1987"),
        upstream_url: "https://nostr.blue/wiki/nkbip-02",
        notes: Some(include_str!("content/nkbip_02.md")),
        naddr: None,
    },
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "03",
        title: "Citations",
        kinds: Some("30, 31, 32, 33"),
        upstream_url: "https://nostr.blue/wiki/nkbip-03",
        notes: Some(include_str!("content/nkbip_03.md")),
        naddr: None,
    },
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "04",
        title: "Directory System",
        kinds: Some("30042-30045"),
        upstream_url: "https://nostr.blue/wiki/nkbip-04",
        notes: Some(include_str!("content/nkbip_04.md")),
        naddr: None,
    },
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "06",
        title: "Nostr MIME Types",
        kinds: Some("M tag"),
        upstream_url: "https://nostr.blue/wiki/nkbip-06",
        notes: Some(include_str!("content/nkbip_06.md")),
        naddr: None,
    },
    SupportedSpec {
        spec_type: SpecType::Nkbip,
        number: "08",
        title: "Book Wikilinks",
        kinds: Some("book:: macro"),
        upstream_url: "https://nostr.blue/wiki/nkbip-08",
        notes: Some(include_str!("content/nkbip_08.md")),
        naddr: None,
    },
    // --- DIPs (Damus Improvement Proposals) ---
    dip_entry("03", "Private Zaps", Some(include_str!("content/dip_03.md"))),
    // --- Market spec ---
    SupportedSpec {
        spec_type: SpecType::Market,
        number: "",
        title: "NIP-99 Marketplace Specification",
        kinds: None,
        upstream_url: "https://github.com/GammaMarkets/market-spec",
        notes: Some(include_str!("content/market_spec.md")),
        naddr: None,
    },
];

/// All supported specs.
pub fn all() -> &'static [SupportedSpec] {
    SUPPORTED_SPECS
}

/// Resolve a route ID (e.g. `"nip-01"`, `"nut-00"`, `"market-spec"`) to a spec.
///
/// For `naddr1…` IDs, returns the matching listed custom NIP only if its naddr
/// is registered in `SUPPORTED_SPECS`; unlisted naddrs return `None`, signalling
/// the caller to fall through to the generic relay-fetch path.
pub fn find(id: &str) -> Option<&'static SupportedSpec> {
    if id.starts_with("naddr") {
        return SUPPORTED_SPECS.iter().find(|s| s.naddr.is_some_and(|n| n == id));
    }
    if id == "market-spec" {
        return SUPPORTED_SPECS.iter().find(|s| s.spec_type == SpecType::Market);
    }
    let (prefix, number) = id.split_once('-')?;
    let spec_type = SpecType::from_prefix(prefix)?;
    SUPPORTED_SPECS
        .iter()
        .find(|s| s.spec_type == spec_type && s.number.eq_ignore_ascii_case(number))
}

/// Build the canonical upstream URL for a spec we don't support.
///
/// Used by the spec-link rewriter (`spec_links::rewrite_spec_link_html`) to route
/// cross-references to unsupported specs out to their authoritative source:
/// GitHub for NIPs/NUTs/BUDs, the nostr.blue wiki for NKBIPs, and the market-spec
/// repo for the marketplace spec.
pub fn upstream_url_for(spec_type: SpecType, number: &str) -> String {
    match spec_type {
        SpecType::Nip => format!("{NIP_URL_BASE}{number}.md"),
        SpecType::Nut => format!("{NUT_URL_BASE}{number}.md"),
        SpecType::Bud => format!("{BUD_URL_BASE}{number}.md"),
        SpecType::Nkbip => format!("https://nostr.blue/wiki/nkbip-{number}"),
        SpecType::Market => "https://github.com/GammaMarkets/market-spec".to_string(),
        SpecType::Dip => format!("{DIP_URL_BASE}{number}.md"),
    }
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
    fn test_find_nip_29() {
        let s = find("nip-29").expect("NIP-29 should be registered");
        assert_eq!(s.spec_type, SpecType::Nip);
        assert_eq!(s.number, "29");
        assert_eq!(s.title, "Relay-based Groups");
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
        // Unlisted custom NIPs (naddr not in SUPPORTED_SPECS) fall through to the
        // generic relay-fetch path. Listed naddrs are resolved by test_find_pinboard below.
        assert!(find("naddr1xyz").is_none());
    }

    #[test]
    fn test_find_pinboard() {
        // The Pinboards custom NIP is listed in the registry via its naddr.
        let pin = SUPPORTED_SPECS
            .iter()
            .find(|s| s.naddr.is_some())
            .expect("a listed naddr spec should exist");
        let id = pin.route_id();
        assert!(id.starts_with("naddr"), "route_id should be the naddr");
        assert_eq!(pin.spec_type, SpecType::Nip);
        assert_eq!(pin.title, "Pinboards");
        let back = find(&id).expect("listed naddr should round-trip through find");
        assert_eq!(back.title, "Pinboards");
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
