//! Community components (NIP-72)
//!
//! This module contains components for Nostr communities
//! including community cards, posts, and post composers.

pub mod card;
pub mod post_card;
pub mod post_composer;

// Re-export main component types for convenience
pub use card::{CommunityCard, CommunityCardSkeleton, CommunityCardWithMembership, JoinButton};
pub use post_card::{CommunityPostCard, CommunityPostCardSkeleton, UserRoleBadge};
pub use post_composer::{CommunityPostComposer, CommunityPostComposerInline};
