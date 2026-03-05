//! Git Service - High-level API for git operations
//!
//! Provides a unified interface for git operations using isomorphic-git Web Worker.
//! Handles repository cloning and file browsing.
//!
//! This service wraps GitWorkerManager and provides Repository-aware methods
//! that handle clone URL selection.
#![allow(dead_code)]
use crate::platform::http::http_client;
use crate::services::git_types::{CommitEntry, FileEntry};
#[cfg(feature = "web")]
use crate::services::git_worker::GitWorkerManager;
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
        #[cfg(feature = "web")]
        { GitWorkerManager::init().await }
        #[cfg(not(feature = "web"))]
        { Ok(()) }
    }
    /// Check if git worker is initialized
    pub fn is_initialized() -> bool {
        #[cfg(feature = "web")]
        { GitWorkerManager::is_initialized() }
        #[cfg(not(feature = "web"))]
        { true }
    }
    /// Get the directory path for a repository
    fn get_dir(repo: &Repository) -> String {
        let id: String = repo.naddr.chars().take(24).collect();
        #[cfg(feature = "web")]
        { format!("/repos/{}", id) }
        #[cfg(not(feature = "web"))]
        {
            let repos_dir = dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("nostr-blue")
                .join("repos")
                .join(&id);
            repos_dir.to_string_lossy().to_string()
        }
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
        let clone_url = Self::select_clone_url(repo)
            .ok_or_else(|| "No clone URL available".to_string())?;
        #[cfg(feature = "web")]
        {
            if GitWorkerManager::repo_exists(&dir).await {
                return Ok(dir);
            }
            log::info!("Cloning {} to {}", clone_url, dir);
            GitWorkerManager::clone_repo(&clone_url, &dir, 1).await?;
        }
        #[cfg(not(feature = "web"))]
        {
            let path = std::path::Path::new(&dir);
            if path.join(".git").exists() {
                return Ok(dir);
            }
            std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
            log::info!("Cloning {} to {}", clone_url, dir);
            let dir_clone = dir.clone();
            tokio::task::spawn_blocking(move || {
                git2::build::RepoBuilder::new()
                    .clone(&clone_url, std::path::Path::new(&dir_clone))
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| e.to_string())??;
        }
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
        let git_ref_str = git_ref.unwrap_or("HEAD").to_string();
        #[cfg(feature = "web")]
        { GitWorkerManager::list_files(&dir, path, &git_ref_str).await }
        #[cfg(not(feature = "web"))]
        {
            let path = path.to_string();
            let dir_clone = dir.clone();
            tokio::task::spawn_blocking(move || {
                crate::services::git_native::list_files(&dir_clone, &path, &git_ref_str)
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }
    /// Read file content
    pub async fn read_file(
        &self,
        repo: &Repository,
        filepath: &str,
        git_ref: Option<&str>,
    ) -> Result<String, String> {
        let dir = self.ensure_cloned(repo).await?;
        let git_ref_str = git_ref.unwrap_or("HEAD").to_string();
        #[cfg(feature = "web")]
        { GitWorkerManager::read_file(&dir, filepath, &git_ref_str).await }
        #[cfg(not(feature = "web"))]
        {
            let filepath = filepath.to_string();
            let dir_clone = dir.clone();
            tokio::task::spawn_blocking(move || {
                crate::services::git_native::read_file(&dir_clone, &filepath, &git_ref_str)
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }
    /// Get branch list
    pub async fn get_branches(&self, repo: &Repository) -> Result<Vec<String>, String> {
        let dir = self.ensure_cloned(repo).await?;
        #[cfg(feature = "web")]
        { GitWorkerManager::get_branches(&dir).await }
        #[cfg(not(feature = "web"))]
        {
            let dir_clone = dir.clone();
            tokio::task::spawn_blocking(move || {
                crate::services::git_native::get_branches(&dir_clone)
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }
    /// Get commit log
    pub async fn get_log(
        &self,
        repo: &Repository,
        git_ref: Option<&str>,
        count: u32,
    ) -> Result<Vec<CommitEntry>, String> {
        let dir = self.ensure_cloned(repo).await?;
        let git_ref_str = git_ref.unwrap_or("HEAD").to_string();
        #[cfg(feature = "web")]
        { GitWorkerManager::get_log(&dir, &git_ref_str, count).await }
        #[cfg(not(feature = "web"))]
        {
            let dir_clone = dir.clone();
            tokio::task::spawn_blocking(move || {
                crate::services::git_native::get_log(&dir_clone, &git_ref_str, count)
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }
    /// List all file paths recursively (flat list for fuzzy finder)
    pub async fn list_all_files(
        &self,
        repo: &Repository,
        git_ref: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let dir = self.ensure_cloned(repo).await?;
        let git_ref_str = git_ref.unwrap_or("HEAD").to_string();
        #[cfg(feature = "web")]
        { GitWorkerManager::list_all_paths(&dir, &git_ref_str).await }
        #[cfg(not(feature = "web"))]
        {
            let dir_clone = dir.clone();
            tokio::task::spawn_blocking(move || {
                crate::services::git_native::list_all_paths(&dir_clone, &git_ref_str)
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }
    /// Compare two refs and generate a unified diff
    pub async fn compare_refs(
        &self,
        repo: &Repository,
        base: &str,
        head: &str,
    ) -> Result<String, String> {
        let dir = self.ensure_cloned(repo).await?;
        #[cfg(feature = "web")]
        {
            match GitWorkerManager::diff_refs(&dir, base, head).await {
                Ok(diff) => Ok(diff),
                Err(e) => {
                    log::warn!("Local diff failed, falling back to GitHub API: {e}");
                    compare_refs_github(repo, base, head).await
                }
            }
        }
        #[cfg(not(feature = "web"))]
        {
            let dir_clone = dir.clone();
            let base_owned = base.to_string();
            let head_owned = head.to_string();
            match tokio::task::spawn_blocking(move || {
                crate::services::git_native::diff_refs(&dir_clone, &base_owned, &head_owned)
            })
            .await
            .map_err(|e| e.to_string())?
            {
                Ok(diff) => Ok(diff),
                Err(e) => {
                    log::warn!("Native diff failed, falling back to GitHub API: {e}");
                    compare_refs_github(repo, base, head).await
                }
            }
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
    let encoded_base = urlencoding::encode(base);
    let encoded_head = urlencoding::encode(head);
    let url = format!(
        "https://api.github.com/repos/{}/{}/compare/{}...{}",
        owner, repo_name, encoded_base, encoded_head
    );
    let resp = http_client()
        .get(&url)
        .header("Accept", "application/vnd.github.v3.diff")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !resp.status().is_success() {
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
