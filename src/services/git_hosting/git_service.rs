//! Git Service - High-level API for git operations
//!
//! Provides a unified interface for git operations using isomorphic-git Web Worker.
//! Handles repository cloning and file browsing.
//!
//! This service wraps GitWorkerManager and provides Repository-aware methods
//! that handle clone URL selection.

use crate::services::git_worker::{FileEntry, GitWorkerManager};
use crate::stores::grasp_servers;
use crate::utils::nip34::Repository;

/// Git Service for repository operations
///
/// Provides high-level methods for browsing files and reading content.
pub struct GitService;

impl Default for GitService {
    fn default() -> Self {
        Self::new()
    }
}

impl GitService {
    /// Create a new GitService
    pub fn new() -> Self {
        Self
    }

    /// Initialize the git worker (call once on app startup)
    pub async fn init() -> Result<(), String> {
        GitWorkerManager::init().await
    }

    /// Check if git worker is initialized
    pub fn is_initialized() -> bool {
        GitWorkerManager::is_initialized()
    }

    /// Get the directory path for a repository
    fn get_dir(repo: &Repository) -> String {
        // Use first 16 chars of naddr as directory name
        let id: String = repo.naddr.chars().take(24).collect();
        format!("/repos/{}", id)
    }

    /// Select the best clone URL for a repository
    ///
    /// Prefers GRASP/ngit URLs (CORS enabled), falls back to GitHub/GitLab
    fn select_clone_url(repo: &Repository) -> Option<String> {
        // First, try GRASP URLs (preferred - no CORS proxy needed)
        // Uses dynamic registry that discovers servers from NIP-34 events
        for url in &repo.clone {
            if Self::is_grasp_url(url) {
                return Some(url.clone());
            }
        }

        // Fall back to any URL (GitHub/GitLab will use CORS proxy)
        repo.clone.first().cloned()
    }

    /// Check if a URL points to a known GRASP server
    fn is_grasp_url(url: &str) -> bool {
        url::Url::parse(url)
            .ok()
            .and_then(|u| u.domain().map(|d| d.to_string()))
            .map(|domain| grasp_servers::is_grasp_server(&domain))
            .unwrap_or(false)
    }

    /// Ensure a repository is cloned and ready
    ///
    /// Returns the directory path where the repo is cloned.
    /// If already cloned, returns immediately. Otherwise clones first.
    pub async fn ensure_cloned(&self, repo: &Repository) -> Result<String, String> {
        let dir = Self::get_dir(repo);

        // Check if already cloned
        if GitWorkerManager::repo_exists(&dir).await {
            return Ok(dir);
        }

        // Select best clone URL
        let clone_url = Self::select_clone_url(repo)
            .ok_or_else(|| "No clone URL available".to_string())?;

        log::info!("Cloning {} to {}", clone_url, dir);

        // Clone with shallow depth (we only need to browse)
        GitWorkerManager::clone_repo(&clone_url, &dir, 1).await?;

        log::info!("Clone complete: {}", dir);
        Ok(dir)
    }

    /// List files in a directory
    ///
    /// Returns file/directory entries at the specified path.
    /// Entries are sorted: directories first, then alphabetically.
    pub async fn list_files(
        &self,
        repo: &Repository,
        path: &str,
        git_ref: Option<&str>,
    ) -> Result<Vec<FileEntry>, String> {
        let dir = self.ensure_cloned(repo).await?;
        let git_ref = git_ref.unwrap_or("HEAD");

        GitWorkerManager::list_files(&dir, path, git_ref).await
    }

    /// Read file content
    ///
    /// Returns the content of a file at the specified path and ref.
    pub async fn read_file(
        &self,
        repo: &Repository,
        filepath: &str,
        git_ref: Option<&str>,
    ) -> Result<String, String> {
        let dir = self.ensure_cloned(repo).await?;
        let git_ref = git_ref.unwrap_or("HEAD");

        GitWorkerManager::read_file(&dir, filepath, git_ref).await
    }

    /// Get branch list
    pub async fn get_branches(&self, repo: &Repository) -> Result<Vec<String>, String> {
        let dir = self.ensure_cloned(repo).await?;
        GitWorkerManager::get_branches(&dir).await
    }
}

/// Global git service instance
static GIT_SERVICE: std::sync::OnceLock<GitService> = std::sync::OnceLock::new();

/// Get the global git service instance
pub fn git_service() -> &'static GitService {
    GIT_SERVICE.get_or_init(GitService::new)
}
