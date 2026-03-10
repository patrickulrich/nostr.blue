//! Topics routes
//! Hashtag-based topical communities
pub mod browse;
pub mod home;
pub mod new_post;
pub mod popular;
pub mod post_detail;
pub mod topic_feed;

pub use browse::TopicsBrowse;
pub use home::TopicsHome;
pub use new_post::TopicNewPost;
pub use popular::TopicsPopular;
pub use post_detail::TopicPostDetail;
pub use topic_feed::TopicFeed;
