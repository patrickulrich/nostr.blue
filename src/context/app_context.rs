/// Application Context
///
/// Provides centralized access to stores and services, reducing prop drilling
/// and making component dependencies explicit.
///
/// Based on Phase 3.2 of performance.md - adopting Notedeck's context pattern
/// to eliminate prop drilling and create cleaner component interfaces.

use dioxus::prelude::Readable;
use nostr_sdk::Client;
use std::sync::Arc;
use crate::stores::{nostr_client, auth_store, bookmarks, profiles, theme_store, signer};

/// AppContext provides read-only access to commonly used stores and services
///
/// Instead of passing individual store references through multiple component layers,
/// components can access what they need through this unified context.
///
/// # Example
/// ```rust
/// let ctx = AppContext::new();
///
/// // Check if user is authenticated
/// if let Some(pubkey) = ctx.get_current_user() {
///     // Access profile
///     if let Some(profile) = ctx.get_profile(&pubkey).await {
///         // ...
///     }
/// }
///
/// // Check bookmark status
/// if ctx.is_bookmarked(event_id) {
///     // ...
/// }
/// ```
#[allow(dead_code)]
#[derive(Clone)]
pub struct AppContext;

#[allow(dead_code)]
impl AppContext {
    /// Create a new AppContext
    ///
    /// AppContext is a lightweight accessor to global stores, so creating
    /// multiple instances is cheap and doesn't duplicate data.
    pub fn new() -> Self {
        Self
    }

    // ============================================================================
    // Authentication & User
    // ============================================================================

    /// Get the current authenticated user's pubkey (if authenticated)
    pub fn get_current_user(&self) -> Option<String> {
        auth_store::get_pubkey()
    }

    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        auth_store::get_pubkey().is_some()
    }

    /// Get signer information (name, logo, etc.)
    pub fn get_signer_info(&self) -> Option<signer::SignerInfo> {
        let info = signer::SIGNER_INFO.read();
        info.clone()
    }

    /// Check if a signer is attached
    pub fn has_signer(&self) -> bool {
        *nostr_client::HAS_SIGNER.read()
    }

    // ============================================================================
    // Nostr Client
    // ============================================================================

    /// Get the Nostr client instance (wrapped in Arc for shared ownership)
    pub fn get_client(&self) -> Option<Arc<Client>> {
        nostr_client::NOSTR_CLIENT.read().clone()
    }

    // ============================================================================
    // Profiles
    // ============================================================================

    /// Get a cached profile (non-blocking)
    ///
    /// Returns None if profile is not in cache. Use fetch_profile() to load it.
    pub fn get_cached_profile(&self, pubkey: &str) -> Option<profiles::Profile> {
        profiles::get_cached_profile(pubkey)
    }

    /// Fetch a profile from relays (async)
    ///
    /// Checks cache first, then fetches from relays if needed
    pub async fn fetch_profile(&self, pubkey: &str) -> Result<profiles::Profile, String> {
        profiles::fetch_profile(pubkey.to_string()).await
    }

    /// Prefetch multiple profiles (async)
    ///
    /// More efficient than individual fetches - use this when loading feeds
    pub async fn prefetch_profiles(&self, pubkeys: Vec<String>) {
        profiles::prefetch_profiles(pubkeys).await
    }

    // ============================================================================
    // Bookmarks
    // ============================================================================

    /// Check if an event is bookmarked
    pub fn is_bookmarked(&self, event_id: &str) -> bool {
        bookmarks::is_bookmarked(event_id)
    }

    /// Get total bookmark count
    pub fn get_bookmarks_count(&self) -> usize {
        bookmarks::get_bookmarks_count()
    }

    /// Add event to bookmarks (async)
    pub async fn bookmark_event(&self, event_id: String) -> Result<(), String> {
        bookmarks::bookmark_event(event_id).await
    }

    /// Remove event from bookmarks (async)
    pub async fn unbookmark_event(&self, event_id: String) -> Result<(), String> {
        bookmarks::unbookmark_event(event_id).await
    }

    // ============================================================================
    // Theme
    // ============================================================================

    /// Get current theme
    pub fn get_theme(&self) -> theme_store::Theme {
        theme_store::get_theme()
    }

    /// Set theme
    pub fn set_theme(&self, theme: theme_store::Theme) {
        theme_store::set_theme(theme);
    }

    /// Toggle between light and dark theme
    pub fn toggle_theme(&self) {
        theme_store::toggle_theme();
    }
}

impl Default for AppContext {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Convenience Macro (Optional)
// ============================================================================

/// Macro to quickly create an AppContext in components
///
/// # Example
/// ```rust
/// #[component]
/// pub fn MyComponent() -> Element {
///     let ctx = app_context!();
///
///     if ctx.is_authenticated() {
///         // ...
///     }
///
///     rsx! { /* ... */ }
/// }
/// ```
#[macro_export]
macro_rules! app_context {
    () => {
        $crate::context::app_context::AppContext::new()
    };
}
