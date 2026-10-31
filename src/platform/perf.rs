//! Main-thread stall + panic attribution breadcrumbs.
//!
//! The stall detector in `main.rs` reports that the wasm event loop slipped,
//! but not *what* blocked it. This module tracks a set of concurrently-open
//! "operation" spans set by our hot paths around their synchronous chunks
//! (and across awaits — on wasm everything runs on the main thread). When
//! the detector fires, or a panic unwinds, the snapshot names every span
//! that was alive at that instant.
//!
//! **Why a set and not a single slot:** the AlreadyBorrowed panic class is
//! caused by a signal borrow held ACROSS an await while another task writes
//! the same signal. In that scenario at least two spans are concurrently
//! open (the holder's and the writer's), and a single-slot design silently
//! loses the earlier one when the later `begin()` overwrites it. The set
//! keeps every live span so a panic dump names the holder.
//!
//! Deliberately a global mutex rather than a thread-local: on wasm all tasks
//! run on the single main thread (a thread-local would work), but desktop
//! builds run dioxus's runtime on threads we don't control — a global is
//! correct everywhere at negligible cost (one short critical section per
//! span begin/end).
//!
//! Ops are `&'static str` so there are no allocations.

// `instant`, NOT `std::time`: wasm32-unknown-unknown has no std clock
// (`Instant::now` panics there); the `instant` crate polyfills to
// `performance.now()` on web — the same reason every other hot path in
// this codebase uses it.
use instant::Instant;
use std::sync::Mutex;
use std::time::Duration;

/// One breadcrumb entry: the op label and a duration (age for open spans,
/// runtime for a completed one).
pub type OpSpan = (&'static str, Duration);

struct PerfState {
    /// All currently-open spans in begin order. A span held across an
    /// `.await` stays here for its whole async section — exactly the
    /// pattern the panic attribution needs to surface.
    open: Vec<(&'static str, Instant)>,
    /// Most recently *completed* span and how long it ran.
    previous: Option<(&'static str, Duration)>,
}

static STATE: Mutex<PerfState> = Mutex::new(PerfState {
    open: Vec::new(),
    previous: None,
});

/// Mark the start of a hot operation. Drop the returned guard to complete
/// the span (records it as `previous`). Guards may nest and interleave
/// across tasks; each begin/drop pair is matched by (op, start-time), so
/// same-named spans from different tasks do not confuse each other.
pub fn begin(op: &'static str) -> PerfGuard {
    let started = Instant::now();
    {
        let mut state = STATE.lock().unwrap_or_else(|p| p.into_inner());
        state.open.push((op, started));
    }
    PerfGuard { op, started }
}

/// RAII completion for [`begin`].
#[must_use]
pub struct PerfGuard {
    op: &'static str,
    started: Instant,
}

impl Drop for PerfGuard {
    fn drop(&mut self) {
        let mut state = STATE.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(pos) = state
            .open
            .iter()
            .position(|(op, started)| *op == self.op && *started == self.started)
        {
            state.open.remove(pos);
        }
        state.previous = Some((self.op, self.started.elapsed()));
    }
}

/// Everything that was running when the caller stalled or panicked: all
/// open spans with their ages (oldest first), plus the last completed span
/// with its duration.
pub fn snapshot() -> (Vec<OpSpan>, Option<OpSpan>) {
    let state = STATE.lock().unwrap_or_else(|p| p.into_inner());
    let open = state
        .open
        .iter()
        .map(|(op, started)| (*op, started.elapsed()))
        .collect();
    (open, state.previous)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Other tests in this binary legitimately open spans concurrently
    /// (stores call `perf::begin` while under test), so these tests must not
    /// assume sole ownership of the global state: every assertion filters to
    /// this test's uniquely-prefixed span names instead of the whole set.
    #[test]
    fn spans_record_open_set_and_previous() {
        const PREFIX: &str = "spans_test_";
        fn own_open() -> Vec<&'static str> {
            snapshot()
                .0
                .into_iter()
                .filter(|(op, _)| op.starts_with(PREFIX))
                .map(|(op, _)| op)
                .collect()
        }
        assert!(own_open().is_empty());
        {
            let _a = begin("spans_test_a");
            {
                let _b = begin("spans_test_b");
                // Both concurrently-open spans are visible — the property a
                // single-slot design loses when a holder spans an await.
                let names = own_open();
                assert_eq!(names, vec!["spans_test_a", "spans_test_b"]);
            }
            let names = own_open();
            assert_eq!(names, vec!["spans_test_a"]);
            let (_, previous) = snapshot();
            assert_eq!(previous.map(|(op, _)| op), Some("spans_test_b"));
        }
        assert!(own_open().is_empty());
        let (_, previous) = snapshot();
        assert_eq!(previous.map(|(op, _)| op), Some("spans_test_a"));
    }

    #[test]
    fn same_named_nested_spans_do_not_confuse_each_other() {
        const NAME: &str = "same_nested_test";
        {
            let _outer = begin(NAME);
            {
                let _inner = begin(NAME);
            }
            // Inner completed; outer still open.
            let (open, previous) = snapshot();
            let own: Vec<_> = open
                .into_iter()
                .filter(|(op, _)| *op == NAME)
                .map(|(op, _)| op)
                .collect();
            assert_eq!(own.len(), 1);
            assert_eq!(previous.map(|(op, _)| op), Some(NAME));
        }
        let own: Vec<_> = snapshot()
            .0
            .into_iter()
            .filter(|(op, _)| *op == NAME)
            .map(|(op, _)| op)
            .collect();
        assert!(own.is_empty());
    }
}
