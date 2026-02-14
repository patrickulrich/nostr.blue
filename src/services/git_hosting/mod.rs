//! Git Hosting Services
//!
//! Service layer for NIP-34 (Git stuff) and NIP-C0 (Code Snippets).
//! Handles fetching, publishing, and caching of git-related events.
pub mod activity;
pub mod bounties;
pub mod discussions;
pub mod file_fetcher;
pub mod git_service;
pub mod github_import;
pub mod issues;
pub mod notifications;
pub mod pull_requests;
pub mod releases;
pub mod repository;
pub mod reviews;
pub mod snippets;
pub mod ssh_keys;
pub mod stars;
pub use file_fetcher::fetch_readme;
pub use git_service::git_service;
pub use issues::*;
pub use pull_requests::*;
pub use repository::*;
pub use snippets::*;
