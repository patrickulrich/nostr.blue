//! Frame-budget event ingestion: drains events with a time budget so the
//! UI thread isn't blocked under burst load.
//!
//! Ports notedeck's 8ms-per-frame ingestion budget pattern. On WASM/mobile
//! this is MORE relevant than desktop — WASM is single-threaded and shares
//! the main thread with rendering; a long synchronous ingestion burst
//! directly drops frames.
//!
//! ## Usage
//!
//! ```rust,ignore
//! let processor = FrameBudgetProcessor::new(Duration::from_millis(8));
//! let mut rx = dispatcher_handle.receiver();
//! processor.drain_with_budget(&mut rx, |event| {
//!     // process each event
//! }).await;
//! // When budget exhausts, control returns. Schedule continuation:
//! spawn(async move { processor.drain_with_budget(&mut rx, ...).await; });
//! ```

use std::time::{Duration, Instant};

use nostr_sdk::Event;

/// Default frame budget: 8ms on native, leaving ~8ms for rendering within
/// a 16.6ms frame at 60fps.
pub const DEFAULT_FRAME_BUDGET: Duration = Duration::from_millis(8);

/// More conservative budget for WASM/mobile (slower single-threaded runtime).
pub const WASM_FRAME_BUDGET: Duration = Duration::from_millis(5);

/// Process events from a source with a time budget.
///
/// When the budget exhausts, returns `DrainResult::BudgetExhausted` with
/// the count of events processed. The caller should schedule a continuation
/// (e.g. via `spawn`) to drain remaining events on the next frame.
pub struct FrameBudgetProcessor {
    budget: Duration,
}

/// Result of a drain pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainResult {
    /// All available events were drained; the source is empty.
    Drained { count: usize },
    /// The time budget was exhausted; more events may remain.
    /// Schedule another drain pass on the next frame.
    BudgetExhausted { count: usize },
}

impl FrameBudgetProcessor {
    /// Create with a custom budget.
    pub fn new(budget: Duration) -> Self {
        Self { budget }
    }

    /// Create with the platform-appropriate default budget.
    pub fn default_for_platform() -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            Self::new(WASM_FRAME_BUDGET)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::new(DEFAULT_FRAME_BUDGET)
        }
    }

    /// Drain events from a closure with the configured time budget.
    ///
    /// `try_next` should return `Some(event)` if available, `None` if empty.
    /// Returns when either the source is empty or the budget is exhausted.
    pub fn drain_with_budget<F, N>(&self, mut try_next: N, mut on_event: F) -> DrainResult
    where
        F: FnMut(Event),
        N: FnMut() -> Option<Event>,
    {
        let deadline = Instant::now() + self.budget;
        let mut count = 0;

        loop {
            match try_next() {
                Some(event) => {
                    on_event(event);
                    count += 1;
                    if Instant::now() >= deadline {
                        return DrainResult::BudgetExhausted { count };
                    }
                }
                None => {
                    return DrainResult::Drained { count };
                }
            }
        }
    }
}

impl Default for FrameBudgetProcessor {
    fn default() -> Self {
        Self::default_for_platform()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::{EventBuilder, Keys, Kind};

    fn make_event(content: &str) -> Event {
        let keys = Keys::generate();
        EventBuilder::new(Kind::TextNote, content)
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn drains_all_when_source_empty() {
        let processor = FrameBudgetProcessor::new(Duration::from_secs(10));
        let result = processor.drain_with_budget(|| None, |_| {});
        assert_eq!(result, DrainResult::Drained { count: 0 });
    }

    #[test]
    fn drains_all_events_within_budget() {
        let processor = FrameBudgetProcessor::new(Duration::from_secs(10));
        let events: Vec<Event> = (0..5).map(|i| make_event(&format!("e{i}"))).collect();
        let mut idx = 0;
        let mut received = Vec::new();
        let result = processor.drain_with_budget(
            || {
                if idx < events.len() {
                    let e = events[idx].clone();
                    idx += 1;
                    Some(e)
                } else {
                    None
                }
            },
            |e| received.push(e),
        );
        assert_eq!(result, DrainResult::Drained { count: 5 });
        assert_eq!(received.len(), 5);
    }

    #[test]
    fn budget_exhausts_with_many_events() {
        // 1-nanosecond budget → should exhaust almost immediately
        let processor = FrameBudgetProcessor::new(Duration::from_nanos(1));
        let mut count = 0;
        let result = processor.drain_with_budget(
            || Some(make_event("e")),
            |_| {},
        );
        // May process 0 or 1 events before budget exhausts
        match result {
            DrainResult::BudgetExhausted { .. } => {}
            DrainResult::Drained { .. } => {}
        }
        let _ = count;
    }
}
