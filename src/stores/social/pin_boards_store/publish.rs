use super::*;

pub async fn publish_pinboard(
    input: PinboardInput,
    existing_d_tag: Option<&str>,
) -> std::result::Result<String, String> {
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish pinboard.".to_string());
    }
    let d_tag = existing_d_tag
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            // Titles made only of symbols (or empty) would slugify to "";
            // fall back to a random id so boards don't collide on "".
            let slug = crate::utils::slugify(&input.title);
            if slug.is_empty() {
                crate::utils::generate_option_id()
            } else {
                slug
            }
        });
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&d_tag),
        Tag::custom(TagKind::Custom("title".into()), vec![input.title.clone()]),
    ];
    if let Some(ref desc) = input.description {
        tags.push(Tag::custom(
            TagKind::Custom("description".into()),
            vec![desc.clone()],
        ));
    }
    if let Some(ref img) = input.image {
        tags.push(Tag::custom(
            TagKind::Custom("image".into()),
            vec![img.clone()],
        ));
    }
    for tag in &input.tags {
        tags.push(Tag::hashtag(tag));
    }
    if input.collaborative {
        tags.push(Tag::custom(
            TagKind::Custom("collaborative".into()),
            Vec::<String>::new(),
        ));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_PINBOARD), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign pinboard: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::PinBoard,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Pinboard published: {}", event_id);
    let pubkey = crate::stores::nostr_client::get_cached_pubkey()
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;
    let naddr = nostr_client::make_naddr_with_hints(KIND_PINBOARD, &pubkey, &d_tag).await?;
    Ok(naddr)
}

/// Union new board addresses with an existing pin's boards, preserving order
/// (new selections first) and removing duplicates.
pub(crate) fn union_board_addresses(new: Vec<String>, existing: Vec<String>) -> Vec<String> {
    let mut merged: Vec<String> = Vec::with_capacity(new.len() + existing.len());
    for addr in new.into_iter().chain(existing) {
        if !addr.is_empty() && !merged.contains(&addr) {
            merged.push(addr);
        }
    }
    merged
}

