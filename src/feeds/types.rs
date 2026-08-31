//! Feed item types: the canonical `FeedItem` enum with composite interaction
//! collapsing, plus interaction-info structs.
//!
//! ## Design
//!
//! `FeedItem` is the single source of truth for "one row in a feed". It has
//! three variants:
//!
//! - `OriginalPost(Event)` — a plain kind-1 text note (or kind-1111 topic post)
//! - `Repost { original, reposted_by, repost_timestamp }` — a kind-6/16 repost;
//!   displays the original note with a "reposted by X" header
//! - `Composite { underlying, reposts, reactions, zaps, latest_interaction }` —
//!   a collapsed interaction cluster: the underlying note PLUS embedded
//!   reactions/zaps/reposts collected for it. Keyed by the underlying note's
//!   id so N reposts of one note show as one row.
//!
//! The `Composite` variant enables rendering a note with its interaction
//! counts pre-computed, bypassing the separate live-count subscription pass.
//! `NoteCard::precomputed_counts` already supports this — `interaction_summary()`
//! produces the values it expects.
//!
//! ## Backward compatibility
//!
//! `utils::repost::FeedItem` re-exports this type, so existing imports
//! (`use crate::utils::repost::FeedItem`) continue to work. The utility
//! functions (`process_events_to_feed_items`, etc.) in `utils/repost.rs`
//! construct only `OriginalPost` and `Repost`; `Composite` is built by the
//! feed orchestration layer when merging interaction events.

use nostr_sdk::{Event, EventId, Kind, PublicKey, Timestamp};

// ─── Interaction info structs ───────────────────────────────────────────────

/// Information about a single repost (kind 6/16) of an underlying note.
#[derive(Clone, Debug)]
pub struct RepostInfo {
    /// Public key of the user who reposted.
    pub by: PublicKey,
    /// When the repost was created.
    pub at: Timestamp,
}

/// Information about a single reaction (kind 7) to an underlying note.
#[derive(Clone, Debug)]
pub struct ReactionInfo {
    /// Public key of the user who reacted.
    pub by: PublicKey,
    /// Reaction content (emoji, "+", etc.).
    pub emoji: String,
    /// When the reaction was created.
    pub at: Timestamp,
}

/// Information about a single zap (kind 9735 receipt) to an underlying note.
#[derive(Clone, Debug)]
pub struct ZapInfo {
    /// Public key of the zap sender (from the zap receipt's `P` tag or
    /// the bolt11 invoice).
    pub by: PublicKey,
    /// Amount in milli-sats (from the bolt11 invoice `amount` tag).
    pub amount_msat: u64,
    /// When the zap receipt was created.
    pub at: Timestamp,
}

/// A summary of interactions on a note, convertible to the
/// `InteractionCounts` type that `NoteCard` expects via
/// `precomputed_counts`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InteractionSummary {
    pub replies: usize,
    pub likes: usize,
    pub reposts: usize,
    pub zaps: usize,
    pub zap_amount_sats: u64,
}

impl InteractionSummary {
    /// Merge another summary into this one (additive).
    pub fn merge(&mut self, other: &InteractionSummary) {
        self.replies += other.replies;
        self.likes += other.likes;
        self.reposts += other.reposts;
        self.zaps += other.zaps;
        self.zap_amount_sats += other.zap_amount_sats;
    }
}

// ─── FeedItem enum ───────────────────────────────────────────────────────────

