//! Podcast components (Podcasting 2.0)
//!
//! This module contains components for podcast shows,
//! episodes, playback, transcripts, and value-for-value.
pub mod add_feed_modal;
pub mod chapters;
pub mod episode_card;
pub mod episode_list;
pub mod persons;
pub mod show_card;
pub mod soundbites;
pub mod transcript;
pub mod v4v;
pub use add_feed_modal::PodcastAddFeedModal;
pub use chapters::PodcastChapters;
pub use episode_card::{DisplayEpisode, PodcastEpisodeCard, PodcastEpisodeCardSkeleton};
pub use episode_list::PodcastEpisodeList;
pub use persons::{InlineCredits, PodcastPersons};
pub use show_card::{PodcastShow, PodcastShowCard, PodcastShowCardSkeleton};
pub use soundbites::{FeaturedSoundbite, PodcastSoundbites};
pub use transcript::PodcastTranscript;
pub use v4v::{V4VBoostButton, V4VInfo};
