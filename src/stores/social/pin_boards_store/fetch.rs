use super::*;

/// Determine the accurate content type for a pin by fetching the referenced event.
/// This is needed because Kind 30023 can be either an article or a recipe (identified by `nostrcooking` tag).
/// Returns the inferred type if fetch fails or referenced event doesn't change the inference.
pub async fn fetch_pin_content_type(pin: &Pin) -> PinContentType {
    if let PinReference::Coordinate { address, .. } = &pin.reference {
        let coord_opt = if address.starts_with("naddr1") {
            Coordinate::from_bech32(address).ok()
        } else {
            Coordinate::parse(address).ok()
        };
        if let Some(coord) = coord_opt {
            if coord.kind.as_u16() == 30023 {
                let filter = Filter::new()
                    .kind(coord.kind)
                    .author(coord.public_key)
                    .identifier(&coord.identifier)
                    .limit(1);
                if let Ok(events) =
                    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await
                {
                    if let Some(event) = events.first() {
                        if event
                            .tags
                            .hashtags()
                            .any(|tag| tag == crate::utils::recipe::RECIPE_TAG_PREFIX)
                        {
                            return PinContentType::Recipe;
                        }
                        return PinContentType::Article;
                    }
                }
            }
        }
    }
    pin.content_type()
}

/// Enrich a list of pins with accurate content types by fetching referenced events.
/// For Kind 30023 references, this checks if they are recipes or articles.
pub async fn enrich_pins_content_types(pins: &[Pin]) -> Vec<(String, PinContentType)> {
    use futures::future::join_all;
    let futures: Vec<_> = pins
        .iter()
        .filter(|pin| {
            if let PinReference::Coordinate { address, .. } = &pin.reference {
                if address.starts_with("30023:") {
                    return true;
                }
                if address.starts_with("naddr1") {
                    if let Ok(coord) = Coordinate::from_bech32(address) {
                        return coord.kind.as_u16() == 30023;
                    }
                }
                false
            } else {
                false
            }
        })
        .map(|pin| {
            let event_id = pin.event_id.clone();
            async move {
                let content_type = fetch_pin_content_type(pin).await;
                (event_id, content_type)
            }
        })
        .collect();
    join_all(futures).await
}

/// Fetch metadata for a pin by retrieving the referenced event.
/// Extracts title, image, and summary from the referenced event's tags.
pub async fn fetch_pin_metadata(pin: &Pin) -> PinMetadata {
    match &pin.reference {
        PinReference::Coordinate { address, .. } => {
            let coord_opt = if address.starts_with("naddr1") {
                Coordinate::from_bech32(address).ok()
            } else {
                Coordinate::parse(address).ok()
            };
            if let Some(coord) = coord_opt {
                let filter = Filter::new()
                    .kind(coord.kind)
                    .author(coord.public_key)
                    .identifier(&coord.identifier)
                    .limit(1);
                if let Ok(events) =
                    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await
                {
                    if let Some(event) = events.first() {
                        return extract_event_metadata(event, coord.kind.as_u16());
                    }
                }
            }
            PinMetadata::default()
        }
        PinReference::Event { id, .. } => {
            if let Ok(event_id) = EventId::from_hex(id) {
                let filter = Filter::new().id(event_id).limit(1);
                if let Ok(events) =
                    nostr_client::fetch_events_aggregated(filter, Duration::from_secs(5)).await
                {
                    if let Some(event) = events.first() {
                        return extract_event_metadata(event, event.kind.as_u16());
                    }
                }
            }
            PinMetadata::default()
        }
        PinReference::External { .. } => PinMetadata::default(),
    }
}

