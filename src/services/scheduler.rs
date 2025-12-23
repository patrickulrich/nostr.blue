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
    // Key package maintenance (every 10 minutes)
    use_future(|| async {
        // Initial delay to let app initialize
        gloo_timers::future::sleep(Duration::from_secs(30)).await;

        loop {
            run_key_package_maintenance().await;
            gloo_timers::future::sleep(Duration::from_secs(600)).await;
        }
    });

    // Stale profile cleanup (every hour)
    use_future(|| async {
        loop {
            gloo_timers::future::sleep(Duration::from_secs(3600)).await;
            run_stale_profile_cleanup().await;
        }
    });
}

/// Key package maintenance - ensure we have a valid MLS key package
/// MDK doesn't have built-in expiration, so we track age ourselves
async fn run_key_package_maintenance() {
    use crate::stores::{auth_store, mdk_store, relay_store};
    use chrono::{Duration as ChronoDuration, Utc};

    // Only run if authenticated
    if !auth_store::is_authenticated() {
        return;
    }

    // Check if we have a key package and its age
    let key_package_info = mdk_store::KEY_PACKAGE_INFO.read().clone();
    let needs_publish = match key_package_info {
        None => true, // No key package - need to publish
        Some(info) => {
            // Check if older than 30 days (like White Noise)
            let age = Utc::now().signed_duration_since(info.published_at);
            age > ChronoDuration::days(30)
        }
    };

    if needs_publish {
        // Get key package relays (fall back to defaults)
        let relay_strs = relay_store::get_key_package_relays();

        if let Err(e) = mdk_store::publish_key_package(relay_strs).await {
            log::warn!("Failed to publish key package: {}", e);
        } else {
            log::info!("Published MLS key package");
        }
    }
}

/// Stale profile cleanup - prune profiles not accessed recently
async fn run_stale_profile_cleanup() {
    use crate::stores::profiles::PROFILE_CACHE;

    // LRU cache auto-evicts, but we log cache size for monitoring
    let cache_size = PROFILE_CACHE.read().len();
    log::debug!("Profile cache size: {}", cache_size);
}
