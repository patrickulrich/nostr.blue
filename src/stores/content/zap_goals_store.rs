use crate::stores::nostr_client::{self, PublishResult};
use crate::stores::profiles;
use ::url::Url;
use chrono::{DateTime, Utc};
use nostr_sdk::prelude::*;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub const KIND_ZAP_GOAL: u16 = 9041;
pub const PROJECT_DONATION_LUD16: &str = "nostrblue@sats.love";
pub const PROJECT_DONATION_NPUB: &str =
    "npub10vz2md22xl8arjprqysn8f7j2guewzunaktnn94c55hlwcwyyu4qm6ac8k";
pub const PROJECT_GOAL_AUTHOR_NPUB: &str = PROJECT_DONATION_NPUB;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZapGoalsFeedType {
    Following,
    Global,
}

impl ZapGoalsFeedType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Following => "Following",
            Self::Global => "Global",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ZapGoal {
    pub event: Event,
    pub event_id: String,
    pub author_pubkey: String,
    pub amount_sats: u64,
    pub amount_msats: u64,
    pub relays: Vec<String>,
    pub summary: Option<String>,
    pub image: Option<String>,
    pub url: Option<String>,
    pub content: String,
    pub created_at: u64,
    pub closed_at: Option<u64>,
    pub is_project_goal: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ZapGoalContributor {
    pub pubkey: String,
    pub amount_sats: u64,
    pub latest_zap_at: u64,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ZapGoalProgress {
    pub goal: ZapGoal,
    pub raised_sats: u64,
    pub contributor_count: usize,
    pub percentage: f32,
    pub recent_contributors: Vec<ZapGoalContributor>,
}

pub fn project_pubkey() -> Result<PublicKey, String> {
    PublicKey::parse(PROJECT_GOAL_AUTHOR_NPUB).map_err(|e| format!("Invalid project pubkey: {e}"))
}

pub fn project_author_hex() -> Result<String, String> {
    project_pubkey().map(|pk| pk.to_hex())
}

fn now_ts() -> u64 {
    Utc::now().timestamp().max(0) as u64
}

fn parse_u64_tag(event: &Event, name: &str) -> Option<u64> {
    event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first().map(|value| value.as_str()) == Some(name) {
            slice.get(1)?.as_str().parse::<u64>().ok()
        } else {
            None
        }
    })
}

fn parse_string_tag(event: &Event, name: &str) -> Option<String> {
    event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first().map(|value| value.as_str()) == Some(name) {
            let value = slice.get(1)?.as_str().trim();
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        } else {
            None
        }
    })
}

fn parse_relays(event: &Event) -> Option<Vec<String>> {
    event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first().map(|value| value.as_str()) == Some("relays") {
            let relays: Vec<String> = slice
                .iter()
                .skip(1)
                .map(|relay| relay.as_str().trim())
                .filter_map(validate_relay_url)
                .collect();
            if relays.is_empty() {
                None
            } else {
                Some(relays)
            }
        } else {
            None
        }
    })
}

fn validate_relay_url(relay: &str) -> Option<String> {
    let relay = relay.trim();
    if relay.is_empty() {
        return None;
    }

    let parsed = Url::parse(relay).ok()?;
    match parsed.scheme() {
        "ws" | "wss" => {}
        _ => return None,
    }

    parsed.host_str()?;
    Some(relay.to_string())
}

fn normalize_relays(relays: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    relays
        .iter()
        .filter_map(|relay| validate_relay_url(relay))
        .filter(|relay| seen.insert(relay.clone()))
        .collect()
}