/// Enrich a list of pins with full metadata by fetching referenced events.
/// This is more comprehensive than enrich_pins_content_types - it also gets image, title, summary.
pub async fn enrich_pins_metadata(pins: &[Pin]) -> std::collections::HashMap<String, PinMetadata> {
    use futures::future::join_all;
    use std::collections::HashMap;
    let futures: Vec<_> = pins
        .iter()
        .filter(|pin| !matches!(pin.reference, PinReference::External { .. }))
        .map(|pin| {
            let event_id = pin.event_id.clone();
            async move {
                let metadata = fetch_pin_metadata(pin).await;
                (event_id, metadata)
            }
        })
        .collect();
    let results = join_all(futures).await;
    results
        .into_iter()
        .filter(|(_, meta)| {
            meta.title.is_some() || meta.image.is_some() || meta.content_type.is_some()
        })
        .collect::<HashMap<_, _>>()
}

/// Fetch pinboards with aggregated DB + relay fetch
pub async fn fetch_pinboards(limit: usize) -> std::result::Result<Vec<Pinboard>, String> {
    *LOADING_PINBOARDS.write() = true;
    let filter = pinboards_filter(limit);
    let current_user = crate::stores::auth_store::get_pubkey();
    log::info!(
        "Discover: Fetching pinboards with filter kind={}, limit={}",
        KIND_PINBOARD,
        limit
    );
    let result = nostr_client::fetch_events_from_relays(filter, Duration::from_secs(15)).await;
    *LOADING_PINBOARDS.write() = false;
    match result {
        Ok(events) => {
            log::info!("Discover: Got {} raw events from relays", events.len());
            let unique_authors: std::collections::HashSet<_> =
                events.iter().map(|e| e.pubkey.to_hex()).collect();
            log::info!(
                "Discover: Events from {} unique authors",
                unique_authors.len()
            );
            let boards: Vec<Pinboard> = events
                .iter()
                .filter_map(|e| parse_pinboard_event(e, current_user.as_deref()))
                .collect();
            cache_pinboards(&boards);
            *PINBOARDS_INITIALIZED.write() = true;
            log::info!("Discover: Parsed {} pinboards successfully", boards.len());
            Ok(boards)
        }
        Err(e) => {
            log::error!("Discover: Failed to fetch pinboards: {}", e);
            Err(e)
        }
    }
}

/// Fetch pinboards with pagination
pub async fn fetch_pinboards_page(
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<Pinboard>, String> {
    let filter = pinboards_paginated_filter(limit, until);
    let current_user = crate::stores::auth_store::get_pubkey();
    let events = nostr_client::fetch_events_from_relays(filter, Duration::from_secs(15)).await?;
    let boards: Vec<Pinboard> = events
        .iter()
        .filter_map(|e| parse_pinboard_event(e, current_user.as_deref()))
        .collect();
    cache_pinboards(&boards);
    log::info!("Fetched {} pinboards (paginated)", boards.len());
    Ok(boards)
}

/// Fetch cookbooks (pinboards tagged with "cookbook")
pub async fn fetch_cookbooks(limit: usize) -> std::result::Result<Vec<Pinboard>, String> {
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_PINBOARD))
        .hashtag("cookbook")
        .limit(limit);
    let current_user = crate::stores::auth_store::get_pubkey();
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await?;
    let mut cookbooks: Vec<Pinboard> = events
        .iter()
        .filter_map(|e| parse_pinboard_event(e, current_user.as_deref()))
        .collect();
    cookbooks.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    cache_pinboards(&cookbooks);
    log::info!("Fetched {} cookbooks", cookbooks.len());
    Ok(cookbooks)
}

/// Fetch the current user's cookbooks (pinboards tagged with "cookbook")
pub async fn fetch_user_cookbooks() -> std::result::Result<Vec<Pinboard>, String> {
    let current_user = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let pubkey = nostr_sdk::PublicKey::from_hex(&current_user)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_PINBOARD))
        .author(pubkey)
        .hashtag("cookbook");
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
    let mut cookbooks: Vec<Pinboard> = events
        .iter()
        .filter_map(|e| parse_pinboard_event(e, Some(&current_user)))
        .collect();
    cookbooks.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    cache_pinboards(&cookbooks);
    log::info!("Fetched {} user cookbooks", cookbooks.len());
    Ok(cookbooks)
}

