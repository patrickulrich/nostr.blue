//! Podcast Episode List Component
//!
//! Displays a list of podcast episodes with optional filtering
//! and continuous playback support.
use super::episode_card::{DisplayEpisode, PodcastEpisodeCard, PodcastEpisodeCardSkeleton};
use crate::stores::music_player::MusicTrack;
use dioxus::prelude::*;
use std::rc::Rc;
#[derive(Props, Clone, PartialEq)]
pub struct PodcastEpisodeListProps {
    /// Episodes to display
    pub episodes: Vec<DisplayEpisode>,
    /// Show podcast title on each card (useful when showing mixed podcasts)
    #[props(default = false)]
    pub show_podcast_title: bool,
    /// Show episode descriptions
    #[props(default = true)]
    pub show_descriptions: bool,
    /// Enable continuous playback (creates playlist from all episodes)
    #[props(default = true)]
    pub enable_playlist: bool,
    /// Maximum number of episodes to show (None = all)
    #[props(default)]
    pub limit: Option<usize>,
    /// Loading state
    #[props(default = false)]
    pub loading: bool,
}
/// Podcast episode list component
#[component]
pub fn PodcastEpisodeList(props: PodcastEpisodeListProps) -> Element {
    let all_episodes = props.episodes.clone();
    let episodes: Vec<_> = if let Some(limit) = props.limit {
        props.episodes.iter().take(limit).cloned().collect()
    } else {
        props.episodes.clone()
    };
    let playlist: Option<Rc<Vec<MusicTrack>>> = if props.enable_playlist && !all_episodes.is_empty()
    {
        Some(Rc::new(
            all_episodes.iter().map(|ep| ep.to_music_track()).collect(),
        ))
    } else {
        None
    };
    rsx! {
        div { class: "space-y-1",
            if props.loading {
                for i in 0..5 {
                    PodcastEpisodeCardSkeleton { key: "{i}" }
                }
            } else if episodes.is_empty() {
                div { class: "flex flex-col items-center justify-center py-8 text-center",
                    div { class: "text-muted-foreground text-sm", "No episodes found" }
                }
            } else {
                for episode in episodes {
                    PodcastEpisodeCard {
                        key: "{episode.id}",
                        episode: episode.clone(),
                        show_podcast_title: props.show_podcast_title,
                        show_description: props.show_descriptions,
                        playlist: playlist.clone(),
                    }
                }
            }
        }
    }
}
