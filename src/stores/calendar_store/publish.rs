use super::*;

/// Parse a calendar event from an unsigned rumor (for NIP-59)
pub(super) fn parse_calendar_rumor(
    rumor: &UnsignedEvent,
    gift_wrap_id: &str,
) -> StdResult<CalendarEvent, String> {
    use crate::utils::nip52::{CalendarEventType, EventTime};
    let kind = rumor.kind.as_u16();
    let mut d_tag = String::new();
    let mut title = String::new();
    let mut start = None;
    let mut end = None;
    let mut summary = None;
    let mut image = None;
    let mut locations = Vec::new();
    let mut geohash = None;
    let mut start_tzid = None;
    let mut end_tzid = None;
    let mut hashtags = Vec::new();
    let mut participants = Vec::new();
    let mut references = Vec::new();
    let mut calendar_refs = Vec::new();
    for tag in rumor.tags.iter() {
        let values: Vec<&str> = tag.as_slice().iter().map(|s| s.as_str()).collect();
        if values.is_empty() {
            continue;
        }
        match values[0] {
            "d" if values.len() > 1 => d_tag = values[1].to_string(),
            "title" if values.len() > 1 => title = values[1].to_string(),
            "start" if values.len() > 1 => {
                if kind == KIND_DATE_CALENDAR_EVENT {
                    start = Some(EventTime::Date(values[1].to_string()));
                } else if let Ok(ts) = values[1].parse::<u64>() {
                    start = Some(EventTime::Timestamp(ts));
                }
            }
            "end" if values.len() > 1 => {
                if kind == KIND_DATE_CALENDAR_EVENT {
                    end = Some(EventTime::Date(values[1].to_string()));
                } else if let Ok(ts) = values[1].parse::<u64>() {
                    end = Some(EventTime::Timestamp(ts));
                }
            }
            "summary" if values.len() > 1 => summary = Some(values[1].to_string()),
            "image" if values.len() > 1 => image = Some(values[1].to_string()),
            "location" if values.len() > 1 => locations.push(values[1].to_string()),
            "g" if values.len() > 1 => geohash = Some(values[1].to_string()),
            "start_tzid" if values.len() > 1 => start_tzid = Some(values[1].to_string()),
            "end_tzid" if values.len() > 1 => end_tzid = Some(values[1].to_string()),
            "t" if values.len() > 1 => hashtags.push(values[1].to_string()),
            "r" if values.len() > 1 => references.push(values[1].to_string()),
            "a" if values.len() > 1 && values[1].starts_with("31924:") => {
                calendar_refs.push(values[1].to_string());
            }
            "p" if values.len() > 1 => {
                participants
                    .push(crate::utils::nip52::EventParticipant {
                        pubkey: values[1].to_string(),
                        relay_hint: values.get(2).map(|s| s.to_string()),
                        role: values.get(3).map(|s| s.to_string()),
                    });
            }
            _ => {}
        }
    }
    if title.is_empty() {
        return Err("Missing title".to_string());
    }
    let coordinate = format!("{}:{}:{}", kind, rumor.pubkey, d_tag);
    let event_type = if kind == KIND_DATE_CALENDAR_EVENT {
        CalendarEventType::DateBased
    } else {
        CalendarEventType::TimeBased
    };
    Ok(CalendarEvent {
        event_id: String::new(),
        pubkey: rumor.pubkey.to_string(),
        d_tag,
        coordinate: coordinate.clone(),
        naddr: String::new(),
        kind,
        event_type,
        title,
        start: start.unwrap_or(EventTime::Timestamp(0)),
        end,
        summary,
        content: rumor.content.clone(),
        image,
        locations,
        geohash,
        start_tzid,
        end_tzid,
        participants,
        hashtags,
        references,
        calendar_refs,
        color: None,
        source: EventSource::Private {
            gift_wrap_id: gift_wrap_id.to_string(),
        },
        created_at: rumor.created_at.as_secs(),
    })
}

