//! nostr-sdk 0.45 compatibility helpers.
//!
//! **Folded into `stores::nostr_client::compat`** (plan: nostr-sdk-dry-facade,
//! Phase 1) so removed-SDK API shims live inside the client wrapper — the next
//! SDK migration then touches exactly one module. This file remains as a
//! re-export shim for existing import paths.
//!
//! Note: `fetch_metadata` now routes through the indexer relays (it previously
//! used a plain READ-relay fetch that could not reach DISCOVERY-only indexers)
//! and returns `Result<_, String>`.

pub use crate::stores::nostr_client::compat::*;
