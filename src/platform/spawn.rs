/// Spawn a detached async task that runs independently.
///
/// On web, uses `wasm_bindgen_futures::spawn_local`.
/// On native, uses `tokio::spawn`.
///
/// Prefer `dioxus::prelude::spawn` when inside a Dioxus component context.
/// Use this for fire-and-forget tasks outside of component scope.
pub fn spawn_detached<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    #[cfg(feature = "web")]
    {
        wasm_bindgen_futures::spawn_local(future);
    }
    #[cfg(not(feature = "web"))]
    {
        tokio::spawn(future);
    }
}
