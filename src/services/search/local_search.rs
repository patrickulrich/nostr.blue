//! Local-first content search: a bounded query against the SDK database
//! (which accumulates every event the app has seen) followed by a custom
//! tokenized client-side matcher.
//!
//! This deliberately does **not** use `Filter::search` against the local
//! database: nostr-ndb silently drops the `search` field and the wasm
//! WebDatabase implements it as a substring full-scan. Instead we fetch a
//! bounded recent corpus by structured fields (kinds / hashtags / authors /
//! time range) and token-match content + allowlisted tags ourselves.
//!
//! Hygiene rules (mirrored from relay-side search behavior):
//! - noisy kinds (reactions, repost references, zap receipts, contact lists,
//!   file headers, gift wraps) are never surfaced;
//! - only `content` and the VALUES of allowlisted tags (`title`,
//!   `description`, `subject`, `name`, `t`, `summary`) may match —
//!   `client`/`alt`/`p`/`e`/`a` tag values are never matched, so embedded
//!   bech32 identifiers and app names cannot produce spurious hits.

use super::content_search::ContentSearchResult;
use super::query_parser::ParsedSearchQuery;
use crate::stores::nostr_client::NOSTR_CLIENT;
use dioxus::prelude::ReadableExt;
use nostr_sdk::prelude::*;

/// Kinds never surfaced in local search results.
const EXCLUDED_KINDS: &[u16] = &[
    7,    // reactions
    6,    // reposts (content is just a reference)
    16,   // generic reposts
    9735, // zap receipts
    3,    // contact lists
    10002,// relay lists
    1063, // file metadata headers
    1064, // file headers
    1059, // gift wraps
    14,   // channel messages (envelopes)
];

/// Tag names whose VALUES participate in matching.
const MATCHABLE_TAG_NAMES: &[&str] = &["title", "description", "subject", "name", "t", "summary"];

/// How many recent events to pull from the local database per query.
const LOCAL_CORPUS_LIMIT: usize = 400;

/// Tokenized match score for an event against lowercased `terms`.
///
/// Every term must match (in `content` or an allowlisted tag value) or the
/// event is rejected (`None`). More content hits, tag hits and recency score
/// higher.
pub fn score_local_match(event: &Event, terms: &[String]) -> Option<u32> {
    if terms.is_empty() {
        return None;
    }
    if EXCLUDED_KINDS.contains(&event.kind.as_u16()) {
        return None;
    }
    let content_lower = event.content.to_lowercase();
    let mut score = 0u32;
    for term in terms {
        let mut matched = false;
        if content_lower.contains(term.as_str()) {
            score += 10;
            if content_lower.starts_with(term.as_str()) {
                score += 2;
            }
            matched = true;
        }
        if !matched {
            for tag in event.tags.iter() {
                if !MATCHABLE_TAG_NAMES.contains(&tag.kind().as_str()) {
                    continue;
                }
                if let Some(value) = tag.content() {
                    if value.to_lowercase().contains(term.as_str()) {
                        score += 8;
                        matched = true;
                        break;
                    }
                }
            }
        }
        if !matched {
            return None;
        }
    }
    let now = Timestamp::now();
    let age_days = now.as_secs().saturating_sub(event.created_at.as_secs()) / 86400;
    if age_days < 1 {
        score += 6;
    } else if age_days < 7 {
        score += 3;
    }
    Some(score)
}

