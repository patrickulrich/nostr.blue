use super::*;

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
    use nostr::nips::nip01::Coordinate;
    let coord = Coordinate::parse(coordinate)
        .map_err(|_| format!("Invalid coordinate format: {}", coordinate))?;
    let event_kind = coord.kind.as_u16();
    let author_hex = coord.public_key.to_hex();
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
) -> StdResult<String, String> {
    // Validate date formats (NIP-52: YYYY-MM-DD)
    let parsed_start = chrono::NaiveDate::parse_from_str(start_date, "%Y-%m-%d")
        .map_err(|_| format!("Invalid start date format '{}', expected YYYY-MM-DD", start_date))?;
    if let Some(end) = end_date {
        let parsed_end = chrono::NaiveDate::parse_from_str(end, "%Y-%m-%d")
            .map_err(|_| format!("Invalid end date format '{}', expected YYYY-MM-DD", end))?;
        if parsed_end < parsed_start {
            return Err("End date must be on or after start date".to_string());
        }
    }

    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let d_tag = format!("event-{}-{}", crate::platform::timestamp::now_millis(), rand::random::<f64>());
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
    let mut invalid_pubkeys = Vec::new();
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
            Err(_) => {
                invalid_pubkeys.push(pubkey.clone());
            }
        }
    }
    if !invalid_pubkeys.is_empty() {
        return Err(format!("Invalid participant pubkeys: {}", invalid_pubkeys.join(", ")));
    }
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let _output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish event: {}", e))?;
    let naddr = encode_naddr(KIND_DATE_CALENDAR_EVENT, &pubkey, &d_tag);
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
) -> StdResult<String, String> {
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let d_tag = format!("event-{}-{}", crate::platform::timestamp::now_millis(), rand::random::<f64>());
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
        if end < start_timestamp {
            return Err("End timestamp must be >= start timestamp".to_string());
        }
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
    let mut invalid_pubkeys = Vec::new();
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
            Err(_) => {
                invalid_pubkeys.push(pubkey.clone());
            }
        }
    }
    if !invalid_pubkeys.is_empty() {
        return Err(format!("Invalid participant pubkeys: {}", invalid_pubkeys.join(", ")));
    }
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let _output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish event: {}", e))?;
    let naddr = encode_naddr(KIND_TIME_CALENDAR_EVENT, &pubkey, &d_tag);
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
    use nostr::nips::nip01::Coordinate;
    Coordinate::parse(event_coordinate)
        .map_err(|_| format!("Invalid event coordinate: {}", event_coordinate))?;
    let d_tag = format!("rsvp-{}-{}", crate::platform::timestamp::now_millis(), rand::random::<f64>());
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
    let d_tag = format!("avail-{}-{}", crate::platform::timestamp::now_millis(), rand::random::<f64>());
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
    const VALID_DAYS: &[&str] = &["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
    for (day, start, end) in schedule {
        let day_lower = day.to_lowercase();
        if !VALID_DAYS.contains(&day_lower.as_str()) {
            return Err(format!("Invalid day name: {}", day));
        }
        if chrono::NaiveTime::parse_from_str(start, "%H:%M").is_err()
            && chrono::NaiveTime::parse_from_str(start, "%H:%M:%S").is_err()
        {
            return Err(format!("Invalid start time format: {}", start));
        }
        if chrono::NaiveTime::parse_from_str(end, "%H:%M").is_err()
            && chrono::NaiveTime::parse_from_str(end, "%H:%M:%S").is_err()
        {
            return Err(format!("Invalid end time format: {}", end));
        }
        builder = builder
            .tag(
                Tag::custom(
                    TagKind::Custom("sch".into()),
                    vec![day_lower, start.clone(), end.clone()],
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
    if end <= start {
        return Err("End time must be after start time".to_string());
    }
    let client = crate::stores::nostr_client::get_client()
        .ok_or("Client not initialized")?;
    let d_tag = format!("block-{}-{}", crate::platform::timestamp::now_millis(), rand::random::<f64>());
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
    use nostr::nips::nip01::Coordinate;
    let d_tag = format!("calendar-{}-{}", crate::platform::timestamp::now_millis(), rand::random::<f64>());
    let mut builder = EventBuilder::new(Kind::Custom(KIND_CALENDAR), description)
        .tag(Tag::identifier(&d_tag))
        .tag(Tag::custom(TagKind::Custom("title".into()), vec![title.to_string()]));
    for coord in event_coordinates {
        Coordinate::parse(coord)
            .map_err(|_| format!("Invalid event coordinate: {}", coord))?;
        builder = builder.tag(Tag::custom(TagKind::a(), vec![coord.clone()]));
    }
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let _output = client
        .send_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to publish calendar: {}", e))?;
    let naddr = encode_naddr(KIND_CALENDAR, &pubkey, &d_tag);
    Ok(naddr)
}
