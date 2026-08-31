use nostr_sdk::nips::nip19::Nip19;
use nostr_sdk::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum SearchType {
    FullText(ParsedSearchQuery),
    ProfileLookup {
        pubkey: PublicKey,
        relays: Vec<String>,
    },
    NoteLookup {
        event_id: EventId,
        author: Option<PublicKey>,
        relays: Vec<String>,
    },
    AddressLookup {
        coordinate: Coordinate,
        relays: Vec<String>,
    },
    Hashtag(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParsedSearchQuery {
    pub raw: String,
    pub text: String,
    pub authors: Vec<PublicKey>,
    pub author_names: Vec<String>,
    pub kinds: Vec<Kind>,
    pub since: Option<Timestamp>,
    pub until: Option<Timestamp>,
    pub hashtags: Vec<String>,
    pub exclude_terms: Vec<String>,
    pub language: Option<String>,
    pub domain: Option<String>,
}

pub fn detect_search_type(input: &str) -> SearchType {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return SearchType::FullText(ParsedSearchQuery::default());
    }

    let normalized = trimmed
        .strip_prefix("nostr:")
        .or_else(|| trimmed.strip_prefix("NOSTR:"))
        .unwrap_or(trimmed);

    if let Ok(nip19) = Nip19::from_bech32(normalized) {
        return match nip19 {
            Nip19::Pubkey(pk) => SearchType::ProfileLookup {
                pubkey: pk,
                relays: vec![],
            },
            Nip19::Profile(p) => SearchType::ProfileLookup {
                pubkey: p.public_key,
                relays: p.relays.iter().map(|r| r.to_string()).collect(),
            },
            Nip19::EventId(id) => SearchType::NoteLookup {
                event_id: id,
                author: None,
                relays: vec![],
            },
            Nip19::Event(e) => SearchType::NoteLookup {
                event_id: e.event_id,
                author: e.author,
                relays: e.relays.iter().map(|r| r.to_string()).collect(),
            },
            Nip19::Coordinate(c) => SearchType::AddressLookup {
                coordinate: c.coordinate,
                relays: c.relays.iter().map(|r| r.to_string()).collect(),
            },
            _ => SearchType::FullText(parse_query(trimmed)),
        };
    }

    if let Ok(pk) = PublicKey::from_hex(normalized) {
        return SearchType::ProfileLookup {
            pubkey: pk,
            relays: vec![],
        };
    }

    if let Ok(id) = EventId::from_hex(normalized) {
        return SearchType::NoteLookup {
            event_id: id,
            author: None,
            relays: vec![],
        };
    }

    if let Some(tag) = normalized.strip_prefix('#') {
        if !tag.is_empty() && !tag.contains(' ') && !tag.contains(':') {
            return SearchType::Hashtag(tag.to_lowercase());
        }
    }

    SearchType::FullText(parse_query(trimmed))
}

pub fn parse_query(input: &str) -> ParsedSearchQuery {
    let mut result = ParsedSearchQuery {
        raw: input.to_string(),
        ..Default::default()
    };

    let tokens = tokenize(input);
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];

        if token.starts_with('-') && token.len() > 1 {
            result.exclude_terms.push(token[1..].to_lowercase());
            i += 1;
            continue;
        }

        if token.starts_with('#') && token.len() > 1 {
            result.hashtags.push(token[1..].to_lowercase());
            i += 1;
            continue;
        }

        if let Some(value) = token.strip_prefix("from:") {
            if let Ok(pk) = PublicKey::from_hex(value) {
                result.authors.push(pk);
            } else if let Ok(nip19) = Nip19::from_bech32(value) {
                match nip19 {
                    Nip19::Pubkey(pk) => result.authors.push(pk),
                    Nip19::Profile(p) => result.authors.push(p.public_key),
                    _ => result.author_names.push(value.to_string()),
                }
            } else {
                result.author_names.push(value.to_string());
            }
            i += 1;
            continue;
        }

        if let Some(value) = token.strip_prefix("kind:") {
            if let Some(kind) = resolve_kind(value) {
                result.kinds.push(kind);
            } else if let Ok(k) = value.parse::<u16>() {
                result.kinds.push(Kind::from(k));
            }
            i += 1;
            continue;
        }

        if let Some(value) = token.strip_prefix("since:") {
            if let Some(ts) = parse_time_value(value) {
                result.since = Some(ts);
            }
            i += 1;
            continue;
        }

        if let Some(value) = token.strip_prefix("until:") {
            if let Some(ts) = parse_time_value(value) {
                result.until = Some(ts);
            }
            i += 1;
            continue;
        }

        if let Some(value) = token.strip_prefix("lang:") {
            result.language = Some(value.to_lowercase());
            i += 1;
            continue;
        }

        if let Some(value) = token.strip_prefix("domain:") {
            result.domain = Some(value.to_lowercase());
            i += 1;
            continue;
        }

        if !result.text.is_empty() {
            result.text.push(' ');
        }
        result.text.push_str(token);
        i += 1;
    }

    result
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            if in_quotes {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                in_quotes = false;
            } else {
                in_quotes = true;
            }
            i += 1;
            continue;
        }
        if c == ' ' && !in_quotes {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            i += 1;
            continue;
        }
        current.push(c);
        i += 1;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn resolve_kind(name: &str) -> Option<Kind> {
    match name.to_lowercase().as_str() {
        "note" | "text" | "post" | "1" => Some(Kind::TextNote),
        "article" | "longform" | "30023" => Some(Kind::LongFormTextNote),
        "photo" | "image" | "20" => Some(Kind::Custom(20)),
        "video" | "21" => Some(Kind::Custom(21)),
        "repost" | "6" => Some(Kind::Repost),
        "reaction" | "7" => Some(Kind::Reaction),
        "metadata" | "0" => Some(Kind::Metadata),
        "channel" | "40" => Some(Kind::ChannelCreation),
        _ => None,
    }
}

