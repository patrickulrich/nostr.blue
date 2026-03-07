//! Wiki components (NIP-54)
//!
//! This module contains components for wiki pages,
//! including cards, content display, backlinks, and downloads.
pub mod backlinks;
pub mod card;
pub mod content;
pub mod download_menu;
pub use backlinks::WikiBacklinks;
pub use card::{WikiCardSearchResult, WikiCardSkeleton, WikiGrid, WikiMetadataCard};
pub use content::{
    WikiForwardLinks, WikiOutline, WikiPageContent, WikiPageNotFound, WikiPageSkeleton,
};
pub use download_menu::WikiDownloadMenu;
