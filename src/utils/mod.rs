pub mod nips;
pub use nips::nip36;
pub use nips::nip34;
pub use nips::nip48;
pub use nips::nip49;
pub use nips::nip52;
pub use nips::nip53;
pub use nips::nip54;
pub use nips::nip58;
pub use nips::nip69;
pub use nips::nip73;
pub use nips::nip84;
pub use nips::nip98;
pub use nips::nip99;
pub use nips::nip_bb;

pub mod nkbips;
pub use nkbips::nkbip03;
pub use nkbips::nkbip06;
pub use nkbips::nkbip08;

pub mod audio;
pub use audio::ics;
pub use audio::podcast;
pub use audio::radio;

pub mod parsing;
pub use parsing::asciidoc;
pub use parsing::content_parser;
pub use parsing::markdown;
pub use parsing::mention_extractor;
pub use parsing::thread_tree;

pub mod recipes;
pub use recipes::recipe;
pub use recipes::recipe_tags;

pub mod article_meta;
pub mod bolt11;
pub mod clipboard;
pub mod custom_emoji;
pub mod data_state;
pub mod date_helpers;
pub mod download;
pub mod duration;
pub mod error;
pub mod event;
pub mod format;
pub mod list_encryption;
pub mod list_kinds;
pub mod nip19;
pub mod notification_nip78;
pub mod path_validation;
pub mod permissions;
pub mod pin_metadata;
pub mod profile_prefetch;
pub mod relay;
pub mod relay_output;
pub mod repost;
pub mod text;
pub mod time;
pub mod timed_serializer;
pub mod url_metadata;
pub mod validation;
pub mod video_kinds;
pub mod route_for_kind;
pub use data_state::DataState;
pub use error::log_fetch_error;
pub use format::{
    format_bytes, format_relative_time_or, format_sats_compact, truncate_pubkey,
};
#[cfg(feature = "cashu")]
pub use format::{format_sats_with_separator, shorten_url};
pub use list_kinds::{get_item_count, get_list_icon, get_list_type_name};
pub use path_validation::is_safe_path;
pub use repost::{extract_reposted_event, process_events_to_feed_items, FeedItem};
pub use thread_tree::{build_thread_tree, extract_root_event_id, ThreadNode};
pub use time::{format_commit_date, format_time_ago, safe_duration_millis};
pub use validation::{css_safe_url, is_valid_http_url};
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
/// Generate a recipe slug for d-tag identifier
///
/// Only replaces spaces with hyphens, preserves other characters.
/// This ensures d-tags are compatible with recipe apps using nostrcooking format.
///
/// Example: "Grandma's Pie" -> "grandma's-pie"
pub fn recipe_slug(input: &str) -> String {
    input.to_lowercase().replace(' ', "-")
}
