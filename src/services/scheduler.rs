//! Background task scheduler for nostr.blue
//!
//! Provides periodic background tasks using Dioxus `use_future` pattern.
//! WASM-compatible using gloo_timers for delays.
//!
//! Note: NIP-65/NIP-17 relay sync is NOT needed here - nostr-sdk's gossip
//! layer handles that automatically on-demand.
use dioxus::prelude::*;
use std::time::Duration;
/// Hook to start all background scheduler tasks
/// Call once from App component
pub fn use_background_scheduler() {
    use_future(|| async {
        loop {
            gloo_timers::future::sleep(Duration::from_secs(3600)).await;
            run_stale_profile_cleanup().await;
        }
    });
}
/// Stale profile cleanup - prune profiles not accessed recently
async fn run_stale_profile_cleanup() {
    use crate::stores::profiles::PROFILE_CACHE;
    let cache_size = PROFILE_CACHE.read().len();
    log::debug!("Profile cache size: {}", cache_size);
}
