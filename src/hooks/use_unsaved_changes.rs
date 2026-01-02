//! Unsaved Changes Detection Hook
//!
//! Provides dirty state tracking and browser navigation warnings
//! for forms with important content (articles, drafts, etc.)

use dioxus::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
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
    pub fn mark_saved(&mut self, current_hash: u64) {
        self.last_saved_hash.set(Some(current_hash));
        self.is_dirty.set(false);
    }

    /// Reset to clean state (no saved hash)
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
    let last_saved_hash = use_signal(|| None::<u64>);

    // Track dirty state when content changes
    use_effect(move || {
        let current = *content_hash.read();

        match *last_saved_hash.read() {
            Some(saved) => {
                // Has been saved before - compare with saved state
                is_dirty.set(current != saved);
            }
            None => {
                // Never saved - dirty if there's any content
                // (hash of empty content would be consistent, so > 0 check isn't reliable)
                // Instead, we check if hash differs from initial empty state
                let empty_hash = calculate_empty_hash();
                is_dirty.set(current != empty_hash);
            }
        }
    });

    // Register beforeunload handler (WASM only)
    #[cfg(target_arch = "wasm32")]
    {
        use_effect(move || {
            register_beforeunload(is_dirty);

            // Cleanup is handled by the closure being replaced
        });
    }

    UseUnsavedChanges {
        is_dirty,
        last_saved_hash,
    }
}

/// Calculate hash for empty state comparison
fn calculate_empty_hash() -> u64 {
    let mut hasher = DefaultHasher::new();
    "".hash(&mut hasher);
    hasher.finish()
}

/// Calculate hash for arbitrary content
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

// ============================================================================
// WASM beforeunload handling
// ============================================================================

#[cfg(target_arch = "wasm32")]
thread_local! {
    static BEFOREUNLOAD_CLOSURE: std::cell::RefCell<Option<Closure<dyn FnMut(web_sys::BeforeUnloadEvent)>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn register_beforeunload(is_dirty: Signal<bool>) {
    use web_sys::BeforeUnloadEvent;

    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };

    // Create closure that checks dirty state
    let closure = Closure::wrap(Box::new(move |event: BeforeUnloadEvent| {
        if *is_dirty.read() {
            // Standard way to trigger browser's "Leave site?" dialog
            event.set_return_value("You have unsaved changes. Are you sure you want to leave?");
            event.prevent_default();
        }
    }) as Box<dyn FnMut(BeforeUnloadEvent)>);

    // Remove any existing listener first
    BEFOREUNLOAD_CLOSURE.with(|cell| {
        if let Some(old_closure) = cell.borrow_mut().take() {
            let _ = window.remove_event_listener_with_callback(
                "beforeunload",
                old_closure.as_ref().unchecked_ref(),
            );
        }
    });

    // Add new listener
    if let Err(e) = window.add_event_listener_with_callback(
        "beforeunload",
        closure.as_ref().unchecked_ref(),
    ) {
        log::warn!("Failed to add beforeunload listener: {:?}", e);
    }

    // Store closure to prevent it from being dropped
    BEFOREUNLOAD_CLOSURE.with(|cell| {
        *cell.borrow_mut() = Some(closure);
    });
}

/// Unregister the beforeunload handler (call on component unmount if needed)
#[cfg(target_arch = "wasm32")]
pub fn unregister_beforeunload() {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };

    BEFOREUNLOAD_CLOSURE.with(|cell| {
        if let Some(closure) = cell.borrow_mut().take() {
            let _ = window.remove_event_listener_with_callback(
                "beforeunload",
                closure.as_ref().unchecked_ref(),
            );
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn unregister_beforeunload() {
    // No-op on non-WASM targets
}

// ============================================================================
// Leave Confirmation Dialog Support
// ============================================================================

/// State for showing a leave confirmation dialog
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

    #[test]
    fn test_empty_hash_consistent() {
        let empty1 = calculate_empty_hash();
        let empty2 = calculate_empty_hash();
        assert_eq!(empty1, empty2);
    }
}
