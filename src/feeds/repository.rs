//! Local database access layer for feeds.
//!
//! Wraps the SDK's `NostrDatabase` behind a mockable trait so feed logic can
//! be unit-tested without relays. The SDK auto-saves all fetched events to
//! its database (verified: `relay/inner.rs:1234` calls `save_event` before
//! emitting notifications), so `query_local` returns whatever was previously
//! fetched via subscribe/fetch/sync.
//!
//! ## Architecture
//!
//! ```
//! UI → use_feed → FeedLoader → FeedRepository → FeedDatabase (trait)
//!                                                      ↓
//!                                               SdkDatabase (prod)
//!                                          wrapping Arc<dyn NostrDatabase>
//!                                                      ↑
//!                                               InMemoryDatabase (test)
//!                                          wrapping HashMap<EventId, Event>
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use nostr::EventId;
use nostr_sdk::{Event, Filter, Timestamp};

use crate::utils::pagination::is_likely_future;
use crate::utils::repost::{process_events_to_feed_items, FeedItem};

/// Errors produced by the feed data-access layer.
#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Client error: {0}")]
    Client(String),
    #[error("Relay not found in pool: {0}")]
    RelayNotFound(String),
    #[error("No client available")]
    NoClient,
    #[error("Timeout")]
    Timeout,
    #[error("{0}")]
    Other(String),
}

impl From<nostr_sdk::client::Error> for FeedError {
    fn from(e: nostr_sdk::client::Error) -> Self {
        FeedError::Client(e.to_string())
    }
}

impl From<nostr_database::DatabaseError> for FeedError {
    fn from(e: nostr_database::DatabaseError) -> Self {
        FeedError::Database(e.to_string())
    }
}

impl From<String> for FeedError {
    fn from(s: String) -> Self {
        FeedError::Other(s)
    }
}

// ─── FeedDatabase trait ─────────────────────────────────────────────────────

/// Abstract database access for feeds. Implementations:
/// - [`SdkDatabase`]: production, wraps `Arc<dyn NostrDatabase>`
/// - [`InMemoryDatabase`]: test/mock, wraps `HashMap<EventId, Event>`
///
/// Uses `?Send` because the SDK's `NostrDatabase` returns non-`Send`
/// `BoxedFuture`s on WASM. This is fine: WASM is single-threaded, and on
/// native the database operations run on the same runtime thread.
#[async_trait(?Send)]
pub trait FeedDatabase: Send + Sync {
    /// Query the local database. Returns events matching the filter,
    /// sorted descending by `created_at` (the SDK's native ordering).
    async fn query(&self, filter: Filter) -> Result<Vec<Event>, FeedError>;

    /// Look up a single event by id.
    async fn get_event(&self, id: &EventId) -> Result<Option<Event>, FeedError>;
}

// ─── SdkDatabase (production) ───────────────────────────────────────────────

/// Production `FeedDatabase` wrapping the SDK's `NostrDatabase`.
pub struct SdkDatabase {
    db: Arc<dyn nostr_database::NostrDatabase>,
}

impl SdkDatabase {
    /// Construct from a `Client` reference (borrows its database Arc).
    pub fn from_client(client: &nostr_sdk::Client) -> Self {
        Self {
            db: client.database().clone(),
        }
    }

    /// Construct from an existing `Arc<dyn NostrDatabase>`.
    pub fn new(db: Arc<dyn nostr_database::NostrDatabase>) -> Self {
        Self { db }
    }
}

#[async_trait(?Send)]
impl FeedDatabase for SdkDatabase {
    async fn query(&self, filter: Filter) -> Result<Vec<Event>, FeedError> {
        let events = self.db.query(filter).await?;
        Ok(events.to_vec())
    }

    async fn get_event(&self, id: &EventId) -> Result<Option<Event>, FeedError> {
        Ok(self.db.event_by_id(id).await?)
    }
}

// ─── InMemoryDatabase (test/mock) ───────────────────────────────────────────

/// In-memory `FeedDatabase` for unit tests. Backed by a `HashMap<EventId, Event>`.
pub struct InMemoryDatabase {
    events: std::sync::RwLock<HashMap<EventId, Event>>,
}