fn parse_time_value(input: &str) -> Option<Timestamp> {
    if let Some(ts) = parse_relative_time(input) {
        return Some(ts);
    }
    parse_date(input)
}

fn parse_relative_time(input: &str) -> Option<Timestamp> {
    let input_lower = input.to_lowercase();
    let now_secs = Timestamp::now().as_secs();

    let (num_str, multiplier) = if input_lower.ends_with('h') {
        (&input_lower[..input_lower.len() - 1], 3600u64)
    } else if input_lower.ends_with('d') {
        (&input_lower[..input_lower.len() - 1], 86400u64)
    } else if input_lower.ends_with('w') {
        (&input_lower[..input_lower.len() - 1], 604800u64)
    } else if input_lower.ends_with('m') {
        (&input_lower[..input_lower.len() - 1], 60u64)
    } else if input_lower.ends_with('y') {
        (&input_lower[..input_lower.len() - 1], 31536000u64)
    } else {
        return None;
    };

    let num: u64 = num_str.parse().ok()?;
    Some(Timestamp::from(now_secs.saturating_sub(num * multiplier)))
}

fn parse_date(input: &str) -> Option<Timestamp> {
    let parts: Vec<&str> = input.split('-').collect();
    match parts.len() {
        1 => {
            let year: u32 = parts[0].parse().ok()?;
            if !(2000..=2100).contains(&year) {
                return None;
            }
            naive_datetime_to_timestamp(year, 1, 1, 0, 0, 0)
        }
        2 => {
            let year: u32 = parts[0].parse().ok()?;
            let month: u32 = parts[1].parse().ok()?;
            if !(1..=12).contains(&month) {
                return None;
            }
            naive_datetime_to_timestamp(year, month, 1, 0, 0, 0)
        }
        3 => {
            let year: u32 = parts[0].parse().ok()?;
            let month: u32 = parts[1].parse().ok()?;
            let day: u32 = parts[2].parse().ok()?;
            if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
                return None;
            }
            naive_datetime_to_timestamp(year, month, day, 0, 0, 0)
        }
        _ => None,
    }
}