async fn fetch_zap_goal_events_paginated(
    mut filter: Filter,
    target_goal_count: usize,
) -> Result<Vec<Event>, String> {
    let timeout = Duration::from_secs(10);
    let batch_size = target_goal_count.max(25);
    let mut events = Vec::new();
    let mut seen = HashSet::new();
    let mut previous_oldest_created_at = None;

    loop {
        let page = nostr_client::fetch_events_aggregated(filter.clone().limit(batch_size), timeout)
            .await?;
        if page.is_empty() {
            break;
        }

        let oldest_created_at = page.iter().map(|event| event.created_at.as_secs()).min();
        let seen_before = seen.len();
        for event in page {
            if seen.insert(event.id) {
                events.push(event);
            }
        }

        let parsed_count = events.iter().filter_map(parse_goal_event).count();
        if parsed_count >= target_goal_count {
            break;
        }

        let Some(oldest_created_at) = oldest_created_at else {
            break;
        };
        if oldest_created_at == 0
            || previous_oldest_created_at == Some(oldest_created_at)
            || seen.len() == seen_before
        {
            break;
        }
        previous_oldest_created_at = Some(oldest_created_at);
        filter = filter.until(Timestamp::from(oldest_created_at));
    }

    Ok(events)
}

async fn fetch_zap_receipts_paginated(goal_event_id: EventId) -> Result<Vec<Event>, String> {
    let timeout = Duration::from_secs(10);
    let mut filter = Filter::new().kind(Kind::ZapReceipt).event(goal_event_id);
    let mut receipts = Vec::new();
    let mut seen = HashSet::new();
    let mut previous_oldest_created_at = None;

    loop {
        let page =
            nostr_client::fetch_events_aggregated(filter.clone().limit(500), timeout).await?;
        if page.is_empty() {
            break;
        }

        let oldest_created_at = page.iter().map(|event| event.created_at.as_secs()).min();
        let seen_before = seen.len();
        for receipt in page {
            if seen.insert(receipt.id) {
                receipts.push(receipt);
            }
        }

        let Some(oldest_created_at) = oldest_created_at else {
            break;
        };
        if oldest_created_at == 0
            || previous_oldest_created_at == Some(oldest_created_at)
            || seen.len() == seen_before
        {
            break;
        }
        previous_oldest_created_at = Some(oldest_created_at);
        filter = filter.until(Timestamp::from(oldest_created_at));
    }

    Ok(receipts)
}

pub fn parse_goal_event(event: &Event) -> Option<ZapGoal> {
    if event.kind != Kind::Custom(KIND_ZAP_GOAL) {
        return None;
    }

    let amount_msats = parse_u64_tag(event, "amount")?;
    let relays = parse_relays(event)?;
    let closed_at = parse_u64_tag(event, "closed_at");
    if closed_at.is_some_and(|timestamp| timestamp <= now_ts()) {
        return None;
    }

    let author_pubkey = event.pubkey.to_hex();
    let is_project_goal = project_author_hex()
        .map(|project| project == author_pubkey)
        .unwrap_or(false);

    Some(ZapGoal {
        event: event.clone(),
        event_id: event.id.to_hex(),
        author_pubkey,
        amount_sats: amount_msats / 1000,
        amount_msats,
        relays,
        summary: parse_string_tag(event, "summary"),
        image: parse_string_tag(event, "image"),
        url: parse_string_tag(event, "r"),
        content: event.content.clone(),
        created_at: event.created_at.as_secs(),
        closed_at,
        is_project_goal,
    })
}

fn sort_goals(goals: &mut [ZapGoal]) {
    goals.sort_by(|left, right| {
        right
            .is_project_goal
            .cmp(&left.is_project_goal)
            .then_with(|| {
                left.closed_at
                    .unwrap_or(u64::MAX)
                    .cmp(&right.closed_at.unwrap_or(u64::MAX))
            })
            .then_with(|| right.created_at.cmp(&left.created_at))
    });
}

pub fn filter_goals_by_query(goals: &[ZapGoalProgress], query: &str) -> Vec<ZapGoalProgress> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return goals.to_vec();
    }

    goals
        .iter()
        .filter(|goal| {
            let profile = profiles::get_cached_profile(&goal.goal.author_pubkey);
            let author_name = profile
                .as_ref()
                .map(|profile| profile.get_display_name().to_lowercase())
                .unwrap_or_default();
            goal.goal
                .summary
                .as_deref()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&query)
                || goal.goal.content.to_lowercase().contains(&query)
                || author_name.contains(&query)
                || goal.goal.author_pubkey.to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
}

