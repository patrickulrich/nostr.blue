use dioxus::prelude::*;

/// Global signal to track unread notification count
pub static UNREAD_COUNT: GlobalSignal<usize> = Signal::global(|| 0);

/// Set the unread notification count
#[allow(dead_code)]
pub fn set_unread_count(count: usize) {
    *UNREAD_COUNT.write() = count;
}

/// Get the current unread count
pub fn get_unread_count() -> usize {
    *UNREAD_COUNT.read()
}

/// Clear the unread notification count (when user views notifications)
pub fn clear_unread_count() {
    *UNREAD_COUNT.write() = 0;
}

/// Increment unread count
#[allow(dead_code)]
pub fn increment_unread_count() {
    *UNREAD_COUNT.write() += 1;
}
