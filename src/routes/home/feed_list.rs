//! Isolated feed-list renderer for the home route (plan wave B2).
//!
//! Why a child component: dioxus reactivity is per-scope — a signal read
//! anywhere in a component subscribes the *whole* component. With the list
//! inline in `Home` (a 2,400-line component), reading `interaction_counts`
//! in the render loop made every live-count / backfill write re-run Home's
//! entire scope. Moving the read into `FeedListBody`'s scope means count
//! updates re-render only this subtree, and `Home`'s body (header,
//! dropdown, composer, skeletons) no longer re-runs at all.
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
    /// Interaction counts signal. Read HERE, not in `Home`, so live count
    /// updates re-render only this subtree.
    pub counts: Signal<HashMap<String, InteractionCounts>>,
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
            && self.counts == other.counts
            && self.cached_muted_posts == other.cached_muted_posts
            && self.cached_blocked_users == other.cached_blocked_users
    }
}

/// Render the feed list. Each card keeps its stable `key` so dioxus'
/// keyed reconciliation reuses component state across list mutations.
#[component]
pub fn FeedListBody(props: FeedListBodyProps) -> Element {
    let counts = props.counts;
    let counts_map = counts.read();
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
                            precomputed_counts: counts_map.get(&event.id.to_hex()).cloned(),
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
