//! Feed subsystem: the canonical feed item types, ordering, pagination
//! cursors, and filter construction.
//!
//! ## Module layout
//!
//! - [`types`]: the `FeedItem` enum (with `Composite` variant) and interaction
//!   info structs.
//! - [`ordering`]: stable feed-item ordering with ascending-id tiebreaker.
//! - [`cursor`]: gap-proof backward pagination (`RelayLoadingCursors`).
//! - [`filter`]: per-feed-type `Filter` builders and limit scaling.
//!
//! ## Architecture
//!
//! This subsystem provides the **pure algorithmic primitives** for the feed
//! pipeline. The data-access layer (repository, outbox routing, negentropy)
//! and orchestration layer (loader, pager, realtime) are added in later
//! phases; for now, Phase 1 is pure, I/O-free, fully unit-testable code that
//! existing modules can adopt incrementally.
//!
//! See the plan document (`feeds/PLAN.md` if present, or the conversation
//! history) for the full architecture and migration order.

// Phase 1 provides pure algorithmic primitives that don't have consumers
// within this crate yet (Phase 2+ will wire them into the feed pipeline).
// Dead-code warnings are expected and suppressed until consumers land.
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(clippy::for_kv_map)]

pub mod cursor;
pub mod filter;
pub mod ingestion;
pub mod loader;
pub mod negentropy;
pub mod ordering;
pub mod outbox;
pub mod pager;
pub mod realtime;
pub mod repository;
pub mod types;

// Re-export the most commonly used items at the module root for convenience.
pub use cursor::{
    RelayCursor, RelayLoadingCursors, DEFAULT_LIVE_TAIL_SECS, DEFAULT_PAGE_LIMIT,
};
pub use filter::{
    following_filter, following_with_replies_filter, global_filter, notifications_filter,
    people_list_filter, relay_feed_filter, scaled_limit, should_since_optimize,
    with_since_optimization, HOME_FEED_KINDS,
};
pub use ingestion::{DrainResult, FrameBudgetProcessor, DEFAULT_FRAME_BUDGET, WASM_FRAME_BUDGET};
pub use loader::{FeedKind, FeedLoader, LoadResult};
pub use negentropy::{NegentropySync, SyncResult};
pub use ordering::{cmp_feed_items, sort_feed_items};
pub use outbox::{OutboxRouter, OutboxTargets, INDEXER_RELAYS, MAX_AUTHORS_PER_FILTER, MIN_REDUNDANCY};
pub use pager::{BackwardRelayPager, PagingStatus};
pub use realtime::{eose_threshold, EoseState, PerSubEoseTracker, RealtimeConfig};
pub use repository::{
    FeedDatabase, FeedError, FeedRepository, InMemoryDatabase, SdkDatabase,
};
pub use types::{FeedItem, InteractionSummary, ReactionInfo, RepostInfo, ZapInfo};
