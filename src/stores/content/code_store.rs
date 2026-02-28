//! Code Store
//! Handles NIP-34 Git repositories, issues, PRs and NIP-C0 code snippets
#![allow(dead_code)]
use dioxus::prelude::*;
use lru::LruCache;
use nostr::Event as NostrEvent;
use nostr_sdk::prelude::*;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::time::Duration;
#[cfg(feature = "web")]
use crate::services::git_worker::GitWorkerManager;
use crate::stores::grasp_servers;
use crate::utils::nip34::{Bounty, Discussion, DisplaySnippet, Issue, IssueStatus, PersistedReview, PullRequest, Release, Repository};
const REPO_CACHE_SIZE: usize = 100;
const ISSUE_CACHE_SIZE: usize = 200;
const PR_CACHE_SIZE: usize = 200;
const SNIPPET_CACHE_SIZE: usize = 200;
const DISCUSSION_CACHE_SIZE: usize = 200;
const RELEASE_CACHE_SIZE: usize = 100;
const BOUNTY_CACHE_SIZE: usize = 200;
const REVIEW_CACHE_SIZE: usize = 200;
/// Repository cache (keyed by coordinate string "30617:pubkey:d-tag")
pub static CODE_REPOS_CACHE: GlobalSignal<LruCache<String, Repository>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(REPO_CACHE_SIZE).unwrap()));
/// Issue cache (keyed by event ID hex)
pub static CODE_ISSUES_CACHE: GlobalSignal<LruCache<String, Issue>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(ISSUE_CACHE_SIZE).unwrap()));
/// Pull Request cache (keyed by event ID hex)
pub static CODE_PRS_CACHE: GlobalSignal<LruCache<String, PullRequest>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(PR_CACHE_SIZE).unwrap()));
/// Code Snippet cache (keyed by event ID hex)
pub static CODE_SNIPPETS_CACHE: GlobalSignal<LruCache<String, DisplaySnippet>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(SNIPPET_CACHE_SIZE).unwrap()));
/// Discussion cache (keyed by event ID hex)
pub static CODE_DISCUSSIONS_CACHE: GlobalSignal<LruCache<String, Discussion>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(DISCUSSION_CACHE_SIZE).unwrap()));
/// Release cache (keyed by event ID hex)
pub static CODE_RELEASES_CACHE: GlobalSignal<LruCache<String, Release>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(RELEASE_CACHE_SIZE).unwrap()));
/// Bounty cache (keyed by issue_event_id, stores Vec<Bounty>)
pub static CODE_BOUNTIES_CACHE: GlobalSignal<LruCache<String, Vec<Bounty>>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(BOUNTY_CACHE_SIZE).unwrap()));
/// Persisted review cache (keyed by pr_event_id, stores Vec<PersistedReview>)
pub static CODE_REVIEWS_CACHE: GlobalSignal<LruCache<String, Vec<PersistedReview>>> = GlobalSignal::new(||
LruCache::new(NonZeroUsize::new(REVIEW_CACHE_SIZE).unwrap()));
/// User's own repositories (coordinate strings)
pub static USER_REPOS: GlobalSignal<Vec<String>> = GlobalSignal::new(Vec::new);
/// User's own code snippets (event IDs)
pub static USER_SNIPPETS: GlobalSignal<Vec<String>> = GlobalSignal::new(Vec::new);
/// Starred repository coordinates
pub static STARRED_REPOS: GlobalSignal<HashSet<String>> = GlobalSignal::new(
    HashSet::new,
);
/// Whether the code store has been initialized
pub static CODE_INITIALIZED: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Currently loading repositories
pub static LOADING_REPOS: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Currently loading snippets
pub static LOADING_SNIPPETS: GlobalSignal<bool> = GlobalSignal::new(|| false);
/// Get a repository from cache by coordinate string
pub fn get_cached_repo(coordinate: &str) -> Option<Repository> {
    CODE_REPOS_CACHE.read().peek(coordinate).cloned()
}
/// Cache a repository
pub fn cache_repo(repo: Repository) {
    let coord_str = format!(
        "{}:{}:{}",
        Kind::GitRepoAnnouncement.as_u16(),
        repo.pubkey,
        repo.id,
    );
    CODE_REPOS_CACHE.write().put(coord_str, repo);
}
/// Parse and cache repositories from events
pub fn cache_repo_events(events: &[NostrEvent]) {
    let mut cache = CODE_REPOS_CACHE.write();
    for event in events {
        if let Some(repo) = Repository::from_event(event) {
            for domain in repo.extract_grasp_domains() {
                grasp_servers::add_grasp_server(&domain);
            }
            let coord_str = format!(
                "{}:{}:{}",
                Kind::GitRepoAnnouncement.as_u16(),
                repo.pubkey,
                repo.id,
            );
            cache.put(coord_str, repo);
        }
    }
    #[cfg(feature = "web")]
    if GitWorkerManager::is_initialized() {
        GitWorkerManager::update_grasp_servers();
    }
}
/// Get user's repositories
pub fn get_user_repos() -> Vec<Repository> {
    let coords = USER_REPOS.read();
    let cache = CODE_REPOS_CACHE.read();
    coords.iter().filter_map(|c| cache.peek(c).cloned()).collect()
}
/// Get an issue from cache by event ID
pub fn get_cached_issue(event_id: &str) -> Option<Issue> {
    CODE_ISSUES_CACHE.read().peek(event_id).cloned()
}
/// Cache an issue
pub fn cache_issue(issue: Issue) {
    CODE_ISSUES_CACHE.write().put(issue.event_id.clone(), issue);
}
/// Parse and cache issues from events
pub fn cache_issue_events(events: &[NostrEvent]) {
    let mut cache = CODE_ISSUES_CACHE.write();
    for event in events {
        if let Some(issue) = Issue::from_event(event) {
            cache.put(issue.event_id.clone(), issue);
        }
    }
}
/// Update issue status from status events (Kind 1630-1633)
pub fn update_issue_statuses(status_events: &[NostrEvent]) {
    let mut cache = CODE_ISSUES_CACHE.write();
    for event in status_events {
        if let Some(status) = IssueStatus::from_kind(event.kind) {
            for tag in event.tags.iter() {
                if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized()
                {
                    let target_id = event_id.to_hex();
                    if let Some(issue) = cache.get_mut(&target_id) {
                        if event.created_at.as_secs() > issue.created_at {
                            issue.status = status;
                        }
                    }
                }
            }
        }
    }
}
/// Get issues for a repository by naddr
pub fn get_repo_issues(repo_naddr: &str) -> Vec<Issue> {
    let cache = CODE_ISSUES_CACHE.read();
    cache
        .iter()
        .filter_map(|(_, issue)| {
            if issue.repository_naddr == repo_naddr { Some(issue.clone()) } else { None }
        })
        .collect()
}
/// Get a PR from cache by event ID
pub fn get_cached_pr(event_id: &str) -> Option<PullRequest> {
    CODE_PRS_CACHE.read().peek(event_id).cloned()
}
/// Cache a PR
pub fn cache_pr(pr: PullRequest) {
    CODE_PRS_CACHE.write().put(pr.event_id.clone(), pr);
}
/// Parse and cache PRs from events
pub fn cache_pr_events(events: &[NostrEvent]) {
    let mut cache = CODE_PRS_CACHE.write();
    for event in events {
        if let Some(pr) = PullRequest::from_event(event) {
            cache.put(pr.event_id.clone(), pr);
        }
    }
}
/// Update PR status from status events
pub fn update_pr_statuses(status_events: &[NostrEvent]) {
    let mut cache = CODE_PRS_CACHE.write();
    for event in status_events {
        if let Some(status) = IssueStatus::from_kind(event.kind) {
            for tag in event.tags.iter() {
                if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized()
                {
                    let target_id = event_id.to_hex();
                    if let Some(pr) = cache.get_mut(&target_id) {
                        if event.created_at.as_secs() > pr.created_at {
                            pr.status = status;
                        }
                    }
                }
            }
        }
    }
}
/// Get PRs for a repository by naddr
pub fn get_repo_prs(repo_naddr: &str) -> Vec<PullRequest> {
    let cache = CODE_PRS_CACHE.read();
    cache
        .iter()
        .filter_map(|(_, pr)| {
            if pr.repository_naddr == repo_naddr { Some(pr.clone()) } else { None }
        })
        .collect()
}
/// Get a snippet from cache by event ID
pub fn get_cached_snippet(event_id: &str) -> Option<DisplaySnippet> {
    CODE_SNIPPETS_CACHE.read().peek(event_id).cloned()
}
/// Cache a snippet
pub fn cache_snippet(snippet: DisplaySnippet) {
    CODE_SNIPPETS_CACHE.write().put(snippet.event_id.clone(), snippet);
}
/// Parse and cache snippets from events
pub fn cache_snippet_events(events: &[NostrEvent]) {
    let mut cache = CODE_SNIPPETS_CACHE.write();
    for event in events {
        if let Some(snippet) = DisplaySnippet::from_event(event) {
            cache.put(snippet.event_id.clone(), snippet);
        }
    }
}
/// Get user's snippets
pub fn get_user_snippets() -> Vec<DisplaySnippet> {
    let ids = USER_SNIPPETS.read();
    let cache = CODE_SNIPPETS_CACHE.read();
    ids.iter().filter_map(|id| cache.peek(id).cloned()).collect()
}
/// Get a discussion from cache by event ID
pub fn get_cached_discussion(event_id: &str) -> Option<Discussion> {
    CODE_DISCUSSIONS_CACHE.read().peek(event_id).cloned()
}
/// Parse and cache discussions from events
pub fn cache_discussion_events(events: &[NostrEvent]) {
    let mut cache = CODE_DISCUSSIONS_CACHE.write();
    for event in events {
        if let Some(discussion) = Discussion::from_event(event) {
            cache.put(discussion.event_id.clone(), discussion);
        }
    }
}
/// Get a release from cache by event ID
pub fn get_cached_release(event_id: &str) -> Option<Release> {
    CODE_RELEASES_CACHE.read().peek(event_id).cloned()
}
/// Parse and cache releases from events
pub fn cache_release_events(events: &[NostrEvent]) {
    let mut cache = CODE_RELEASES_CACHE.write();
    for event in events {
        if let Some(release) = Release::from_event(event) {
            cache.put(release.event_id.clone(), release);
        }
    }
}
/// Get cached bounties for an issue by issue event ID
pub fn get_cached_bounties(issue_event_id: &str) -> Option<Vec<Bounty>> {
    CODE_BOUNTIES_CACHE.read().peek(issue_event_id).cloned()
}
/// Parse and cache bounties from events (grouped by issue_event_id)
pub fn cache_bounty_events(events: &[NostrEvent]) {
    use std::collections::HashMap;
    let mut grouped: HashMap<String, Vec<Bounty>> = HashMap::new();
    for event in events {
        if let Some(bounty) = Bounty::from_event(event) {
            grouped.entry(bounty.issue_event_id.clone()).or_default().push(bounty);
        }
    }
    let mut cache = CODE_BOUNTIES_CACHE.write();
    for (issue_id, new_bounties) in grouped {
        let mut merged = cache.peek(&issue_id).cloned().unwrap_or_default();
        merged.extend(new_bounties);
        merged.dedup_by(|a, b| a.event_id == b.event_id);
        cache.put(issue_id, merged);
    }
}
/// Parse and cache persisted reviews from events (grouped by pr_event_id)
pub fn cache_persisted_review_events(events: &[NostrEvent]) {
    use std::collections::HashMap;
    let mut grouped: HashMap<String, Vec<PersistedReview>> = HashMap::new();
    for event in events {
        if let Some(review) = PersistedReview::from_event(event) {
            grouped.entry(review.pr_event_id.clone()).or_default().push(review);
        }
    }
    let mut cache = CODE_REVIEWS_CACHE.write();
    for (pr_id, new_reviews) in grouped {
        let mut merged = cache.peek(&pr_id).cloned().unwrap_or_default();
        merged.extend(new_reviews);
        merged.dedup_by(|a, b| a.event_id == b.event_id);
        cache.put(pr_id, merged);
    }
}
/// Get snippets by language
pub fn get_snippets_by_language(language: &str) -> Vec<DisplaySnippet> {
    let cache = CODE_SNIPPETS_CACHE.read();
    cache
        .iter()
        .filter_map(|(_, snippet)| {
            if snippet.display_language() == Some(language) {
                Some(snippet.clone())
            } else {
                None
            }
        })
        .collect()
}
/// Check if a repo is starred
pub fn is_repo_starred(coordinate: &str) -> bool {
    STARRED_REPOS.read().contains(coordinate)
}
/// Add a repo to starred
pub fn star_repo(coordinate: &str) {
    STARRED_REPOS.write().insert(coordinate.to_string());
}
/// Remove a repo from starred
pub fn unstar_repo(coordinate: &str) {
    STARRED_REPOS.write().remove(coordinate);
}
/// Get starred repos
pub fn get_starred_repos() -> Vec<Repository> {
    let starred = STARRED_REPOS.read();
    let cache = CODE_REPOS_CACHE.read();
    starred.iter().filter_map(|c| cache.peek(c).cloned()).collect()
}
/// Watched repository coordinates (localStorage-persisted)
pub static WATCHED_REPOS: GlobalSignal<HashSet<String>> = GlobalSignal::new(
    HashSet::new,
);
/// Check if a repo is watched
pub fn is_repo_watched(coordinate: &str) -> bool {
    WATCHED_REPOS.read().contains(coordinate)
}
/// Watch a repository (adds to local set and persists to localStorage)
pub fn watch_repo(coordinate: &str) {
    WATCHED_REPOS.write().insert(coordinate.to_string());
    persist_watched_repos();
}
/// Unwatch a repository
pub fn unwatch_repo(coordinate: &str) {
    WATCHED_REPOS.write().remove(coordinate);
    persist_watched_repos();
}
/// Get watched repos
pub fn get_watched_repos() -> Vec<Repository> {
    let watched = WATCHED_REPOS.read();
    let cache = CODE_REPOS_CACHE.read();
    watched.iter().filter_map(|c| cache.peek(c).cloned()).collect()
}
/// Load watched repos from localStorage
pub fn load_watched_repos() {
    #[cfg(feature = "web")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                if let Ok(Some(watched_json)) = storage
                    .get_item("nostr_blue_watched_repos")
                {
                    if let Ok(watched) = serde_json::from_str::<
                        Vec<String>,
                    >(&watched_json) {
                        let mut signal = WATCHED_REPOS.write();
                        signal.clear();
                        for coord in watched {
                            signal.insert(coord);
                        }
                    }
                }
            }
        }
    }
}
/// Persist watched repos to localStorage
fn persist_watched_repos() {
    #[cfg(feature = "web")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let watched: Vec<String> = WATCHED_REPOS
                    .read()
                    .iter()
                    .cloned()
                    .collect();
                if let Ok(json) = serde_json::to_string(&watched) {
                    let _ = storage.set_item("nostr_blue_watched_repos", &json);
                }
            }
        }
    }
}
/// Forked repository coordinates (localStorage-persisted)
pub static FORKED_REPOS: GlobalSignal<HashSet<String>> = GlobalSignal::new(HashSet::new);
/// Check if user has forked a repo
pub fn is_repo_forked(coordinate: &str) -> bool {
    FORKED_REPOS.read().contains(coordinate)
}
/// Track a fork (when user forks a repo)
pub fn track_fork(coordinate: &str) {
    FORKED_REPOS.write().insert(coordinate.to_string());
    persist_forked_repos();
}
/// Load forked repos from localStorage
pub fn load_forked_repos() {
    #[cfg(feature = "web")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                if let Ok(Some(forked_json)) = storage
                    .get_item("nostr_blue_forked_repos")
                {
                    if let Ok(forked) = serde_json::from_str::<
                        Vec<String>,
                    >(&forked_json) {
                        let mut signal = FORKED_REPOS.write();
                        signal.clear();
                        for coord in forked {
                            signal.insert(coord);
                        }
                    }
                }
            }
        }
    }
}
/// Persist forked repos to localStorage
fn persist_forked_repos() {
    #[cfg(feature = "web")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let forked: Vec<String> = FORKED_REPOS.read().iter().cloned().collect();
                if let Ok(json) = serde_json::to_string(&forked) {
                    let _ = storage.set_item("nostr_blue_forked_repos", &json);
                }
            }
        }
    }
}
const README_CACHE_SIZE: usize = 50;
/// README content cache (keyed by coordinate string)
pub static README_CACHE: GlobalSignal<LruCache<String, String>> = GlobalSignal::new(|| LruCache::new(
    NonZeroUsize::new(README_CACHE_SIZE).unwrap(),
));
/// Get cached README content
pub fn get_cached_readme(coordinate: &str) -> Option<String> {
    README_CACHE.read().peek(coordinate).cloned()
}
/// Cache README content
pub fn cache_readme(coordinate: &str, content: String) {
    README_CACHE.write().put(coordinate.to_string(), content);
}
/// Build filter for fetching repositories by author
pub fn repo_filter_by_author(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new().kind(Kind::GitRepoAnnouncement).author(pubkey).limit(limit)
}
/// Build filter for fetching a specific repository
pub fn repo_filter_by_coordinate(pubkey: PublicKey, identifier: &str) -> Filter {
    Filter::new().kind(Kind::GitRepoAnnouncement).author(pubkey).identifier(identifier)
}
/// Build filter for fetching issues for a repository
pub fn issues_filter_by_repo(coordinate: &Coordinate) -> Filter {
    Filter::new()
        .kind(Kind::GitIssue)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), coordinate.to_string())
}
/// Build filter for fetching PRs for a repository
pub fn prs_filter_by_repo(coordinate: &Coordinate) -> Filter {
    Filter::new()
        .kind(Kind::GitPatch)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), coordinate.to_string())
}
/// Build filter for fetching status events for an issue/PR
pub fn status_filter_for_event(event_id: EventId) -> Filter {
    Filter::new()
        .kinds(
            vec![
                Kind::GitStatusOpen,
                Kind::GitStatusApplied,
                Kind::GitStatusClosed,
                Kind::GitStatusDraft,
            ],
        )
        .event(event_id)
}
/// Build filter for fetching code snippets by author
pub fn snippets_filter_by_author(pubkey: PublicKey, limit: usize) -> Filter {
    Filter::new().kind(Kind::CodeSnippet).author(pubkey).limit(limit)
}
/// Build filter for recent code snippets
pub fn recent_snippets_filter(limit: usize) -> Filter {
    Filter::new().kind(Kind::CodeSnippet).limit(limit)
}
/// Build filter for stars (reactions) on a repository
pub fn stars_filter_for_repo(coordinate: &Coordinate) -> Filter {
    Filter::new()
        .kind(Kind::Reaction)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), coordinate.to_string())
}
/// Initialize the code store for a user
pub async fn initialize_for_user(pubkey: &PublicKey) {
    if *CODE_INITIALIZED.read() {
        return;
    }
    *LOADING_REPOS.write() = true;
    *LOADING_SNIPPETS.write() = true;
    let repo_filter = repo_filter_by_author(*pubkey, 50);
    if let Ok(events) = crate::stores::nostr_client::fetch_events_aggregated(
            repo_filter,
            Duration::from_secs(10),
        )
        .await
    {
        cache_repo_events(&events);
        let mut user_repos = USER_REPOS.write();
        user_repos.clear();
        for event in &events {
            if let Some(repo) = Repository::from_event(event) {
                let coord = format!(
                    "{}:{}:{}",
                    Kind::GitRepoAnnouncement.as_u16(),
                    repo.pubkey,
                    repo.id,
                );
                user_repos.push(coord);
            }
        }
    }
    *LOADING_REPOS.write() = false;
    let snippet_filter = snippets_filter_by_author(*pubkey, 50);
    if let Ok(events) = crate::stores::nostr_client::fetch_events_aggregated(
            snippet_filter,
            Duration::from_secs(10),
        )
        .await
    {
        cache_snippet_events(&events);
        let mut user_snippets = USER_SNIPPETS.write();
        user_snippets.clear();
        for event in &events {
            if event.kind == Kind::CodeSnippet {
                user_snippets.push(event.id.to_hex());
            }
        }
    }
    *LOADING_SNIPPETS.write() = false;
    *CODE_INITIALIZED.write() = true;
}
/// Clear all caches (e.g., on logout)
pub fn clear_caches() {
    CODE_REPOS_CACHE.write().clear();
    CODE_ISSUES_CACHE.write().clear();
    CODE_PRS_CACHE.write().clear();
    CODE_SNIPPETS_CACHE.write().clear();
    CODE_DISCUSSIONS_CACHE.write().clear();
    CODE_RELEASES_CACHE.write().clear();
    CODE_BOUNTIES_CACHE.write().clear();
    CODE_REVIEWS_CACHE.write().clear();
    USER_REPOS.write().clear();
    USER_SNIPPETS.write().clear();
    STARRED_REPOS.write().clear();
    WATCHED_REPOS.write().clear();
    FORKED_REPOS.write().clear();
    README_CACHE.write().clear();
    *CODE_INITIALIZED.write() = false;
}
