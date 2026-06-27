//! A reusable coalescer for bursty item streams.
//!
//! Dioxus batches signal writes that happen within a single async step, but it
//! does NOT batch writes that cross `.await` boundaries. A `while let Some(ev)
//! = stream.next().await { signal.set(...) }` loop therefore produces one render
//! per event. [`DebouncedCollector`] fixes this by buffering items and flushing
//! them at most once per debounce window.

use crate::platform::timer::sleep_ms;
use dioxus::prelude::spawn;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Coalesces a bursty stream of items into periodic batched flushes.
///
/// The first [`extend`](Self::extend) after an idle period schedules a flush
/// task that sleeps for `debounce_ms`, then drains the internal buffer and
/// invokes `on_flush` with the whole batch. Subsequent pushes within the
/// window only extend the buffer. At most one flush task is ever in flight
/// (guarded by an internal flag), so a burst of N items produces roughly one
/// `on_flush` per window instead of N individual signal writes.
///
/// `on_flush` runs from a spawned task and typically calls `Signal::set` (which
/// takes `&mut self`), so it must be `FnMut + 'static`. Because `Signal` is
/// `Copy`, capturing a target signal in the closure is cheap.
///
/// After the source stream completes, call [`drain`](Self::drain) to flush any
/// items buffered after the last window fired — this is the correctness anchor
/// that prevents item loss.
pub struct DebouncedCollector<T: 'static> {
    buffer: Rc<RefCell<Vec<T>>>,
    pending: Rc<Cell<bool>>,
    debounce_ms: u32,
}

impl<T: 'static> DebouncedCollector<T> {
    /// Create a new collector with the given debounce window in milliseconds.
    pub fn new(debounce_ms: u32) -> Self {
        Self {
            buffer: Rc::new(RefCell::new(Vec::new())),
            pending: Rc::new(Cell::new(false)),
            debounce_ms,
        }
    }

    /// Queue items for a debounced flush.
    ///
    /// The first call since idle schedules a flush task; later calls within the
    /// window only append to the buffer. `on_flush` is invoked (at most once per
    /// window) with the drained batch.
    pub fn extend<F>(&self, items: impl IntoIterator<Item = T>, mut on_flush: F)
    where
        F: FnMut(Vec<T>) + 'static,
    {
        self.buffer.borrow_mut().extend(items);
        if !self.pending.get() {
            self.pending.set(true);
            let buffer = self.buffer.clone();
            let pending = self.pending.clone();
            let ms = self.debounce_ms;
            spawn(async move {
                sleep_ms(ms).await;
                let batch: Vec<T> = buffer.borrow_mut().drain(..).collect();
                if !batch.is_empty() {
                    on_flush(batch);
                }
                pending.set(false);
            });
        }
    }

    /// Synchronously drain any buffered items not yet flushed.
    ///
    /// Call this after the source stream completes so tail items buffered after
    /// the last window are not lost.
    pub fn drain(&self) -> Vec<T> {
        self.buffer.borrow_mut().drain(..).collect()
    }
}

impl<T: 'static> Clone for DebouncedCollector<T> {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer.clone(),
            pending: self.pending.clone(),
            debounce_ms: self.debounce_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_returns_buffered_items() {
        let collector = DebouncedCollector::<i32>::new(50);
        // Bypass extend (which spawns) and push directly into the buffer.
        collector.buffer.borrow_mut().extend([1, 2, 3]);
        let drained = collector.drain();
        assert_eq!(drained, vec![1, 2, 3]);
        // Second drain is empty.
        assert!(collector.drain().is_empty());
    }

    #[test]
    fn pending_flag_toggles() {
        let collector = DebouncedCollector::<i32>::new(50);
        assert!(!collector.pending.get());
        // Manually set as extend would.
        collector.pending.set(true);
        assert!(collector.pending.get());
        collector.pending.set(false);
        assert!(!collector.pending.get());
    }
}
