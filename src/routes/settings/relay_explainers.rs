//! Relay section education for the Settings relay screens (issue #359).
//!
//! Single source of truth for the plain-language copy shown on
//! `/settings/relays`: the always-visible hint under each section header and
//! the expandable "What is this?" explainers. Keep copy edits here — never
//! inline explainer strings in the page component.

use crate::routes::Route;
use dioxus::prelude::*;

/// Which relay settings section an explainer describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelaySectionKind {
    General,
    DmInbox,
    Search,
    Blocked,
    Indexer,
    PrivateOutbox,
    FavoriteFeed,
    Proxy,
    Trusted,
    Local,
    Broadcast,
    Connected,
}

/// A curated example relay shown inside an explainer.
#[derive(Clone, Copy, Debug)]
pub struct RelayExample {
    pub url: &'static str,
    /// Free/paid/personal annotation.
    pub note: &'static str,
}

/// Educational copy for one relay settings section.
pub struct RelaySectionInfo {
    /// One-line hint rendered under the section header (always visible).
    pub hint: &'static str,
    /// Plain-language explainer: what this relay type does and who uses it.
    pub explainer: &'static str,
    /// How many relays to use, when applicable.
    pub recommended_count: Option<&'static str>,
    /// Curated free/paid example relays.
    pub examples: &'static [RelayExample],
}

const GENERAL: RelaySectionInfo = RelaySectionInfo {
    hint: "NIP-65 • Read: fetch content • Write: publish content",
    explainer: "Write relays store all of your content: nostr.blue publishes your \
        posts here, and other clients use these relays to find it. Read relays \
        receive the replies, comments, likes and zaps to your posts. Operator \
        limits on read relays affect which notifications you receive — for \
        example, paid relays can filter out comment spam if you're being \
        attacked.",
    recommended_count: Some(
        "Use 1–3 write relays and 1–3 read relays — they can be personal \
         relays, paid relays, or public relays.",
    ),
    examples: &[],
};

const DM_INBOX: RelaySectionInfo = RelaySectionInfo {
    hint: "NIP-17 • Where others send you direct messages",
    explainer: "These relays are your private inbox: others will use them to \
        send you DMs. A good DM inbox relay accepts messages from anyone but \
        only lets you download yours.",
    recommended_count: Some("Use 1–3 relays as your private inbox."),
    examples: &[
        RelayExample { url: "wss://inbox.nostr.wine", note: "paid" },
        RelayExample { url: "wss://auth.nostr1.com", note: "free" },
        RelayExample { url: "wss://you.nostr1.com", note: "personal, paid" },
    ],
};

const SEARCH: RelaySectionInfo = RelaySectionInfo {
    hint: "NIP-50 • Relays that support full-text search",
    explainer: "Relays used when searching content or users. Search will not \
        work if none are configured — make sure the relays you add implement \
        NIP-50.",
    recommended_count: None,
    examples: &[
        RelayExample { url: "wss://relay.nostr.band", note: "free" },
        RelayExample { url: "wss://relay.noswhere.com", note: "free" },
    ],
};

const BLOCKED: RelaySectionInfo = RelaySectionInfo {
    hint: "NIP-51 • Relays to never connect to",
    explainer: "nostr.blue will never connect to these relays, for any \
        purpose.",
    recommended_count: None,
    examples: &[],
};

const INDEXER: RelaySectionInfo = RelaySectionInfo {
    hint: "Discover users' relays and metadata (gift-wrapped, private)",
    explainer: "Relays that specialize in hosting everyone's metadata and \
        relay lists, like purplepag.es. nostr.blue uses these to find users \
        that are not in your lists.",
    recommended_count: None,
    examples: &[],
};

const PRIVATE_OUTBOX: RelaySectionInfo = RelaySectionInfo {
    hint: "Store events only you can see, like drafts and app settings",
    explainer: "Private storage relays for events no one else can see, like \
        drafts and app settings. Ideally these are local relays or require \
        authentication before downloading each user's content.",
    recommended_count: Some("Use 1–3 relays to store events no one else can see."),
    examples: &[],
};

const FAVORITE_FEED: RelaySectionInfo = RelaySectionInfo {
    hint: "Your preferred relays for reading feeds (plain, visible)",
    explainer: "Relays you frequently visit for their Global feed.",
    recommended_count: None,
    examples: &[],
};

const PROXY: RelaySectionInfo = RelaySectionInfo {
    hint: "Feed aggregator relays (gift-wrapped, private)",
    explainer: "Aggregator relays the app downloads your feeds from, like \
        filter.nostr.wine. This replaces the outbox model and makes the app \
        connect only to the relays in your lists.",
    recommended_count: None,
    examples: &[],
};

const TRUSTED: RelaySectionInfo = RelaySectionInfo {
    hint: "Relays you trust for accurate data (gift-wrapped, private)",
    explainer: "Relays you trust enough to use for sensitive operations.",
    recommended_count: None,
    examples: &[],
};

const LOCAL: RelaySectionInfo = RelaySectionInfo {
    hint: "Localhost/LAN relays (stored locally, not published to Nostr)",
    explainer: "Relays running on this device or your local network.",
    recommended_count: None,
    examples: &[],
};

