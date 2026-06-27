//! Consolidated feed loader: one entry point for all feed types.
//!
//! Replaces the 6+ duplicated inline loaders in `routes/home/feed_loaders.rs`.
//! All event processing funnels through `process_events_to_feed_items`.
//!
//! ## Architecture
//!
//! ```
//! UI → use_feed → FeedLoader → FeedRepository → SDK database
//!                          ↓                ↑
//!                     OutboxRouter ───→ subscribe_targeted
//!                          ↓
//!                     RealtimeConfig (live tail)
//! ```

use std::sync::Arc;
use std::time::Duration;

use nostr_sdk::{Client, Filter, PublicKey, RelayUrl, Timestamp};

use super::filter::{
    following_filter, following_with_replies_filter, global_filter, people_list_filter,
    relay_feed_filter, scaled_limit, should_since_optimize, with_since_optimization,
    DEFAULT_PAGE_LIMIT, SINCE_GAP_SECS,
};
use super::outbox::OutboxRouter;
use super::repository::{FeedError, FeedRepository};
use crate::utils::repost::FeedItem;

/// The feed types supported by the loader.
#[derive(Clone, Debug)]
pub enum FeedKind {
    /// Posts from followed authors (root posts only; replies filtered out).
    Following { authors: Vec<PublicKey> },
    /// Posts from followed authors including replies.
    FollowingWithReplies { authors: Vec<PublicKey> },
    /// Global firehose (all posts from connected relays).
    Global,
    /// Posts from a NIP-51 people-list's members.
    PeopleList { members: Vec<PublicKey> },
    /// Posts from a specific relay set.
    RelayFeed { urls: Vec<RelayUrl> },
    /// Notifications for the user (mentions, reactions, zaps, reposts).
    Notifications { pubkey: PublicKey },
}

/// Result of an initial feed load.
#[derive(Clone, Debug, Default)]
pub struct LoadResult {
    /// The loaded feed items, sorted descending by sort_timestamp.
    pub items: Vec<FeedItem>,
    /// Whether the loader fell back to the global feed (e.g. user has 0 follows).
    pub fell_back_to_global: bool,
    /// The number of authors that were successfully routed (for diagnostics).
    pub authors_covered: usize,
}

/// The consolidated feed loader. One per feed scope.
pub struct FeedLoader {
    client: Arc<Client>,
    repository: FeedRepository,
    #[allow(dead_code)]
    outbox: OutboxRouter,
}

impl FeedLoader {
    /// Construct with the SDK client.
    pub fn new(client: Arc<Client>) -> Self {
        let repository = FeedRepository::from_client(&client);
        let outbox = OutboxRouter::from_client(client.clone());
        Self {
            client,
            repository,
            outbox,
        }
    }

    /// Get the feed kind's filter for the initial page.
    ///
    /// `until` is `None` for the initial load; `Some(ts)` for pagination.
    /// `since` is `None` unless since-optimization is applicable.
    pub fn build_filter(
        &self,
        kind: &FeedKind,
        until: Option<Timestamp>,
        since: Option<Timestamp>,
    ) -> Filter {
        match kind {
            FeedKind::Following { authors }
            | FeedKind::FollowingWithReplies { authors } => {
                if matches!(kind, FeedKind::Following { .. }) {
                    following_filter(authors, until, since)
                } else {
                    following_with_replies_filter(authors, until, since)
                }
            }
            FeedKind::Global => global_filter(until, since),
            FeedKind::PeopleList { members } => people_list_filter(members, until, since),
            FeedKind::RelayFeed { .. } => relay_feed_filter(until, since),
            FeedKind::Notifications { pubkey } => {
                super::filter::notifications_filter(pubkey, since)
            }
        }
    }