/// Find the current user's existing pin for the same content reference
/// (same `d` coordinate). Pins are addressable (NIP-01), so re-pinning
/// replaces the prior event; the caller merges board A tags to avoid
/// silently removing the pin from earlier boards.
async fn fetch_my_existing_pin_boards(d_tag: &str) -> Vec<String> {
    // Cache scan first (pins are keyed by event id, so scan values).
    if let Some(pubkey) = crate::stores::auth_store::get_pubkey() {
        let cached = PINS_CACHE
            .read()
            .iter()
            .find(|(_, pin)| pin.pubkey == pubkey && pin.d_tag == d_tag)
            .map(|(_, pin)| pin.board_addresses.clone());
        if let Some(boards) = cached {
            return boards;
        }
    }
    // Fall back to a relay lookup by coordinate.
    let pubkey = match crate::stores::auth_store::get_pubkey() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let Ok(pubkey) = nostr::PublicKey::from_hex(&pubkey) else {
        return Vec::new();
    };
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_PIN))
        .author(pubkey)
        .identifier(d_tag)
        .limit(1);
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await {
        Ok(events) => events
            .iter()
            .filter_map(|e| {
                parse_pin_event(e).map(|pin| {
                    pin.board_addresses
                        .into_iter()
                        .filter(|a| a.starts_with("30067:"))
                        .collect::<Vec<_>>()
                })
            })
            .flatten()
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub async fn publish_pin(input: PinInput) -> std::result::Result<String, String> {
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish pin.".to_string());
    }
    // Spec: the pin d tag is derived from the referenced content and is
    // the first tag.
    let d_tag = input.reference.d_tag_value();
    // Re-pinning the same content replaces the prior pin event (addressable);
    // merge its board refs so it stays pinned to earlier boards.
    let board_addresses =
        union_board_addresses(input.board_addresses.clone(), fetch_my_existing_pin_boards(&d_tag).await);
    let mut tags: Vec<Tag> = vec![Tag::identifier(&d_tag)];
    for board_addr in &board_addresses {
        let coord_opt = if board_addr.starts_with("naddr1") {
            Coordinate::from_bech32(board_addr).ok()
        } else {
            Coordinate::parse(board_addr).ok()
        };
        if let Some(coord) = coord_opt {
            tags.push(Tag::from_standardized(TagStandard::Coordinate {
                coordinate: coord,
                relay_url: None,
                uppercase: true,
            }));
        } else {
            log::warn!("Failed to parse board address: {}", board_addr);
            tags.push(Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::uppercase(Alphabet::A)),
                vec![board_addr.clone()],
            ));
        }
    }
    match &input.reference {
        PinReference::Event { id, relay_hint } => {
            let mut vals = vec![id.clone()];
            if let Some(relay) = relay_hint {
                vals.push(relay.clone());
            }
            tags.push(Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)),
                vals,
            ));
        }
        PinReference::Coordinate {
            address,
            relay_hint,
        } => {
            let mut vals = vec![address.clone()];
            if let Some(relay) = relay_hint {
                vals.push(relay.clone());
            }
            tags.push(Tag::custom(
                TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::A)),
                vals,
            ));
        }
        PinReference::External { content, hint } => {
            tags.push(Tag::from_standardized(TagStandard::ExternalContent {
                content: content.clone(),
                hint: hint.as_ref().and_then(|h| Url::parse(h).ok()),
                uppercase: false,
            }));
            tags.push(Tag::from_standardized(TagStandard::Nip73Kind {
                kind: content.kind(),
                uppercase: false,
            }));
        }
    }
    if let Some(ref title) = input.title {
        tags.push(Tag::custom(
            TagKind::Custom("title".into()),
            vec![title.clone()],
        ));
    }
    if let Some(ref image) = input.image {
        tags.push(Tag::custom(
            TagKind::Custom("image".into()),
            vec![image.clone()],
        ));
    }
    for tag in &input.tags {
        tags.push(Tag::hashtag(tag));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_PIN), input.content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign pin: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::PinBoard,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Pin published: {}", event_id);
    Ok(event_id)
}

pub async fn delete_pin(pin_event_id: &str) -> std::result::Result<(), String> {
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot delete pin.".to_string());
    }
    let event_id =
        EventId::from_hex(pin_event_id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let deletion_request = EventDeletionRequest::new().id(event_id);
    let builder = EventBuilder::delete(deletion_request);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign pin deletion: {}", e))?;
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::PinBoard,
        None,
        std::collections::HashMap::new(),
    ).await;
    remove_pin_from_cache(pin_event_id);
    log::info!("Pin deleted: {}", pin_event_id);
    Ok(())
}

pub async fn delete_pinboard(board: &Pinboard) -> std::result::Result<String, String> {
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot delete pinboard.".to_string());
    }
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if board.pubkey != current_pubkey {
        return Err("You can only delete your own pinboards".to_string());
    }
    let coord =
        Coordinate::new(Kind::Custom(KIND_PINBOARD), board.event.pubkey).identifier(&board.d_tag);
    let deletion_request = EventDeletionRequest::new().coordinate(coord);
    let builder = EventBuilder::delete(deletion_request);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign pinboard deletion: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::PinBoard,
        None,
        std::collections::HashMap::new(),
    ).await;
    remove_pinboard_from_cache(&board.a_tag);
    log::info!("Pinboard deleted: {}", event_id);
    Ok(event_id)
}

