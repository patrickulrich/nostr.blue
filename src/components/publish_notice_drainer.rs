//! Drains the publish-queue notice queue and renders toasts via the
//! standard toast system.
//!
//! The publish queue processor (`stores/publish_queue/processor.rs`) runs in
//! a spawned task outside any component scope, so it cannot call
//! `consume_toast()` directly. Instead it pushes `PublishNotice`s into
//! `stores::publish_queue::PUBLISH_NOTICES`. This component, mounted at the
//! app root next to `ToastProvider`, drains the queue whenever the signal
//! changes and forwards each entry to the standard toast API.
//!
//! Mirrors `mostro_toast_drainer.rs`.

use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::time::Duration;

use crate::stores::publish_queue::{drain_notices, PublishNotice};

/// Drain the publish-queue notices and surface each via the standard toast
/// system. Renders nothing — this is a side-effect component.
#[component]
pub fn PublishNoticeDrainer() -> Element {
    // Drain inside use_effect. The queue signal MUST be read inside the
    // effect callback: Dioxus 0.7 effects re-run only when a signal read
    // *inside* the closure changes — a captured `usize` length (R4 review)
    // registers no dependency, so the mount-time empty-queue early return
    // left the effect dead and no notice ever rendered. The drain call
    // clones + clears atomically.
    use_effect(move || {
        if crate::stores::publish_queue::PUBLISH_NOTICES.read().is_empty() {
            return;
        }
        let drained = drain_notices();
        if drained.is_empty() {
            return;
        }
        let toast = consume_toast();
        for notice in drained {
            match notice {
                PublishNotice::Failed { label, error } => {
                    toast.error(
                        format!("{label} failed to publish"),
                        ToastOptions::new()
                            .description(format!(
                                "{error} — it stays in the publish queue for a manual retry"
                            ))
                            .duration(Duration::from_secs(8)),
                    );
                }
                PublishNotice::CacheRecovery { error } => {
                    // The reload only happens on wasm (`processor.rs` sets
                    // the flag + reloads there); native builds surface the
                    // notice and rebuild via the next launch's boot probe
                    // (R4 review) — don't promise a reload that won't come.
                    let action = if cfg!(target_arch = "wasm32") {
                        "Reloading to rebuild the local caches…"
                    } else {
                        "The local caches will be rebuilt on the next app restart."
                    };
                    toast.warning(
                        "Local cache corrupted — rebuilding".to_string(),
                        ToastOptions::new()
                            .description(format!("{error}. {action}"))
                            .duration(Duration::from_secs(3)),
                    );
                }
                #[cfg(target_arch = "wasm32")]
                PublishNotice::CacheRecoveryDegraded { error } => {
                    // R4 review circuit breaker: reload recovery exhausted —
                    // surface it persistently instead of cycling the app.
                    toast.warning(
                        "Local cache recovery paused".to_string(),
                        ToastOptions::new()
                            .description(format!(
                                "{error}. Automatic rebuilds kept failing — publishing \
                                 continues, and the local caches will be rebuilt on the \
                                 next app restart."
                            ))
                            .duration(Duration::from_secs(30)),
                    );
                }
            }
        }
    });

    rsx! {}
}