fn naive_datetime_to_timestamp(
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<Timestamp> {
    let days = [
        0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365,
    ];
    let y = (year as u64).saturating_sub(1970);
    let leap_years = (y + 1) / 4 - (y + 69) / 100 + (y + 369) / 400;
    let is_leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let day_of_year = days
        .get(month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or(0)
        + day
        + if is_leap && month > 2 { 1 } else { 0 };
    let total_days = y * 365 + leap_years + day_of_year as u64 - 1;
    let total_secs = total_days * 86400
        + hour as u64 * 3600
        + minute as u64 * 60
        + second as u64;
    Some(Timestamp::from(total_secs))
}

/// Build relay `Filter`s for a parsed query.
///
/// `default_kinds` supplies the search-tab's kind scope (Posts/Articles/
/// Photos/Videos); it is only used when the query itself has no `kind:`
/// operators. `None` falls back to the built-in multi-group defaults.
pub fn build_search_filters(
    query: &ParsedSearchQuery,
    limit: usize,
    default_kinds: Option<Vec<Kind>>,
) -> Vec<Filter> {
    if query.text.is_empty()
        && query.hashtags.is_empty()
        && query.authors.is_empty()
        && query.author_names.is_empty()
        && query.kinds.is_empty()
        && query.since.is_none()
        && query.until.is_none()
    {
        return vec![];
    }

    let mut search_string = query.text.clone();
    if let Some(lang) = &query.language {
        if !search_string.is_empty() {
            search_string.push(' ');
        }
        search_string.push_str(&format!("language:{}", lang));
    }
    if let Some(domain) = &query.domain {
        if !search_string.is_empty() {
            search_string.push(' ');
        }
        search_string.push_str(&format!("domain:{}", domain));
    }

    let kind_groups: Vec<Vec<Kind>> = if !query.kinds.is_empty() {
        vec![query.kinds.clone()]
    } else {
        match default_kinds {
            Some(kinds) => vec![kinds],
            None => vec![
                vec![Kind::TextNote, Kind::Repost, Kind::GenericRepost],
                vec![Kind::LongFormTextNote],
                vec![Kind::Custom(20), Kind::Custom(21), Kind::Custom(22)],
            ],
        }
    };

    let mut filters = Vec::new();
    for kinds in kind_groups {
        let mut filter = Filter::new();

        filter = filter.kinds(kinds);

        if !search_string.is_empty() {
            filter = filter.search(&search_string);
        }

        if !query.authors.is_empty() {
            filter = filter.authors(query.authors.iter().cloned());
        }

        if !query.hashtags.is_empty() {
            filter = filter.hashtags(query.hashtags.iter().map(|s| s.as_str()));
        }

        if let Some(since) = query.since {
            filter = filter.since(since);
        }
        if let Some(until) = query.until {
            filter = filter.until(until);
        }

        filter = filter.limit(limit);
        filters.push(filter);
    }

    filters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_npub() {
        let keys = Keys::generate();
        let npub = keys.public_key().to_bech32().unwrap();
        match detect_search_type(&npub) {
            SearchType::ProfileLookup { pubkey, .. } => {
                assert_eq!(pubkey, keys.public_key());
            }
            _ => panic!("Expected ProfileLookup"),
        }
    }

    #[test]
    fn test_detect_hashtag() {
        match detect_search_type("#nostr") {
            SearchType::Hashtag(tag) => assert_eq!(tag, "nostr"),
            _ => panic!("Expected Hashtag"),
        }
    }

    #[test]
    fn test_detect_free_text() {
        match detect_search_type("hello world") {
            SearchType::FullText(q) => {
                assert_eq!(q.text, "hello world");
            }
            _ => panic!("Expected FullText"),
        }
    }

    #[test]
    fn test_parse_kind_operator() {
        let q = parse_query("kind:note hello");
        assert_eq!(q.kinds, vec![Kind::TextNote]);
        assert_eq!(q.text, "hello");
    }

    #[test]
    fn test_parse_since_operator() {
        let q = parse_query("since:7d test");
        assert!(q.since.is_some());
        assert!(q.since.unwrap().as_secs() > 0);
        assert_eq!(q.text, "test");
    }

    #[test]
    fn test_parse_exclude() {
        let q = parse_query("hello -spam");
        assert_eq!(q.text, "hello");
        assert_eq!(q.exclude_terms, vec!["spam"]);
    }

    #[test]
    fn test_parse_hashtag_operator() {
        let q = parse_query("#bitcoin price");
        assert_eq!(q.hashtags, vec!["bitcoin"]);
        assert_eq!(q.text, "price");
    }

    #[test]
    fn test_build_filters() {
        let q = parse_query("kind:note hello");
        let filters = build_search_filters(&q, 50, None);
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].kinds, Some(vec![Kind::TextNote].into_iter().collect()));
        assert_eq!(filters[0].search, Some("hello".to_string()));
    }

    #[test]
    fn test_build_filters_default_groups() {
        let q = parse_query("hello");
        let filters = build_search_filters(&q, 50, None);
        assert_eq!(filters.len(), 3);
    }

    #[test]
    fn test_build_filters_tab_kinds() {
        // Tab kinds collapse to a single group when the query has no kind: op
        let q = parse_query("hello");
        let filters = build_search_filters(&q, 50, Some(vec![Kind::TextNote]));
        assert_eq!(filters.len(), 1);
        assert_eq!(
            filters[0].kinds,
            Some(vec![Kind::TextNote].into_iter().collect())
        );

        // Explicit kind: operators win over the tab scope
        let q = parse_query("kind:article hello");
        let filters = build_search_filters(&q, 50, Some(vec![Kind::TextNote]));
        assert_eq!(filters.len(), 1);
        assert_eq!(
            filters[0].kinds,
            Some(vec![Kind::LongFormTextNote].into_iter().collect())
        );
    }

    #[test]
    fn test_relative_time_parsing() {
        let ts = parse_relative_time("1h");
        assert!(ts.is_some());
        let ts = parse_relative_time("7d");
        assert!(ts.is_some());
        let ts = parse_relative_time("30m");
        assert!(ts.is_some());
        let ts = parse_relative_time("1y");
        assert!(ts.is_some());
    }

    #[test]
    fn test_date_parsing() {
        let ts = parse_date("2025-01");
        assert!(ts.is_some());
        let ts = parse_date("2025-06-15");
        assert!(ts.is_some());
        let ts = parse_date("2024");
        assert!(ts.is_some());
    }

    #[test]
    fn test_quoted_strings() {
        let q = parse_query("\"exact phrase\" hello");
        assert!(q.text.contains("exact phrase"));
    }

    #[test]
    fn test_language_operator() {
        let q = parse_query("lang:en hello");
        assert_eq!(q.language, Some("en".to_string()));
        assert_eq!(q.text, "hello");
    }

    #[test]
    fn test_domain_operator() {
        let q = parse_query("domain:example.com hello");
        assert_eq!(q.domain, Some("example.com".to_string()));
        assert_eq!(q.text, "hello");
    }

    #[test]
    fn test_detect_note_id() {
        let event_id = EventId::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
        let nevent = Nip19Event::new(event_id);
        let bech32 = Nip19::Event(nevent).to_bech32().unwrap();
        match detect_search_type(&bech32) {
            SearchType::NoteLookup { .. } => {}
            other => panic!("Expected NoteLookup, got {:?}", other),
        }
    }
}