impl InMemoryDatabase {
    pub fn new() -> Self {
        Self {
            events: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Insert an event into the mock database.
    pub fn insert(&self, event: Event) {
        let mut events = self.events.write().unwrap();
        events.insert(event.id, event);
    }

    /// Insert multiple events.
    pub fn extend(&self, events: impl IntoIterator<Item = Event>) {
        let mut store = self.events.write().unwrap();
        for event in events {
            store.insert(event.id, event);
        }
    }

    /// Number of stored events.
    pub fn len(&self) -> usize {
        self.events.read().unwrap().len()
    }

    /// Whether the database is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for InMemoryDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait(?Send)]
impl FeedDatabase for InMemoryDatabase {
    async fn query(&self, filter: Filter) -> Result<Vec<Event>, FeedError> {
        let store = self.events.read().unwrap();
        let mut results: Vec<Event> = store
            .values()
            .filter(|event| filter_set_matches_event(&filter, event))
            .cloned()
            .collect();
        // Sort descending by created_at, then ascending by id (matches SDK Event::Ord).
        results.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        // Apply limit
        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }
        Ok(results)
    }

    async fn get_event(&self, id: &EventId) -> Result<Option<Event>, FeedError> {
        Ok(self.events.read().unwrap().get(id).cloned())
    }
}

/// Client-side filter matching for the InMemoryDatabase mock.
/// Mirrors the essential fields of NIP-01 filter semantics.
fn filter_set_matches_event(filter: &Filter, event: &Event) -> bool {
    // ids
    if let Some(ids) = &filter.ids {
        if !ids.contains(&event.id) {
            return false;
        }
    }
    // authors
    if let Some(authors) = &filter.authors {
        if !authors.contains(&event.pubkey) {
            return false;
        }
    }
    // kinds
    if let Some(kinds) = &filter.kinds {
        if !kinds.contains(&event.kind) {
            return false;
        }
    }
    // since
    if let Some(since) = filter.since {
        if event.created_at <= since {
            return false;
        }
    }
    // until
    if let Some(until) = filter.until {
        if event.created_at >= until {
            return false;
        }
    }
    // search (NIP-50): simple substring check
    if let Some(search) = &filter.search {
        if !event.content.to_lowercase().contains(&search.to_lowercase()) {
            return false;
        }
    }
    // Generic tags (#e, #p, #t, #d, etc.) — check if event has matching tag values
    for (tag_key, tag_values) in &filter.generic_tags {
        let letter = tag_key.as_char();
        let matches = event.tags.iter().any(|tag| {
            if let Some(std) = tag.as_standardized() {
                // Check if this tag's single-letter matches and value is in the filter set
                match std {
                    nostr_sdk::TagStandard::Event { event_id, .. } if letter == 'e' => {
                        tag_values.contains(event_id.to_hex().as_str())
                    }
                    nostr_sdk::TagStandard::PublicKey { public_key, .. } if letter == 'p' => {
                        tag_values.contains(public_key.to_hex().as_str())
                    }
                    nostr_sdk::TagStandard::Hashtag(hash) if letter == 't' => {
                        tag_values.contains(hash.as_str())
                    }
                    nostr_sdk::TagStandard::Identifier(id) if letter == 'd' => {
                        tag_values.contains(id.as_str())
                    }
                    _ => false,
                }
            } else {
                false
            }
        });
        if !matches {
            return false;
        }
    }
    true
}

// ─── FeedRepository ──────────────────────────────────────────────────────────

/// High-level feed data access. Combines the database with event-to-FeedItem
/// conversion and future-event filtering.
pub struct FeedRepository {
    db: Arc<dyn FeedDatabase>,
}

impl FeedRepository {
    /// Construct with a `FeedDatabase` implementation.
    pub fn new(db: Arc<dyn FeedDatabase>) -> Self {
        Self { db }
    }

    /// Construct with the production `SdkDatabase` from a `Client`.
    pub fn from_client(client: &nostr_sdk::Client) -> Self {
        Self::new(Arc::new(SdkDatabase::from_client(client)))
    }

    /// Query the local database for raw events matching the filter.
    pub async fn query_local(&self, filter: Filter) -> Result<Vec<Event>, FeedError> {
        self.db.query(filter).await
    }

    /// Look up a single event by id.
    pub async fn get_event(&self, id: &EventId) -> Result<Option<Event>, FeedError> {
        self.db.get_event(id).await
    }

    /// Convert raw events to FeedItems via `process_events_to_feed_items`,
    /// applying future-event filtering.
    ///
    /// This is the canonical "events → feed items" pipeline:
    /// 1. Drop events with future timestamps (defense against clock-skewed spam)
    /// 2. Delegate to `utils::repost::process_events_to_feed_items` which:
    ///    - Expands kind-6/16 reposts (NIP-18)
    ///    - Filters out replies (keeps root posts only)
    ///    - Handles kind-1111 topic posts
    ///    - Sorts descending by sort_timestamp
    pub fn events_to_feed_items(events: Vec<Event>) -> Vec<FeedItem> {
        let now = Timestamp::now();
        let filtered: Vec<Event> = events
            .into_iter()
            .filter(|e| e.created_at <= now || !is_likely_future(e.created_at))
            .collect();
        process_events_to_feed_items(filtered)
    }

    /// Query the local DB and return FeedItems in one step.
    pub async fn load_page(&self, filter: Filter) -> Result<Vec<FeedItem>, FeedError> {
        let events = self.query_local(filter).await?;
        Ok(Self::events_to_feed_items(events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::{EventBuilder, Keys, Kind, RelayUrl, Timestamp};

    fn make_event(kind: Kind, content: &str, secs: u64) -> Event {
        let keys = Keys::generate();
        EventBuilder::new(kind, content)
            .custom_created_at(Timestamp::from(secs))
            .sign_with_keys(&keys)
            .unwrap()
    }

    fn make_event_by(keys: &Keys, kind: Kind, content: &str, secs: u64) -> Event {
        EventBuilder::new(kind, content)
            .custom_created_at(Timestamp::from(secs))
            .sign_with_keys(keys)
            .unwrap()
    }

    #[tokio::test]
    async fn inmemory_db_query_by_kind() {
        let db = InMemoryDatabase::new();
        db.insert(make_event(Kind::TextNote, "hello", 1000));
        db.insert(make_event(Kind::Reaction, "+", 1001));
        let filter = Filter::new().kind(Kind::TextNote);
        let results = FeedDatabase::query(&db, filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "hello");
    }

    #[tokio::test]
    async fn inmemory_db_query_by_author() {
        let db = InMemoryDatabase::new();
        let keys = Keys::generate();
        db.insert(make_event_by(&keys, Kind::TextNote, "mine", 1000));
        db.insert(make_event(Kind::TextNote, "other", 1001));
        let filter = Filter::new().author(keys.public_key());
        let results = FeedDatabase::query(&db, filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "mine");
    }

    #[tokio::test]
    async fn inmemory_db_query_with_limit() {
        let db = InMemoryDatabase::new();
        for i in 0..10u64 {
            db.insert(make_event(Kind::TextNote, &format!("post {i}"), 1000 + i));
        }
        let filter = Filter::new().kind(Kind::TextNote).limit(3);
        let results = FeedDatabase::query(&db, filter).await.unwrap();
        assert_eq!(results.len(), 3);
        // Newest first (descending)
        assert_eq!(results[0].content, "post 9");
        assert_eq!(results[1].content, "post 8");
        assert_eq!(results[2].content, "post 7");
    }

    #[tokio::test]
    async fn inmemory_db_query_with_until() {
        let db = InMemoryDatabase::new();
        db.insert(make_event(Kind::TextNote, "old", 500));
        db.insert(make_event(Kind::TextNote, "new", 1000));
        let filter = Filter::new()
            .kind(Kind::TextNote)
            .until(Timestamp::from(750));
        let results = FeedDatabase::query(&db, filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "old");
    }

    #[tokio::test]
    async fn inmemory_db_query_with_since() {
        let db = InMemoryDatabase::new();
        db.insert(make_event(Kind::TextNote, "old", 500));
        db.insert(make_event(Kind::TextNote, "new", 1000));
        let filter = Filter::new()
            .kind(Kind::TextNote)
            .since(Timestamp::from(750));
        let results = FeedDatabase::query(&db, filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "new");
    }

    #[tokio::test]
    async fn inmemory_db_get_event_by_id() {
        let db = InMemoryDatabase::new();
        let event = make_event(Kind::TextNote, "find me", 1000);
        let id = event.id;
        db.insert(event);
        let result = FeedDatabase::get_event(&db, &id).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().content, "find me");
    }

    #[tokio::test]
    async fn repository_load_page_returns_feed_items() {
        let db = Arc::new(InMemoryDatabase::new());
        db.insert(make_event(Kind::TextNote, "post 1", 1000));
        db.insert(make_event(Kind::TextNote, "post 2", 2000));
        let repo = FeedRepository::new(db);
        let filter = Filter::new().kind(Kind::TextNote);
        let items = repo.load_page(filter).await.unwrap();
        assert_eq!(items.len(), 2);
        // Descending: post 2 first
        assert_eq!(items[0].event().content, "post 2");
        assert_eq!(items[1].event().content, "post 1");
    }

    #[tokio::test]
    async fn repository_empty_filter_returns_empty() {
        let db = Arc::new(InMemoryDatabase::new());
        let repo = FeedRepository::new(db);
        let filter = Filter::new().kind(Kind::TextNote);
        let items = repo.load_page(filter).await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn events_to_feed_items_filters_replies() {
        // This test verifies the integration with process_events_to_feed_items
        // which drops events with reply/root markers.
        let keys = Keys::generate();
        let root = EventBuilder::new(Kind::TextNote, "root post")
            .custom_created_at(Timestamp::from(1000))
            .sign_with_keys(&keys)
            .unwrap();
        // Create a reply (with root marker)
        let reply = EventBuilder::new(Kind::TextNote, "reply")
            .custom_created_at(Timestamp::from(2000))
            .tag(nostr_sdk::Tag::from_standardized_without_cell(
                nostr_sdk::TagStandard::Event {
                    event_id: root.id,
                    relay_url: None,
                    public_key: Some(root.pubkey),
                    marker: Some(nostr_sdk::nips::nip10::Marker::Root),
                    uppercase: false,
                },
            ))
            .sign_with_keys(&keys)
            .unwrap();
        let events = vec![root, reply];
        let items = FeedRepository::events_to_feed_items(events);
        // Only the root post should remain (reply is filtered out)
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].event().content, "root post");
    }
}
