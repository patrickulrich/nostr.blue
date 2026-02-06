//! Publication components
//!
//! This module contains components for publications,
//! including cards, sections, table of contents, and AsciiDoc rendering.
pub mod asciidoc_content;
pub mod card;
pub mod section;
pub mod toc;
pub use asciidoc_content::{AsciiDocContent, CitationMetadata, WikilinksList};
pub use card::{PublicationCardCompact, PublicationCardSkeleton, PublicationGrid};
pub use section::{
    PublicationSectionContent, PublicationSectionSkeleton, SectionMetadata, SectionNavigation,
    SectionOutline,
};
pub use toc::{
    PublicationProgress, PublicationTocDynamic, PublicationTocHorizontal, PublicationTocSkeleton,
};
