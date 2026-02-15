//! Git Service - High-level API for git operations
//!
//! Provides a unified interface for git operations using isomorphic-git Web Worker.
//! Handles repository cloning and file browsing.
//!
//! This service wraps GitWorkerManager and provides Repository-aware methods
//! that handle clone URL selection.
#![allow(dead_code)]
use crate::services::git_worker::{CommitEntry, FileEntry, GitWorkerManager};
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
        let id: String = repo.naddr.chars().take(24).collect();
        format!("/repos/{}", id)
    }
    /// Select the best clone URL for a repository
    ///
    /// Prefers GRASP/ngit URLs (CORS enabled), falls back to GitHub/GitLab
    fn select_clone_url(repo: &Repository) -> Option<String> {
        for url in &repo.clone {
            if Self::is_grasp_url(url) {
                return Some(url.clone());
            }
        }
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
        if GitWorkerManager::repo_exists(&dir).await {
            return Ok(dir);
        }
        let clone_url = Self::select_clone_url(repo)
            .ok_or_else(|| "No clone URL available".to_string())?;
        log::info!("Cloning {} to {}", clone_url, dir);
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
    /// Get commit log
    pub async fn get_log(
        &self,
        repo: &Repository,
        git_ref: Option<&str>,
        count: u32,
    ) -> Result<Vec<CommitEntry>, String> {
        let dir = self.ensure_cloned(repo).await?;
        let git_ref = git_ref.unwrap_or("HEAD");
        GitWorkerManager::get_log(&dir, git_ref, count).await
    }
    /// List all file paths recursively (flat list for fuzzy finder)
    pub async fn list_all_files(
        &self,
        repo: &Repository,
        git_ref: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let dir = self.ensure_cloned(repo).await?;
        let git_ref = git_ref.unwrap_or("HEAD");
        GitWorkerManager::list_all_paths(&dir, git_ref).await
    }
    /// Compare two refs and generate a unified diff
    ///
    /// Tries local isomorphic-git diff first, falls back to GitHub API.
    pub async fn compare_refs(
        &self,
        repo: &Repository,
        base: &str,
        head: &str,
    ) -> Result<String, String> {
        let dir = self.ensure_cloned(repo).await?;
        match GitWorkerManager::diff_refs(&dir, base, head).await {
            Ok(diff) if !diff.is_empty() => Ok(diff),
            _ => compare_refs_github(repo, base, head).await,
        }
    }
}
/// Global git service instance
static GIT_SERVICE: std::sync::OnceLock<GitService> = std::sync::OnceLock::new();
/// Get the global git service instance
pub fn git_service() -> &'static GitService {
    GIT_SERVICE.get_or_init(GitService::new)
}
/// Compare two refs using GitHub API
///
/// Returns the diff content as a string. Only works for GitHub-hosted repositories.
pub async fn compare_refs_github(
    repo: &Repository,
    base: &str,
    head: &str,
) -> Result<String, String> {
    let (owner, repo_name) = extract_github_info(repo).ok_or("Not a GitHub repository")?;
    let url = format!(
        "https://api.github.com/repos/{}/{}/compare/{}...{}",
        owner, repo_name, base, head
    );
    let resp = gloo_net::http::Request::get(&url)
        .header("Accept", "application/vnd.github.v3.diff")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if resp.status() != 200 {
        return Err(format!("GitHub API returned status {}", resp.status()));
    }
    resp.text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))
}
/// Extract owner and repo name from a Repository's GitHub URLs
pub(crate) fn extract_github_info(repo: &Repository) -> Option<(String, String)> {
    use super::github_import::parse_github_url;
    for url in repo.web.iter().chain(repo.clone.iter()) {
        if let Some(parts) = parse_github_url(url) {
            return Some(parts);
        }
    }
    None
}
