//! Code/Git hosting components (NIP-34 + NIP-C0)
//!
//! This module contains components for displaying Git repositories,
//! issues, pull requests, code snippets, and file browsing.

pub mod repo_card;
pub mod status_badge;
pub mod snippet_card;
pub mod issue_card;
pub mod pull_card;
pub mod file_tree;
pub mod file_viewer;

// Re-export main component types for convenience
pub use repo_card::{CodeRepoCard, CodeRepoCardCompact};
pub use status_badge::CodeStatusBadge;
pub use snippet_card::CodeSnippetCard;
pub use issue_card::CodeIssueRow;
pub use pull_card::CodePullRow;
pub use file_tree::{CodeFileTree, FileTreeSkeleton, FilePathBreadcrumb, BranchSelector};
pub use file_viewer::{CodeFileViewer, CodeFileViewerSkeleton};
