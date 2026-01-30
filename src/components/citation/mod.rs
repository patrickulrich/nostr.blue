//! Citation components (NKBIP-03 Kinds 30-33)
//!
//! This module contains components for academic citations
//! including citation cards, editors, and pickers.
pub mod card;
pub mod editor_modal;
pub mod picker_modal;
pub use picker_modal::{CitationPickerModal, CitationSelection};
