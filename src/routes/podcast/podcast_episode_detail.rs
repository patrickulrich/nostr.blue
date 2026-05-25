use crate::components::viewers::podcast_episode_viewer::PodcastRssEpisodeDetail as PodcastRssEpisodeDetailViewer;
use crate::routes::Route;
use dioxus::prelude::*;

#[component]
pub fn PodcastNostrEpisodeDetail(naddr: String) -> Element {
    let nav = navigator();
    nav.replace(Route::AddressViewer { address: naddr });
    rsx! {}
}

#[component]
pub fn PodcastRssEpisodeDetail(podcast_id: String, episode_id: String) -> Element {
    rsx! { PodcastRssEpisodeDetailViewer { podcast_id, episode_id } }
}
