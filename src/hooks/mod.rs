pub mod use_author_metadata;
pub mod use_fetch_event_by_coordinate;
pub mod use_fetch_event_by_id;
pub mod use_infinite_scroll;
pub mod use_lists;
pub mod use_mute_block_cache;
pub mod use_reaction;
pub mod use_unsaved_changes;
// Community hooks available for future refactoring - currently pages use store functions directly
#[allow(dead_code)]
mod use_community;

pub use use_author_metadata::use_author_metadata;
pub use use_fetch_event_by_coordinate::use_fetch_event_by_coordinate_with_message;
pub use use_fetch_event_by_id::use_fetch_event_by_id;
// Struct exports available for future direct use:
// pub use use_fetch_event_by_coordinate::UseFetchEventByCoordinate;
// pub use use_fetch_event_by_id::UseFetchEventById;
// Additional exports available for future use:
// pub use use_fetch_event_by_coordinate::use_fetch_event_by_coordinate;
// pub use use_fetch_event_by_id::{use_fetch_event_by_id_resource, use_fetch_event_by_id_any_kind};
pub use use_infinite_scroll::use_infinite_scroll;
pub use use_lists::{delete_list, use_user_lists, UserList};
pub use use_mute_block_cache::use_mute_block_cache;
pub use use_reaction::{format_count, use_reaction, ReactionEmoji, ReactionState, UseReaction};
// UseUnsavedChanges struct and calculate_hash kept for future manual save controls
#[allow(unused_imports)]
pub use use_unsaved_changes::{
    calculate_hash, calculate_multi_hash, use_unsaved_changes, UseUnsavedChanges,
};