pub async fn fetch_project_goals(limit: usize) -> Result<Vec<ZapGoal>, String> {
    let author = project_pubkey()?;
    fetch_goals_for_authors(vec![author], limit, None).await
}

pub async fn fetch_goals_for_authors(
    authors: Vec<PublicKey>,
    limit: usize,
    until: Option<u64>,
) -> Result<Vec<ZapGoal>, String> {
    if authors.is_empty() {
        return Ok(Vec::new());
    }

    let mut filter = Filter::new()
        .kind(Kind::Custom(KIND_ZAP_GOAL))
        .authors(authors);
    if let Some(until) = until {
        filter = filter.until(Timestamp::from(until));
    }

    let mut goals: Vec<ZapGoal> = fetch_zap_goal_events_paginated(filter, limit)
        .await?
        .into_iter()
        .filter_map(|event| parse_goal_event(&event))
        .collect();
    dedupe_goals(&mut goals);
    sort_goals(&mut goals);
    goals.truncate(limit);
    Ok(goals)
}

pub async fn fetch_global_goals(limit: usize, until: Option<u64>) -> Result<Vec<ZapGoal>, String> {
    let mut filter = Filter::new().kind(Kind::Custom(KIND_ZAP_GOAL));
    if let Some(until) = until {
        filter = filter.until(Timestamp::from(until));
    }

    let mut goals: Vec<ZapGoal> = fetch_zap_goal_events_paginated(filter, limit)
        .await?
        .into_iter()
        .filter_map(|event| parse_goal_event(&event))
        .collect();
    dedupe_goals(&mut goals);
    sort_goals(&mut goals);
    goals.truncate(limit);
    Ok(goals)
}

pub fn dedupe_goals(goals: &mut Vec<ZapGoal>) {
    let mut seen = HashSet::new();
    goals.retain(|goal| seen.insert(goal.event_id.clone()));
}

fn parse_description_json(event: &Event) -> Option<Value> {
    event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first().map(|value| value.as_str()) == Some("description") {
            serde_json::from_str::<Value>(slice.get(1)?.as_str()).ok()
        } else {
            None
        }
    })
}

fn extract_zap_amount_sats(event: &Event) -> Option<u64> {
    if let Some(bolt11_tag) = event.tags.iter().find(|tag| {
        tag.as_slice()
            .first()
            .map(|value| value.as_str() == "bolt11")
            .unwrap_or(false)
    }) {
        if let Some(bolt11) = bolt11_tag.as_slice().get(1) {
            if let Some(amount) = parse_bolt11_amount(bolt11.as_str()) {
                return Some(amount);
            }
        }
    }

    let json = parse_description_json(event)?;
    if let Some(tags) = json.get("tags").and_then(|value| value.as_array()) {
        for tag in tags {
            if let Some(tag_values) = tag.as_array() {
                if tag_values.first().and_then(|value| value.as_str()) == Some("amount") {
                    if let Some(msats) = tag_values.get(1).and_then(|value| value.as_str()) {
                        if let Ok(parsed) = msats.parse::<u64>() {
                            return Some(parsed / 1000);
                        }
                    }
                }
            }
        }
    }

    json.get("amount")
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str()?.parse::<u64>().ok())
        })
        .map(|msats| msats / 1000)
}

fn parse_bolt11_amount(bolt11: &str) -> Option<u64> {
    let lower = bolt11.to_lowercase();
    if !lower.starts_with("lnbc") && !lower.starts_with("lntb") && !lower.starts_with("lnsb") {
        return None;
    }
    let rest = &lower[4..];
    if rest.starts_with('1')
        && (rest.len() == 1
            || !rest
                .chars()
                .nth(1)
                .is_some_and(|ch| ch.is_ascii_digit() || ['m', 'u', 'n', 'p'].contains(&ch)))
    {
        return None;
    }
    let mut amount_end = 0;
    let mut multiplier = None;
    for (index, ch) in rest.chars().enumerate() {
        if ch.is_ascii_digit() {
            amount_end = index + 1;
        } else if ['m', 'u', 'n', 'p'].contains(&ch) {
            multiplier = Some(ch);
            amount_end = index;
            break;
        } else {
            amount_end = index;
            break;
        }
    }
    if amount_end == 0 {
        return None;
    }

    let separator_index = amount_end + usize::from(multiplier.is_some());
    if rest.chars().nth(separator_index) != Some('1') {
        return None;
    }

    let amount: u64 = rest[..amount_end].parse().ok()?;
    match multiplier {
        Some('m') => Some(amount * 100_000),
        Some('u') => Some(amount * 100),
        Some('n') => Some(amount / 10),
        Some('p') => Some(amount / 10000),
        Some(_) => None,
        None => Some(amount * 100_000_000),
    }
}