    /// Initial load: query the local database for cached events.
    ///
    /// This is instant for cached data. The caller should also issue
    /// relay subscriptions for fresh data (see `start_realtime`).
    ///
    /// For `Following` feeds with 0 authors, falls back to `Global`.
    pub async fn initial_load(&self, kind: &FeedKind) -> Result<LoadResult, FeedError> {
        match kind {
            FeedKind::Following { authors }
            | FeedKind::FollowingWithReplies { authors } => {
                if authors.is_empty() {
                    // Fall back to global
                    let filter = global_filter(None, None);
                    let items = self.repository.load_page(filter).await?;
                    return Ok(LoadResult {
                        items,
                        fell_back_to_global: true,
                        authors_covered: 0,
                    });
                }
                let filter = self.build_filter(kind, None, None);
                let items = self.repository.load_page(filter).await?;
                Ok(LoadResult {
                    items,
                    fell_back_to_global: false,
                    authors_covered: authors.len(),
                })
            }
            FeedKind::Global => {
                let filter = global_filter(None, None);
                let items = self.repository.load_page(filter).await?;
                Ok(LoadResult {
                    items,
                    fell_back_to_global: false,
                    authors_covered: 0,
                })
            }
            FeedKind::PeopleList { members } => {
                if members.is_empty() {
                    return Ok(LoadResult::default());
                }
                let filter = self.build_filter(kind, None, None);
                let items = self.repository.load_page(filter).await?;
                Ok(LoadResult {
                    items,
                    fell_back_to_global: false,
                    authors_covered: members.len(),
                })
            }
            FeedKind::RelayFeed { urls } => {
                // Connect relays first (they may not be in the pool yet)
                for url in urls {
                    let _ = self.client.add_relay(url.clone()).await;
                }
                let _ = self.client.connect().await;

                let filter = relay_feed_filter(None, None);
                let items = self.repository.load_page(filter).await?;
                Ok(LoadResult {
                    items,
                    fell_back_to_global: false,
                    authors_covered: 0,
                })
            }
            FeedKind::Notifications { pubkey: _ } => {
                let filter = self.build_filter(kind, None, None);
                let items = self.repository.load_page(filter).await?;
                Ok(LoadResult {
                    items,
                    fell_back_to_global: false,
                    authors_covered: 0,
                })
            }
        }
    }

    /// Load a paginated page using `until` cursor.
    pub async fn load_page(
        &self,
        kind: &FeedKind,
        until: Timestamp,
    ) -> Result<Vec<FeedItem>, FeedError> {
        let filter = self.build_filter(kind, Some(until), None);
        self.repository.load_page(filter).await
    }

    /// Compute the `since` timestamp for the live-tail subscription.
    ///
    /// Uses since-optimization: if the local DB has at least `limit` events,
    /// returns `latest_local - 60s` (the notedeck pattern with overlap buffer).
    /// Otherwise returns `None` (no `since` filter — fetch everything).
    pub async fn compute_since(
        &self,
        kind: &FeedKind,
        limit: usize,
    ) -> Option<Timestamp> {
        // Query local DB count (lightweight)
        let count_filter = self.build_filter(kind, None, None);
        let local_count = self.repository.query_local(count_filter).await.ok()?.len();

        if !should_since_optimize(limit, local_count) {
            return None;
        }

        // Get the latest local event timestamp
        let mut filter = self.build_filter(kind, None, None);
        filter = filter.limit(1);
        let events = self.repository.query_local(filter).await.ok()?;
        events
            .first()
            .map(|e| Timestamp::from(e.created_at.as_secs().saturating_sub(SINCE_GAP_SECS)))
    }

    /// Build the realtime subscription filter for the live tail.
    pub fn realtime_filter(&self, kind: &FeedKind, since: Option<Timestamp>) -> Filter {
        match kind {
            FeedKind::Following { authors }
            | FeedKind::FollowingWithReplies { authors } => {
                following_filter(authors, None, since)
            }
            FeedKind::Global => global_filter(None, since),
            FeedKind::PeopleList { members } => people_list_filter(members, None, since),
            FeedKind::RelayFeed { .. } => relay_feed_filter(None, since),
            FeedKind::Notifications { pubkey } => {
                super::filter::notifications_filter(pubkey, since)
            }
        }
    }

    /// The page limit for this feed kind (used for since-optimization and pagination).
    pub fn page_limit(&self, kind: &FeedKind) -> usize {
        match kind {
            FeedKind::Following { authors }
            | FeedKind::FollowingWithReplies { authors } => scaled_limit(authors.len()),
            FeedKind::Global | FeedKind::RelayFeed { .. } => DEFAULT_PAGE_LIMIT,
            FeedKind::PeopleList { members } => scaled_limit(members.len()),
            FeedKind::Notifications { .. } => 500,
        }
    }

    /// Get a reference to the repository (for the pager).
    pub fn repository(&self) -> &FeedRepository {
        &self.repository
    }

    /// Get a reference to the outbox router.
    pub fn outbox(&self) -> &OutboxRouter {
        &self.outbox
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::Keys;

    fn pks(n: usize) -> Vec<PublicKey> {
        (0..n).map(|_| Keys::generate().public_key()).collect()
    }

    #[test]
    fn page_limit_scales_with_authors() {
        // Can't construct FeedLoader without a Client, but we can test
        // the static limit computation.
        assert_eq!(scaled_limit(10), 100);
        assert_eq!(scaled_limit(100), 500);
        assert_eq!(scaled_limit(0), 50);
    }

    #[test]
    fn feed_kind_debug() {
        let kind = FeedKind::Global;
        assert!(format!("{:?}", kind).contains("Global"));
    }
}
