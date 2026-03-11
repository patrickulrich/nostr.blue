//! Shared HTTP client with connection pooling and timeout.
//!
//! Consolidates the identical `http_client()` function previously duplicated
//! across 10 service modules. Provides a 15-second timeout on native platforms.

/// Returns a shared, lazily-initialized HTTP client.
///
/// On native platforms, a 15-second timeout is configured. On WASM, the timeout
/// API is not available so the client relies on the browser's fetch timeout.
pub(crate) fn http_client() -> Result<&'static reqwest::Client, &'static reqwest::Error> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::Client, reqwest::Error>> =
        std::sync::OnceLock::new();
    match CLIENT.get_or_init(|| {
        let builder =
            reqwest::Client::builder().user_agent("Mozilla/5.0 (compatible; NostrBlueBot/1.0)");
        #[cfg(not(target_arch = "wasm32"))]
        let builder = builder
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none());
        builder.build()
    }) {
        Ok(client) => Ok(client),
        Err(e) => Err(e),
    }
}