fn extract_zap_sender(event: &Event) -> Option<String> {
    event
        .tags
        .iter()
        .find_map(|tag| {
            let slice = tag.as_slice();
            if slice.len() >= 2 && slice.first()?.as_str() == "P" {
                Some(slice.get(1)?.as_str().to_string())
            } else {
                None
            }
        })
        .or_else(|| {
            parse_description_json(event)?
                .get("pubkey")
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
}

fn extract_zap_comment(event: &Event) -> Option<String> {
    parse_description_json(event)?
        .get("content")
        .and_then(|value| value.as_str())
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
}

pub async fn fetch_goal_progress(goal: &ZapGoal) -> Result<ZapGoalProgress, String> {
    let goal_event_id = EventId::parse(&goal.event_id)
        .map_err(|e| format!("Invalid goal event id {}: {e}", goal.event_id))?;
    let receipts = fetch_zap_receipts_paginated(goal_event_id).await?;

    let mut total_sats = 0u64;
    let mut contributors: HashMap<String, ZapGoalContributor> = HashMap::new();

    for receipt in receipts {
        let created_at = receipt.created_at.as_secs();
        if goal
            .closed_at
            .is_some_and(|closed_at| created_at > closed_at)
        {
            continue;
        }

        let amount_sats = match extract_zap_amount_sats(&receipt) {
            Some(amount) if amount > 0 => amount,
            _ => continue,
        };
        total_sats = total_sats.saturating_add(amount_sats);

        if let Some(pubkey) = extract_zap_sender(&receipt) {
            let comment = extract_zap_comment(&receipt);
            contributors
                .entry(pubkey.clone())
                .and_modify(|entry| {
                    entry.amount_sats = entry.amount_sats.saturating_add(amount_sats);
                    if created_at >= entry.latest_zap_at {
                        entry.latest_zap_at = created_at;
                        if comment.is_some() {
                            entry.comment = comment.clone();
                        }
                    }
                })
                .or_insert(ZapGoalContributor {
                    pubkey,
                    amount_sats,
                    latest_zap_at: created_at,
                    comment,
                });
        }
    }

    let contributor_count = contributors.len();
    let mut recent_contributors: Vec<ZapGoalContributor> = contributors.into_values().collect();
    recent_contributors.sort_by(|left, right| {
        right
            .latest_zap_at
            .cmp(&left.latest_zap_at)
            .then_with(|| right.amount_sats.cmp(&left.amount_sats))
    });
    recent_contributors.truncate(8);

    Ok(ZapGoalProgress {
        goal: goal.clone(),
        raised_sats: total_sats,
        contributor_count,
        percentage: if goal.amount_sats == 0 {
            0.0
        } else {
            (total_sats as f32 / goal.amount_sats as f32) * 100.0
        },
        recent_contributors,
    })
}

pub async fn fetch_goal_progress_batch(goals: &[ZapGoal]) -> Result<Vec<ZapGoalProgress>, String> {
    let results = futures::future::join_all(goals.iter().map(fetch_goal_progress)).await;
    let mut progress = Vec::new();
    for (goal, result) in goals.iter().zip(results) {
        match result {
            Ok(goal_progress) => progress.push(goal_progress),
            Err(error) => {
                log::warn!(
                    "Falling back to empty zap goal progress for {} ({}): {}",
                    goal.event_id,
                    goal.author_pubkey,
                    error
                );
                progress.push(ZapGoalProgress {
                    goal: goal.clone(),
                    raised_sats: 0,
                    contributor_count: 0,
                    percentage: 0.0,
                    recent_contributors: Vec::new(),
                });
            }
        }
    }
    Ok(progress)
}

pub async fn publish_zap_goal_tracked(
    amount_sats: u64,
    summary: Option<String>,
    content: String,
    image: Option<String>,
    closed_at: Option<u64>,
    relays: Vec<String>,
    url: Option<String>,
) -> Result<PublishResult, String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if amount_sats == 0 {
        return Err("Amount must be greater than zero".to_string());
    }
    if let Some(closed_at) = closed_at {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        if closed_at <= now {
            return Err("closed_at must be a future timestamp".to_string());
        }
    }
    let relays = normalize_relays(&relays);
    if relays.is_empty() {
        return Err("At least one valid relay is required".to_string());
    }

    let mut builder = EventBuilder::new(Kind::Custom(KIND_ZAP_GOAL), content).tag(Tag::custom(
        TagKind::custom("amount"),
        vec![amount_sats.saturating_mul(1000).to_string()],
    ));
    builder = builder.tag(Tag::custom(TagKind::custom("relays"), relays));

    if let Some(summary) = summary
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        builder = builder.tag(Tag::custom(
            TagKind::custom("summary"),
            vec![summary.to_string()],
        ));
    }
    if let Some(image) = image
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        builder = builder.tag(Tag::custom(
            TagKind::custom("image"),
            vec![image.to_string()],
        ));
    }
    if let Some(url) = url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        builder = builder.tag(Tag::custom(TagKind::custom("r"), vec![url.to_string()]));
    }
    if let Some(closed_at) = closed_at {
        builder = builder.tag(Tag::custom(
            TagKind::custom("closed_at"),
            vec![closed_at.to_string()],
        ));
    }

    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish zap goal: {e}"))?;
    Ok(PublishResult::from_output(output).ignoring_duplicate_event_failures())
}

