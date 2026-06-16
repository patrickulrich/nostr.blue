//! Mostro deep link state.
//!
//! Phase 11: holds a pending deep-link order_id for the root component
//! to navigate to. Service modules can't call `navigator()` directly
//! (no Dioxus scope), so they set this signal and the root component's
//! `use_effect` picks it up on the next render.

use dioxus::prelude::*;

/// Pending deep-link order_id to navigate to. Set by
/// `services::mostro_deeplink::handle_mostro_deep_link`, consumed by
/// the root component's `use_effect`.
pub static PENDING_DEEP_LINK: GlobalSignal<Option<String>> = Signal::global(|| None);

/// Set a pending deep link target. The root component will navigate
/// to `P2POrderDetail { order_id }` on the next render.
pub fn set_pending_deep_link(order_id: String) {
    *PENDING_DEEP_LINK.write() = Some(order_id);
}

/// Consume (take) the pending deep link, returning it for navigation.
pub fn take_pending_deep_link() -> Option<String> {
    let pending = PENDING_DEEP_LINK.read().clone();
    if pending.is_some() {
        *PENDING_DEEP_LINK.write() = None;
    }
    pending
}
