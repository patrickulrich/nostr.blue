use super::*;

/// Create a new pinboard or update an existing one
pub async fn publish_pinboard(
    input: PinboardInput,
    existing_d_tag: Option<&str>,
) -> std::result::Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish pinboard.".to_string());
    }
    let d_tag = existing_d_tag
        .map(|s| s.to_string())
        .unwrap_or_else(|| crate::utils::slugify(&input.title));
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
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish pinboard: {}", e))?;
    if output.success.is_empty() {
        return Err("No relays accepted event".to_string());
    }
    let event_id = output.id().to_hex();
    log::info!("Pinboard published: {}", event_id);
    let signer = client
        .signer()
        .await
        .map_err(|e| format!("Failed to get signer: {}", e))?;
    let pubkey = signer
        .get_public_key()
        .await
        .map_err(|e| format!("Failed to get pubkey: {}", e))?;
    let naddr = nostr_client::make_naddr_with_hints(KIND_PINBOARD, &pubkey, &d_tag).await?;
    Ok(naddr)
}

/// Create a new pin
pub async fn publish_pin(input: PinInput) -> std::result::Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot publish pin.".to_string());
    }
    let mut tags: Vec<Tag> = vec![];
    for board_addr in &input.board_addresses {
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
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to publish pin: {}", e))?;
    if output.success.is_empty() {
        return Err("No relays accepted event".to_string());
    }
    let event_id = output.id().to_hex();
    log::info!("Pin published: {}", event_id);
    Ok(event_id)
}

/// Delete a pin
pub async fn delete_pin(pin_event_id: &str) -> std::result::Result<(), String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    if !*nostr_client::HAS_SIGNER.read() {
        return Err("No signer attached. Cannot delete pin.".to_string());
    }
    let event_id =
        EventId::from_hex(pin_event_id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let deletion_request = EventDeletionRequest::new().id(event_id);
    let builder = EventBuilder::delete(deletion_request);
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to delete pin: {}", e))?;
    if output.success.is_empty() {
        return Err("No relays accepted event".to_string());
    }
    remove_pin_from_cache(pin_event_id);
    log::info!("Pin deleted: {}", pin_event_id);
    Ok(())
}

/// Delete a pinboard
pub async fn delete_pinboard(board: &Pinboard) -> std::result::Result<String, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
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
    let output = client
        .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
        .await
        .map_err(|e| format!("Failed to delete pinboard: {}", e))?;
    if output.success.is_empty() {
        return Err("No relays accepted event".to_string());
    }
    remove_pinboard_from_cache(&board.a_tag);
    log::info!("Pinboard deleted: {}", output.id().to_hex());
    Ok(output.id().to_hex())
}

/// Update pinboard metadata
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

/// Toggle reaction on a pinboard (add or remove)
pub async fn toggle_pinboard_reaction(
    board: &Pinboard,
    content: &str,
) -> std::result::Result<bool, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let reactions = fetch_pinboard_reactions(&board.a_tag).await?;
    let existing_reaction = reactions.iter().find(|r| r.pubkey == current_pubkey);
    if let Some(reaction) = existing_reaction {
        let event_id = EventId::from_hex(&reaction.event_id)
            .map_err(|e| format!("Invalid event ID: {}", e))?;
        let deletion_request = EventDeletionRequest::new().id(event_id);
        let builder = EventBuilder::delete(deletion_request);
        let output = client
            .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
            .await
            .map_err(|e| format!("Failed to delete reaction: {}", e))?;
        if output.success.is_empty() {
            return Err("No relays accepted event".to_string());
        }
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
        let output = client
            .send_event_builder(crate::utils::nips::nip89::tag_event_builder(builder))
            .await
            .map_err(|e| format!("Failed to send reaction: {}", e))?;
        if output.success.is_empty() {
            return Err("No relays accepted event".to_string());
        }
        Ok(true)
    }
}

/// Get a shareable naddr with relay hints for a pinboard
/// Per NIP-19, relay hints help other clients locate the event
pub async fn get_shareable_naddr(board: &Pinboard) -> std::result::Result<String, String> {
    let pubkey =
        nostr::PublicKey::from_hex(&board.pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    nostr_client::make_naddr_with_hints(KIND_PINBOARD, &pubkey, &board.d_tag).await
}
