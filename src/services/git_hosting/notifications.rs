//! Code Notification Service
//!
//! Tracks code-related events for user notifications.
#![allow(dead_code)]
use nostr_sdk::prelude::*;
use std::fmt;
use std::time::Duration;

use crate::stores::auth_store;
use crate::stores::nostr_client::fetch_events_aggregated;

/// Default timeout for fetching notification events
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Types of code notifications
#[derive(Debug, Clone, PartialEq)]
pub enum CodeNotificationType {
    IssueOpened,
    PullRequestOpened,
    ReviewReceived,
    Mentioned,
    StatusChanged,
    CommentAdded,
}

impl fmt::Display for CodeNotificationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IssueOpened => write!(f, "New Issue"),
            Self::PullRequestOpened => write!(f, "New Pull Request"),
            Self::ReviewReceived => write!(f, "Review Received"),
            Self::Mentioned => write!(f, "Mentioned"),
            Self::StatusChanged => write!(f, "Status Changed"),
            Self::CommentAdded => write!(f, "New Comment"),
        }
    }
}

/// A code notification
#[derive(Debug, Clone, PartialEq)]
pub struct CodeNotification {
    pub notification_type: CodeNotificationType,
    pub title: String,
    pub summary: String,
    pub event_id: String,
    pub author_pubkey: String,
    pub repository_naddr: Option<String>,
    pub created_at: u64,
    pub read: bool,
}

impl CodeNotification {
    /// Create from a Nostr event
    pub fn from_event(event: &Event, user_pubkey: &str) -> Option<Self> {
        let kind = event.kind;
        let (notification_type, title) = if kind == Kind::GitIssue {
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
            (CodeNotificationType::IssueOpened, subject)
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
            (CodeNotificationType::PullRequestOpened, subject)
        } else if kind == Kind::Custom(9807) {
            (
                CodeNotificationType::ReviewReceived,
                "Review on your PR".to_string(),
            )
        } else if kind == Kind::Comment {
            // Check if user is mentioned via p-tag
            let is_mention = event.tags.iter().any(|t| {
                let v = t.as_slice();
                v.len() >= 2 && v[0] == "p" && v[1] == user_pubkey
            });
            if is_mention {
                (
                    CodeNotificationType::Mentioned,
                    "You were mentioned".to_string(),
                )
            } else {
                (
                    CodeNotificationType::CommentAdded,
                    "New comment".to_string(),
                )
            }
        } else if kind == Kind::GitStatusOpen
            || kind == Kind::GitStatusApplied
            || kind == Kind::GitStatusClosed
            || kind == Kind::GitStatusDraft
        {
            (
                CodeNotificationType::StatusChanged,
                "Status updated".to_string(),
            )
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

        let content = event.content.to_string();
        let summary = if content.chars().count() > 100 {
            format!("{}...", content.chars().take(100).collect::<String>())
        } else {
            content
        };

        Some(Self {
            notification_type,
            title,
            summary,
            event_id: event.id.to_hex(),
            author_pubkey: event.pubkey.to_hex(),
            repository_naddr: repo_naddr,
            created_at: event.created_at.as_secs(),
            read: false,
        })
    }
}

/// Fetch code notifications for the current user.
///
/// Looks for events that tag the user via p-tag (mentions, reviews, etc.)
/// across code-related event kinds.
pub async fn fetch_code_notifications(limit: usize) -> Result<Vec<CodeNotification>, String> {
    let pubkey_hex = auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pubkey =
        PublicKey::from_hex(&pubkey_hex).map_err(|e| format!("Invalid pubkey: {}", e))?;

    // Fetch events that tag the user via p-tag (mentions, reviews, etc.)
    let filter = Filter::new()
        .kinds(vec![
            Kind::GitIssue,
            Kind::GitPatch,
            Kind::Custom(9807),
            Kind::Comment,
            Kind::GitStatusOpen,
            Kind::GitStatusApplied,
            Kind::GitStatusClosed,
            Kind::GitStatusDraft,
        ])
        .custom_tag(SingleLetterTag::lowercase(Alphabet::P), pubkey.to_hex())
        .limit(limit);

    let events = fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch notifications: {}", e))?;

    let mut notifications: Vec<CodeNotification> = events
        .iter()
        .filter(|e| e.pubkey.to_hex() != pubkey_hex) // Exclude own events
        .filter_map(|e| CodeNotification::from_event(e, &pubkey_hex))
        .collect();

    notifications.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    notifications.truncate(limit);
    Ok(notifications)
}
