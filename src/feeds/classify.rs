//! Unified feed classification: one pure predicate that converts raw Nostr
//! events into [`FeedItem`]s for every feed surface.
//!
//! Before this module, the repost-unwrap / reply-filter / comment-gate logic
//! was inlined (with subtle per-site drift) at ~11 call sites: the loaders in
//! `routes/home/feed_loaders.rs`, the streaming path, the NDB pre-step
//! (`utils::repost.rs::process_events_to_feed_items`), and the three
//! real-time callbacks in `routes/home/mod.rs`. Those sites now all delegate
//! here; each feed picks a [`FeedRules`] flavor and gets identical behavior
//! on initial load, pagination, streaming, and real-time.
//!
//! # Rule flavors (characterized from the pre-unification call sites)
//!
//! - [`RepliesMode::TopLevelOnly`] — the Following tab: kind-1 thread replies
//!   are dropped; kind-1111 comments only as topic posts (NIP-73 `I` hashtag
//!   tags), also reply-filtered.
//! - [`RepliesMode::All`] — the FollowingWithReplies tab: every kind-1 note
//!   and every kind-1111 comment from the feed's authors.
//! - [`RepliesMode::Broadcast`] — global / people-list / relay feeds: kind-1
//!   notes keep their replies; kind-1111 comments only as topic posts
//!   (reply status not considered).
//!
//! Reposts (kind 6 and kind 16, NIP-18) are unwrapped into
//! [`FeedItem::Repost`] under every rules flavor; a repost whose content does
//! not parse as the original event is logged and dropped.

use crate::feeds::types::FeedItem;
use nostr::event::{Event, Kind};

/// How a feed treats conversation events (kind-1 replies and kind-1111
/// comments). See the module docs for the flavor each feed uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepliesMode {
    /// Following flavor: top-level posts only.
    TopLevelOnly,
    /// FollowingWithReplies flavor: every note and every comment.
    All,
    /// Broadcast flavor (global, people lists, relay feeds).
    Broadcast,
}

/// Classification rules for one feed surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedRules {
    pub replies: RepliesMode,
}

impl FeedRules {
    /// The Following tab (and its favorite-relay / streaming fallbacks):
    /// top-level posts only.
    pub const FOLLOWING: FeedRules = FeedRules {
        replies: RepliesMode::TopLevelOnly,
    };
    /// The FollowingWithReplies tab: all notes and comments.
    pub const FOLLOWING_WITH_REPLIES: FeedRules = FeedRules {
        replies: RepliesMode::All,
    };
    /// Global, people-list and relay feeds.
    pub const BROADCAST: FeedRules = FeedRules {
        replies: RepliesMode::Broadcast,
    };
}

/// Classify a single event into a feed item under `rules`.
///
/// Returns `None` for events the feed does not render (off-kind events,
/// replies the rules drop, comments that are not topic posts where the rules
/// require it, and reposts whose embedded original fails to parse).
pub fn classify_event(event: &Event, rules: FeedRules) -> Option<FeedItem> {
    if crate::utils::repost::is_repost(event) {
        return match crate::utils::repost::extract_reposted_event(event) {
            Ok(original) => Some(FeedItem::Repost {
                original,
                reposted_by: event.pubkey,
                repost_timestamp: event.created_at,
            }),
            Err(e) => {
                log::warn!("Failed to parse repost event {}: {}", event.id, e);
                None
            }
        };
    }
    match event.kind {
        Kind::TextNote => {
            let keep = match rules.replies {
                RepliesMode::TopLevelOnly => !crate::utils::event::is_thread_reply(event),
                RepliesMode::All | RepliesMode::Broadcast => true,
            };
            keep.then(|| FeedItem::OriginalPost(event.clone()))
        }
        Kind::Comment => match rules.replies {
            RepliesMode::All => Some(FeedItem::OriginalPost(event.clone())),
            RepliesMode::TopLevelOnly => {
                if crate::stores::topic_store::is_topic_post(event)
                    && !crate::utils::event::is_thread_reply(event)
                {
                    Some(FeedItem::OriginalPost(event.clone()))
                } else {
                    None
                }
            }
            RepliesMode::Broadcast => {
                if crate::stores::topic_store::is_topic_post(event) {
                    Some(FeedItem::OriginalPost(event.clone()))
                } else {
                    None
                }
            }
        },
        _ => None,
    }
}