pub fn format_time_remaining(closed_at: Option<u64>) -> String {
    let Some(closed_at) = closed_at else {
        return "Open-ended".to_string();
    };
    let now = now_ts();
    if closed_at <= now {
        return "Closed".to_string();
    }

    let remaining = closed_at - now;
    let days = remaining / 86_400;
    let hours = (remaining % 86_400) / 3600;
    if days > 0 {
        format!("{days}d {hours}h left")
    } else {
        let minutes = (remaining % 3600) / 60;
        format!("{hours}h {minutes}m left")
    }
}

pub fn format_goal_date(timestamp: u64) -> String {
    let timestamp = timestamp.min(253_402_300_799);
    DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .map(|date| date.format("%b %-d, %Y").to_string())
        .unwrap_or_else(|| "Unknown date".to_string())
}

#[cfg(test)]
mod tests {
    use super::{normalize_relays, parse_bolt11_amount};

    #[test]
    fn parse_bolt11_amount_accepts_amounts_starting_with_one() {
        assert_eq!(parse_bolt11_amount("lnbc1m1example"), Some(100_000));
        assert_eq!(parse_bolt11_amount("lnbc100m1example"), Some(10_000_000));
    }

    #[test]
    fn parse_bolt11_amount_rejects_zero_amount_invoices() {
        assert_eq!(parse_bolt11_amount("lnbc1"), None);
        assert_eq!(parse_bolt11_amount("lnbc1qpayload"), None);
    }

    #[test]
    fn parse_bolt11_amount_rejects_missing_separator() {
        assert_eq!(parse_bolt11_amount("lnbc100uNOT_AN_INVOICE"), None);
        assert_eq!(parse_bolt11_amount("lnbc100000invoicepayload"), None);
    }

    #[test]
    fn normalize_relays_trims_filters_and_dedupes() {
        assert_eq!(
            normalize_relays(&[
                " wss://relay.one ".to_string(),
                "".to_string(),
                "https://relay.invalid".to_string(),
                "wss://".to_string(),
                "wss://:".to_string(),
                "ws://relay.two".to_string(),
                "wss://relay.one".to_string(),
            ]),
            vec!["wss://relay.one".to_string(), "ws://relay.two".to_string(),]
        );
    }
}
