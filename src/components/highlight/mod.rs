//! Highlight components (NIP-84)
//!
//! This module contains components for Nostr highlights
//! including highlight cards and highlight modals.
pub mod card;
pub mod modal;
#[allow(unused_imports)]
pub use card::{HighlightCard, HighlightCardSkeleton};
#[allow(unused_imports)]
pub use modal::HighlightModal;
