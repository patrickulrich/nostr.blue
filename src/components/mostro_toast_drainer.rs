//! Phase 2.3 (M4/M8): Drains the Mostro background-toast queue and renders
//! toasts via the standard toast system.
//!
//! The background trade monitor (`mostro::client::start_background_trade_monitor`)
//! runs in a spawned task outside any component scope, so it cannot call
//! `consume_toast()` directly. Instead, it pushes `(title, body)` tuples
//! into `mostro::MOSTRO_BACKGROUND_TOASTS`. This component, mounted at the
//! app root, drains the queue on every render and forwards each entry to
//! the standard toast API.
//!
//! Mounted next to `ToastProvider` in `main.rs`.

use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use std::time::Duration;

use crate::stores::mostro;

/// Drain the background-toast queue and surface each entry via the standard
/// toast system. Renders nothing — this is a side-effect component.
///
/// Also triggers trade backfill when the tab regains visibility (covers the
/// case where a push notification wakes the device — the SW shows the OS
/// notification, the user clicks it, the tab becomes visible, and we
/// backfill any events that arrived while it was backgrounded).
#[component]
pub fn MostroBackgroundToastDrainer() -> Element {
    // Subscribe to the queue signal so we re-render on push.
    let queue_len = mostro::MOSTRO_BACKGROUND_TOASTS.read().len();

    // Drain inside use_effect so we run after every render where the queue
    // is non-empty. The drain call clones + clears atomically.
    use_effect(move || {
        if queue_len == 0 {
            return;
        }
        let drained = mostro::drain_background_toasts();
        if drained.is_empty() {
            return;
        }
        let toast = consume_toast();
        for (title, body) in drained {
            toast.warning(
                title,
                ToastOptions::new()
                    .description(body)
                    .duration(Duration::from_secs(8)),
            );
        }
    });

    // Phase 3: poll for missed events while the tab is in the background.
    // The service worker's push handler shows an OS notification and posts
    // a `mostro-wake` message. Here we use a periodic backfill poll as a
    // simple catch-all: every 60s, run backfill if there are active trades.
    // This ensures events are processed even without the visibility event.
    //
    // B2: also drains the notification_store dirty flag — bursts of pushes
    // coalesce into a single NIP-78 publish per cycle.
    use_future(move || async move {
        loop {
            crate::platform::timer::sleep(Duration::from_secs(60)).await;
            if !mostro::trade_store::active_trades().is_empty() {
                crate::stores::mostro::client::backfill_active_trades().await;
            }
            if crate::stores::mostro::notification_store::is_dirty() {
                if let Err(e) = crate::stores::mostro::notification_store::publish().await {
                    log::debug!("Mostro notifications publish failed (non-fatal): {e}");
                }
            }
        }
    });

    rsx! {}
}
