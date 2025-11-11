/// Timed serializer with debouncing for LocalStorage operations
///
/// Reduces frequent write operations by batching updates within a time window.
/// Inspired by Notedeck's approach to minimize storage I/O.
///
/// # Benefits
/// - Reduces LocalStorage writes from dozens per second to one per interval
/// - Prevents UI jank from synchronous storage operations
/// - Saves battery on mobile devices
/// - Reduces wear on storage (especially important for embedded devices)
///
/// # Example
/// ```
/// // Instead of saving on every bookmark toggle:
/// bookmarks.save_immediately(); // Called 10x per second = 10 writes
///
/// // Use debounced saving:
/// bookmarks.request_save(); // Called 10x per second = 1 write after 1s delay
/// ```

use gloo_timers::callback::Timeout;
use std::cell::RefCell;
use std::rc::Rc;

/// A debouncer that delays execution until a quiet period
#[allow(dead_code)]
pub struct Debouncer {
    timeout: Rc<RefCell<Option<Timeout>>>,
    delay_ms: u32,
}

#[allow(dead_code)]
impl Debouncer {
    /// Create a new debouncer with specified delay in milliseconds
    pub fn new(delay_ms: u32) -> Self {
        Self {
            timeout: Rc::new(RefCell::new(None)),
            delay_ms,
        }
    }

    /// Schedule a callback to run after the delay period
    /// If called again before the delay expires, the previous call is cancelled
    pub fn debounce<F>(&self, callback: F)
    where
        F: FnOnce() + 'static,
    {
        // Cancel any existing timeout
        *self.timeout.borrow_mut() = None;

        // Schedule new timeout
        let timeout = Timeout::new(self.delay_ms, callback);
        *self.timeout.borrow_mut() = Some(timeout);
    }

    /// Cancel any pending debounced call
    pub fn cancel(&self) {
        *self.timeout.borrow_mut() = None;
    }

    /// Flush any pending debounced call immediately
    pub fn flush(&self) {
        // Dropping the timeout without letting it fire effectively cancels it
        // The caller should then call the action directly
        *self.timeout.borrow_mut() = None;
    }
}

impl Clone for Debouncer {
    fn clone(&self) -> Self {
        Self {
            timeout: Rc::clone(&self.timeout),
            delay_ms: self.delay_ms,
        }
    }
}

/// Timed serializer that debounces save operations
///
/// Generic over the data type T which must be serializable
#[allow(dead_code)]
pub struct TimedSerializer<T: Clone + 'static> {
    debouncer: Debouncer,
    pending_data: Rc<RefCell<Option<T>>>,
}

#[allow(dead_code)]
impl<T: Clone + 'static> TimedSerializer<T> {
    /// Create a new timed serializer with default 1 second delay
    pub fn new() -> Self {
        Self::with_delay(1000)
    }

    /// Create a new timed serializer with custom delay in milliseconds
    pub fn with_delay(delay_ms: u32) -> Self {
        Self {
            debouncer: Debouncer::new(delay_ms),
            pending_data: Rc::new(RefCell::new(None)),
        }
    }

    /// Request to save data (will be debounced)
    ///
    /// Multiple calls within the delay window will be batched into one save
    pub fn save<F>(&self, data: T, save_fn: F)
    where
        F: FnOnce(T) + 'static,
    {
        // Store the latest data
        *self.pending_data.borrow_mut() = Some(data.clone());

        // Schedule debounced save
        let pending_data = Rc::clone(&self.pending_data);
        self.debouncer.debounce(move || {
            if let Some(data) = pending_data.borrow_mut().take() {
                save_fn(data);
            }
        });
    }

    /// Immediately flush any pending save
    pub fn flush<F>(&self, save_fn: F)
    where
        F: FnOnce(T),
    {
        self.debouncer.flush();
        if let Some(data) = self.pending_data.borrow_mut().take() {
            save_fn(data);
        }
    }

    /// Cancel any pending save
    pub fn cancel(&self) {
        self.debouncer.cancel();
        *self.pending_data.borrow_mut() = None;
    }
}

impl<T: Clone + 'static> Default for TimedSerializer<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + 'static> Clone for TimedSerializer<T> {
    fn clone(&self) -> Self {
        Self {
            debouncer: self.debouncer.clone(),
            pending_data: Rc::clone(&self.pending_data),
        }
    }
}

/// Helper function to create a debounced callback
///
/// # Example
/// ```
/// let debounced_save = create_debounced(1000, || {
///     save_to_storage();
/// });
///
/// // Call multiple times rapidly
/// debounced_save(); // Scheduled
/// debounced_save(); // Previous cancelled, new scheduled
/// debounced_save(); // Previous cancelled, new scheduled
/// // Only one save happens after 1 second
/// ```
#[allow(dead_code)]
pub fn create_debounced<F>(delay_ms: u32, callback: F) -> impl Fn()
where
    F: Fn() + 'static,
{
    let debouncer = Debouncer::new(delay_ms);
    let callback = Rc::new(callback);

    move || {
        let callback = Rc::clone(&callback);
        debouncer.debounce(move || callback());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_debouncer_creation() {
        let debouncer = Debouncer::new(1000);
        assert_eq!(debouncer.delay_ms, 1000);
    }

    #[test]
    fn test_timed_serializer_creation() {
        let serializer = TimedSerializer::<String>::new();
        assert!(serializer.pending_data.borrow().is_none());
    }

    #[test]
    fn test_timed_serializer_with_custom_delay() {
        let serializer = TimedSerializer::<String>::with_delay(500);
        assert_eq!(serializer.debouncer.delay_ms, 500);
    }

    #[test]
    fn test_pending_data_storage() {
        let serializer = TimedSerializer::<String>::new();
        let called = Arc::new(Mutex::new(false));
        let called_clone = Arc::clone(&called);

        serializer.save("test".to_string(), move |_data| {
            *called_clone.lock().unwrap() = true;
        });

        // Data should be stored
        assert!(serializer.pending_data.borrow().is_some());
    }

    #[test]
    fn test_cancel() {
        let serializer = TimedSerializer::<String>::new();
        serializer.save("test".to_string(), |_| {});

        serializer.cancel();

        // Data should be cleared
        assert!(serializer.pending_data.borrow().is_none());
    }
}
