//! Unsaved Changes Detection Hook
//!
//! Provides dirty state tracking and browser navigation warnings
//! for forms with important content (articles, drafts, etc.)
use dioxus::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
#[cfg(feature = "web")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "web")]
use wasm_bindgen::JsCast;
/// Result of the unsaved changes hook
///
/// This struct is Copy since Signal<T> is Copy, allowing it to be
/// used in multiple closures without ownership issues.
#[derive(Clone, Copy)]
pub struct UseUnsavedChanges {
    /// Whether content has changed since last save
    pub is_dirty: Signal<bool>,
    /// Hash of the last saved state (None if never saved)
    pub last_saved_hash: Signal<Option<u64>>,
}
impl UseUnsavedChanges {
    /// Mark current state as saved (updates hash, clears dirty flag)
    /// Kept for future manual save control integration
    #[allow(dead_code)]
    pub fn mark_saved(&mut self, current_hash: u64) {
        self.last_saved_hash.set(Some(current_hash));
        self.is_dirty.set(false);
    }
    /// Reset to clean state (no saved hash)
    /// Kept for future "New Article" button that resets dirty state
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.last_saved_hash.set(None);
        self.is_dirty.set(false);
    }
}
/// Hook for tracking unsaved changes in forms
///
/// Tracks whether content has changed and registers a `beforeunload`
/// handler to warn users before leaving with unsaved changes.
///
/// # Arguments
/// * `content_hash` - A memoized signal containing the hash of current content
///
/// # Returns
/// `UseUnsavedChanges` struct with signals for dirty state and last saved hash
///
/// # Example
/// ```rust,ignore
/// let title = use_signal(|| String::new());
/// let content = use_signal(|| String::new());
///
/// let content_hash = use_memo(move || {
///     calculate_hash(&title.read(), &content.read())
/// });
///
/// let unsaved = use_unsaved_changes(content_hash);
///
/// // Check if dirty
/// if *unsaved.is_dirty.read() {
///     // Show save reminder
/// }
///
/// // After successful save
/// unsaved.mark_saved(*content_hash.read());
/// ```
pub fn use_unsaved_changes(content_hash: Memo<u64>) -> UseUnsavedChanges {
    let mut is_dirty = use_signal(|| false);
    let mut last_saved_hash = use_signal(|| None::<u64>);
    use_effect(move || {
        let current = *content_hash.read();
        let saved_hash = *last_saved_hash.read();
        match saved_hash {
            Some(saved) => {
                is_dirty.set(current != saved);
            }
            None => {
                last_saved_hash.set(Some(current));
                is_dirty.set(false);
            }
        }
    });
    #[cfg(feature = "web")]
    {
        let mut closure_id = use_signal(|| 0u32);
        use_effect(move || {
            closure_id.set(register_beforeunload(is_dirty));
        });
        use_drop(move || {
            if *closure_id.read() != 0 {
                unregister_beforeunload_id(*closure_id.read());
            }
        });
    }
    UseUnsavedChanges {
        is_dirty,
        last_saved_hash,
    }
}
/// Calculate hash for arbitrary content
/// Kept for future single-field hash calculations
#[allow(dead_code)]
pub fn calculate_hash<T: Hash>(content: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}
/// Calculate hash for multiple string fields
pub fn calculate_multi_hash(fields: &[&str]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for field in fields {
        field.hash(&mut hasher);
    }
    hasher.finish()
}
#[cfg(feature = "web")]
thread_local! {
    #[allow(clippy::type_complexity)]
    static BEFOREUNLOAD_CLOSURES: std::cell::RefCell<
        Vec<(u32, Closure<dyn FnMut(web_sys::BeforeUnloadEvent)>)>,
    > = const { std::cell::RefCell::new(Vec::new()) };
    static NEXT_CLOSURE_ID: std::cell::RefCell<u32> = const { std::cell::RefCell::new(1) };
}
#[cfg(feature = "web")]
fn register_beforeunload(is_dirty: Signal<bool>) -> u32 {
    use web_sys::BeforeUnloadEvent;
    let window = match web_sys::window() {
        Some(w) => w,
        None => return 0,
    };
    let closure = Closure::wrap(Box::new(move |event: BeforeUnloadEvent| {
        if *is_dirty.read() {
            event.set_return_value("You have unsaved changes. Are you sure you want to leave?");
            event.prevent_default();
        }
    }) as Box<dyn FnMut(BeforeUnloadEvent)>);
    let closure_id = NEXT_CLOSURE_ID.with(|cell| {
        let id = *cell.borrow();
        *cell.borrow_mut() += 1;
        id
    });
    if let Err(e) =
        window.add_event_listener_with_callback("beforeunload", closure.as_ref().unchecked_ref())
    {
        log::warn!("Failed to add beforeunload listener: {:?}", e);
        return 0;
    }
    BEFOREUNLOAD_CLOSURES.with(|cell| {
        cell.borrow_mut().push((closure_id, closure));
    });
    closure_id
}
/// Unregister a specific beforeunload handler by its ID
#[allow(dead_code)]
#[cfg(feature = "web")]
pub fn unregister_beforeunload_id(closure_id: u32) {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    BEFOREUNLOAD_CLOSURES.with(|cell| {
        let mut vec = cell.borrow_mut();
        if let Some(pos) = vec.iter().position(|(id, _)| *id == closure_id) {
            let (_, closure) = vec.remove(pos);
            let _ = window.remove_event_listener_with_callback(
                "beforeunload",
                closure.as_ref().unchecked_ref(),
            );
        }
    });
}
/// Unregister the beforeunload handler (call on component unmount if needed)
/// Kept for backward compatibility - unregisters all handlers
#[allow(dead_code)]
#[cfg(feature = "web")]
pub fn unregister_beforeunload() {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    BEFOREUNLOAD_CLOSURES.with(|cell| {
        let mut vec = cell.borrow_mut();
        for (_id, closure) in vec.drain(..) {
            let _ = window.remove_event_listener_with_callback(
                "beforeunload",
                closure.as_ref().unchecked_ref(),
            );
        }
    });
}
#[allow(dead_code)]
#[cfg(not(feature = "web"))]
pub fn unregister_beforeunload() {}
/// State for showing a leave confirmation dialog
/// Kept for future in-app navigation warning implementation
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct LeaveConfirmation {
    /// Whether to show the confirmation dialog
    pub show: bool,
    /// The intended navigation destination (for programmatic navigation)
    pub destination: Option<String>,
}
/// Hook for managing leave confirmation dialogs
///
/// Use this alongside `use_unsaved_changes` for in-app navigation warnings.
/// The browser's beforeunload handles browser close/refresh, while this
/// handles in-app route changes.
/// Kept for future in-app navigation warning implementation
#[allow(dead_code)]
pub fn use_leave_confirmation() -> Signal<LeaveConfirmation> {
    use_signal(LeaveConfirmation::default)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_calculate_hash() {
        let hash1 = calculate_hash(&"test");
        let hash2 = calculate_hash(&"test");
        let hash3 = calculate_hash(&"different");
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
    #[test]
    fn test_calculate_multi_hash() {
        let hash1 = calculate_multi_hash(&["a", "b", "c"]);
        let hash2 = calculate_multi_hash(&["a", "b", "c"]);
        let hash3 = calculate_multi_hash(&["a", "b", "d"]);
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}
