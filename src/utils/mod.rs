// Utility functions
// Helper functions for common operations

pub mod nip19;
pub mod time;
pub mod validation;
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

pub use thread_tree::{ThreadNode, build_thread_tree};
pub use list_kinds::{get_list_type_name, get_list_icon, get_item_count};
pub use data_state::DataState;
pub use format::{format_sats_with_separator, format_sats_compact, truncate_pubkey, shorten_url};
pub use repost::{FeedItem, extract_reposted_event};

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