/// Represents one row in a feed. The canonical feed item type.
///
/// See the module docs for the three-variant design rationale.
#[derive(Clone, Debug)]
pub enum FeedItem {
    /// A regular post from the feed (kind 1 text note or kind 1111 topic post).
    OriginalPost(Event),
    /// A repost (kind 6 or 16) with the original event and repost metadata.
    Repost {
        /// The original event that was reposted.
        original: Event,
        /// Public key of the user who reposted it.
        reposted_by: PublicKey,
        /// Timestamp when the repost was made.
        repost_timestamp: Timestamp,
    },
    /// A collapsed interaction cluster: the underlying note plus embedded
    /// reactions/zaps/reposts collected for it. Built by the feed
    /// orchestration layer when merging interaction events; NOT constructed
    /// by `process_events_to_feed_items`.
    Composite {
        /// The underlying note being interacted with.
        underlying: Event,
        /// Reposts of this note (dedup by reposter pubkey).
        reposts: Vec<RepostInfo>,
        /// Reactions to this note (dedup by reactor pubkey).
        reactions: Vec<ReactionInfo>,
        /// Zaps on this note (dedup by zap sender).
        zaps: Vec<ZapInfo>,
        /// The most recent interaction timestamp; drives sort ordering.
        latest_interaction: Timestamp,
    },
}

impl FeedItem {
    /// Get the underlying/visible event (the note that should be rendered).
    pub fn event(&self) -> &Event {
        match self {
            FeedItem::OriginalPost(event) => event,
            FeedItem::Repost { original, .. } => original,
            FeedItem::Composite { underlying, .. } => underlying,
        }
    }

    /// Get the timestamp to use for feed sorting.
    ///
    /// - `OriginalPost`: the event's `created_at`.
    /// - `Repost`: the repost time (not the original's `created_at`) — matches
    ///   NIP-18 semantics.
    /// - `Composite`: `latest_interaction` (the most recent reaction/zap/repost).
    pub fn sort_timestamp(&self) -> Timestamp {
        match self {
            FeedItem::OriginalPost(event) => event.created_at,
            FeedItem::Repost {
                repost_timestamp, ..
            } => *repost_timestamp,
            FeedItem::Composite {
                latest_interaction, ..
            } => *latest_interaction,
        }
    }

    /// Get repost metadata if this is a repost.
    ///
    /// For `Composite`, returns the first reposter (if any) for rendering
    /// compatibility with `NoteCard`'s `repost_info` prop.
    pub fn repost_info(&self) -> Option<(PublicKey, Timestamp)> {
        match self {
            FeedItem::OriginalPost(_) => None,
            FeedItem::Repost {
                reposted_by,
                repost_timestamp,
                ..
            } => Some((*reposted_by, *repost_timestamp)),
            FeedItem::Composite { reposts, .. } => {
                reposts.first().map(|r| (r.by, r.at))
            }
        }
    }

    /// Get an interaction summary (counts) for rendering.
    ///
    /// For `OriginalPost` and `Repost`, returns zeros (those variants don't
    /// carry interaction data). For `Composite`, returns counts derived from
    /// the embedded reaction/zap/repost lists.
    pub fn interaction_summary(&self) -> InteractionSummary {
        match self {
            FeedItem::OriginalPost(_) | FeedItem::Repost { .. } => InteractionSummary::default(),
            FeedItem::Composite {
                reposts,
                reactions,
                zaps,
                ..
            } => {
                let zap_amount_sats: u64 = zaps.iter().map(|z| z.amount_msat / 1000).sum();
                InteractionSummary {
                    replies: 0, // Replies are tracked separately (thread structure)
                    likes: reactions.len(),
                    reposts: reposts.len(),
                    zaps: zaps.len(),
                    zap_amount_sats,
                }
            }
        }
    }

