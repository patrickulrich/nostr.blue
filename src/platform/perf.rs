//! Main-thread stall attribution breadcrumbs.
//!
//! The stall detector in `main.rs` reports that the wasm event loop slipped,
//! but not *what* blocked it. This module keeps a tiny global "current
//! operation" label set by our hot paths around their synchronous chunks
//! (and across awaits — on wasm everything runs on the main thread). When
//! the detector fires it prints the current op and the most recent
//! completed op, so a stall lands next to its likely owner.
//!
//! Deliberately a global mutex rather than a thread-local: on wasm all tasks
//! run on the single main thread (a thread-local would work), but desktop
//! builds run dioxus's runtime on threads we don't control — a global is
//! correct everywhere at negligible cost (one short critical section per
//! span begin/end).
//!
//! Phase 1 of the main-thread-stall investigation — see the DRY plan's
//! perf workstream. Ops are `&'static str` so there are no allocations.

// `instant`, NOT `std::time`: wasm32-unknown-unknown has no std clock
// (`Instant::now` panics there); the `instant` crate polyfills to
// `performance.now()` on web — the same reason every other hot path in
// this codebase uses it.
use instant::Instant;
use std::sync::Mutex;
use std::time::Duration;

struct PerfState {
    /// Currently-running span, if any.
    current: Option<(&'static str, Instant)>,
    /// Most recently *completed* span and how long it ran.
    previous: Option<(&'static str, Duration)>,
}

static STATE: Mutex<PerfState> = Mutex::new(PerfState {
    current: None,
    previous: None,
});

/// Mark the start of a hot operation. Drop the returned guard to complete
/// the span (records it as `previous`). Guard has no cost on drop beyond
/// the mutex update; it does NOT cancel anything.
///
/// Spans may be held across `.await`s — the label simply stays current for
/// the whole async section, which is what we want: the await interleaves on
/// the main thread, so anything that stalls while this span is current gets
/// attributed to it.
pub fn begin(op: &'static str) -> PerfGuard {
    {
        let mut state = STATE.lock().unwrap_or_else(|p| p.into_inner());
        state.current = Some((op, Instant::now()));
    }
    PerfGuard { op }
}

/// RAII completion for [`begin`].
#[must_use]
pub struct PerfGuard {
    op: &'static str,
}

impl Drop for PerfGuard {
    fn drop(&mut self) {
        let mut state = STATE.lock().unwrap_or_else(|p| p.into_inner());
        let elapsed = state
            .current
            .filter(|(op, _)| *op == self.op)
            .map(|(_, started)| started.elapsed())
            .unwrap_or_default();
        state.previous = Some((self.op, elapsed));
        if state.current.map(|(op, _)| op) == Some(self.op) {
            state.current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_record_current_and_previous() {
        // Sole user of the global in the test binary.
        assert!(snapshot().0.is_none());
        {
            let _a = begin("op_a");
            assert_eq!(snapshot().0.map(|(op, _)| op), Some("op_a"));
        }
        let (current, previous) = snapshot();
        assert!(current.is_none());
        assert_eq!(previous.map(|(op, _)| op), Some("op_a"));
        {
            let _b = begin("op_b");
        }
        assert_eq!(snapshot().1.map(|(op, _)| op), Some("op_b"));
    }
}

/// One breadcrumb entry: the op label and a duration (age for the current
/// span, runtime for a completed one).
pub type OpSpan = (&'static str, Duration);

/// What was running when the caller stalled: the current span with its age,
/// plus the last completed span with its duration.
pub fn snapshot() -> (Option<OpSpan>, Option<OpSpan>) {
    let state = STATE.lock().unwrap_or_else(|p| p.into_inner());
    let current = state
        .current
        .map(|(op, started)| (op, started.elapsed()));
    (current, state.previous)
}
