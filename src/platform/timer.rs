use std::time::Duration;

/// Sleep for the given duration (async, cross-platform).
///
/// Gated on target architecture, not feature: the `web` feature also compiles
/// on native hosts (unit tests), where the gloo/wasm-bindgen backends are
/// non-functional stubs that panic. Mirrors `nostr_client::platform_sleep_ms`.
pub async fn sleep(duration: Duration) {
    #[cfg(target_arch = "wasm32")]
    {
        gloo_timers::future::sleep(duration).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::sleep(duration).await;
    }
}

/// Sleep for the given number of milliseconds (async, cross-platform).
pub async fn sleep_ms(ms: u32) {
    #[cfg(target_arch = "wasm32")]
    {
        gloo_timers::future::TimeoutFuture::new(ms).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::sleep(Duration::from_millis(ms as u64)).await;
    }
}
