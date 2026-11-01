//! Drains the publish-queue notice queue and renders toasts via the
//! standard toast system.
//!
//! The publish queue processor (`stores/publish_queue/processor.rs`) runs in
//! a spawned task outside any component scope, so it cannot call
//! `consume_toast()` directly. Instead it pushes `PublishNotice`s into
//! `stores::publish_queue::PUBLISH_NOTICES`. This component, mounted at the
//! app root next to `ToastProvider`, drains the queue on every render and
//! forwards each entry to the standard toast API.
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
    // Subscribe to the queue signal so we re-render on push.
    let queue_len = crate::stores::publish_queue::PUBLISH_NOTICES.read().len();

    // Drain inside use_effect so we run after every render where the queue
    // is non-empty. The drain call clones + clears atomically.
    use_effect(move || {
        if queue_len == 0 {
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
                    toast.warning(
                        "Local cache corrupted — rebuilding".to_string(),
                        ToastOptions::new()
                            .description(format!(
                                "{error}. Reloading to rebuild the local caches…"
                            ))
                            .duration(Duration::from_secs(3)),
                    );
                }
            }
        }
    });

    rsx! {}
}
