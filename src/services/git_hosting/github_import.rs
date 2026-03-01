//! GitHub Import Service
//!
//! Fetches repository metadata from GitHub for import wizard.
#![allow(dead_code)]
use serde::Deserialize;

/// GitHub repository metadata
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GitHubRepo {
    pub name: String,
    pub full_name: String,
    pub description: Option<String>,
    pub html_url: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub default_branch: String,
    pub stargazers_count: u32,
    pub forks_count: u32,
    pub open_issues_count: u32,
    pub language: Option<String>,
    pub topics: Vec<String>,
    pub license: Option<GitHubLicense>,
    pub owner: GitHubOwner,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GitHubLicense {
    pub key: String,
    pub name: String,
    pub spdx_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GitHubOwner {
    pub login: String,
    pub avatar_url: String,
    #[serde(rename = "type")]
    pub owner_type: String,
}
/// Parse a GitHub URL to extract owner and repo
pub fn parse_github_url(url: &str) -> Option<(String, String)> {
    let url = url.trim().trim_end_matches(".git").trim_end_matches('/');

    // Try standard URL parsing first for strict host validation
    if let Ok(parsed) = url::Url::parse(url) {
        if parsed.domain() == Some("github.com") || parsed.domain() == Some("www.github.com") {
            let segments: Vec<&str> = parsed.path_segments()?.filter(|s| !s.is_empty()).collect();
            if segments.len() >= 2 {
                return Some((segments[0].to_string(), segments[1].to_string()));
            }
        }
        return None;
    }

    // Fallback for git@github.com:owner/repo SSH format (not RFC-compliant, won't parse above)
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() >= 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }

    // Fallback for bare github.com/owner/repo (no scheme, won't parse above)
    if let Some(rest) = url.strip_prefix("github.com/") {
        let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() >= 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }

    None
}
/// Fetch repository metadata from GitHub
pub async fn fetch_github_repo(owner: &str, repo: &str) -> Result<GitHubRepo, String> {
    let url = format!("https://api.github.com/repos/{}/{}", owner, repo);
    let response = crate::platform::http::http_client()
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("Repository not found".to_string());
    }
    if !response.status().is_success() {
        return Err(
            format!("GitHub API error: {}", response.status()),
        );
    }
    response.json().await.map_err(|e| format!("Failed to parse response: {}", e))
}
/// Fetch repository from URL
pub async fn fetch_repo_from_url(github_url: &str) -> Result<GitHubRepo, String> {
    let (owner, repo) = parse_github_url(github_url)
        .ok_or_else(|| "Invalid GitHub URL".to_string())?;
    fetch_github_repo(&owner, &repo).await
}
/// Fetch user's repositories
pub async fn fetch_user_repos(
    username: &str,
    limit: usize,
) -> Result<Vec<GitHubRepo>, String> {
    let url = format!(
        "https://api.github.com/users/{}/repos?sort=updated&per_page={}",
        username,
        limit,
    );
    let response = crate::platform::http::http_client()
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }
    response.json().await.map_err(|e| format!("Failed to parse response: {}", e))
}
/// Search GitHub repositories
pub async fn search_repos(query: &str, limit: usize) -> Result<Vec<GitHubRepo>, String> {
    let url = format!(
        "https://api.github.com/search/repositories?q={}&per_page={}",
        urlencoding::encode(query),
        limit,
    );
    let response = crate::platform::http::http_client()
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }
    #[derive(Deserialize)]
    struct SearchResponse {
        items: Vec<GitHubRepo>,
    }
    let search: SearchResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    Ok(search.items)
}
/// GitHub commit info
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GitHubCommit {
    pub sha: String,
    pub commit: GitHubCommitDetail,
    pub author: Option<GitHubOwner>,
    pub html_url: String,
}
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GitHubCommitDetail {
    pub message: String,
    pub author: GitHubCommitAuthor,
}
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GitHubCommitAuthor {
    pub name: String,
    pub email: String,
    pub date: String,
}
/// Fetch recent commits
pub async fn fetch_commits(
    owner: &str,
    repo: &str,
    limit: usize,
) -> Result<Vec<GitHubCommit>, String> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/commits?per_page={}",
        owner,
        repo,
        limit,
    );
    let response = crate::platform::http::http_client()
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }
    response.json().await.map_err(|e| format!("Failed to parse response: {}", e))
}
/// Fetch recent commits for a specific file path
pub async fn fetch_file_commits(
    owner: &str,
    repo: &str,
    path: &str,
    git_ref: &str,
    limit: usize,
) -> Result<Vec<GitHubCommit>, String> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/commits?path={}&sha={}&per_page={}",
        owner,
        repo,
        urlencoding::encode(path),
        urlencoding::encode(git_ref),
        limit,
    );
    let response = crate::platform::http::http_client()
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }
    response.json().await.map_err(|e| format!("Failed to parse response: {}", e))
}
/// Fetch branches
pub async fn fetch_branches(owner: &str, repo: &str) -> Result<Vec<String>, String> {
    let url = format!("https://api.github.com/repos/{}/{}/branches", owner, repo);
    let response = crate::platform::http::http_client()
        .get(&url)
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", "nostr-blue")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!("GitHub API error: {}", response.status()));
    }
    #[derive(Deserialize)]
    struct Branch {
        name: String,
    }
    let branches: Vec<Branch> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    Ok(branches.into_iter().map(|b| b.name).collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_parse_github_url() {
        assert_eq!(
            parse_github_url("https://github.com/owner/repo"),
            Some(("owner".to_string(), "repo".to_string())),
        );
        assert_eq!(
            parse_github_url("https://github.com/owner/repo.git"),
            Some(("owner".to_string(), "repo".to_string())),
        );
        assert_eq!(
            parse_github_url("git@github.com:owner/repo.git"),
            Some(("owner".to_string(), "repo".to_string())),
        );
        assert_eq!(
            parse_github_url("github.com/owner/repo"),
            Some(("owner".to_string(), "repo".to_string())),
        );
        assert_eq!(parse_github_url("not-a-github-url"), None);
        // Spoofed domain variants must be rejected
        assert_eq!(parse_github_url("https://github.com.evil.com/owner/repo"), None);
        assert_eq!(parse_github_url("https://fakegithub.com/owner/repo"), None);
    }
}