pub async fn update_pinboard_metadata(
    naddr: &str,
    title: Option<String>,
    description: Option<String>,
    image: Option<String>,
    tags: Option<Vec<String>>,
) -> std::result::Result<String, String> {
    let board = fetch_pinboard_by_naddr(naddr)
        .await?
        .ok_or("Pinboard not found")?;
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if board.pubkey != current_pubkey {
        return Err("You can only edit your own pinboards".to_string());
    }
    let input = PinboardInput {
        title: title.unwrap_or(board.title.clone()),
        description: description.or(board.description.clone()),
        image: image.or(board.image.clone()),
        tags: tags.unwrap_or(board.tags.clone()),
        collaborative: board.collaborative,
    };
    publish_pinboard(input, Some(&board.d_tag)).await
}

pub async fn toggle_pinboard_reaction(
    board: &Pinboard,
    content: &str,
) -> std::result::Result<bool, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let reactions = fetch_pinboard_reactions(&board.a_tag).await?;
    let existing_reaction = reactions.iter().find(|r| r.pubkey == current_pubkey);
    if let Some(reaction) = existing_reaction {
        let event_id = EventId::from_hex(&reaction.event_id)
            .map_err(|e| format!("Invalid event ID: {}", e))?;
        let deletion_request = EventDeletionRequest::new().id(event_id);
        let builder = EventBuilder::delete(deletion_request);
        let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
            .await
            .map_err(|e| format!("Failed to sign reaction deletion: {}", e))?;
        crate::stores::publish_queue::enqueue(
            event,
            crate::stores::publish_queue::types::QueueEventType::PinBoard,
            None,
            std::collections::HashMap::new(),
        ).await;
        Ok(false)
    } else {
        let author_pubkey = PublicKey::from_hex(&board.pubkey)
            .map_err(|e| format!("Invalid author pubkey: {}", e))?;
        let tags = vec![
            Tag::from_standardized(TagStandard::Coordinate {
                coordinate: Coordinate::new(Kind::Custom(KIND_PINBOARD), author_pubkey)
                    .identifier(&board.d_tag),
                relay_url: None,
                uppercase: false,
            }),
            Tag::public_key(author_pubkey),
        ];
        let builder = EventBuilder::new(Kind::Reaction, content).tags(tags);
        let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
            .await
            .map_err(|e| format!("Failed to sign reaction: {}", e))?;
        crate::stores::publish_queue::enqueue(
            event,
            crate::stores::publish_queue::types::QueueEventType::PinBoard,
            None,
            std::collections::HashMap::new(),
        ).await;
        Ok(true)
    }
}

pub async fn get_shareable_naddr(board: &Pinboard) -> std::result::Result<String, String> {
    let pubkey =
        nostr::PublicKey::from_hex(&board.pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    nostr_client::make_naddr_with_hints(KIND_PINBOARD, &pubkey, &board.d_tag).await
}

#[cfg(test)]
mod tests {
    use super::union_board_addresses;

    #[test]
    fn test_union_merges_without_duplicates() {
        let new = vec!["30067:a:board1".to_string()];
        let existing = vec!["30067:a:board1".to_string(), "30067:a:board2".to_string()];
        assert_eq!(
            union_board_addresses(new, existing),
            vec!["30067:a:board1".to_string(), "30067:a:board2".to_string()]
        );
    }

    #[test]
    fn test_union_keeps_all_when_disjoint() {
        let new = vec!["30067:a:x".to_string()];
        let existing = vec!["30067:a:y".to_string(), "30067:a:z".to_string()];
        assert_eq!(
            union_board_addresses(new, existing),
            vec![
                "30067:a:x".to_string(),
                "30067:a:y".to_string(),
                "30067:a:z".to_string()
            ]
        );
    }

    #[test]
    fn test_union_empty_existing_keeps_new() {
        assert_eq!(
            union_board_addresses(vec!["30067:a:x".to_string()], vec![]),
            vec!["30067:a:x".to_string()]
        );
    }

    #[test]
    fn test_union_filters_empty_strings() {
        assert_eq!(
            union_board_addresses(
                vec!["".to_string(), "30067:a:x".to_string()],
                vec!["".to_string()]
            ),
            vec!["30067:a:x".to_string()]
        );
    }
}
