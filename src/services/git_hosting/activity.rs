//! Code Activity Service
//!
//! Tracks user activity in the code section by querying their authored events.
//! Queries existing Nostr events authored by a user to build an activity timeline
//! across repos created, issues filed, PRs submitted, reviews given, etc.
#![allow(dead_code)]
use nostr_sdk::prelude::*;
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use crate::stores::auth_store;
use crate::stores::nostr_client::fetch_events_aggregated;
use crate::utils::nip34::Repository;

/// Default timeout for fetching activity events
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Types of code activities
#[derive(Debug, Clone, PartialEq)]
pub enum ActivityType {
    RepoCreated,
    IssueOpened,
    PullRequestOpened,
    ReviewSubmitted,
    CommentPosted,
    StatusChanged,
    SnippetCreated,
}

impl fmt::Display for ActivityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RepoCreated => write!(f, "Created repository"),
            Self::IssueOpened => write!(f, "Opened issue"),
            Self::PullRequestOpened => write!(f, "Opened pull request"),
            Self::ReviewSubmitted => write!(f, "Submitted review"),
            Self::CommentPosted => write!(f, "Posted comment"),
            Self::StatusChanged => write!(f, "Changed status"),
            Self::SnippetCreated => write!(f, "Created snippet"),
        }
    }
}

/// A single activity entry
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub activity_type: ActivityType,
    pub title: String,
    pub event_id: String,
    pub author: String,
    pub repository_naddr: Option<String>,
    pub created_at: u64,
}

