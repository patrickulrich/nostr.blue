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

fn log_caught_panic(name: &str, panic: Box<dyn std::any::Any>) {
    let msg = panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic>".to_string());
    let full = format!("CAUGHT PANIC in task '{}': {}", name, msg);
    #[cfg(feature = "web")]
    {
        let js_val: wasm_bindgen::JsValue = full.clone().into();
        web_sys::console::error_1(&js_val);
        web_sys::console::log_1(&full.as_str().into());
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item("__rust_panic__", &full);
            }
        }
    }
    #[cfg(not(feature = "web"))]
    {
        log::error!("{}", full);
    }
}

pub fn spawn_catch_unwind<F>(name: &'static str, future: F) -> dioxus::core::Task
where
    F: std::future::Future<Output = ()> + 'static,
{
    dioxus::prelude::spawn(async move {
        use std::panic::AssertUnwindSafe;
        use futures::FutureExt;
        if let Err(p) = AssertUnwindSafe(future).catch_unwind().await {
            log_caught_panic(name, p);
        }
    })
}

#[cfg(target_arch = "wasm32")]
pub fn spawn_local_catch_unwind<F>(name: &'static str, future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(async move {
        use std::panic::AssertUnwindSafe;
        use futures::FutureExt;
        if let Err(p) = AssertUnwindSafe(future).catch_unwind().await {
            log_caught_panic(name, p);
        }
    });
}

pub fn spawn_forever_catch_unwind<F>(name: &'static str, future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    use dioxus::core as dioxus_core;
    dioxus_core::spawn_forever(async move {
        use std::panic::AssertUnwindSafe;
        use futures::FutureExt;
        if let Err(p) = AssertUnwindSafe(future).catch_unwind().await {
            log_caught_panic(name, p);
        }
    });
}
