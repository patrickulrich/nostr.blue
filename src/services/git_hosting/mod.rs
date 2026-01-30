//! Git Hosting Services
//!
//! Service layer for NIP-34 (Git stuff) and NIP-C0 (Code Snippets).
//! Handles fetching, publishing, and caching of git-related events.

pub mod file_fetcher;
pub mod git_service;
pub mod github_import;
pub mod issues;
pub mod pull_requests;
pub mod repository;
pub mod snippets;
pub mod stars;

// Re-export commonly used items
pub use file_fetcher::fetch_readme;
pub use git_service::git_service;
pub use issues::*;
pub use pull_requests::*;
pub use repository::*;
pub use snippets::*;
// file_fetcher and stars are available but not re-exported (use git_hosting::file_fetcher::* if needed)
