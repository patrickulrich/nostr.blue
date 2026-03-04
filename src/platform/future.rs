//! Platform-aware future type aliases
//!
//! On native (desktop/mobile): futures must be `Send` (multi-threaded tokio)
//! On WASM (web): futures don't need `Send` (single-threaded)
//!
//! Follows the pattern from nostr-sdk:
//! - `target_arch = "wasm32"` + `target_os = "unknown"` = browser WASM
//! - This excludes wasi, emscripten, etc.

use std::future::Future;
use std::pin::Pin;

/// Boxed future with platform-appropriate Send bound
///
/// - Native (desktop/mobile): `Pin<Box<dyn Future + Send>>`
/// - WASM (web): `Pin<Box<dyn Future>>` (no Send required)
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub type BoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Boxed future without Send bound (WASM is single-threaded)
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub type BoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
