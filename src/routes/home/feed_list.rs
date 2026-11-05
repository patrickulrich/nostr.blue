//! Isolated feed-list renderer for the home route (plan wave B2 + R3a).
//!
//! Why a child component: dioxus reactivity is per-scope — a signal read
//! anywhere in a component subscribes the *whole* component. With the list
//! inline in `Home` (a 2,400-line component), reading `interaction_counts`
//! in the render loop made every live-count / backfill write re-run Home's
//! entire scope. Extracting the list isolated Home; R3a goes further — the
//! counts signal is now *passed through unread* (handle-identity compare,
//! so this component still re-renders only when the items list changes)
//! and each NoteCard subscribes its own tiny counts-sync leaf to it. A
//! count write therefore re-renders only N tiny leaves, never this body,
//! Home, or any card body.
//!
//! Props are memoized with an `Rc::ptr_eq` fast path on the items list:
//! the feed state signal holds `Rc<Vec<FeedItem>>` and a new `Rc` is only
//! allocated when the list contents actually changed (wave B1's
//! write-skipping), so pointer equality is exact — an O(1) compare instead
//! of a deep compare of every event.

use crate::components::{ArticleCard, NoteCard};
use crate::services::aggregation::InteractionCounts;
use crate::utils::FeedItem;
use dioxus::prelude::*;
use nostr::event::Kind;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Props, Clone)]
pub struct FeedListBodyProps {
    /// The whole feed, newest first, shared with the signal's `Rc` (a
    /// parent render clones a refcount, not the events).
    pub items: Rc<Vec<FeedItem>>,
    /// Interaction counts map signal. Passed THROUGH (never read here):
    /// signal props compare by handle identity, so this never re-renders
    /// `FeedListBody`. Each `NoteCard` subscribes its own tiny counts-sync
    /// leaf to this signal — a count write re-renders only those leaves,
    /// not this list body or any card body.
    pub counts_signal: Signal<HashMap<String, InteractionCounts>>,
    pub cached_muted_posts: Option<Rc<HashSet<String>>>,
    pub cached_blocked_users: Option<Rc<HashSet<String>>>,
}

// Hand-written instead of derived: the items fast path is pointer equality
// (a fresh Rc is only allocated when the list changed), the counts signal
// compares by handle identity (same signal every render), and the mute and
// block caches are rare-changing `Rc`s.
impl PartialEq for FeedListBodyProps {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.items, &other.items)
            && self.counts_signal == other.counts_signal
            && self.cached_muted_posts == other.cached_muted_posts
            && self.cached_blocked_users == other.cached_blocked_users
    }
}

/// Render the feed list. Each card keeps its stable `key` so dioxus'
/// keyed reconciliation reuses component state across list mutations.
#[component]
pub fn FeedListBody(props: FeedListBodyProps) -> Element {
    rsx! {
        for feed_item in props.items.iter() {
            {
                let event = feed_item.event();
                let repost_info = feed_item.repost_info();
                if event.kind == Kind::LongFormTextNote {
                    rsx! {
                        ArticleCard { key: "{event.id}", event: event.clone() }
                    }
                } else {
                    rsx! {
                        NoteCard {
                            key: "{event.id}",
                            event: event.clone(),
                            repost_info,
                            counts_signal: props.counts_signal,
                            collapsible: true,
                            cached_muted_posts: props.cached_muted_posts.clone(),
                            cached_blocked_users: props.cached_blocked_users.clone(),
                        }
                    }
                }
            }
        }
    }
}
