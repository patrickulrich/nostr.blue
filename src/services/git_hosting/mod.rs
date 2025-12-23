//! Git Hosting Services
//!
//! Service layer for NIP-34 (Git stuff) and NIP-C0 (Code Snippets).
//! Handles fetching, publishing, and caching of git-related events.

pub mod repository;
pub mod issues;
pub mod pull_requests;
pub mod snippets;
pub mod file_fetcher;
pub mod github_import;
pub mod stars;
pub mod git_service;

// Re-export commonly used items
pub use repository::*;
pub use issues::*;
pub use pull_requests::*;
pub use snippets::*;
pub use git_service::git_service;
pub use file_fetcher::fetch_readme;
// file_fetcher and stars are available but not re-exported (use git_hosting::file_fetcher::* if needed)
