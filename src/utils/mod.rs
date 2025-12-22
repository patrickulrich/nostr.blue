// Utility functions
// Helper functions for common operations

pub mod nip19;
pub mod time;
pub mod validation;
pub mod error;
pub mod content_parser;
pub mod thread_tree;
pub mod article_meta;
pub mod markdown;
pub mod list_kinds;
pub mod notification_nip78;
pub mod mention_extractor;
pub mod data_state;
pub mod timed_serializer;
pub mod format;
pub mod url_metadata;
pub mod profile_prefetch;
pub mod repost;
pub mod event;
pub mod clipboard;
pub mod nip73;
pub mod podcast;
pub mod nip34;
pub mod nip69;
pub mod nip99;  // NIP-99 Marketplace (products, collections, shipping, reviews)
pub mod nip52;  // NIP-52 Calendar events
pub mod nip53;  // NIP-53 Live activities
pub mod nip54;  // NIP-54 Wiki (kind 30818)
pub mod nip58;  // NIP-58 Badges
pub mod nip48;  // NIP-48 Proxy Tags (bridged content)
pub mod ics;    // ICS (iCalendar) import/export
pub mod recipe;       // Recipe parsing and validation
pub mod recipe_tags;  // Recipe category tags with emojis
pub mod asciidoc;     // AsciiDoc to HTML rendering
pub mod nkbip03;      // NKBIP-03 Citations (kinds 30-33)
pub mod nkbip06;      // NKBIP-06 Nostr MIME types (M tag)
pub mod nkbip08;      // NKBIP-08 Book wikilinks
pub mod date_helpers; // Date manipulation helpers for calendars

pub use thread_tree::{ThreadNode, ThreadNodeSource, build_thread_tree, merge_pending_into_tree};
pub use list_kinds::{get_list_type_name, get_list_icon, get_item_count};
pub use data_state::DataState;
pub use format::{format_sats_with_separator, format_sats_compact, truncate_pubkey, shorten_url, format_relative_time_or};
// date_helpers are used via crate::utils::date_helpers::* by calendar components
pub use repost::{FeedItem, extract_reposted_event};
pub use validation::{SignerValidationResult, get_current_user_pubkey, is_valid_http_url};
pub use time::safe_duration_millis;
pub use error::log_fetch_error;

/// Generate a random alphanumeric ID (9 characters)
/// Used for poll options and other unique identifiers
pub fn generate_option_id() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..9)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

/// Slugify a string for use as a d-tag or URL-safe identifier
///
/// Converts to lowercase, replaces non-alphanumeric characters with hyphens,
/// and removes duplicate/leading/trailing hyphens.
pub fn slugify(input: &str) -> String {
    input
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Generate a recipe slug matching zap.cooking format
///
/// Only replaces spaces with hyphens, preserves other characters.
/// This ensures d-tags are compatible with zap.cooking/nostrcooking.
///
/// Example: "Grandma's Pie" -> "grandma's-pie"
pub fn recipe_slug(input: &str) -> String {
    input.to_lowercase().replace(' ', "-")
}
