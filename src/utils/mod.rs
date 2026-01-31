pub mod article_meta;
pub mod asciidoc;
pub mod clipboard;
pub mod content_parser;
pub mod data_state;
pub mod date_helpers;
pub mod download;
pub mod duration;
pub mod error;
pub mod event;
pub mod format;
pub mod ics;
pub mod list_encryption;
pub mod list_kinds;
pub mod markdown;
pub mod mention_extractor;
pub mod nip19;
pub mod nip34;
pub mod nip48;
pub mod nip49;
pub mod nip52;
pub mod nip53;
pub mod nip54;
pub mod nip58;
pub mod nip69;
pub mod nip73;
pub mod nip84;
pub mod nip98;
pub mod nip99;
pub mod nkbip03;
pub mod nkbip06;
pub mod nkbip08;
pub mod notification_nip78;
pub mod pin_metadata;
pub mod podcast;
pub mod profile_prefetch;
pub mod radio;
pub mod recipe;
pub mod recipe_tags;
pub mod repost;
pub mod thread_tree;
pub mod time;
pub mod timed_serializer;
pub mod url_metadata;
pub mod validation;
pub use data_state::DataState;
pub use format::{
    format_relative_time_or, format_sats_compact, format_sats_with_separator,
    shorten_url, truncate_pubkey,
};
pub use list_kinds::{get_item_count, get_list_icon, get_list_type_name};
pub use thread_tree::{build_thread_tree, ThreadNode};
pub use error::log_fetch_error;
pub use repost::{extract_reposted_event, FeedItem};
pub use time::{format_time_ago, safe_duration_millis};
pub use validation::{css_safe_url, is_valid_http_url};
/// Generate a random alphanumeric ID (9 characters)
/// Used for poll options and other unique identifiers
pub fn generate_option_id() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..9).map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char).collect()
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
/// Generate a recipe slug for d-tag identifier
///
/// Only replaces spaces with hyphens, preserves other characters.
/// This ensures d-tags are compatible with recipe apps using nostrcooking format.
///
/// Example: "Grandma's Pie" -> "grandma's-pie"
pub fn recipe_slug(input: &str) -> String {
    input.to_lowercase().replace(' ', "-")
}