    /// Attempt to merge an interaction event (kind 6/7/9735) into this
    /// composite item. Returns `true` if the interaction was new (sender not
    /// already present), `false` if it was a duplicate or the item is not a
    /// `Composite`.
    ///
    /// For non-composite items, this always returns `false` — callers should
    /// upgrade the item to `Composite` first if they want to merge interactions.
    pub fn merge_interaction(&mut self, event: &Event) -> bool {
        let FeedItem::Composite {
            reposts,
            reactions,
            zaps,
            latest_interaction,
            ..
        } = self
        else {
            return false;
        };

        match event.kind {
            Kind::Repost | Kind::GenericRepost => {
                let by = event.pubkey;
                if reposts.iter().any(|r| r.by == by) {
                    return false;
                }
                reposts.push(RepostInfo {
                    by,
                    at: event.created_at,
                });
                if event.created_at > *latest_interaction {
                    *latest_interaction = event.created_at;
                }
                true
            }
            Kind::Reaction => {
                let by = event.pubkey;
                if reactions.iter().any(|r| r.by == by) {
                    return false;
                }
                reactions.push(ReactionInfo {
                    by,
                    emoji: event.content.clone(),
                    at: event.created_at,
                });
                if event.created_at > *latest_interaction {
                    *latest_interaction = event.created_at;
                }
                true
            }
            Kind::ZapReceipt => {
                let by = event.pubkey;
                if zaps.iter().any(|z| z.by == by) {
                    return false;
                }
                // Amount extraction from bolt11 is non-trivial; store 0 for now.
                // The feed orchestration layer can enrich this later.
                zaps.push(ZapInfo {
                    by,
                    amount_msat: 0,
                    at: event.created_at,
                });
                if event.created_at > *latest_interaction {
                    *latest_interaction = event.created_at;
                }
                true
            }
            _ => false,
        }
    }

    /// Get the event id of the underlying/visible event.
    pub fn event_id(&self) -> &EventId {
        &self.event().id
    }

