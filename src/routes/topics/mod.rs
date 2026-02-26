//! Topics routes
//! Hashtag-based topical communities
pub mod home;
pub mod popular;
pub mod browse;
pub mod topic_feed;
pub mod post_detail;
pub mod new_post;

pub use home::TopicsHome;
pub use popular::TopicsPopular;
pub use browse::TopicsBrowse;
pub use topic_feed::TopicFeed;
pub use post_detail::TopicPostDetail;
pub use new_post::TopicNewPost;
