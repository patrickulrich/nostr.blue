use std::time::Duration;

/// Sleep for the given duration (async, cross-platform).
pub async fn sleep(duration: Duration) {
    #[cfg(feature = "web")]
    {
        gloo_timers::future::sleep(duration).await;
    }
    #[cfg(feature = "native")]
    {
        tokio::time::sleep(duration).await;
    }
}

/// Sleep for the given number of milliseconds (async, cross-platform).
pub async fn sleep_ms(ms: u32) {
    #[cfg(feature = "web")]
    {
        gloo_timers::future::TimeoutFuture::new(ms).await;
    }
    #[cfg(feature = "native")]
    {
        tokio::time::sleep(Duration::from_millis(ms as u64)).await;
    }
}
