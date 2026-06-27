//! Music components
//!
//! This module contains components for music playback,
//! discovery, albums, artists, tracks, and radio.

pub const FALLBACK_ART_URL: &str = "https://api.dicebear.com/7.x/shapes/svg?seed=music";

pub mod album_card;
pub mod artist_card;
pub mod explore_album_card;
pub mod explore_artist_card;
pub mod explore_track_card;
pub mod player;
pub mod playlist_card;
pub mod radio_card;
pub mod source_tabs;
pub mod track_card;
pub mod unified_track_card;
pub mod zap_dialog;
pub use album_card::{AlbumCard, AlbumCardSkeleton};
pub use artist_card::{ArtistCard, ArtistCardSkeleton};
pub use explore_album_card::{ExploreAlbumCard, ExploreAlbumCardSkeleton};
pub use explore_artist_card::{ExploreArtistCard, ExploreArtistCardSkeleton};
pub use explore_track_card::{ExploreTrackCard, ExploreTrackCardSkeleton};
pub use player::PersistentMusicPlayer;
pub use playlist_card::{PlaylistCard, PlaylistCardSkeleton};
pub use radio_card::{RadioCard, RadioCardSkeleton};
pub use source_tabs::{DiscoveryTab, DiscoveryTabs};
pub use track_card::TrackCard;
pub use unified_track_card::{UnifiedTrackCard, UnifiedTrackCardSkeleton};
pub use zap_dialog::MusicZapDialog;