    /// Create a `Composite` from this item, preserving the underlying event.
    /// Returns `self` unchanged if already composite.
    pub fn into_composite(self) -> FeedItem {
        match self {
            FeedItem::Composite { .. } => self,
            FeedItem::OriginalPost(event) => FeedItem::Composite {
                underlying: event,
                reposts: Vec::new(),
                reactions: Vec::new(),
                zaps: Vec::new(),
                latest_interaction: Timestamp::default(),
            },
            FeedItem::Repost {
                original,
                reposted_by,
                repost_timestamp,
                ..
            } => FeedItem::Composite {
                underlying: original,
                reposts: vec![RepostInfo {
                    by: reposted_by,
                    at: repost_timestamp,
                }],
                reactions: Vec::new(),
                zaps: Vec::new(),
                latest_interaction: repost_timestamp,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::{EventBuilder, JsonUtil, Keys};

    fn make_text_note(content: &str) -> Event {
        let keys = Keys::generate();
        EventBuilder::new(Kind::TextNote, content)
            .sign_with_keys(&keys)
            .unwrap()
    }

    fn make_reaction_to(target: &Event, emoji: &str) -> Event {
        let keys = Keys::generate();
        EventBuilder::reaction(target, emoji)
            .sign_with_keys(&keys)
            .unwrap()
    }

    fn make_repost_of(original: &Event) -> Event {
        let keys = Keys::generate();
        EventBuilder::new(Kind::Repost, original.as_json())
            .sign_with_keys(&keys)
            .unwrap()
    }

    fn pk() -> PublicKey {
        Keys::generate().public_key()
    }

    #[test]
    fn original_post_accessors() {
        let event = make_text_note("hello");
        let item = FeedItem::OriginalPost(event.clone());
        assert_eq!(item.event().id, event.id);
        assert_eq!(item.sort_timestamp(), event.created_at);
        assert!(item.repost_info().is_none());
        assert_eq!(item.interaction_summary(), InteractionSummary::default());
    }

    #[test]
    fn repost_accessors() {
        let original = make_text_note("original");
        let reposter = pk();
        let ts = Timestamp::from(999);
        let item = FeedItem::Repost {
            original: original.clone(),
            reposted_by: reposter,
            repost_timestamp: ts,
        };
        assert_eq!(item.event().id, original.id);
        assert_eq!(item.sort_timestamp(), ts);
        let (by, at) = item.repost_info().unwrap();
        assert_eq!(by, reposter);
        assert_eq!(at, ts);
    }

    #[test]
    fn composite_sort_timestamp_uses_latest_interaction() {
        let event = make_text_note("underlying");
        let t2 = Timestamp::from(300);
        let item = FeedItem::Composite {
            underlying: event,
            reposts: vec![],
            reactions: vec![],
            zaps: vec![],
            latest_interaction: t2,
        };
        assert_eq!(item.sort_timestamp(), t2);
    }

    #[test]
    fn composite_repost_info_returns_first_reposter() {
        let event = make_text_note("underlying");
        let r1_by = pk();
        let r1_at = Timestamp::from(100);
        let item = FeedItem::Composite {
            underlying: event,
            reposts: vec![RepostInfo {
                by: r1_by,
                at: r1_at,
            }],
            reactions: vec![],
            zaps: vec![],
            latest_interaction: r1_at,
        };
        let (by, at) = item.repost_info().unwrap();
        assert_eq!(by, r1_by);
        assert_eq!(at, r1_at);
    }

    #[test]
    fn composite_interaction_summary_counts_correctly() {
        let event = make_text_note("underlying");
        let item = FeedItem::Composite {
            underlying: event,
            reposts: vec![
                RepostInfo {
                    by: pk(),
                    at: Timestamp::from(100),
                },
                RepostInfo {
                    by: pk(),
                    at: Timestamp::from(200),
                },
            ],
            reactions: vec![ReactionInfo {
                by: pk(),
                emoji: "+".to_string(),
                at: Timestamp::from(150),
            }],
            zaps: vec![ZapInfo {
                by: pk(),
                amount_msat: 10_000, // 10 sats
                at: Timestamp::from(300),
            }],
            latest_interaction: Timestamp::from(300),
        };
        let summary = item.interaction_summary();
        assert_eq!(summary.reposts, 2);
        assert_eq!(summary.likes, 1);
        assert_eq!(summary.zaps, 1);
        assert_eq!(summary.zap_amount_sats, 10);
    }

    #[test]
    fn merge_interaction_returns_true_for_new_reaction() {
        let event = make_text_note("underlying");
        let reaction = make_reaction_to(&event, "+");
        let mut item = FeedItem::OriginalPost(event).into_composite();
        assert!(item.merge_interaction(&reaction));
        if let FeedItem::Composite { reactions, .. } = &item {
            assert_eq!(reactions.len(), 1);
        }
    }

    #[test]
    fn merge_interaction_returns_false_for_duplicate_sender() {
        let event = make_text_note("underlying");
        // Build two reactions from the SAME keypair
        let reactor_keys = Keys::generate();
        let reaction1 = EventBuilder::reaction(&event, "+")
            .sign_with_keys(&reactor_keys)
            .unwrap();
        let reaction2 = EventBuilder::reaction(&event, "🔥")
            .sign_with_keys(&reactor_keys) // SAME pubkey
            .unwrap();
        let mut item = FeedItem::OriginalPost(event).into_composite();
        assert!(item.merge_interaction(&reaction1));
        assert!(!item.merge_interaction(&reaction2)); // dup sender
    }

    #[test]
    fn merge_interaction_returns_false_for_non_composite() {
        let event = make_text_note("underlying");
        let reaction = make_reaction_to(&event, "+");
        let mut item = FeedItem::OriginalPost(event);
        assert!(!item.merge_interaction(&reaction));
    }

    #[test]
    fn into_composite_preserves_repost_info() {
        let original = make_text_note("original");
        let reposter = pk();
        let ts = Timestamp::from(500);
        let item = FeedItem::Repost {
            original: original.clone(),
            reposted_by: reposter,
            repost_timestamp: ts,
        };
        let composite = item.into_composite();
        if let FeedItem::Composite {
            reposts,
            latest_interaction,
            ..
        } = &composite
        {
            assert_eq!(reposts.len(), 1);
            assert_eq!(reposts[0].by, reposter);
            assert_eq!(*latest_interaction, ts);
        } else {
            panic!("expected Composite");
        }
    }
}
