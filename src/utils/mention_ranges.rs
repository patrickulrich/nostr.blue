//! Pretty `@Name` mention ranges for the composer family.
//!
//! The editor shows friendly `@Name` labels while the user composes; at
//! publish time [`materialize_mentions`] splices canonical `nostr:nprofile1…`
//! URIs into the wire content. Ranges are UTF-8 byte offsets and are kept in
//! sync with every text mutation via [`shift_ranges`] (minimal-diff shifting;
//! edits overlapping a range demote it — i.e. the mention is dropped from the
//! range list and becomes plain text).

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

/// A pretty mention anchored in the composer text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MentionRange {
    /// UTF-8 byte offset of the `@` (inclusive).
    pub start: usize,
    /// UTF-8 byte offset just past the label (exclusive).
    pub end: usize,
    pub pubkey: PublicKey,
    /// The sanitized label as it appears in the text (without `@`).
    pub label: String,
    /// Relay hints embedded in the nprofile.
    pub hints: Vec<RelayUrl>,
}

/// Maximum label length (chars) to keep mentions visually compact.
const MAX_LABEL_CHARS: usize = 32;

/// Sanitize a display name for use as an in-text mention label: trimmed,
/// leading `@` stripped, internal whitespace collapsed to `_` (so the label
/// cannot contain a space that would end the mention or an `@` that would
/// re-trigger autocomplete). Falls back to a truncated npub for empty results.
pub fn sanitize_display_name(name: &str, pubkey: &PublicKey) -> String {
    let trimmed = name.trim().trim_start_matches('@').trim();
    let sanitized: String = trimmed
        .chars()
        .take(MAX_LABEL_CHARS)
        .map(|c| if c.is_whitespace() || c == '@' { '_' } else { c })
        .collect();
    if sanitized.is_empty() {
        let npub = pubkey.to_bech32().unwrap_or_else(|_| pubkey.to_hex());
        return format!("{}…", &npub[..npub.len().min(10)]);
    }
    sanitized
}

/// Compute the byte length of the longest common prefix of `a` and `b`,
/// snapped to a char boundary of `a`.
fn common_prefix_len(a: &str, b: &str) -> usize {
    let mut idx = 0usize;
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    while idx < a_bytes.len()
        && idx < b_bytes.len()
        && a_bytes[idx] == b_bytes[idx]
        && !is_continuation(a_bytes[idx])
    {
        idx += 1;
    }
    idx
}

fn is_continuation(byte: u8) -> bool {
    byte & 0xC0 == 0x80
}

/// Compute the longest common suffix length (bytes), snapped to char
/// boundaries, and capped so prefix + suffix do not overlap either string.
fn common_suffix_len(a: &str, b: &str, prefix: usize) -> usize {
    let mut suffix = 0usize;
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    while suffix < a_bytes.len().saturating_sub(prefix)
        && suffix < b_bytes.len().saturating_sub(prefix)
        && a_bytes[a_bytes.len() - 1 - suffix] == b_bytes[b_bytes.len() - 1 - suffix]
        && !is_continuation(a_bytes[a_bytes.len() - 1 - suffix])
    {
        suffix += 1;
    }
    suffix
}

/// Shift ranges from `old` to `new` text using a minimal diff. Edits entirely
/// before a range shift it by the length delta; edits overlapping a range
/// demote it (the mention becomes plain text and is removed).
pub fn shift_ranges(ranges: &[MentionRange], old: &str, new: &str) -> Vec<MentionRange> {
    if old == new {
        return ranges.to_vec();
    }
    let prefix = common_prefix_len(old, new);
    let suffix = common_suffix_len(old, new, prefix);
    let old_edit_end = old.len() - suffix;
    let delta = new.len() as isize - old.len() as isize;

    ranges
        .iter()
        .filter_map(|range| {
            if range.start >= old_edit_end {
                // Entirely after the edit — shift.
                Some(MentionRange {
                    start: (range.start as isize + delta) as usize,
                    end: (range.end as isize + delta) as usize,
                    ..range.clone()
                })
            } else if range.end <= prefix {
                // Entirely before the edit — unchanged.
                Some(range.clone())
            } else {
                // Overlaps the edit — demote to plain text.
                None
            }
        })
        .filter(|range| {
            // Safety: keep only ranges still aligned on char boundaries and
            // still bracketing `@label`.
            range.start <= range.end
                && range.end <= new.len()
                && new.is_char_boundary(range.start)
                && new.is_char_boundary(range.end)
                && new[range.start..].starts_with('@')
                && new[range.start + 1..range.end] == *range.label
        })
        .collect()
}

