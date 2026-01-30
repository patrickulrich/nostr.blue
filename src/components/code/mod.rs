//! Code/Git hosting components (NIP-34 + NIP-C0)
//!
//! This module contains components for displaying Git repositories,
//! issues, pull requests, code snippets, and file browsing.
pub mod file_tree;
pub mod file_viewer;
pub mod issue_card;
pub mod pull_card;
pub mod readme_viewer;
pub mod repo_action_bar;
pub mod repo_card;
pub mod repo_header;
pub mod repo_tab_nav;
pub mod snippet_card;
pub mod status_badge;
pub use file_tree::{BranchSelector, CodeFileTree, FilePathBreadcrumb, FileTreeSkeleton};
pub use file_viewer::{CodeFileViewer, CodeFileViewerSkeleton};
pub use issue_card::CodeIssueRow;
pub use pull_card::CodePullRow;
pub use readme_viewer::ReadmeViewer;
pub use repo_action_bar::RepoActionBar;
pub use repo_card::CodeRepoCard;
#[allow(unused_imports)]
pub use repo_card::CodeRepoCardCompact;
pub use repo_header::RepoHeader;
pub use repo_tab_nav::RepoTabNav;
pub use snippet_card::CodeSnippetCard;
pub use status_badge::CodeStatusBadge;
