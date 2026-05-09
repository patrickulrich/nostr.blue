pub mod podcasts;
pub use podcasts::podcast_index;
pub use podcasts::podcast_rss;

pub mod search;
pub use search::content_search;
pub use search::profile_search;
pub use search::search_relays;
pub use search::trending;

pub mod payments;
pub use payments::btc_price;
pub use payments::lnurl;
pub use payments::mempool;
pub mod ppq;

#[cfg(all(feature = "web", feature = "desktop"))]
compile_error!("Cannot enable both 'web' and 'desktop' features");

#[cfg(all(feature = "web", feature = "mobile_platform"))]
compile_error!("Cannot enable both 'web' and 'mobile' features");

#[cfg(all(feature = "desktop", feature = "mobile_platform"))]
compile_error!("Cannot enable both 'desktop' and 'mobile' features");

#[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile_platform")))]
compile_error!("Must enable exactly one of 'web', 'desktop', or 'mobile' feature");

pub mod admission_policy;
pub mod aggregation;
pub mod ai_chat;
pub mod ai_tools;
pub mod bible_api;
pub mod bible_offline;
#[cfg(feature = "native")]
pub mod bible_offline_sqlite;
#[cfg(feature = "web")]
pub mod bible_offline_indexeddb;
pub mod geocoding;
pub mod git_hosting;
#[cfg(any(feature = "desktop", feature = "mobile_platform"))]
#[allow(dead_code)]
pub mod git_native;
pub mod git_types;
#[cfg(feature = "web")]
pub mod git_worker;
pub mod github_nips;
pub mod openlibrary;
pub mod pages;
pub mod profile_stats;
pub mod scheduler;
pub mod sync;
pub mod wavlake;