/// Search the local SDK database for events matching the parsed query,
/// restricted to `kinds`. Returns scored results (highest first), up to
/// `limit`.
pub async fn search_local_content(
    parsed: &ParsedSearchQuery,
    kinds: &[Kind],
    limit: usize,
    contact_pubkeys: &[PublicKey],
) -> Vec<ContentSearchResult> {
    let terms: Vec<String> = parsed
        .text
        .split_whitespace()
        .map(|t| t.to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    if terms.is_empty() && parsed.hashtags.is_empty() && parsed.authors.is_empty() {
        return Vec::new();
    }

    let client = match (*NOSTR_CLIENT.read()).clone() {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut filter = Filter::new().kinds(kinds.to_vec()).limit(LOCAL_CORPUS_LIMIT);
    if !parsed.hashtags.is_empty() {
        filter = filter.hashtags(parsed.hashtags.iter().map(|s| s.as_str()));
    }
    if !parsed.authors.is_empty() {
        filter = filter.authors(parsed.authors.iter().cloned());
    }
    if let Some(since) = parsed.since {
        filter = filter.since(since);
    }
    if let Some(until) = parsed.until {
        filter = filter.until(until);
    }

    let events = match client.database().query(filter).await {
        Ok(events) => events,
        Err(e) => {
            log::debug!("Local search database query failed: {e}");
            return Vec::new();
        }
    };

    let mut results: Vec<ContentSearchResult> = events
        .into_iter()
        .filter_map(|event| {
            // Hashtag queries already matched via the #t filter; text terms
            // still need tokenized matching.
            let score = score_local_match(&event, &terms)?;
            let is_from_contact = contact_pubkeys.contains(&event.pubkey);
            Some(ContentSearchResult {
                is_from_contact,
                relevance: score + if is_from_contact { 10_000 } else { 0 },
                event,
                engagement: None,
            })
        })
        .collect();
    results.sort_by_key(|r| std::cmp::Reverse(r.relevance));
    results.truncate(limit);
    log::debug!(
        "Local search for '{}' returned {} results",
        parsed.raw,
        results.len()
    );
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event_with(kind: Kind, content: &str, tags: Vec<(&str, &str)>) -> Event {
        let keys = Keys::generate();
        let tags: Vec<Tag> = tags
            .into_iter()
            .filter_map(|(name, value)| Tag::parse([name, value]).ok())
            .collect();
        EventBuilder::new(kind, content).tags(tags)
            .sign_with_keys(&keys)
            .unwrap()
    }

    fn terms(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn all_terms_must_match() {
        let ev = event_with(Kind::TextNote, "hello world", vec![]);
        assert!(score_local_match(&ev, &terms(&["hello"])).is_some());
        assert!(score_local_match(&ev, &terms(&["hello", "world"])).is_some());
        assert!(score_local_match(&ev, &terms(&["hello", "missing"])).is_none());
    }

    #[test]
    fn matching_is_case_insensitive() {
        let ev = event_with(Kind::TextNote, "Hello WORLD", vec![]);
        assert!(score_local_match(&ev, &terms(&["hello", "world"])).is_some());
    }

    #[test]
    fn noisy_kinds_are_excluded() {
        let reaction = event_with(Kind::Reaction, "hello", vec![]);
        assert!(score_local_match(&reaction, &terms(&["hello"])).is_none());
        let repost = event_with(Kind::Repost, "hello", vec![]);
        assert!(score_local_match(&repost, &terms(&["hello"])).is_none());
    }

    #[test]
    fn allowlisted_tag_values_match() {
        let article = event_with(
            Kind::LongFormTextNote,
            "completely different body text",
            vec![("title", "Bitcoin Scaling")],
        );
        assert!(score_local_match(&article, &terms(&["bitcoin"])).is_some());
    }

    #[test]
    fn non_allowlisted_tag_values_never_match() {
        // client/alt/p/e/a tag values must not produce hits
        let note = event_with(
            Kind::TextNote,
            "nothing relevant here",
            vec![("client", "brandnewapp"), ("alt", "hidden description")],
        );
        assert!(score_local_match(&note, &terms(&["brandnewapp"])).is_none());
        assert!(score_local_match(&note, &terms(&["hidden"])).is_none());
    }

    #[test]
    fn empty_terms_never_match() {
        let ev = event_with(Kind::TextNote, "hello", vec![]);
        assert!(score_local_match(&ev, &[]).is_none());
    }
}