/// Build the canonical `nostr:nprofile1…` (or `nostr:npub1…` fallback) URI
/// for a mention.
pub fn build_mention_uri(mention: &MentionRange) -> String {
    let nprofile = Nip19Profile::new(mention.pubkey, mention.hints.clone());
    match nprofile.to_bech32() {
        Ok(bech32) => format!("nostr:{bech32}"),
        Err(_) => format!(
            "nostr:{}",
            mention.pubkey.to_bech32().unwrap_or_else(|_| mention.pubkey.to_hex())
        ),
    }
}

/// Splice canonical `nostr:` URIs over the pretty `@label` ranges, in
/// descending order so earlier offsets stay valid. Idempotent by
/// construction — the ranges always describe `@label` spans in `content`.
pub fn materialize_mentions(content: &str, ranges: &[MentionRange]) -> String {
    if ranges.is_empty() {
        return content.to_string();
    }
    let mut result = content.to_string();
    let mut sorted: Vec<&MentionRange> = ranges.iter().collect();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.start));
    for range in sorted {
        if range.start <= result.len()
            && range.end <= result.len()
            && result.is_char_boundary(range.start)
            && result.is_char_boundary(range.end)
        {
            let uri = build_mention_uri(range);
            result.replace_range(range.start..range.end, &uri);
        }
    }
    result
}

