pub mod home;
pub mod podcast_episode_detail;
pub mod podcast_nostr_detail;
pub mod podcast_rss_detail;
pub mod podcast_trending;

pub use home::PodcastHome;
pub use podcast_episode_detail::{PodcastNostrEpisodeDetail, PodcastRssEpisodeDetail};
pub use podcast_nostr_detail::PodcastNostrDetail;
pub use podcast_rss_detail::PodcastRssFeedDetail;
pub use podcast_trending::PodcastTrending;