/// Classify a batch of events into feed items sorted newest-first.
///
/// Does not dedup (callers own merge/dedup — see `feeds::list::FeedList`);
/// sorting is by `sort_timestamp` descending (repost time for reposts,
/// `created_at` otherwise), matching every pre-unification call site.
pub fn classify_events(events: Vec<Event>, rules: FeedRules) -> Vec<FeedItem> {
    let mut items: Vec<FeedItem> = events
        .into_iter()
        .filter_map(|event| classify_event(&event, rules))
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::{EventBuilder, EventId, FinalizeEvent, Tag};
    use nostr::nips::nip10::{Marker, Nip10Tag};
    use nostr::nips::nip22::Nip22Tag;
    use nostr::nips::nip73::ExternalContentId;
    use nostr_sdk::prelude::{Keys, Timestamp};

    fn keys() -> Keys {
        Keys::generate()
    }

    fn root_marker_tag() -> Tag {
        Tag::from(Nip10Tag::Event {
            id: EventId::from_slice(&[1u8; 32]).expect("valid id"),
            relay_hint: None,
            marker: Some(Marker::Root),
            public_key: None,
        })
    }

    fn note(kind: Kind, secs: u64, tags: Vec<Tag>) -> Event {
        EventBuilder::new(kind, "test")
            .tags(tags)
            .custom_created_at(Timestamp::from(secs))
            .finalize(&keys())
            .expect("signed test event")
    }

    fn topic_tag() -> Tag {
        Tag::from(Nip22Tag::ExternalContent {
            content: ExternalContentId::Hashtag("rust".to_string()),
            hint: None,
            uppercase: true,
        })
    }

    fn kind_name(kind: Kind) -> &'static str {
        match kind {
            Kind::TextNote => "note",
            Kind::Repost => "repost",
            Kind::GenericRepost => "generic repost",
            _ => "other",
        }
    }

    #[test]
    fn top_level_only_keeps_plain_notes() {
        let event = note(Kind::TextNote, 100, vec![]);
        assert!(classify_event(&event, FeedRules::FOLLOWING).is_some());
    }

    #[test]
    fn top_level_only_drops_thread_replies() {
        let reply = note(Kind::TextNote, 100, vec![root_marker_tag()]);
        assert!(classify_event(&reply, FeedRules::FOLLOWING).is_none());
    }

    #[test]
    fn top_level_only_keeps_topic_comment_roots_only() {
        let topic_root = note(Kind::Comment, 100, vec![topic_tag()]);
        assert!(classify_event(&topic_root, FeedRules::FOLLOWING).is_some());

        let plain_comment = note(Kind::Comment, 100, vec![]);
        assert!(classify_event(&plain_comment, FeedRules::FOLLOWING).is_none());

        // Topic post that is itself a reply: still dropped under Following.
        let topic_reply = note(Kind::Comment, 100, vec![topic_tag(), root_marker_tag()]);
        assert!(classify_event(&topic_reply, FeedRules::FOLLOWING).is_none());
    }

    #[test]
    fn all_keeps_replies_and_every_comment() {
        let reply = note(Kind::TextNote, 100, vec![root_marker_tag()]);
        assert!(classify_event(&reply, FeedRules::FOLLOWING_WITH_REPLIES).is_some());

        let plain_comment = note(Kind::Comment, 100, vec![]);
        assert!(
            classify_event(&plain_comment, FeedRules::FOLLOWING_WITH_REPLIES).is_some()
        );

        let topic_reply = note(Kind::Comment, 100, vec![topic_tag(), root_marker_tag()]);
        assert!(
            classify_event(&topic_reply, FeedRules::FOLLOWING_WITH_REPLIES).is_some()
        );
    }

    #[test]
    fn broadcast_keeps_note_replies_and_topic_comments() {
        // Characterizes `load_global_feed` / `load_people_list_feed` /
        // `load_relay_feed`: replies stay (a global firehose has no thread
        // context to drop them from), comments still need a topic tag.
        let reply = note(Kind::TextNote, 100, vec![root_marker_tag()]);
        assert!(classify_event(&reply, FeedRules::BROADCAST).is_some());

        let topic_reply = note(Kind::Comment, 100, vec![topic_tag(), root_marker_tag()]);
        assert!(classify_event(&topic_reply, FeedRules::BROADCAST).is_some());

        let plain_comment = note(Kind::Comment, 100, vec![]);
        assert!(classify_event(&plain_comment, FeedRules::BROADCAST).is_none());
    }

    #[test]
    fn reposts_of_both_kinds_are_unwrapped() {
        let original = note(Kind::TextNote, 50, vec![]);
        for kind in [Kind::Repost, Kind::GenericRepost] {
            let repost = EventBuilder::new(kind, original.as_json())
                .custom_created_at(Timestamp::from(100))
                .finalize(&keys())
                .expect("signed repost");
            let item =
                classify_event(&repost, FeedRules::FOLLOWING).unwrap_or_else(|| {
                    panic!("{} should classify", kind_name(kind))
                });
            match item {
                FeedItem::Repost {
                    original: inner,
                    repost_timestamp,
                    ..
                } => {
                    assert_eq!(inner.id, original.id);
                    assert_eq!(repost_timestamp.as_secs(), 100);
                }
                other => panic!("{} should be a Repost, got {other:?}", kind_name(kind)),
            }
        }
    }

    #[test]
    fn malformed_repost_is_dropped() {
        let repost = EventBuilder::new(Kind::Repost, "not json")
            .finalize(&keys())
            .expect("signed malformed repost");
        assert!(classify_event(&repost, FeedRules::FOLLOWING).is_none());
    }

    #[test]
    fn off_feed_kinds_are_dropped() {
        for kind in [Kind::Metadata, Kind::Reaction, Kind::LongFormTextNote] {
            let event = note(kind, 100, vec![]);
            assert!(classify_event(&event, FeedRules::BROADCAST).is_none());
        }
    }

    #[test]
    fn classify_events_sorts_newest_first_by_repost_time() {
        let original = note(Kind::TextNote, 50, vec![]);
        let old_note = note(Kind::TextNote, 100, vec![]);
        let new_repost = EventBuilder::new(Kind::Repost, original.as_json())
            .custom_created_at(Timestamp::from(300))
            .finalize(&keys())
            .expect("signed repost");
        let mid_note = note(Kind::TextNote, 200, vec![]);

        let items = classify_events(
            vec![old_note, new_repost, mid_note],
            FeedRules::FOLLOWING,
        );
        let stamps: Vec<u64> = items.iter().map(|i| i.sort_timestamp().as_secs()).collect();
        assert_eq!(stamps, vec![300, 200, 100]);
        // The repost sorts by repost time, not the original's created_at.
        assert!(items[0].repost_info().is_some());
    }
}
