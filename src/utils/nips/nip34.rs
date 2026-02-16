//! NIP-34: Git Stuff & NIP-C0: Code Snippets
//!
//! Types and utilities for decentralized Git hosting and code snippets on Nostr.
//! Uses rust-nostr SDK's built-in NIP-34 and NIP-C0 support for event building,
//! with custom parsing for display/caching.
#![allow(dead_code)]
use nostr_sdk::prelude::*;
/// Error type for NIP-34/NIP-C0 operations
pub type Nip34Error = nostr::nips::nip19::Error;
/// Issue/PR status derived from status events (Kind 1630-1633)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IssueStatus {
    #[default]
    Open,
    Applied,
    Closed,
    Draft,
}
impl IssueStatus {
    /// Get status from a Kind value
    pub fn from_kind(kind: Kind) -> Option<Self> {
        match kind {
            Kind::GitStatusOpen => Some(Self::Open),
            Kind::GitStatusApplied => Some(Self::Applied),
            Kind::GitStatusClosed => Some(Self::Closed),
            Kind::GitStatusDraft => Some(Self::Draft),
            _ => None,
        }
    }
    /// Get the Kind for this status
    #[allow(clippy::wrong_self_convention)]
    pub fn to_kind(&self) -> Kind {
        match self {
            Self::Open => Kind::GitStatusOpen,
            Self::Applied => Kind::GitStatusApplied,
            Self::Closed => Kind::GitStatusClosed,
            Self::Draft => Kind::GitStatusDraft,
        }
    }
    /// Human-readable label
    pub fn label(&self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Applied => "Applied",
            Self::Closed => "Closed",
            Self::Draft => "Draft",
        }
    }
    /// CSS class for styling
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Open => "status-open",
            Self::Applied => "status-applied",
            Self::Closed => "status-closed",
            Self::Draft => "status-draft",
        }
    }
    /// Tailwind background color class for status indicators
    pub fn bg_class(&self) -> &'static str {
        match self {
            Self::Open => "bg-green-500",
            Self::Applied => "bg-purple-500",
            Self::Closed => "bg-red-500",
            Self::Draft => "bg-gray-500",
        }
    }
}
/// Repository parsed from a Kind 30617 event
#[derive(Debug, Clone, PartialEq)]
pub struct Repository {
    /// Repository identifier (d-tag)
    pub id: String,
    /// Human-readable project name
    pub name: Option<String>,
    /// Brief description
    pub description: Option<String>,
    /// Web URLs for the project
    pub web: Vec<String>,
    /// Git clone URLs
    pub clone: Vec<String>,
    /// Relays that monitor this repo
    pub relays: Vec<String>,
    /// Maintainer pubkeys (hex)
    pub maintainers: Vec<String>,
    /// Earliest unique commit (for fork identification)
    pub euc: Option<String>,
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// NIP-19 naddr for this repo
    pub naddr: String,
    /// Created timestamp
    pub created_at: u64,
    pub star_count: u32,
    pub issue_count: u32,
    pub pr_count: u32,
    /// Forked from this repository coordinate (naddr)
    pub fork_of: Option<String>,
    /// Required number of approvals before merge (0 = no requirement)
    pub required_approvals: u32,
    /// Repository topics/tags (from hashtag tags)
    pub topics: Vec<String>,
}
impl Repository {
    /// Parse a Repository from a Kind 30617 event
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Kind::GitRepoAnnouncement {
            return None;
        }
        let mut id = None;
        let mut name = None;
        let mut description = None;
        let mut web = Vec::new();
        let mut clone = Vec::new();
        let mut relays = Vec::new();
        let mut maintainers = Vec::new();
        let mut euc = None;
        let mut fork_of = None;
        let mut required_approvals = 0u32;
        let mut topics = Vec::new();
        for tag in event.tags.iter() {
            let tag_vec = tag.as_slice();
            // Parse custom tags first
            if tag_vec.len() >= 3 && tag_vec[0] == "e" && tag_vec.get(3).map(|s| s.as_str()) == Some("fork") {
                fork_of = Some(tag_vec[1].to_string());
                continue;
            }
            if tag_vec.len() >= 2 && tag_vec[0] == "required-approvals" {
                required_approvals = tag_vec[1].parse().unwrap_or(0);
                continue;
            }
            match tag.as_standardized() {
                Some(TagStandard::Identifier(d)) => id = Some(d.clone()),
                Some(TagStandard::Name(n)) => name = Some(n.clone()),
                Some(TagStandard::Description(d)) => description = Some(d.clone()),
                Some(TagStandard::Web(urls)) => {
                    web = urls.iter().map(|u| u.to_string()).collect();
                }
                Some(TagStandard::GitClone(urls)) => {
                    clone = urls.iter().map(|u| u.to_string()).collect();
                }
                Some(TagStandard::Relays(rs)) => {
                    relays = rs.iter().map(|r| r.to_string()).collect();
                }
                Some(TagStandard::GitMaintainers(pks)) => {
                    maintainers = pks.iter().map(|pk| pk.to_hex()).collect();
                }
                Some(TagStandard::GitEarliestUniqueCommitId(hash)) => {
                    euc = Some(hash.to_string());
                }
                Some(TagStandard::Hashtag(t)) => topics.push(t.clone()),
                _ => {}
            }
        }
        let id = id?;
        let coordinate = Coordinate::new(Kind::GitRepoAnnouncement, event.pubkey)
            .identifier(&id);
        let naddr = Nip19Coordinate::new(coordinate, vec![])
            .to_bech32()
            .unwrap_or_default();
        Some(Self {
            id,
            name,
            description,
            web,
            clone,
            relays,
            maintainers,
            euc,
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            naddr,
            created_at: event.created_at.as_secs(),
            star_count: 0,
            issue_count: 0,
            pr_count: 0,
            fork_of,
            required_approvals,
            topics,
        })
    }
    /// Get display name (name or id)
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
    /// Get primary clone URL
    pub fn primary_clone_url(&self) -> Option<&str> {
        self.clone.first().map(|s| s.as_str())
    }
    /// Get primary web URL
    pub fn primary_web_url(&self) -> Option<&str> {
        self.web.first().map(|s| s.as_str())
    }
    /// Check if this is a GitHub repo
    pub fn is_github(&self) -> bool {
        self.clone.iter().any(|url| url.contains("github.com"))
    }
    /// Check if this is a GitLab repo
    pub fn is_gitlab(&self) -> bool {
        self.clone.iter().any(|url| url.contains("gitlab.com"))
    }
    /// Check if this is a Codeberg repo
    pub fn is_codeberg(&self) -> bool {
        self.clone.iter().any(|url| url.contains("codeberg.org"))
    }
    /// Get truncated pubkey for display
    pub fn pubkey_display(&self) -> String {
        if self.pubkey.len() > 12 {
            format!("{}...{}", &self.pubkey[..6], &self.pubkey[self.pubkey.len() - 4..])
        } else {
            self.pubkey.clone()
        }
    }
    /// Extract GRASP server domains from clone URLs
    ///
    /// A domain is considered a GRASP server if it appears in both:
    /// - `clone` tags (as https://[domain]/...) AND
    /// - `relays` tags (as wss://[domain])
    ///
    /// This follows NIP-34 convention for self-hosted git servers.
    pub fn extract_grasp_domains(&self) -> Vec<String> {
        use ::url::Url;
        use std::collections::HashSet;
        let relay_domains: HashSet<String> = self
            .relays
            .iter()
            .filter_map(|r| Url::parse(r).ok())
            .filter_map(|u| u.domain().map(|d| d.to_string()))
            .collect();
        self.clone
            .iter()
            .filter_map(|url| Url::parse(url).ok())
            .filter_map(|u| u.domain().map(|d| d.to_string()))
            .filter(|domain| relay_domains.contains(domain))
            .collect()
    }
}
/// Issue parsed from a Kind 1621 event
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// Repository naddr this issue belongs to
    pub repository_naddr: String,
    /// Issue content (markdown)
    pub content: String,
    /// Subject/title
    pub subject: Option<String>,
    /// Labels (from t tags)
    pub labels: Vec<String>,
    /// Assignee pubkeys (hex, from p tags)
    pub assignees: Vec<String>,
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Created timestamp
    pub created_at: u64,
    /// Status (derived from status events)
    pub status: IssueStatus,
    /// Comment count
    pub comment_count: u32,
}
impl Issue {
    /// Parse an Issue from a Kind 1621 event
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Kind::GitIssue {
            return None;
        }
        let mut repository = None;
        let mut subject = None;
        let mut labels = Vec::new();
        let mut assignees = Vec::new();
        for tag in event.tags.iter() {
            match tag.as_standardized() {
                Some(TagStandard::Coordinate { coordinate, .. }) => {
                    repository = Some(coordinate.clone());
                }
                Some(TagStandard::Subject(s)) => subject = Some(s.clone()),
                Some(TagStandard::Hashtag(t)) => labels.push(t.clone()),
                Some(TagStandard::PublicKey { public_key, .. }) => {
                    // p tags on issues are assignees (not the repo owner tag)
                    let hex = public_key.to_hex();
                    if !assignees.contains(&hex) {
                        assignees.push(hex);
                    }
                }
                _ => {}
            }
        }
        let repository = repository?;
        // Remove the repo owner from assignees (they're tagged for routing, not assignment)
        assignees.retain(|pk| *pk != repository.public_key.to_hex());
        let repository_naddr = Nip19Coordinate::new(repository, vec![])
            .to_bech32()
            .unwrap_or_default();
        Some(Self {
            repository_naddr,
            content: event.content.clone(),
            subject,
            labels,
            assignees,
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            created_at: event.created_at.as_secs(),
            status: IssueStatus::Open,
            comment_count: 0,
        })
    }
    /// Get display title (subject or truncated content)
    pub fn display_title(&self) -> String {
        if let Some(subject) = &self.subject {
            subject.clone()
        } else {
            let first_line = self.content.lines().next().unwrap_or("");
            if first_line.chars().count() > 80 {
                format!("{}...", first_line.chars().take(77).collect::<String>())
            } else {
                first_line.to_string()
            }
        }
    }
    /// Get truncated pubkey for display
    pub fn pubkey_display(&self) -> String {
        if self.pubkey.len() > 12 {
            format!("{}...{}", &self.pubkey[..6], &self.pubkey[self.pubkey.len() - 4..])
        } else {
            self.pubkey.clone()
        }
    }
    /// Get short event ID for display
    pub fn event_id_short(&self) -> String {
        self.event_id.chars().take(8).collect()
    }
}
/// Pull Request (Patch) parsed from a Kind 1617 event
#[derive(Debug, Clone, PartialEq)]
pub struct PullRequest {
    /// Repository naddr this PR belongs to
    pub repository_naddr: String,
    /// Patch content or cover letter description
    pub content: String,
    /// Commit hash (for patches)
    pub commit: Option<String>,
    /// Parent commit hash
    pub parent_commit: Option<String>,
    /// Labels (from t tags)
    pub labels: Vec<String>,
    /// Whether this is a cover letter
    pub is_cover_letter: bool,
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Created timestamp
    pub created_at: u64,
    /// Status (derived from status events)
    pub status: IssueStatus,
    /// Comment count
    pub comment_count: u32,
    /// Event IDs of issues this PR closes
    pub linked_issues: Vec<String>,
    /// Source branch name
    pub branch_name: Option<String>,
    /// Clone URLs for this PR's source
    pub clone_urls: Vec<String>,
}
impl PullRequest {
    /// Parse a PullRequest from a Kind 1617 event
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Kind::GitPatch {
            return None;
        }
        let mut repository = None;
        let mut commit = None;
        let mut parent_commit = None;
        let mut labels = Vec::new();
        let mut is_cover_letter = false;
        let mut linked_issues = Vec::new();
        let mut branch_name = None;
        let mut clone_urls = Vec::new();
        for tag in event.tags.iter() {
            let tag_vec = tag.as_slice();
            // Check for "closes" marker on e-tags (linked issues)
            if tag_vec.len() >= 4 && tag_vec[0] == "e" && tag_vec[3] == "closes" {
                linked_issues.push(tag_vec[1].to_string());
                continue;
            }
            match tag.as_standardized() {
                Some(TagStandard::Coordinate { coordinate, .. }) => {
                    repository = Some(coordinate.clone());
                }
                Some(TagStandard::GitCommit(hash)) => {
                    commit = Some(hash.to_string());
                }
                Some(TagStandard::Hashtag(t)) => {
                    if t == "cover-letter" {
                        is_cover_letter = true;
                    } else {
                        labels.push(t.clone());
                    }
                }
                Some(TagStandard::GitClone(urls)) => {
                    clone_urls = urls.iter().map(|u| u.to_string()).collect();
                }
                _ => {}
            }
            // Parse branch name (custom tag)
            if tag_vec.len() >= 2 && tag_vec[0] == "branch" {
                branch_name = Some(tag_vec[1].to_string());
            }
            if tag.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("parent-commit"))
            {
                if let Some(hash) = tag.content() {
                    parent_commit = Some(hash.to_string());
                }
            }
        }
        let repository = repository?;
        let repository_naddr = Nip19Coordinate::new(repository, vec![])
            .to_bech32()
            .unwrap_or_default();
        Some(Self {
            repository_naddr,
            content: event.content.clone(),
            commit,
            parent_commit,
            labels,
            is_cover_letter,
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            created_at: event.created_at.as_secs(),
            status: IssueStatus::Open,
            comment_count: 0,
            linked_issues,
            branch_name,
            clone_urls,
        })
    }
    /// Get display title from content (first line or subject)
    pub fn display_title(&self) -> String {
        for line in self.content.lines() {
            if let Some(subject) = line.strip_prefix("Subject: ") {
                let cleaned = if let Some(idx) = subject.find(']') {
                    subject[idx + 1..].trim()
                } else {
                    subject
                };
                return cleaned.to_string();
            }
        }
        let first_line = self.content.lines().next().unwrap_or("Untitled PR");
        if first_line.chars().count() > 80 {
            format!("{}...", first_line.chars().take(77).collect::<String>())
        } else {
            first_line.to_string()
        }
    }
    /// Get truncated pubkey for display
    pub fn pubkey_display(&self) -> String {
        if self.pubkey.len() > 12 {
            format!("{}...{}", &self.pubkey[..6], &self.pubkey[self.pubkey.len() - 4..])
        } else {
            self.pubkey.clone()
        }
    }
    /// Get short event ID for display
    pub fn event_id_short(&self) -> String {
        self.event_id.chars().take(8).collect()
    }
}
/// Discussion parsed from a Kind 9805 event
#[derive(Debug, Clone, PartialEq)]
pub struct Discussion {
    /// Repository naddr this discussion belongs to
    pub repository_naddr: String,
    /// Discussion content (markdown)
    pub content: String,
    /// Subject/title
    pub subject: Option<String>,
    /// Category (from first hashtag)
    pub category: Option<String>,
    /// Labels (from t tags, excluding category)
    pub labels: Vec<String>,
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Created timestamp
    pub created_at: u64,
    /// Comment count
    pub comment_count: u32,
}
impl Discussion {
    pub const KIND: u16 = 9805;
    /// Parse a Discussion from a Kind 9805 event
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Kind::Custom(Self::KIND) {
            return None;
        }
        let mut repository = None;
        let mut subject = None;
        let mut hashtags = Vec::new();
        for tag in event.tags.iter() {
            match tag.as_standardized() {
                Some(TagStandard::Coordinate { coordinate, .. }) => {
                    repository = Some(coordinate.clone());
                }
                Some(TagStandard::Hashtag(t)) => hashtags.push(t.clone()),
                Some(TagStandard::Subject(s)) => subject = Some(s.clone()),
                _ => {}
            }
            // Check for custom "subject" tag (overrides standard if present)
            if tag.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("subject")) {
                if let Some(s) = tag.content() {
                    subject = Some(s.to_string());
                }
            }
        }
        let repository = repository?;
        let repository_naddr = Nip19Coordinate::new(repository, vec![])
            .to_bech32()
            .unwrap_or_default();
        let category = hashtags.first().cloned();
        let labels = if hashtags.len() > 1 { hashtags[1..].to_vec() } else { Vec::new() };
        Some(Self {
            repository_naddr,
            content: event.content.clone(),
            subject,
            category,
            labels,
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            created_at: event.created_at.as_secs(),
            comment_count: 0,
        })
    }
    /// Get display title (subject or truncated content)
    pub fn display_title(&self) -> String {
        if let Some(subject) = &self.subject {
            subject.clone()
        } else {
            let first_line = self.content.lines().next().unwrap_or("");
            if first_line.chars().count() > 80 {
                format!("{}...", first_line.chars().take(77).collect::<String>())
            } else {
                first_line.to_string()
            }
        }
    }
    /// Get truncated pubkey for display
    pub fn pubkey_display(&self) -> String {
        if self.pubkey.len() > 12 {
            format!("{}...{}", &self.pubkey[..6], &self.pubkey[self.pubkey.len() - 4..])
        } else {
            self.pubkey.clone()
        }
    }
}
/// Release parsed from a Kind 1626 event (custom)
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// Repository naddr this release belongs to
    pub repository_naddr: String,
    /// Tag name / version (d-tag identifier)
    pub tag_name: String,
    /// Human-readable title
    pub title: Option<String>,
    /// Release description (content)
    pub description: String,
    /// Download/asset URLs
    pub assets: Vec<String>,
    /// Whether this is a pre-release
    pub prerelease: bool,
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Created timestamp
    pub created_at: u64,
}
impl Release {
    pub const KIND: u16 = 1626;
    /// Parse a Release from a Kind 1626 event
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Kind::Custom(Self::KIND) {
            return None;
        }
        let mut tag_name = None;
        let mut title = None;
        let mut repository = None;
        let mut assets = Vec::new();
        let mut prerelease = false;
        for tag in event.tags.iter() {
            match tag.as_standardized() {
                Some(TagStandard::Identifier(d)) => tag_name = Some(d.clone()),
                Some(TagStandard::Coordinate { coordinate, .. }) => {
                    repository = Some(coordinate.clone());
                }
                _ => {}
            }
            let kind = tag.kind();
            if kind == TagKind::Custom(std::borrow::Cow::Borrowed("name")) {
                if let Some(n) = tag.content() {
                    title = Some(n.to_string());
                }
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("url")) {
                if let Some(u) = tag.content() {
                    assets.push(u.to_string());
                }
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("prerelease")) {
                if let Some(v) = tag.content() {
                    prerelease = v == "true";
                }
            }
        }
        let tag_name = tag_name?;
        let repository = repository?;
        let repository_naddr = Nip19Coordinate::new(repository, vec![])
            .to_bech32()
            .unwrap_or_default();
        Some(Self {
            repository_naddr,
            tag_name,
            title,
            description: event.content.clone(),
            assets,
            prerelease,
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            created_at: event.created_at.as_secs(),
        })
    }
    /// Get display name (title or tag_name)
    pub fn display_name(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.tag_name)
    }
}
/// Bounty status for Kind 9806 events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BountyStatus {
    #[default]
    Pending,
    Claimed,
    Paid,
}
impl BountyStatus {
    pub fn from_str(s: &str) -> Self {
        match s {
            "claimed" => Self::Claimed,
            "paid" => Self::Paid,
            _ => Self::Pending,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Claimed => "Claimed",
            Self::Paid => "Paid",
        }
    }
}
/// Bounty parsed from a Kind 9806 event
#[derive(Debug, Clone, PartialEq)]
pub struct Bounty {
    /// Issue event ID this bounty is attached to
    pub issue_event_id: String,
    /// Amount in satoshis
    pub amount_sats: u64,
    /// Bounty status
    pub status: BountyStatus,
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Created timestamp
    pub created_at: u64,
    /// Claimer pubkey (hex) - set when status is "claimed", parsed from p tag or event author
    pub claimer: Option<String>,
}
impl Bounty {
    pub const KIND: u16 = 9806;
    /// Parse a Bounty from a Kind 9806 event
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Kind::Custom(Self::KIND) {
            return None;
        }
        let mut issue_event_id = None;
        let mut amount_sats = 0u64;
        let mut status = BountyStatus::Pending;
        let mut claimer_pubkey = None;
        for tag in event.tags.iter() {
            if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                if issue_event_id.is_none() {
                    issue_event_id = Some(event_id.to_hex());
                }
            }
            if let Some(TagStandard::PublicKey { public_key, .. }) = tag.as_standardized() {
                claimer_pubkey = Some(public_key.to_hex());
            }
            let kind = tag.kind();
            if kind == TagKind::Custom(std::borrow::Cow::Borrowed("amount")) {
                if let Some(amt) = tag.content() {
                    amount_sats = amt.parse().unwrap_or(0);
                }
            } else if kind == TagKind::Custom(std::borrow::Cow::Borrowed("status")) {
                if let Some(s) = tag.content() {
                    status = BountyStatus::from_str(s);
                }
            }
        }
        let issue_event_id = issue_event_id?;
        // For claimed bounties, use p tag as claimer; fall back to event author
        let claimer = if status == BountyStatus::Claimed {
            Some(claimer_pubkey.unwrap_or_else(|| event.pubkey.to_hex()))
        } else {
            claimer_pubkey
        };
        Some(Self {
            issue_event_id,
            amount_sats,
            status,
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            created_at: event.created_at.as_secs(),
            claimer,
        })
    }
}
/// Review state for persisted PR reviews
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReviewState {
    #[default]
    Commented,
    Approved,
    ChangesRequested,
}
impl ReviewState {
    pub fn from_str(s: &str) -> Self {
        match s {
            "approved" => Self::Approved,
            "changes_requested" => Self::ChangesRequested,
            _ => Self::Commented,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Approved => "Approved",
            Self::ChangesRequested => "Changes Requested",
            Self::Commented => "Commented",
        }
    }
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Approved => "text-green-500",
            Self::ChangesRequested => "text-red-500",
            Self::Commented => "text-muted-foreground",
        }
    }
}
/// Persisted PR review (Kind 1985)
#[derive(Debug, Clone, PartialEq)]
pub struct PersistedReview {
    /// PR event ID this review targets
    pub pr_event_id: String,
    /// Review content
    pub content: String,
    /// Review state
    pub state: ReviewState,
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Created timestamp
    pub created_at: u64,
}
impl PersistedReview {
    pub const KIND: u16 = 1985;
    /// Parse a PersistedReview from a Kind 1985 event
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Kind::Custom(Self::KIND) {
            return None;
        }
        let mut pr_event_id = None;
        let mut state = ReviewState::Commented;
        for tag in event.tags.iter() {
            if let Some(TagStandard::Event { event_id, .. }) = tag.as_standardized() {
                if pr_event_id.is_none() {
                    pr_event_id = Some(event_id.to_hex());
                }
            }
            if tag.kind() == TagKind::Custom(std::borrow::Cow::Borrowed("state")) {
                if let Some(s) = tag.content() {
                    state = ReviewState::from_str(s);
                }
            }
        }
        let pr_event_id = pr_event_id?;
        Some(Self {
            pr_event_id,
            content: event.content.clone(),
            state,
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            created_at: event.created_at.as_secs(),
        })
    }
}
/// Git comment (Kind 1111 - NIP-22 Comment)
#[derive(Debug, Clone, PartialEq)]
pub struct GitComment {
    /// Comment content
    pub content: String,
    /// Target event ID being commented on
    pub target_event_id: String,
    /// Root event ID (for threading)
    pub root_event_id: Option<String>,
    /// Repository naddr (if tagged)
    pub repository_naddr: Option<String>,
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Created timestamp
    pub created_at: u64,
}
impl GitComment {
    /// Parse a GitComment from a Kind 1111 (Comment) event
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Kind::Comment {
            return None;
        }
        let mut target_event_id = None;
        let mut root_event_id = None;
        let mut repository = None;
        for tag in event.tags.iter() {
            match tag.as_standardized() {
                Some(TagStandard::Event { event_id, marker, .. }) => {
                    match marker {
                        Some(Marker::Root) => root_event_id = Some(event_id.to_hex()),
                        Some(Marker::Reply) | None => {
                            if target_event_id.is_none() {
                                target_event_id = Some(event_id.to_hex());
                            }
                        }
                    }
                }
                Some(TagStandard::Coordinate { coordinate, .. }) => {
                    repository = Some(coordinate.clone());
                }
                _ => {}
            }
        }
        let target_event_id = target_event_id?;
        let repository_naddr = repository
            .map(|coord| {
                Nip19Coordinate::new(coord, vec![]).to_bech32().unwrap_or_default()
            });
        Some(Self {
            content: event.content.clone(),
            target_event_id,
            root_event_id,
            repository_naddr,
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            created_at: event.created_at.as_secs(),
        })
    }
}
/// Code Snippet (Kind 1337 - NIP-C0)
/// Wrapper around SDK type with event metadata for display
#[derive(Debug, Clone, PartialEq)]
pub struct DisplaySnippet {
    /// The actual code
    pub code: String,
    /// Programming language
    pub language: Option<String>,
    /// Filename
    pub name: Option<String>,
    /// File extension
    pub extension: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Runtime specification
    pub runtime: Option<String>,
    /// License
    pub license: Option<String>,
    /// Dependencies
    pub dependencies: Vec<String>,
    /// Source repository URL
    pub repo: Option<String>,
    /// Event ID (hex)
    pub event_id: String,
    /// Author pubkey (hex)
    pub pubkey: String,
    /// Created timestamp
    pub created_at: u64,
}
impl DisplaySnippet {
    /// Parse a DisplaySnippet from a Kind 1337 event
    pub fn from_event(event: &Event) -> Option<Self> {
        if event.kind != Kind::CodeSnippet {
            return None;
        }
        let mut language = None;
        let mut name = None;
        let mut extension = None;
        let mut description = None;
        let mut runtime = None;
        let mut license = None;
        let mut dependencies = Vec::new();
        let mut repo = None;
        for tag in event.tags.iter() {
            match tag.as_standardized() {
                Some(TagStandard::Label { value, .. }) => language = Some(value.clone()),
                Some(TagStandard::Name(n)) => name = Some(n.clone()),
                Some(TagStandard::Extension(e)) => extension = Some(e.clone()),
                Some(TagStandard::Description(d)) => description = Some(d.clone()),
                Some(TagStandard::Runtime(r)) => runtime = Some(r.clone()),
                Some(TagStandard::License(l)) => license = Some(l.clone()),
                Some(TagStandard::Dependency(d)) => dependencies.push(d.clone()),
                Some(TagStandard::Repository(r)) => repo = Some(r.clone()),
                _ => {}
            }
        }
        Some(Self {
            code: event.content.clone(),
            language,
            name,
            extension,
            description,
            runtime,
            license,
            dependencies,
            repo,
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            created_at: event.created_at.as_secs(),
        })
    }
    /// Get truncated pubkey for display
    pub fn pubkey_display(&self) -> String {
        if self.pubkey.len() > 12 {
            format!("{}...{}", &self.pubkey[..6], &self.pubkey[self.pubkey.len() - 4..])
        } else {
            self.pubkey.clone()
        }
    }
    /// Get display language (language or inferred from extension)
    pub fn display_language(&self) -> Option<&str> {
        if let Some(lang) = &self.language {
            return Some(lang.as_str());
        }
        if let Some(ext) = &self.extension {
            return Some(
                match ext.as_str() {
                    "rs" => "rust",
                    "js" => "javascript",
                    "ts" => "typescript",
                    "py" => "python",
                    "rb" => "ruby",
                    "go" => "go",
                    "java" => "java",
                    "c" | "h" => "c",
                    "cpp" | "hpp" | "cc" | "cxx" => "cpp",
                    "cs" => "csharp",
                    "swift" => "swift",
                    "kt" => "kotlin",
                    "php" => "php",
                    "sh" | "bash" => "bash",
                    "sql" => "sql",
                    "html" => "html",
                    "css" => "css",
                    "json" => "json",
                    "yaml" | "yml" => "yaml",
                    "xml" => "xml",
                    "md" => "markdown",
                    "toml" => "toml",
                    _ => ext.as_str(),
                },
            );
        }
        None
    }
    /// Get display name (name or "Untitled")
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("Untitled")
    }
    /// Get line count
    pub fn line_count(&self) -> usize {
        self.code.lines().count()
    }
}
/// Encode a repository coordinate to naddr bech32
pub fn encode_repo_naddr(
    coordinate: &Coordinate,
    relay_hints: &[RelayUrl],
) -> Result<String, Nip34Error> {
    use nostr::nips::nip19::{Nip19Coordinate, ToBech32};
    let nip19 = Nip19Coordinate::new(coordinate.clone(), relay_hints.to_vec());
    nip19.to_bech32()
}
/// Decode a repository naddr bech32 to coordinate
pub fn decode_repo_naddr(
    naddr: &str,
) -> Result<(Coordinate, Vec<RelayUrl>), Nip34Error> {
    use nostr::nips::nip19::{FromBech32, Nip19Coordinate};
    let decoded = Nip19Coordinate::from_bech32(naddr)?;
    let coordinate = Coordinate::new(
            decoded.coordinate.kind,
            decoded.coordinate.public_key,
        )
        .identifier(decoded.coordinate.identifier);
    Ok((coordinate, decoded.relays))
}
/// Decode naddr to Coordinate only (convenience wrapper)
pub fn decode_naddr(naddr: &str) -> Result<Coordinate, Nip34Error> {
    let (coordinate, _) = decode_repo_naddr(naddr)?;
    Ok(coordinate)
}
/// Encode an event to nevent bech32 (for issues, PRs, snippets)
pub fn encode_nevent(
    event_id: EventId,
    author: Option<PublicKey>,
    kind: Option<Kind>,
    relay_hints: &[RelayUrl],
) -> Result<String, Nip34Error> {
    use nostr::nips::nip19::{Nip19Event, ToBech32};
    let mut nevent = Nip19Event::new(event_id).relays(relay_hints.to_vec());
    if let Some(pk) = author {
        nevent = nevent.author(pk);
    }
    if let Some(k) = kind {
        nevent = nevent.kind(k);
    }
    nevent.to_bech32()
}
/// Decode a nevent bech32 to event info
#[allow(clippy::type_complexity)]
pub fn decode_nevent(
    nevent: &str,
) -> Result<(EventId, Option<PublicKey>, Option<Kind>, Vec<RelayUrl>), Nip34Error> {
    use nostr::nips::nip19::{FromBech32, Nip19Event};
    let decoded = Nip19Event::from_bech32(nevent)?;
    Ok((decoded.event_id, decoded.author, decoded.kind, decoded.relays))
}
/// Decode either note1 or nevent1 to EventId
pub fn decode_event_id(bech32: &str) -> Result<EventId, Nip34Error> {
    use nostr::nips::nip19::FromBech32;
    if bech32.starts_with("note1") {
        EventId::from_bech32(bech32)
    } else if bech32.starts_with("nevent1") {
        let (event_id, _, _, _) = decode_nevent(bech32)?;
        Ok(event_id)
    } else {
        Err(Nip34Error::TryFromSlice)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_issue_status_from_kind() {
        assert_eq!(IssueStatus::from_kind(Kind::GitStatusOpen), Some(IssueStatus::Open));
        assert_eq!(
            IssueStatus::from_kind(Kind::GitStatusApplied),
            Some(IssueStatus::Applied),
        );
        assert_eq!(
            IssueStatus::from_kind(Kind::GitStatusClosed),
            Some(IssueStatus::Closed),
        );
        assert_eq!(
            IssueStatus::from_kind(Kind::GitStatusDraft),
            Some(IssueStatus::Draft),
        );
        assert_eq!(IssueStatus::from_kind(Kind::TextNote), None);
    }
    #[test]
    fn test_issue_status_to_kind() {
        assert_eq!(IssueStatus::Open.to_kind(), Kind::GitStatusOpen);
        assert_eq!(IssueStatus::Applied.to_kind(), Kind::GitStatusApplied);
        assert_eq!(IssueStatus::Closed.to_kind(), Kind::GitStatusClosed);
        assert_eq!(IssueStatus::Draft.to_kind(), Kind::GitStatusDraft);
    }
}