/// Parse calendar event from unsigned event (rumor)
pub(super) fn parse_unsigned_calendar_event(
    rumor: &UnsignedEvent,
) -> StdResult<CalendarEvent, String> {
    use crate::utils::nip52::{CalendarEventType, EventParticipant};
    let kind = rumor.kind.as_u16();
    let event_type = CalendarEventType::from_kind(kind)
        .ok_or_else(|| format!("Expected kind 31922 or 31923, got {}", kind))?;
    let pubkey = rumor.pubkey.to_hex();
    let get_tag = |name: &str| -> Option<String> {
        rumor
            .tags
            .iter()
            .find(|t| t.as_slice().first().map(|s| s.as_str()) == Some(name))
            .and_then(|t| t.as_slice().get(1).map(|s| s.to_string()))
    };
    let get_all_tags = |name: &str| -> Vec<String> {
        rumor
            .tags
            .iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some(name))
            .filter_map(|t| t.as_slice().get(1).map(|s| s.to_string()))
            .collect()
    };
    let d_tag = get_tag("d").ok_or("Missing required 'd' tag")?;
    let coordinate = format!("{}:{}:{}", kind, pubkey, d_tag);
    let naddr = "".to_string();
    let title = get_tag("title")
        .or_else(|| get_tag("name"))
        .ok_or("Missing required 'title' tag")?;
    let start_str = get_tag("start").ok_or("Missing required 'start' tag")?;
    let start = EventTime::parse(&start_str, event_type)
        .ok_or_else(|| format!("Invalid start time: {}", start_str))?;
    let end = get_tag("end").and_then(|s| EventTime::parse(&s, event_type));
    let participants: Vec<EventParticipant> = rumor
        .tags
        .iter()
        .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("p"))
        .filter_map(|t| {
            let slice = t.as_slice();
            let pk = slice.get(1)?.to_string();
            let relay = slice.get(2).map(|s| s.to_string()).filter(|s| !s.is_empty());
            let role = slice.get(3).map(|s| s.to_string()).filter(|s| !s.is_empty());
            Some(EventParticipant {
                pubkey: pk,
                relay_hint: relay,
                role,
            })
        })
        .collect();
    Ok(CalendarEvent {
        d_tag,
        event_id: "".to_string(),
        pubkey,
        naddr,
        coordinate,
        kind,
        event_type,
        created_at: rumor.created_at.as_secs(),
        title,
        start,
        end,
        start_tzid: get_tag("start_tzid").or_else(|| get_tag("timezone")),
        end_tzid: get_tag("end_tzid"),
        summary: get_tag("summary"),
        content: rumor.content.clone(),
        image: get_tag("image"),
        locations: get_all_tags("location"),
        geohash: get_tag("g"),
        participants,
        hashtags: get_all_tags("t"),
        references: get_all_tags("r"),
        calendar_refs: Vec::new(),
        color: None,
        source: EventSource::Public,
    })
}

/// Helper to encode naddr (simplified)
pub(super) fn encode_naddr(kind: u16, pubkey: &str, d_tag: &str) -> String {
    use nostr::nips::nip01::Coordinate;
    use nostr::nips::nip19::ToBech32;
    if let Ok(pk) = PublicKey::from_hex(pubkey) {
        let coord = Coordinate::new(Kind::Custom(kind), pk).identifier(d_tag);
        if let Ok(bech32) = coord.to_bech32() {
            return bech32;
        }
    }
    format!("{}:{}:{}", kind, pubkey, d_tag)
}

/// Publish a comment on a calendar event
/// Uses proper NIP-22 threading tags (A/K/P for root, a/k/p for parent)
/// Author is derived from coordinate (format: kind:pubkey:d-tag) to ensure consistency
pub async fn publish_event_comment(
    coordinate: &str,
    content: &str,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 {
        return Err(
            format!(
                "Invalid coordinate format '{}': expected 'kind:pubkey:d-tag'",
                coordinate,
            ),
        );
    }
    let event_kind: u16 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid kind in coordinate: {}", parts[0]))?;
    let author_pubkey = PublicKey::parse(parts[1])
        .map_err(|e| format!("Invalid pubkey in coordinate '{}': {}", parts[1], e))?;
    let author_hex = author_pubkey.to_hex();
    let builder = EventBuilder::new(Kind::Custom(1111), content)
        .tag(
            Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::A)),
                vec![coordinate.to_string()],
            ),
        )
        .tag(
            Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::K)),
                vec![event_kind.to_string()],
            ),
        )
        .tag(
            Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::P)),
                vec![author_hex.clone()],
            ),
        )
        .tag(
            Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::A)),
                vec![coordinate.to_string()],
            ),
        )
        .tag(
            Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::K)),
                vec![event_kind.to_string()],
            ),
        )
        .tag(
            Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::P)),
                vec![author_hex],
            ),
        );
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish comment: {}", e))?;
    Ok(output.id().to_string())
}