/// Fetch a pinboard by naddr
pub async fn fetch_pinboard_by_naddr(naddr: &str) -> std::result::Result<Option<Pinboard>, String> {
    if let Some(cached) = get_cached_pinboard_by_naddr(naddr) {
        return Ok(Some(cached));
    }
    let coord = Coordinate::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let filter = pinboard_by_coord_filter(coord.public_key, &coord.identifier);
    let current_user = crate::stores::auth_store::get_pubkey();
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await?;
    if let Some(event) = events.first() {
        if let Some(board) = parse_pinboard_event(event, current_user.as_deref()) {
            cache_pinboard(board.clone());
            return Ok(Some(board));
        }
    }
    Ok(None)
}

/// Fetch pins for a board
pub async fn fetch_pins_for_board(board_a_tag: &str) -> std::result::Result<Vec<Pin>, String> {
    let filter = pins_for_board_filter(board_a_tag, 500);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await?;
    let mut pins: Vec<Pin> = events.iter().filter_map(parse_pin_event).collect();
    pins.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    cache_pins(&pins);
    log::info!("Fetched {} pins for board {}", pins.len(), board_a_tag);
    Ok(pins)
}

/// Fetch only the board owner's pins for a board
pub async fn fetch_owner_pins_for_board(
    board_a_tag: &str,
    owner_pubkey: &str,
) -> std::result::Result<Vec<Pin>, String> {
    let pk = PublicKey::parse(owner_pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = pins_by_author_for_board_filter(pk, board_a_tag, 500);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await?;
    let mut pins: Vec<Pin> = events.iter().filter_map(parse_pin_event).collect();
    pins.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    cache_pins(&pins);
    log::info!(
        "Fetched {} owner pins for board {}",
        pins.len(),
        board_a_tag
    );
    Ok(pins)
}

/// Fetch pins for a board with flexible author filtering
///
/// Modes:
/// - `owner_pubkey: Some(pk)` = Only owner's pins (default mode)
/// - `owner_pubkey: None` + `allowed_authors: Some(vec)` = Specific collaborators only
/// - `owner_pubkey: None` + `allowed_authors: None` = All pins (full collaborative mode)
pub async fn fetch_pins_for_board_filtered(
    board_a_tag: &str,
    owner_pubkey: Option<&str>,
    allowed_authors: Option<Vec<String>>,
) -> std::result::Result<Vec<Pin>, String> {
    let mut filter = Filter::new()
        .kind(Kind::Custom(KIND_PIN))
        .custom_tag(SingleLetterTag::uppercase(Alphabet::A), board_a_tag)
        .limit(500);
    if let Some(owner) = owner_pubkey {
        let pk = PublicKey::parse(owner).map_err(|e| format!("Invalid owner pubkey: {}", e))?;
        filter = filter.author(pk);
    } else if let Some(ref authors) = allowed_authors {
        let pks: Vec<PublicKey> = authors
            .iter()
            .filter_map(|a| PublicKey::parse(a).ok())
            .collect();
        if !pks.is_empty() {
            filter = filter.authors(pks);
        }
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await?;
    let mut pins: Vec<Pin> = events.iter().filter_map(parse_pin_event).collect();
    pins.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    cache_pins(&pins);
    let mode = if owner_pubkey.is_some() {
        "owner-only"
    } else if allowed_authors.is_some() {
        "filtered-authors"
    } else {
        "all-authors"
    };
    log::info!(
        "Fetched {} pins for board {} (mode: {})",
        pins.len(),
        board_a_tag,
        mode
    );
    Ok(pins)
}

/// Fetch a pinboard with its pins (two-stage loading)
pub async fn fetch_pinboard_with_pins(
    naddr: &str,
) -> std::result::Result<Option<PinboardWithPins>, String> {
    let board = match fetch_pinboard_by_naddr(naddr).await? {
        Some(b) => b,
        None => return Ok(None),
    };
    let pins = fetch_pins_for_board(&board.a_tag).await?;
    Ok(Some(PinboardWithPins { board, pins }))
}

/// Fetch pinboards by author
pub async fn fetch_pinboards_by_author(
    pubkey: &str,
    limit: usize,
) -> std::result::Result<Vec<Pinboard>, String> {
    let pk = PublicKey::parse(pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = pinboards_by_author_filter(pk, limit);
    let current_user = crate::stores::auth_store::get_pubkey();
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await?;
    let boards: Vec<Pinboard> = events
        .iter()
        .filter_map(|e| parse_pinboard_event(e, current_user.as_deref()))
        .collect();
    cache_pinboards(&boards);
    log::info!("Fetched {} pinboards for author {}", boards.len(), pubkey);
    Ok(boards)
}

/// Fetch current user's pinboards
pub async fn fetch_my_pinboards() -> std::result::Result<Vec<Pinboard>, String> {
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    fetch_pinboards_by_author(&pubkey, 100).await
}

/// Fetch all pins by the current user
pub async fn fetch_my_pins() -> std::result::Result<Vec<Pin>, String> {
    let pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not authenticated")?;
    let pk = PublicKey::parse(&pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = pins_by_author_filter(pk, 500);
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await?;
    let mut pins: Vec<Pin> = events.iter().filter_map(parse_pin_event).collect();
    pins.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    cache_pins(&pins);
    log::info!("Fetched {} pins for current user", pins.len());
    Ok(pins)
}

/// Search pinboards by title/description (local cache search)
pub fn search_pinboards_local(query: &str) -> Vec<Pinboard> {
    let query_lower = query.to_lowercase();
    let cache = PINBOARDS_CACHE.read();
    cache
        .iter()
        .filter(|(_, board)| {
            board.title.to_lowercase().contains(&query_lower)
                || board
                    .description
                    .as_ref()
                    .is_some_and(|d| d.to_lowercase().contains(&query_lower))
                || board
                    .tags
                    .iter()
                    .any(|t| t.to_lowercase().contains(&query_lower))
        })
        .map(|(_, board)| board.clone())
        .collect()
}

/// Fetch reactions for a pinboard
pub async fn fetch_pinboard_reactions(
    a_tag: &str,
) -> std::result::Result<Vec<BoardReaction>, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;
    let filter = pinboard_reactions_filter(a_tag, 500);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch reactions: {}", e))?;
    let reactions: Vec<BoardReaction> = events
        .iter()
        .map(|event| BoardReaction {
            event_id: event.id.to_hex(),
            pubkey: event.pubkey.to_hex(),
            content: event.content.clone(),
            created_at: event.created_at.as_secs(),
        })
        .collect();
    Ok(reactions)
}

/// Fetch zap receipts for a pinboard
pub async fn fetch_pinboard_zaps(a_tag: &str) -> std::result::Result<Vec<BoardZap>, String> {
    let client = nostr_client::get_client().ok_or("Nostr client not initialized")?;
    let filter = pinboard_zaps_filter(a_tag, 500);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch zaps: {}", e))?;
    let zaps: Vec<BoardZap> = events
        .iter()
        .filter_map(|event| {
            let amount_msats = extract_zap_amount(event);
            if amount_msats == 0 {
                return None;
            }
            let sender_pubkey = extract_zap_sender(event);
            let comment = extract_zap_comment(event);
            Some(BoardZap {
                event_id: event.id.to_hex(),
                sender_pubkey,
                amount_msats,
                comment,
                created_at: event.created_at.as_secs(),
            })
        })
        .collect();
    Ok(zaps)
}

/// Calculate total zap amount in sats for a pinboard
pub async fn fetch_pinboard_zap_total(a_tag: &str) -> std::result::Result<u64, String> {
    let zaps = fetch_pinboard_zaps(a_tag).await?;
    let total_msats: u64 = zaps.iter().map(|z| z.amount_msats).sum();
    Ok(total_msats / 1000)
}

/// Count reactions for a pinboard
pub async fn fetch_pinboard_reaction_count(a_tag: &str) -> std::result::Result<usize, String> {
    let reactions = fetch_pinboard_reactions(a_tag).await?;
    Ok(reactions.len())
}

/// Fetch both reaction count and current-user reacted state from a single query
pub async fn fetch_pinboard_reaction_state(
    a_tag: &str,
) -> std::result::Result<(usize, bool), String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey();
    let reactions = fetch_pinboard_reactions(a_tag).await?;
    let reacted = current_pubkey
        .as_ref()
        .map(|pubkey| reactions.iter().any(|r| &r.pubkey == pubkey))
        .unwrap_or(false);
    Ok((reactions.len(), reacted))
}

/// Check if current user has reacted to a pinboard
pub async fn has_user_reacted_to_pinboard(a_tag: &str) -> std::result::Result<bool, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let reactions = fetch_pinboard_reactions(a_tag).await?;
    Ok(reactions.iter().any(|r| r.pubkey == current_pubkey))
}

/// Extract zap amount from a zap receipt event
fn extract_zap_amount(event: &NostrEvent) -> u64 {
    for tag in event.tags.iter() {
        if tag.kind() == TagKind::Custom("bolt11".into()) {
            if let Some(bolt11) = tag.content() {
                if let Some(amount) = parse_bolt11_amount(bolt11) {
                    return amount;
                }
            }
        }
        if tag.kind() == TagKind::Description {
            if let Some(desc) = tag.content() {
                if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(desc) {
                    if let Some(amount) = zap_request.get("amount").and_then(|a| a.as_u64()) {
                        return amount;
                    }
                }
            }
        }
    }
    0
}

/// Parse amount from bolt11 invoice string (returns msats)
fn parse_bolt11_amount(bolt11: &str) -> Option<u64> {
    let lower = bolt11.to_lowercase();
    if !lower.starts_with("lnbc") && !lower.starts_with("lntb") {
        return None;
    }
    let prefix_len = 4;
    let rest = &lower[prefix_len..];
    let mut amount_end = 0;
    let mut multiplier_char = None;
    for (i, c) in rest.chars().enumerate() {
        if c.is_ascii_digit() {
            amount_end = i + 1;
        } else if ['m', 'u', 'n', 'p'].contains(&c) {
            multiplier_char = Some(c);
            amount_end = i;
            break;
        } else {
            amount_end = i;
            break;
        }
    }
    if amount_end == 0 {
        return None;
    }
    let amount_str = &rest[..amount_end];
    let amount: u64 = amount_str.parse().ok()?;
    let msats = match multiplier_char {
        Some('m') => amount * 100_000_000,
        Some('u') => amount * 100_000,
        Some('n') => amount * 100,
        Some('p') => amount / 10,
        Some(_) => return None,
        None => amount * 100_000_000_000,
    };
    Some(msats)
}

/// Extract sender pubkey from zap receipt
fn extract_zap_sender(event: &NostrEvent) -> Option<String> {
    for tag in event.tags.iter() {
        if tag.kind() == TagKind::Description {
            if let Some(desc) = tag.content() {
                if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(desc) {
                    if let Some(pubkey) = zap_request.get("pubkey").and_then(|p| p.as_str()) {
                        return Some(pubkey.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Extract comment from zap receipt
fn extract_zap_comment(event: &NostrEvent) -> Option<String> {
    for tag in event.tags.iter() {
        if tag.kind() == TagKind::Description {
            if let Some(desc) = tag.content() {
                if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(desc) {
                    if let Some(content) = zap_request.get("content").and_then(|c| c.as_str()) {
                        if !content.is_empty() {
                            return Some(content.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Alias for old fetch function name
pub async fn fetch_pin_boards(limit: usize) -> std::result::Result<Vec<Pinboard>, String> {
    fetch_pinboards(limit).await
}

/// Alias for old fetch function name
pub async fn fetch_board_by_naddr(naddr: &str) -> std::result::Result<Option<Pinboard>, String> {
    fetch_pinboard_by_naddr(naddr).await
}

/// Alias for old fetch function name
pub async fn fetch_my_boards() -> std::result::Result<Vec<Pinboard>, String> {
    fetch_my_pinboards().await
}
