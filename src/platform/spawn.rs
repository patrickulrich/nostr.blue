//! Spawn a detached async task that runs independently.
//!
//! On web, uses `wasm_bindgen_futures::spawn_local`.
//! On native, uses `tokio::spawn`.
//!
//! Prefer `dioxus::prelude::spawn` when inside a Dioxus component context.
//! Use this for fire-and-forget tasks outside of component scope.

/// Spawn a detached task on native platforms (requires Send)
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_detached<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future);
}

/// Spawn a detached task on WASM (no Send requirement)
#[cfg(target_arch = "wasm32")]
pub fn spawn_detached<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}