/// Publish a date-based calendar event (kind 31922)
/// participants: slice of (pubkey_hex, role) tuples
#[allow(clippy::too_many_arguments)]
pub async fn publish_date_event(
    title: &str,
    start_date: &str,
    end_date: Option<&str>,
    summary: Option<&str>,
    content: Option<&str>,
    image: Option<&str>,
    locations: &[String],
    hashtags: &[String],
    participants: &[(String, String)],
    is_private: bool,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let d_tag = format!("event-{}", js_sys::Date::now() as u64);
    let mut builder = EventBuilder::new(
            Kind::Custom(KIND_DATE_CALENDAR_EVENT),
            content.unwrap_or(""),
        )
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("title".into()), vec![title.to_string()]))
        .tag(Tag::custom(TagKind::Custom("start".into()), vec![start_date.to_string()]));
    if let Some(end) = end_date {
        builder = builder
            .tag(Tag::custom(TagKind::Custom("end".into()), vec![end.to_string()]));
    }
    if let Some(s) = summary {
        builder = builder
            .tag(Tag::custom(TagKind::Custom("summary".into()), vec![s.to_string()]));
    }
    if let Some(img) = image {
        builder = builder
            .tag(Tag::custom(TagKind::Custom("image".into()), vec![img.to_string()]));
    }
    for loc in locations {
        builder = builder
            .tag(Tag::custom(TagKind::Custom("location".into()), vec![loc.to_string()]));
    }
    for tag in hashtags {
        builder = builder.tag(Tag::hashtag(tag));
    }
    for (pubkey, role) in participants {
        match PublicKey::parse(pubkey) {
            Ok(pk) => {
                builder = builder
                    .tag(
                        Tag::custom(
                            TagKind::p(),
                            vec![pk.to_hex(), "".to_string(), role.clone()],
                        ),
                    );
            }
            Err(e) => {
                log::warn!("Invalid participant pubkey '{}': {}", pubkey, e);
            }
        }
    }
    if is_private {
        return Err("Private events not yet implemented".to_string());
    }
    let _output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish event: {}", e))?;
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let naddr = format!(
        "nostr:{}",
        encode_naddr(KIND_DATE_CALENDAR_EVENT, &pubkey, &d_tag),
    );
    Ok(naddr)
}
/// Publish a time-based calendar event (kind 31923)
/// participants: slice of (pubkey_hex, role) tuples
#[allow(clippy::too_many_arguments)]
pub async fn publish_time_event(
    title: &str,
    start_timestamp: u64,
    end_timestamp: Option<u64>,
    summary: Option<&str>,
    content: Option<&str>,
    image: Option<&str>,
    locations: &[String],
    hashtags: &[String],
    participants: &[(String, String)],
    timezone: Option<&str>,
    is_private: bool,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let d_tag = format!("event-{}", js_sys::Date::now() as u64);
    let mut builder = EventBuilder::new(
            Kind::Custom(KIND_TIME_CALENDAR_EVENT),
            content.unwrap_or(""),
        )
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("title".into()), vec![title.to_string()]))
        .tag(
            Tag::custom(
                TagKind::Custom("start".into()),
                vec![start_timestamp.to_string()],
            ),
        );
    if let Some(end) = end_timestamp {
        builder = builder
            .tag(Tag::custom(TagKind::Custom("end".into()), vec![end.to_string()]));
    }
    if let Some(s) = summary {
        builder = builder
            .tag(Tag::custom(TagKind::Custom("summary".into()), vec![s.to_string()]));
    }
    if let Some(img) = image {
        builder = builder
            .tag(Tag::custom(TagKind::Custom("image".into()), vec![img.to_string()]));
    }
    if let Some(tz) = timezone {
        builder = builder
            .tag(
                Tag::custom(TagKind::Custom("start_tzid".into()), vec![tz.to_string()]),
            );
    }
    for loc in locations {
        builder = builder
            .tag(Tag::custom(TagKind::Custom("location".into()), vec![loc.to_string()]));
    }
    for tag in hashtags {
        builder = builder.tag(Tag::hashtag(tag));
    }
    for (pubkey, role) in participants {
        match PublicKey::parse(pubkey) {
            Ok(pk) => {
                builder = builder
                    .tag(
                        Tag::custom(
                            TagKind::p(),
                            vec![pk.to_hex(), "".to_string(), role.clone()],
                        ),
                    );
            }
            Err(e) => {
                log::warn!("Invalid participant pubkey '{}': {}", pubkey, e);
            }
        }
    }
    if is_private {
        return Err("Private events not yet implemented".to_string());
    }
    let _output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish event: {}", e))?;
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let naddr = format!(
        "nostr:{}",
        encode_naddr(KIND_TIME_CALENDAR_EVENT, &pubkey, &d_tag),
    );
    Ok(naddr)
}