const BROADCAST: RelaySectionInfo = RelaySectionInfo {
    hint: "Extra write targets for the post menu Broadcast action (stored locally)",
    explainer: "Relays that specialize in pushing your notes to all of the \
        other relays, like sendit.nosflare.com.",
    recommended_count: None,
    examples: &[RelayExample { url: "wss://sendit.nosflare.com", note: "free" }],
};

const CONNECTED: RelaySectionInfo = RelaySectionInfo {
    hint: "Currently active connections with live statistics",
    explainer: "Every relay nostr.blue is currently connected to, with live \
        traffic and reliability stats. This includes the relays from the \
        sections above plus any added automatically by the outbox/gossip \
        model.",
    recommended_count: None,
    examples: &[],
};

/// Look up the educational copy for a section.
pub fn section_info(kind: RelaySectionKind) -> &'static RelaySectionInfo {
    match kind {
        RelaySectionKind::General => &GENERAL,
        RelaySectionKind::DmInbox => &DM_INBOX,
        RelaySectionKind::Search => &SEARCH,
        RelaySectionKind::Blocked => &BLOCKED,
        RelaySectionKind::Indexer => &INDEXER,
        RelaySectionKind::PrivateOutbox => &PRIVATE_OUTBOX,
        RelaySectionKind::FavoriteFeed => &FAVORITE_FEED,
        RelaySectionKind::Proxy => &PROXY,
        RelaySectionKind::Trusted => &TRUSTED,
        RelaySectionKind::Local => &LOCAL,
        RelaySectionKind::Broadcast => &BROADCAST,
        RelaySectionKind::Connected => &CONNECTED,
    }
}

/// Convenience: the always-visible hint for a section.
pub fn section_hint(kind: RelaySectionKind) -> &'static str {
    section_info(kind).hint
}

/// Expandable "What is this?" explainer rendered under a section header.
#[component]
pub fn SectionExplainer(kind: RelaySectionKind) -> Element {
    let mut expanded = use_signal(|| false);
    let info = section_info(kind);
    rsx! {
        div { class: "mt-1",
            button {
                class: "inline-flex items-center gap-1 text-xs text-blue-600 dark:text-blue-400 hover:underline",
                onclick: move |_| expanded.set(!expanded()),
                span { class: "text-[10px]", if expanded() { "▾" } else { "▸" } },
                "What is this?"
            }
            if expanded() {
                div { class: "mt-2 p-3 bg-blue-50 dark:bg-blue-950/30 border border-blue-100 dark:border-blue-900 rounded-lg text-sm text-gray-700 dark:text-gray-300 space-y-2",
                    p { "{info.explainer}" }
                    if let Some(count) = info.recommended_count {
                        p { class: "font-medium text-gray-900 dark:text-white", "{count}" }
                    }
                    if !info.examples.is_empty() {
                        div { class: "flex flex-wrap gap-2",
                            for example in info.examples {
                                span {
                                    key: "{example.url}",
                                    class: "inline-flex items-center gap-1 px-2 py-0.5 bg-white dark:bg-gray-800 border border-blue-200 dark:border-blue-800 rounded text-xs",
                                    span { class: "font-mono", "{example.url}" }
                                    span { class: "text-muted-foreground", "({example.note})" }
                                }
                            }
                        }
                    }
                    Link {
                        to: Route::RelayExplorer {},
                        class: "inline-flex items-center gap-1 text-xs text-blue-600 dark:text-blue-400 hover:underline",
                        "Find more relays →"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KINDS: [RelaySectionKind; 12] = [
        RelaySectionKind::General,
        RelaySectionKind::DmInbox,
        RelaySectionKind::Search,
        RelaySectionKind::Blocked,
        RelaySectionKind::Indexer,
        RelaySectionKind::PrivateOutbox,
        RelaySectionKind::FavoriteFeed,
        RelaySectionKind::Proxy,
        RelaySectionKind::Trusted,
        RelaySectionKind::Local,
        RelaySectionKind::Broadcast,
        RelaySectionKind::Connected,
    ];

    #[test]
    fn every_section_has_hint_and_explainer() {
        for kind in ALL_KINDS {
            let info = section_info(kind);
            assert!(!info.hint.is_empty(), "missing hint for {kind:?}");
            assert!(
                info.explainer.len() > 20,
                "missing explainer for {kind:?}"
            );
        }
    }

    #[test]
    fn example_urls_are_secure_relay_urls() {
        for kind in ALL_KINDS {
            for example in section_info(kind).examples {
                assert!(
                    example.url.starts_with("wss://"),
                    "example {} for {kind:?} must be a wss:// URL",
                    example.url
                );
                assert!(!example.note.is_empty());
            }
        }
    }

    #[test]
    fn sections_with_guidance_have_count_copy() {
        assert!(section_info(RelaySectionKind::General).recommended_count.is_some());
        assert!(section_info(RelaySectionKind::DmInbox).recommended_count.is_some());
        assert!(
            section_info(RelaySectionKind::PrivateOutbox)
                .recommended_count
                .is_some()
        );
    }
}
