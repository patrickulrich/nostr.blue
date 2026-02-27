//! File Fetcher Service
//!
//! Multi-source file fetching with fallback chain:
//! GitHub API → GitLab API → Codeberg API → Generic HTTP
#![allow(dead_code)]
use reqwest::Client;
use serde::Deserialize;
use crate::utils::nip34::Repository;
/// A file or directory entry in the repository tree
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    pub entry_type: EntryType,
    pub size: Option<u64>,
}
/// Type of tree entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
    Submodule,
}
/// GitHub API tree response
#[derive(Debug, Deserialize)]
struct GitHubTreeResponse {
    tree: Vec<GitHubTreeEntry>,
}
#[derive(Debug, Deserialize)]
struct GitHubTreeEntry {
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
    size: Option<u64>,
}
/// GitHub API content response
#[derive(Debug, Deserialize)]
struct GitHubContentResponse {
    content: Option<String>,
    encoding: Option<String>,
}
/// Extract owner and repo from GitHub URL
fn parse_github_url(url: &str) -> Option<(String, String)> {
    // Normalize SSH URLs (git@github.com:owner/repo.git) to HTTPS form
    let url = if url.contains("github.com:") && !url.contains("://") {
        let after_colon = url.split("github.com:").nth(1)?;
        let normalized = format!("https://github.com/{}", after_colon);
        return parse_github_url(&normalized);
    } else {
        url
    };
    let url = url.trim_end_matches(".git").trim_end_matches('/');
    let parts: Vec<&str> = url.split('/').collect();
    let github_idx = parts.iter().position(|&p| p.contains("github.com"))?;
    if parts.len() > github_idx + 2 {
        return Some((
            parts[github_idx + 1].to_string(),
            parts[github_idx + 2].to_string(),
        ));
    }
    None
}
/// Extract owner and repo from GitLab URL
fn parse_gitlab_url(url: &str) -> Option<(String, String)> {
    let url = url.trim_end_matches(".git").trim_end_matches('/');
    let parts: Vec<&str> = url.split('/').collect();
    let gitlab_idx = parts.iter().position(|&p| p.contains("gitlab.com"))?;
    if parts.len() > gitlab_idx + 2 {
        return Some((
            parts[gitlab_idx + 1].to_string(),
            parts[gitlab_idx + 2].to_string(),
        ));
    }
    None
}
/// Extract owner and repo from Codeberg URL
fn parse_codeberg_url(url: &str) -> Option<(String, String)> {
    let url = url.trim_end_matches(".git").trim_end_matches('/');
    let parts: Vec<&str> = url.split('/').collect();
    let codeberg_idx = parts.iter().position(|&p| p.contains("codeberg.org"))?;
    if parts.len() > codeberg_idx + 2 {
        return Some((
            parts[codeberg_idx + 1].to_string(),
            parts[codeberg_idx + 2].to_string(),
        ));
    }
    None
}
/// Fetch file content from a repository
pub async fn fetch_file(
    repo: &Repository,
    path: &str,
    git_ref: Option<&str>,
) -> Result<String, String> {
    let refs_to_try: Vec<&str> = match git_ref {
        Some(r) => vec![r],
        None => vec!["main", "master"],
    };
    for ref_name in refs_to_try {
        for url in &repo.clone {
            if url.contains("github.com") {
                if let Some((owner, repo_name)) = parse_github_url(url) {
                    match fetch_github_file(&owner, &repo_name, path, ref_name).await {
                        Ok(content) => return Ok(content),
                        Err(_) => continue,
                    }
                }
            } else if url.contains("gitlab.com") {
                if let Some((owner, repo_name)) = parse_gitlab_url(url) {
                    match fetch_gitlab_file(&owner, &repo_name, path, ref_name).await {
                        Ok(content) => return Ok(content),
                        Err(_) => continue,
                    }
                }
            } else if url.contains("codeberg.org") {
                if let Some((owner, repo_name)) = parse_codeberg_url(url) {
                    match fetch_codeberg_file(&owner, &repo_name, path, ref_name).await {
                        Ok(content) => return Ok(content),
                        Err(_) => continue,
                    }
                }
            }
        }
    }
    Err("Failed to fetch file from any source".to_string())
}
/// Fetch directory listing from a repository
pub async fn fetch_directory(
    repo: &Repository,
    path: &str,
    git_ref: Option<&str>,
) -> Result<Vec<TreeEntry>, String> {
    let refs_to_try: Vec<&str> = match git_ref {
        Some(r) => vec![r],
        None => vec!["main", "master"],
    };
    for ref_name in refs_to_try {
        for url in &repo.clone {
            if url.contains("github.com") {
                if let Some((owner, repo_name)) = parse_github_url(url) {
                    match fetch_github_tree(&owner, &repo_name, path, ref_name).await {
                        Ok(entries) => return Ok(entries),
                        Err(_) => continue,
                    }
                }
            } else if url.contains("gitlab.com") {
                if let Some((owner, repo_name)) = parse_gitlab_url(url) {
                    match fetch_gitlab_tree(&owner, &repo_name, path, ref_name).await {
                        Ok(entries) => return Ok(entries),
                        Err(_) => continue,
                    }
                }
            } else if url.contains("codeberg.org") {
                if let Some((owner, repo_name)) = parse_codeberg_url(url) {
                    match fetch_codeberg_tree(&owner, &repo_name, path, ref_name).await {
                        Ok(entries) => return Ok(entries),
                        Err(_) => continue,
                    }
                }
            }
        }
    }
    Err("Failed to fetch directory from any source".to_string())
}
async fn fetch_github_file(
    owner: &str,
    repo: &str,
    path: &str,
    git_ref: &str,
) -> Result<String, String> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
        owner,
        repo,
        path,
        git_ref,
    );
    let response = Client::new()
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }
    let content: GitHubContentResponse = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;
    if let (Some(content), Some(encoding)) = (content.content, content.encoding) {
        if encoding == "base64" {
            let cleaned = content.replace('\n', "");
            let decoded = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &cleaned,
                )
                .map_err(|e| format!("Base64 decode error: {}", e))?;
            return String::from_utf8(decoded)
                .map_err(|e| format!("UTF-8 decode error: {}", e));
        }
    }
    Err("No content in response".to_string())
}
async fn fetch_github_tree(
    owner: &str,
    repo: &str,
    path: &str,
    git_ref: &str,
) -> Result<Vec<TreeEntry>, String> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
        owner,
        repo,
        git_ref,
    );
    let response = Client::new()
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }
    let tree: GitHubTreeResponse = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;
    let prefix = if path.is_empty() { "" } else { path };
    let _depth = if prefix.is_empty() { 0 } else { prefix.matches('/').count() + 1 };
    let entries = tree
        .tree
        .into_iter()
        .filter(|e| {
            if prefix.is_empty() {
                !e.path.contains('/')
            } else {
                e.path.starts_with(prefix) && e.path.len() > prefix.len()
                    && e.path.chars().nth(prefix.len()) == Some('/')
                    && e.path[prefix.len() + 1..].matches('/').count() == 0
            }
        })
        .map(|e| {
            let name = if prefix.is_empty() {
                e.path.clone()
            } else {
                e.path[prefix.len() + 1..].to_string()
            };
            TreeEntry {
                name,
                path: e.path,
                entry_type: match e.entry_type.as_str() {
                    "tree" => EntryType::Directory,
                    "commit" => EntryType::Submodule,
                    _ => EntryType::File,
                },
                size: e.size,
            }
        })
        .collect();
    Ok(entries)
}
async fn fetch_gitlab_file(
    owner: &str,
    repo: &str,
    path: &str,
    git_ref: &str,
) -> Result<String, String> {
    let project_id = format!("{}/{}", owner, repo);
    let encoded_path = urlencoding::encode(path);
    let url = format!(
        "https://gitlab.com/api/v4/projects/{}/repository/files/{}/raw?ref={}",
        urlencoding::encode(&project_id),
        encoded_path,
        git_ref,
    );
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("GitLab API error: {}", response.status()));
    }
    response.text().await.map_err(|e| format!("Read error: {}", e))
}
async fn fetch_gitlab_tree(
    owner: &str,
    repo: &str,
    path: &str,
    git_ref: &str,
) -> Result<Vec<TreeEntry>, String> {
    let project_id = format!("{}/{}", owner, repo);
    let url = format!(
        "https://gitlab.com/api/v4/projects/{}/repository/tree?ref={}&path={}",
        urlencoding::encode(&project_id),
        git_ref,
        urlencoding::encode(path),
    );
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("GitLab API error: {}", response.status()));
    }
    #[derive(Deserialize)]
    struct GitLabEntry {
        name: String,
        path: String,
        #[serde(rename = "type")]
        entry_type: String,
    }
    let entries: Vec<GitLabEntry> = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;
    Ok(
        entries
            .into_iter()
            .map(|e| TreeEntry {
                name: e.name,
                path: e.path,
                entry_type: match e.entry_type.as_str() {
                    "tree" => EntryType::Directory,
                    _ => EntryType::File,
                },
                size: None,
            })
            .collect(),
    )
}
async fn fetch_codeberg_file(
    owner: &str,
    repo: &str,
    path: &str,
    git_ref: &str,
) -> Result<String, String> {
    let url = format!(
        "https://codeberg.org/api/v1/repos/{}/{}/raw/{}?ref={}",
        owner,
        repo,
        path,
        git_ref,
    );
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Codeberg API error: {}", response.status()));
    }
    response.text().await.map_err(|e| format!("Read error: {}", e))
}
async fn fetch_codeberg_tree(
    owner: &str,
    repo: &str,
    path: &str,
    git_ref: &str,
) -> Result<Vec<TreeEntry>, String> {
    let url = format!(
        "https://codeberg.org/api/v1/repos/{}/{}/contents/{}?ref={}",
        owner,
        repo,
        path,
        git_ref,
    );
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("Codeberg API error: {}", response.status()));
    }
    #[derive(Deserialize)]
    struct CodebergEntry {
        name: String,
        path: String,
        #[serde(rename = "type")]
        entry_type: String,
        size: Option<u64>,
    }
    let entries: Vec<CodebergEntry> = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;
    Ok(
        entries
            .into_iter()
            .map(|e| TreeEntry {
                name: e.name,
                path: e.path,
                entry_type: match e.entry_type.as_str() {
                    "dir" => EntryType::Directory,
                    "submodule" => EntryType::Submodule,
                    _ => EntryType::File,
                },
                size: e.size,
            })
            .collect(),
    )
}
/// Fetch all file paths in the repository (recursive).
///
/// Returns a flat list of all file paths (blobs only, no directories).
/// Currently supports GitHub repositories; returns an error for other sources.
pub async fn fetch_all_file_paths(
    repo: &Repository,
    git_ref: Option<&str>,
) -> Result<Vec<String>, String> {
    let refs_to_try: Vec<&str> = match git_ref {
        Some(r) => vec![r],
        None => vec!["main", "master"],
    };
    for ref_name in refs_to_try {
        for url in &repo.clone {
            if url.contains("github.com") {
                if let Some((owner, repo_name)) = parse_github_url(url) {
                    match fetch_github_all_paths(&owner, &repo_name, ref_name).await {
                        Ok(paths) => return Ok(paths),
                        Err(_) => continue,
                    }
                }
            }
        }
    }
    Err("Recursive file listing is only supported for GitHub repositories".to_string())
}