/// Publish a calendar event RSVP
pub async fn publish_rsvp(
    event_coordinate: &str,
    event_author: &str,
    status: crate::utils::nip52::RsvpStatus,
    note: &str,
) -> StdResult<String, String> {
    use crate::utils::nip52::FreeBusy;
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let d_tag = format!("rsvp-{}", js_sys::Date::now() as u64);
    let free_busy = FreeBusy::from_rsvp_status(status);
    let event = EventBuilder::new(Kind::Custom(KIND_CALENDAR_RSVP), note)
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::a(), vec![event_coordinate.to_string()]))
        .tag(
            Tag::public_key(
                PublicKey::from_hex(event_author)
                    .map_err(|e| format!("Invalid author pubkey: {}", e))?,
            ),
        )
        .tag(
            Tag::custom(
                TagKind::Custom("status".into()),
                vec![status.as_str().to_string()],
            ),
        )
        .tag(
            Tag::custom(
                TagKind::Custom("fb".into()),
                vec![free_busy.as_str().to_string()],
            ),
        );
    let output = client
        .send_event_builder(event)
        .await
        .map_err(|e| format!("Failed to publish RSVP: {}", e))?;
    Ok(output.id().to_string())
}

/// Publish an availability template (kind 31926)
#[allow(clippy::too_many_arguments)]
pub async fn publish_availability_template(
    title: &str,
    duration_minutes: u32,
    interval_minutes: Option<u32>,
    buffer_before: Option<u32>,
    buffer_after: Option<u32>,
    timezone: Option<&str>,
    min_notice_days: Option<u32>,
    max_advance_days: Option<u32>,
    amount_sats: Option<u64>,
    schedule: &[(String, String, String)],
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let d_tag = format!("avail-{}", js_sys::Date::now() as u64);
    let mut builder = EventBuilder::new(Kind::Custom(KIND_AVAILABILITY_TEMPLATE), "")
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("title".into()), vec![title.to_string()]))
        .tag(
            Tag::custom(
                TagKind::Custom("duration".into()),
                vec![format!("PT{}M", duration_minutes)],
            ),
        );
    if let Some(interval) = interval_minutes {
        builder = builder
            .tag(
                Tag::custom(
                    TagKind::Custom("interval".into()),
                    vec![format!("PT{}M", interval)],
                ),
            );
    }
    if let Some(before) = buffer_before {
        builder = builder
            .tag(
                Tag::custom(
                    TagKind::Custom("buffer_before".into()),
                    vec![format!("PT{}M", before)],
                ),
            );
    }
    if let Some(after) = buffer_after {
        builder = builder
            .tag(
                Tag::custom(
                    TagKind::Custom("buffer_after".into()),
                    vec![format!("PT{}M", after)],
                ),
            );
    }
    if let Some(tz) = timezone {
        builder = builder
            .tag(Tag::custom(TagKind::Custom("tzid".into()), vec![tz.to_string()]));
    }
    if let Some(notice) = min_notice_days {
        builder = builder
            .tag(
                Tag::custom(
                    TagKind::Custom("min_notice".into()),
                    vec![format!("P{}D", notice)],
                ),
            );
    }
    if let Some(advance) = max_advance_days {
        builder = builder
            .tag(
                Tag::custom(
                    TagKind::Custom("max_advance".into()),
                    vec![format!("P{}D", advance)],
                ),
            );
    }
    if let Some(amount) = amount_sats {
        builder = builder
            .tag(
                Tag::custom(TagKind::Custom("amount".into()), vec![amount.to_string()]),
            );
    }
    for (day, start, end) in schedule {
        builder = builder
            .tag(
                Tag::custom(
                    TagKind::Custom("sch".into()),
                    vec![day.clone(), start.clone(), end.clone()],
                ),
            );
    }
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish availability template: {}", e))?;
    Ok(output.id().to_string())
}
/// Publish an availability block (kind 31927)
pub async fn publish_availability_block(
    start: u64,
    end: u64,
    title: Option<&str>,
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let d_tag = format!("block-{}", js_sys::Date::now() as u64);
    let mut builder = EventBuilder::new(Kind::Custom(KIND_AVAILABILITY_BLOCK), "")
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("start".into()), vec![start.to_string()]))
        .tag(Tag::custom(TagKind::Custom("end".into()), vec![end.to_string()]));
    if let Some(t) = title {
        builder = builder
            .tag(Tag::custom(TagKind::Custom("title".into()), vec![t.to_string()]));
    }
    let output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish availability block: {}", e))?;
    Ok(output.id().to_string())
}

/// Publish a calendar collection (kind 31924)
pub async fn publish_calendar(
    title: &str,
    description: &str,
    event_coordinates: &[String],
) -> StdResult<String, String> {
    use crate::utils::nip52::KIND_CALENDAR;
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let d_tag = format!("calendar-{}", js_sys::Date::now() as u64);
    let mut builder = EventBuilder::new(Kind::Custom(KIND_CALENDAR), description)
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("title".into()), vec![title.to_string()]));
    for coord in event_coordinates {
        builder = builder.tag(Tag::custom(TagKind::a(), vec![coord.clone()]));
    }
    let _output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish calendar: {}", e))?;
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let naddr = encode_naddr(KIND_CALENDAR, &pubkey, &d_tag);
    Ok(naddr)
}