impl Activity {
    /// Parse an event into an Activity, returning None if the kind is unrecognized.
    pub fn from_event(event: &Event) -> Option<Self> {
        let kind = event.kind;

        let (activity_type, title) = if kind == Kind::GitRepoAnnouncement {
            let name = event
                .tags
                .iter()
                .find_map(|t| {
                    let v = t.as_slice();
                    if v.len() >= 2 && v[0] == "name" {
                        Some(v[1].to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "Untitled Repo".to_string());
            (ActivityType::RepoCreated, name)
        } else if kind == Kind::GitIssue {
            let subject = event
                .tags
                .iter()
                .find_map(|t| {
                    let v = t.as_slice();
                    if v.len() >= 2 && v[0] == "subject" {
                        Some(v[1].to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "Untitled Issue".to_string());
            (ActivityType::IssueOpened, subject)
        } else if kind == Kind::GitPatch {
            let subject = event
                .tags
                .iter()
                .find_map(|t| {
                    let v = t.as_slice();
                    if v.len() >= 2 && v[0] == "subject" {
                        Some(v[1].to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "Untitled PR".to_string());
            (ActivityType::PullRequestOpened, subject)
        } else if kind == Kind::Custom(9807) {
            (ActivityType::ReviewSubmitted, "Code review".to_string())
        } else if kind == Kind::Comment {
            (ActivityType::CommentPosted, "Comment".to_string())
        } else if kind == Kind::GitStatusOpen
            || kind == Kind::GitStatusApplied
            || kind == Kind::GitStatusClosed
            || kind == Kind::GitStatusDraft
        {
            (ActivityType::StatusChanged, "Status update".to_string())
        } else if kind == Kind::CodeSnippet {
            let title_tag = event
                .tags
                .iter()
                .find_map(|t| {
                    let v = t.as_slice();
                    if v.len() >= 2 && v[0] == "title" {
                        Some(v[1].to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "Code Snippet".to_string());
            (ActivityType::SnippetCreated, title_tag)
        } else {
            return None;
        };

        let repo_naddr = event.tags.iter().find_map(|t| {
            let v = t.as_slice();
            if v.len() >= 2 && v[0] == "a" && v[1].starts_with("30617:") {
                Some(v[1].to_string())
            } else {
                None
            }
        });

        Some(Self {
            activity_type,
            title,
            event_id: event.id.to_hex(),
            author: event.pubkey.to_hex(),
            repository_naddr: repo_naddr,
            created_at: event.created_at.as_secs(),
        })
    }
}

/// Fetch recent activity for a user
pub async fn fetch_user_activity(
    pubkey: &PublicKey,
    limit: usize,
) -> Result<Vec<Activity>, String> {
    let filter = Filter::new()
        .kinds(vec![
            Kind::GitRepoAnnouncement,
            Kind::GitIssue,
            Kind::GitPatch,
            Kind::Custom(9807),
            Kind::Comment,
            Kind::GitStatusOpen,
            Kind::GitStatusApplied,
            Kind::GitStatusClosed,
            Kind::GitStatusDraft,
            Kind::CodeSnippet,
        ])
        .author(*pubkey)
        .limit(limit);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch activity: {e}"))?;

    let mut activities: Vec<Activity> = events.iter().filter_map(Activity::from_event).collect();

    activities.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    activities.truncate(limit);
    Ok(activities)
}

/// Weekly contribution data for heatmap
#[derive(Debug, Clone, PartialEq)]
pub struct ContributionWeek {
    pub days: [u32; 7], // Sun=0 through Sat=6 counts
    pub week_start: u64, // Unix timestamp of the Sunday starting this week
}

/// Fetch 52 weeks of contribution data for a user's code activity.
///
/// Returns exactly 52 `ContributionWeek` entries (oldest first) covering
/// the past year of code-related Nostr events.
pub async fn fetch_contribution_graph(pubkey: &PublicKey) -> Result<Vec<ContributionWeek>, String> {
    let now = Timestamp::now().as_secs();
    let seconds_per_day: u64 = 86400;
    let seconds_per_week: u64 = 7 * seconds_per_day;

    // Find the start of the current week (Sunday 00:00 UTC)
    // Nostr timestamps are UTC; day-of-week: Thu 1 Jan 1970 = day 4
    let days_since_epoch = now / seconds_per_day;
    let current_weekday = (days_since_epoch + 4) % 7; // 0=Sun
    let current_week_start = (days_since_epoch - current_weekday) * seconds_per_day;

    // Go back 51 more weeks to get 52 total
    let graph_start = current_week_start - 51 * seconds_per_week;

    let filter = Filter::new()
        .kinds(vec![
            Kind::GitRepoAnnouncement,
            Kind::GitIssue,
            Kind::GitPatch,
            Kind::Custom(9807),
            Kind::Comment,
            Kind::GitStatusOpen,
            Kind::GitStatusApplied,
            Kind::GitStatusClosed,
            Kind::GitStatusDraft,
            Kind::CodeSnippet,
        ])
        .author(*pubkey)
        .since(Timestamp::from(graph_start));

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch contribution data: {e}"))?;

    // Initialize 52 weeks
    let mut weeks: Vec<ContributionWeek> = (0..52)
        .map(|i| ContributionWeek {
            days: [0; 7],
            week_start: graph_start + i as u64 * seconds_per_week,
        })
        .collect();

    // Bucket each event into its week/day
    for event in &events {
        let ts = event.created_at.as_secs();
        if ts < graph_start {
            continue;
        }
        let offset = ts - graph_start;
        let week_index = (offset / seconds_per_week) as usize;
        if week_index >= 52 {
            continue;
        }
        let day_in_week = ((offset % seconds_per_week) / seconds_per_day) as usize;
        if day_in_week < 7 {
            weeks[week_index].days[day_in_week] += 1;
        }
    }

    Ok(weeks)
}

/// Fetch activity for the current authenticated user
pub async fn fetch_my_activity(limit: usize) -> Result<Vec<Activity>, String> {
    let pubkey_hex = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey =
        PublicKey::from_hex(&pubkey_hex).map_err(|e| format!("Invalid public key: {e}"))?;
    fetch_user_activity(&pubkey, limit).await
}

/// A developer's contribution summary for ranking
#[derive(Debug, Clone, PartialEq)]
pub struct TopDeveloper {
    pub pubkey: String,
    pub contribution_count: usize,
}

/// Platform-wide stats
#[derive(Debug, Clone, PartialEq)]
pub struct PlatformStats {
    pub total_repos: usize,
    pub total_issues: usize,
    pub total_prs: usize,
    pub total_snippets: usize,
}

/// Fetch trending repositories (most starred in last 7 days)
///
/// Fetches recent repository announcements from the last 7 days and
/// sorts them by star_count descending to surface popular repos.
pub async fn fetch_trending_repositories(limit: usize) -> Result<Vec<Repository>, String> {
    let now = Timestamp::now().as_secs();
    let seven_days_ago = now.saturating_sub(7 * 24 * 60 * 60);

    let filter = Filter::new()
        .kind(Kind::GitRepoAnnouncement)
        .since(Timestamp::from(seven_days_ago))
        .limit(200);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch trending repositories: {e}"))?;

    let mut repos: Vec<Repository> = events.iter().filter_map(Repository::from_event).collect();

    // Sort by star count descending, then by creation time descending as tiebreaker
    repos.sort_by(|a, b| {
        b.star_count
            .cmp(&a.star_count)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    repos.truncate(limit);
    Ok(repos)
}

/// Fetch top developers by contribution count (last 30 days)
///
/// Groups all code-related events by author and ranks developers by
/// total event count across repos, issues, PRs, reviews, and snippets.
pub async fn fetch_top_developers(limit: usize) -> Result<Vec<TopDeveloper>, String> {
    let now = Timestamp::now().as_secs();
    let thirty_days_ago = now.saturating_sub(30 * 24 * 60 * 60);

    let filter = Filter::new()
        .kinds(vec![
            Kind::GitRepoAnnouncement,
            Kind::GitIssue,
            Kind::GitPatch,
            Kind::Custom(9807),
            Kind::Comment,
            Kind::GitStatusOpen,
            Kind::GitStatusApplied,
            Kind::GitStatusClosed,
            Kind::GitStatusDraft,
            Kind::CodeSnippet,
        ])
        .since(Timestamp::from(thirty_days_ago))
        .limit(500);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch developer activity: {e}"))?;

    let mut counts: HashMap<String, usize> = HashMap::new();
    for event in &events {
        *counts.entry(event.pubkey.to_hex()).or_insert(0) += 1;
    }

    let mut developers: Vec<TopDeveloper> = counts
        .into_iter()
        .map(|(pubkey, contribution_count)| TopDeveloper {
            pubkey,
            contribution_count,
        })
        .collect();

    developers.sort_by(|a, b| b.contribution_count.cmp(&a.contribution_count));
    developers.truncate(limit);
    Ok(developers)
}

/// Fetch recent global activity (all users)
///
/// Same as `fetch_user_activity` but without an author filter, returning
/// code activity from all users across the network.
pub async fn fetch_recent_global_activity(limit: usize) -> Result<Vec<Activity>, String> {
    let filter = Filter::new()
        .kinds(vec![
            Kind::GitRepoAnnouncement,
            Kind::GitIssue,
            Kind::GitPatch,
            Kind::Custom(9807),
            Kind::Comment,
            Kind::GitStatusOpen,
            Kind::GitStatusApplied,
            Kind::GitStatusClosed,
            Kind::GitStatusDraft,
            Kind::CodeSnippet,
        ])
        .limit(limit);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch global activity: {e}"))?;

    let mut activities: Vec<Activity> = events.iter().filter_map(Activity::from_event).collect();

    activities.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    activities.truncate(limit);
    Ok(activities)
}

/// Fetch platform-wide stats
///
/// Counts total repositories, issues, PRs, and snippets
/// across the entire network.
pub async fn fetch_platform_stats() -> Result<PlatformStats, String> {
    // Fetch counts for each category with small limits to be efficient
    let repo_filter = Filter::new()
        .kind(Kind::GitRepoAnnouncement)
        .limit(1000);
    let issue_filter = Filter::new().kind(Kind::GitIssue).limit(1000);
    let pr_filter = Filter::new().kind(Kind::GitPatch).limit(1000);
    let snippet_filter = Filter::new().kind(Kind::CodeSnippet).limit(1000);

    let timeout = Duration::from_secs(10);

    let (repos, issues, prs, snippets) = futures::join!(
        fetch_events_aggregated(repo_filter, timeout),
        fetch_events_aggregated(issue_filter, timeout),
        fetch_events_aggregated(pr_filter, timeout),
        fetch_events_aggregated(snippet_filter, timeout),
    );

    Ok(PlatformStats {
        total_repos: repos.map(|e| e.len()).unwrap_or(0),
        total_issues: issues.map(|e| e.len()).unwrap_or(0),
        total_prs: prs.map(|e| e.len()).unwrap_or(0),
        total_snippets: snippets.map(|e| e.len()).unwrap_or(0),
    })
}
