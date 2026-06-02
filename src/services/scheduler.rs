use dioxus::prelude::*;
use std::time::Duration;

pub fn use_background_scheduler() {
    use_future(|| async {
        loop {
            crate::platform::timer::sleep(Duration::from_secs(3600)).await;
            run_stale_profile_cleanup().await;
        }
    });
}

async fn run_stale_profile_cleanup() {
    use crate::stores::profiles::{CACHE_TTL_SECONDS, PROFILE_CACHE};
    let mut cache = PROFILE_CACHE.write();
    let before = cache.len();
    let stale_keys: Vec<String> = cache
        .iter()
        .filter(|(_, profile)| {
            let age = chrono::Utc::now().signed_duration_since(profile.fetched_at);
            age.num_seconds() >= CACHE_TTL_SECONDS
        })
        .map(|(k, _)| k.clone())
        .collect();
    for key in &stale_keys {
        cache.pop(key);
    }
    if !stale_keys.is_empty() {
        log::info!(
            "Profile cache cleanup: removed {}/{} stale entries",
            stale_keys.len(),
            before
        );
    }
}
