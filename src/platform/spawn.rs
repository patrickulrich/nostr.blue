//! Spawn a detached async task that runs independently.
//!
//! On web, uses `wasm_bindgen_futures::spawn_local`.
//! On native (desktop/mobile), uses `tokio::spawn`.
//!
//! Prefer `dioxus::prelude::spawn` when inside a Dioxus component context.
//! Use this for fire-and-forget tasks outside of component scope.

#[cfg(all(feature = "web", feature = "native"))]
compile_error!("Cannot enable both 'web' and 'native' features simultaneously");

#[cfg(not(any(feature = "web", feature = "native")))]
compile_error!("Must enable either 'web' or 'native' feature");

#[cfg(feature = "native")]
pub fn spawn_detached<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future);
}

#[cfg(feature = "web")]
pub fn spawn_detached<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}