/// Compute a pretty `@Label ` insertion replacing the typed `@query` span.
///
/// `at` is the byte offset of the `@` and `query_len` the length of the typed
/// query; the span `[at, at + 1 + query_len)` is replaced by `@Label `.
/// Returns the new content, the new caret (after the trailing space) and the
/// recorded range (covering `@Label`, without the trailing space).
pub fn build_pretty_insert(
    content: &str,
    at: usize,
    query_len: usize,
    pubkey: PublicKey,
    display_name: &str,
    hints: Vec<RelayUrl>,
) -> (String, usize, MentionRange) {
    let label = sanitize_display_name(display_name, &pubkey);
    let span_end = (at + 1 + query_len)
        .min(content.len())
        .max(at.saturating_sub(0));
    let mut start = at;
    let mut end = span_end;
    if !content.is_char_boundary(start) || start > content.len() {
        start = content.len();
    }
    if !content.is_char_boundary(end) || end > content.len() || end < start {
        end = start;
    }
    let pretty = format!("@{label} ");
    let mut new_content =
        String::with_capacity(content.len() + pretty.len());
    new_content.push_str(&content[..start]);
    let range = MentionRange {
        start,
        end: start + 1 + label.len(),
        pubkey,
        label,
        hints,
    };
    new_content.push_str(&pretty);
    new_content.push_str(&content[end..]);
    let caret = start + pretty.len();
    (new_content, caret, range)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pubkey() -> PublicKey {
        PublicKey::from_hex(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .unwrap()
    }

    fn range(start: usize, end: usize, label: &str) -> MentionRange {
        MentionRange {
            start,
            end,
            pubkey: test_pubkey(),
            label: label.to_string(),
            hints: Vec::new(),
        }
    }

    #[test]
    fn sanitize_strips_and_collapses() {
        let pk = test_pubkey();
        assert_eq!(sanitize_display_name("Alice", &pk), "Alice");
        assert_eq!(sanitize_display_name("  @Bob  ", &pk), "Bob");
        assert_eq!(sanitize_display_name("Multi Word Name", &pk), "Multi_Word_Name");
        assert_eq!(sanitize_display_name("a@b", &pk), "a_b");
        assert!(!sanitize_display_name("", &pk).is_empty());
    }

    #[test]
    fn sanitize_truncates_long_names() {
        let pk = test_pubkey();
        let long = "x".repeat(100);
        assert_eq!(sanitize_display_name(&long, &pk).chars().count(), 32);
    }

    #[test]
    fn shift_before_range_shifts_it() {
        let ranges = vec![range(12, 18, "Alice")]; // "hello world @Alice"
        let old = "hello world @Alice";
        let new = "hello! world @Alice";
        let shifted = shift_ranges(&ranges, old, new);
        assert_eq!(shifted.len(), 1);
        assert_eq!(shifted[0].start, 13);
        assert_eq!(shifted[0].end, 19);
        assert_eq!(&new[shifted[0].start..shifted[0].end], "@Alice");
    }

    #[test]
    fn shift_after_range_keeps_it() {
        let ranges = vec![range(0, 6, "Alice")];
        let old = "@Alice and more";
        let new = "@Alice AND MUCH MORE TEXT";
        let shifted = shift_ranges(&ranges, old, new);
        assert_eq!(shifted.len(), 1);
        assert_eq!(shifted[0].start, 0);
        assert_eq!(shifted[0].end, 6);
    }

    #[test]
    fn shift_overlapping_edit_demotes() {
        let ranges = vec![range(6, 12, "Alice")];
        let old = "hello @Alice";
        let new = "hello @Alicq"; // typo edit inside the label
        assert!(shift_ranges(&ranges, old, new).is_empty());
    }

    #[test]
    fn shift_deleting_range_demotes() {
        let ranges = vec![range(6, 12, "Alice")];
        let old = "hello @Alice world";
        let new = "hello  world"; // the mention was deleted
        assert!(shift_ranges(&ranges, old, new).is_empty());
    }

    #[test]
    fn shift_multibyte_safe() {
        // emoji prefix (4 bytes) before the mention
        let old = "\u{1F600} @Bob";
        let new = "\u{1F600}\u{1F600} @Bob";
        let ranges = vec![MentionRange {
            start: 5,
            end: 9,
            pubkey: test_pubkey(),
            label: "Bob".to_string(),
            hints: Vec::new(),
        }];
        let shifted = shift_ranges(&ranges, old, new);
        assert_eq!(shifted.len(), 1);
        assert_eq!(shifted[0].start, 9);
        assert_eq!(shifted[0].end, 13);
        assert_eq!(&new[shifted[0].start..shifted[0].end], "@Bob");
    }

    #[test]
    fn materialize_replaces_labels() {
        let content = "hi @Alice see this";
        let ranges = vec![range(3, 9, "Alice")];
        let out = materialize_mentions(content, &ranges);
        assert!(out.starts_with("hi nostr:nprofile1"));
        assert!(out.ends_with(" see this"));
    }

    #[test]
    fn materialize_multiple_descending() {
        let content = "@Alice and @Bob";
        let ranges = vec![range(0, 6, "Alice"), range(11, 15, "Bob")];
        let out = materialize_mentions(content, &ranges);
        assert!(out.starts_with("nostr:nprofile1"));
        assert!(out.contains(" and nostr:nprofile1"));
    }

    #[test]
    fn materialize_idempotent_when_no_ranges() {
        let content = "plain text";
        assert_eq!(materialize_mentions(content, &[]), content);
    }

    #[test]
    fn pretty_insert_replaces_typed_query() {
        let pk = test_pubkey();
        // "hi @abi there" — typed query "abi" at byte 3
        let (new_content, caret, range) =
            build_pretty_insert("hi @abi there", 3, 3, pk, "Alice", Vec::new());
        assert_eq!(new_content, "hi @Alice  there");
        assert_eq!(caret, 10); // after "@Alice "
        assert_eq!(range.start, 3);
        assert_eq!(range.end, 9); // covers "@Alice"
        assert_eq!(range.label, "Alice");
        // Materialization replaces the exact range
        let out = materialize_mentions(&new_content, &[range]);
        assert!(out.starts_with("hi nostr:nprofile1"));
    }

    #[test]
    fn pretty_insert_sanitizes_label() {
        let pk = test_pubkey();
        let (_, _, range) =
            build_pretty_insert("@jo sm", 0, 2, pk, "Jo Smith", Vec::new());
        assert_eq!(range.label, "Jo_Smith");
    }
}
