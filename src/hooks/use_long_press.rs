//! Long-press gesture detector for touch-capable targets (mobile WebView,
//! touch web).
//!
//! ## Why this exists
//!
//! nostr.blue previously overloaded `oncontextmenu` to detect long-press on
//! mobile, because the W3C spec fires `contextmenu` for both right-click
//! (desktop) and long-press (mobile WebView). The problem: calling
//! `prevent_default()` on the mobile `contextmenu` event also suppresses
//! Android's native text-selection / paste ActionMode popup. That made
//! copy/paste unavailable anywhere inside elements with an `oncontextmenu`
//! handler (nest cards, reaction buttons, stage tiles).
//!
//! This hook sidesteps the conflict by detecting long-press via `touchstart`
//! followed by a timer. `contextmenu` is left untouched on mobile (so native
//! selection works) and is only used for desktop right-click (gated at the
//! call site with `cfg!(feature = "mobile_platform")`).
//!
//! ## Cancellation semantics
//!
//! The timer is canceled (via generation bump) on `touchmove` (scroll/drag),
//! `touchend` (lift), or `touchcancel` (system interruption). Mirrors the
//! `picker_session_id` generation pattern at
//! `components/reaction/button.rs:73`.
//!
//! ## Usage
//!
//! ```ignore
//! let (on_touch_start, on_touch_move, on_touch_end, on_touch_cancel) =
//!     use_long_press(Callback::new(move |_| show_actions.set(true)), 500);
//!
//! rsx! {
//!     div {
//!         oncontextmenu: move |e: MouseEvent| {
//!             if cfg!(feature = "mobile_platform") { return; }
//!             e.prevent_default();
//!             show_actions.set(true);
//!         },
//!         ontouchstart:  on_touch_start,
//!         ontouchmove:   on_touch_move,
//!         ontouchend:    on_touch_end,
//!         ontouchcancel: on_touch_cancel,
//!     }
//! }
//! ```

use crate::platform::timer::sleep_ms;
use dioxus::prelude::*;

/// Default long-press duration. Matches Android's `ViewConfiguration`
/// long-press timeout (~500ms).
pub const DEFAULT_LONG_PRESS_MS: u32 = 500;

/// Detect a long-press gesture via a `touchstart`-initiated timer.
///
/// Returns the four touch event handlers as a tuple. All four must be wired
/// onto the same element for correct cancellation semantics.
///
/// `on_long_press` fires once after `duration_ms` of continuous touch (no
/// movement, no lift). The timer is canceled by `touchmove`, `touchend`, or
/// `touchcancel`.
///
/// This hook is a no-op on platforms without touch input — desktop builds
/// still wire up the handlers but never receive events, so desktop right-click
/// behavior must be preserved separately via `oncontextmenu`.
#[allow(clippy::type_complexity)]
pub fn use_long_press(
    on_long_press: Callback<()>,
    duration_ms: u32,
) -> (
    impl FnMut(TouchEvent),
    impl FnMut(TouchEvent),
    impl FnMut(TouchEvent),
    impl FnMut(TouchEvent),
) {
    // Generation counter: bumped on every cancellation, so a stale timer can
    // detect it was superseded by comparing its captured token to the current
    // value.
    let mut generation = use_signal(|| 0u32);

    // Capture the callback into a Signal so the closures can read it without
    // capturing non-`Copy` state directly. Refresh on every render so a
    // callback that closes over changed state isn't stale (use_signal only
    // runs its initializer once, so without this the first-render callback
    // would fire forever).
    let mut callback = use_signal(move || on_long_press);
    callback.set(on_long_press);

    (
        move |_| {
            let token = generation.peek().wrapping_add(1);
            generation.set(token);
            let cb = *callback.peek();
            spawn(async move {
                sleep_ms(duration_ms).await;
                if *generation.peek() == token {
                    cb.call(());
                }
            });
        },
        move |_| {
            let next = generation.peek().wrapping_add(1);
            generation.set(next);
        },
        move |_| {
            let next = generation.peek().wrapping_add(1);
            generation.set(next);
        },
        move |_| {
            let next = generation.peek().wrapping_add(1);
            generation.set(next);
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: the default duration matches Android's ViewConfiguration
    /// default (~500ms). The actual timer semantics are exercised via
    /// integration on device.
    #[test]
    fn default_duration_matches_android() {
        assert_eq!(DEFAULT_LONG_PRESS_MS, 500);
    }
}