/// Fetch all blob paths from GitHub's recursive tree API
async fn fetch_github_all_paths(
    owner: &str,
    repo: &str,
    git_ref: &str,
) -> Result<Vec<String>, String> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
        owner, repo, git_ref,
    );
    let response = Client::new()
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }
    let tree: GitHubTreeResponse = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;
    Ok(tree
        .tree
        .into_iter()
        .filter(|e| e.entry_type == "blob")
        .map(|e| e.path)
        .collect())
}

/// Fetch README content for a repository (tries common names)
///
/// Tries REST APIs (GitHub/GitLab/Codeberg) first, then falls back to
/// isomorphic-git for GRASP/ngit and other sources.
pub async fn fetch_readme(
    repo: &Repository,
    git_ref: Option<&str>,
) -> Result<String, String> {
    let readme_names = ["README.md", "README", "readme.md", "Readme.md"];
    // Try REST APIs first
    for name in readme_names {
        if let Ok(content) = fetch_file(repo, name, git_ref).await {
            return Ok(content);
        }
    }
    // Fall back to isomorphic-git (works for all sources including GRASP)
    use super::git_service;
    if git_service::GitService::is_initialized()
        || git_service::GitService::init().await.is_ok()
    {
        for name in readme_names {
            if let Ok(content) = git_service::git_service()
                .read_file(repo, name, git_ref.map(|r| if r == "main" { "HEAD" } else { r }))
                .await
            {
                return Ok(content);
            }
        }
    }
    Err("No README found".to_string())
}
