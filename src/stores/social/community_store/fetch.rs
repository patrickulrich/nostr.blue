use super::*;

/// Fetch all communities
pub async fn fetch_communities(limit: usize) -> std::result::Result<Vec<Community>, String> {
    *LOADING_COMMUNITIES.write() = true;
    let filter = communities_filter(limit);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;
    *LOADING_COMMUNITIES.write() = false;
    match result {
        Ok(events) => {
            let communities: Vec<Community> =
                events.iter().filter_map(parse_community_event).collect();
            cache_communities(&communities);
            *COMMUNITY_INITIALIZED.write() = true;
            log::info!("Fetched {} communities", communities.len());
            Ok(communities)
        }
        Err(e) => {
            log::error!("Failed to fetch communities: {}", e);
            Err(e)
        }
    }
}

/// Fetch a specific community by naddr
pub async fn fetch_community_by_naddr(
    naddr: &str,
) -> std::result::Result<Option<Community>, String> {
    if let Some(cached) = get_cached_community_by_naddr(naddr) {
        return Ok(Some(cached));
    }
    let coord = Coordinate::from_bech32(naddr).map_err(|e| format!("Invalid naddr: {}", e))?;
    let filter = community_by_coord_filter(coord.public_key, &coord.identifier);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    if let Some(event) = events.first() {
        if let Some(community) = parse_community_event(event) {
            cache_community(community.clone());
            return Ok(Some(community));
        }
    }
    Ok(None)
}

/// Fetch a specific community by a_tag
pub async fn fetch_community_by_a_tag(
    a_tag: &str,
) -> std::result::Result<Option<Community>, String> {
    if let Some(cached) = get_cached_community(a_tag) {
        return Ok(Some(cached));
    }
    let parts: Vec<&str> = a_tag.splitn(3, ':').collect();
    if parts.len() != 3 {
        return Err("Invalid a_tag format".to_string());
    }
    let pubkey =
        PublicKey::from_hex(parts[1]).map_err(|e| format!("Invalid pubkey in a_tag: {}", e))?;
    let identifier = parts[2];
    let filter = community_by_coord_filter(pubkey, identifier);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;
    if let Some(event) = events.first() {
        if let Some(community) = parse_community_event(event) {
            cache_community(community.clone());
            return Ok(Some(community));
        }
    }
    Ok(None)
}

/// Fetch posts for a community with approval status computation
pub async fn fetch_community_posts(
    community: &Community,
    limit: usize,
    include_pending: bool,
    until: Option<u64>,
) -> std::result::Result<Vec<CommunityPost>, String> {
    *LOADING_POSTS.write() = true;
    let posts_filter = posts_filter_by_community(&community.a_tag, limit, until);
    let approvals_filter = approvals_filter_by_community(&community.a_tag, 500);
    let removals_filter = removals_filter_by_community(&community.a_tag);
    let (posts_result, approvals_result, removals_result) = futures::join!(
        crate::stores::nostr_client::fetch_events_aggregated(posts_filter, Duration::from_secs(15)),
        crate::stores::nostr_client::fetch_events_aggregated(
            approvals_filter,
            Duration::from_secs(10)
        ),
        crate::stores::nostr_client::fetch_events_aggregated(
            removals_filter,
            Duration::from_secs(5)
        ),
    );
    *LOADING_POSTS.write() = false;
    if let Ok(approval_events) = approvals_result {
        let mut approvals_cache = APPROVALS_CACHE.write();
        for event in &approval_events {
            if let Some((post_id, approval)) = parse_approval_event(event) {
                approvals_cache.entry(post_id).or_default().push(approval);
            }
        }
    }
    if let Ok(removal_events) = removals_result {
        let mut removals_cache = REMOVALS_CACHE.write();
        for event in &removal_events {
            if let Some((post_id, removal)) = parse_removal_event(event) {
                removals_cache.insert(post_id, removal);
            }
        }
    }
    let posts_events = posts_result?;
    let mut posts: Vec<CommunityPost> = posts_events
        .iter()
        .filter_map(|e| parse_community_post(e, &community.a_tag))
        .map(|mut post| {
            post.approval_status = compute_approval_status(&post, community);
            post
        })
        .collect();
    if !include_pending {
        posts.retain(|p| {
            !matches!(
                p.approval_status,
                ApprovalStatus::Pending | ApprovalStatus::Removed(_)
            )
        });
    }
    posts.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    cache_posts(&posts);
    log::info!(
        "Fetched {} posts for community {}",
        posts.len(),
        community.a_tag
    );
    Ok(posts)
}

/// Fetch pending posts for moderation queue
pub async fn fetch_pending_posts(
    community: &Community,
) -> std::result::Result<Vec<CommunityPost>, String> {
    let all_posts = fetch_community_posts(community, 200, true, None).await?;
    Ok(all_posts
        .into_iter()
        .filter(|p| matches!(p.approval_status, ApprovalStatus::Pending))
        .collect())
}

/// Fetch communities where user is owner, moderator, or approved member
/// Used to show "Your Communities" at top of /communities page
pub async fn fetch_user_communities(
    user_pubkey: &str,
) -> std::result::Result<Vec<Community>, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    crate::stores::nostr_client::ensure_relays_ready(&client).await;
    let pubkey = PublicKey::from_hex(user_pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let owned_filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .author(pubkey);
    let mod_filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .pubkey(pubkey);
    let member_filter = Filter::new()
        .kind(Kind::Custom(KIND_APPROVED_MEMBERS))
        .pubkey(pubkey);
    let recent_filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .limit(200);
    let (owned_result, mod_result, member_result, recent_result) = futures::join!(
        client.fetch_events(owned_filter, Duration::from_secs(5)),
        client.fetch_events(mod_filter, Duration::from_secs(5)),
        client.fetch_events(member_filter, Duration::from_secs(5)),
        client.fetch_events(recent_filter, Duration::from_secs(5)),
    );
    let owned_events = owned_result.map_err(|e| format!("Failed to fetch owned: {}", e))?;
    let mod_events = mod_result.unwrap_or_default();
    let member_events = member_result.unwrap_or_default();
    let recent_events = recent_result.unwrap_or_default();
    log::debug!(
        "User communities query: owned={}, mod_filter={}, member_lists={}, recent={}",
        owned_events.len(),
        mod_events.len(),
        member_events.len(),
        recent_events.len()
    );
    let mut seen = HashSet::new();
    let mut communities = Vec::new();
    let user_pk_str = user_pubkey.to_lowercase();
    for event in owned_events.into_iter() {
        if let Some(community) = parse_community_event(&event) {
            if seen.insert(community.a_tag.clone()) {
                communities.push(community);
            }
        }
    }
    for event in mod_events.into_iter() {
        if let Some(community) = parse_community_event(&event) {
            if community
                .moderators
                .iter()
                .any(|m| m.to_lowercase() == user_pk_str)
                && seen.insert(community.a_tag.clone())
            {
                communities.push(community);
            }
        }
    }
    let mut member_community_a_tags: HashSet<String> = HashSet::new();
    for event in member_events.into_iter() {
        log::debug!(
            "Processing member list event {} (kind {})",
            event.id.to_hex(),
            event.kind.as_u16()
        );
        if let Some(a_tag) = event
            .tags
            .iter()
            .find(|t| t.kind() == TagKind::a())
            .and_then(|t| t.content())
        {
            let a_tag_str = a_tag.to_string();
            log::debug!(
                "Found community a_tag in approved member list: {}",
                a_tag_str
            );
            if a_tag_str.starts_with(&format!("{}:", KIND_COMMUNITY_DEFINITION)) {
                member_community_a_tags.insert(a_tag_str.clone());
                let members: HashSet<String> = event
                    .tags
                    .iter()
                    .filter(|t| t.kind() == TagKind::p())
                    .filter_map(|t| t.content().map(|s| s.to_lowercase()))
                    .collect();
                log::debug!(
                    "Caching {} approved members for community {}",
                    members.len(),
                    &a_tag_str
                );
                APPROVED_MEMBERS_CACHE.write().insert(a_tag_str, members);
            } else {
                log::warn!(
                    "Approved member list has non-community a_tag: {}",
                    a_tag_str
                );
            }
        } else if let Some(d_tag) = event
            .tags
            .iter()
            .find(|t| t.kind() == TagKind::d())
            .and_then(|t| t.content())
        {
            log::debug!("  No 'a' tag, but found 'd' tag: {}", d_tag);
            let potential_a_tag = if d_tag.starts_with(&format!("{}:", KIND_COMMUNITY_DEFINITION)) {
                d_tag.to_string()
            } else {
                log::warn!("  d_tag '{}' doesn't look like a community a_tag", d_tag);
                continue;
            };
            member_community_a_tags.insert(potential_a_tag.clone());
            let members: HashSet<String> = event
                .tags
                .iter()
                .filter(|t| t.kind() == TagKind::p())
                .filter_map(|t| t.content().map(|s| s.to_lowercase()))
                .collect();
            log::debug!(
                "Caching {} approved members from d_tag for {}",
                members.len(),
                &potential_a_tag
            );
            APPROVED_MEMBERS_CACHE
                .write()
                .insert(potential_a_tag, members);
        } else {
            log::warn!(
                "Approved member list event {} has no 'a' or 'd' tag",
                event.id.to_hex()
            );
        }
    }
    let member_community_count = member_community_a_tags.len();
    if !member_community_a_tags.is_empty() {
        log::info!(
            "Fetching {} communities where user is approved member: {:?}",
            member_community_count,
            member_community_a_tags.iter().take(5).collect::<Vec<_>>()
        );
        let uncached_a_tags: Vec<String> = member_community_a_tags
            .into_iter()
            .filter(|a_tag| {
                if seen.contains(a_tag) {
                    log::debug!("Skipping already-seen community: {}", a_tag);
                    false
                } else {
                    true
                }
            })
            .collect();
        let (cached, uncached): (Vec<_>, Vec<_>) = uncached_a_tags
            .into_iter()
            .partition(|a_tag| get_cached_community(a_tag).is_some());
        for a_tag in &cached {
            if let Some(community) = get_cached_community(a_tag) {
                log::debug!("Found member community in cache: {}", community.a_tag);
                if seen.insert(community.a_tag.clone()) {
                    communities.push(community);
                }
            }
        }
        if !uncached.is_empty() {
            log::info!(
                "Fetching {} uncached member communities in parallel",
                uncached.len()
            );
            use futures::stream::{self, StreamExt};
            let fetched: Vec<_> = stream::iter(uncached)
                .map(|a_tag| async move {
                    fetch_community_by_a_tag(&a_tag).await.ok().flatten()
                })
                .buffer_unordered(4)
                .collect()
                .await;
            for community in fetched.into_iter().flatten() {
                log::debug!("Fetched member community: {}", community.a_tag);
                if seen.insert(community.a_tag.clone()) {
                    communities.push(community);
                }
            }
        }
    }
    for event in recent_events.into_iter() {
        if let Some(community) = parse_community_event(&event) {
            let is_owner = community.pubkey.to_lowercase() == user_pk_str;
            let is_moderator = community
                .moderators
                .iter()
                .any(|m| m.to_lowercase() == user_pk_str);
            let is_member = APPROVED_MEMBERS_CACHE
                .read()
                .get(&community.a_tag)
                .map(|members| members.contains(&user_pk_str))
                .unwrap_or(false);
            if (is_owner || is_moderator || is_member) && seen.insert(community.a_tag.clone()) {
                log::debug!(
                    "Found user community from recent: {} (owner={}, mod={}, member={})",
                    community.name.as_ref().unwrap_or(&community.d_tag),
                    is_owner,
                    is_moderator,
                    is_member
                );
                communities.push(community);
            }
        }
    }
    communities.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    cache_communities(&communities);
    log::info!(
        "Fetched {} user communities for {} ({} member communities attempted)",
        communities.len(),
        truncate_pubkey(user_pubkey),
        member_community_count
    );
    Ok(communities)
}

/// Search communities by name/description
/// Tries NIP-50 search first, falls back to fetching all and filtering client-side
pub async fn search_communities(
    query: &str,
    limit: usize,
) -> std::result::Result<Vec<Community>, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let query_lower = query.to_lowercase();
    let search_filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .search(query)
        .limit(limit);
    let search_result = client
        .fetch_events(search_filter, Duration::from_secs(5))
        .await;
    if let Ok(events) = search_result {
        if !events.is_empty() {
            let communities: Vec<Community> = events
                .into_iter()
                .filter_map(|e| parse_community_event(&e))
                .collect();
            cache_communities(&communities);
            log::info!(
                "NIP-50 search returned {} communities for '{}'",
                communities.len(),
                query
            );
            return Ok(communities);
        }
    }
    let fallback_filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .limit(500);
    let events = client
        .fetch_events(fallback_filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Search fallback failed: {}", e))?;
    let mut communities: Vec<Community> = events
        .into_iter()
        .filter_map(|e| parse_community_event(&e))
        .filter(|c| {
            c.name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&query_lower))
                .unwrap_or(false)
                || c.description
                    .as_ref()
                    .map(|d| d.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
                || c.d_tag.to_lowercase().contains(&query_lower)
        })
        .take(limit)
        .collect();
    communities.sort_by(|a, b| {
        let a_name_match = a
            .name
            .as_ref()
            .map(|n| n.to_lowercase().contains(&query_lower))
            .unwrap_or(false);
        let b_name_match = b
            .name
            .as_ref()
            .map(|n| n.to_lowercase().contains(&query_lower))
            .unwrap_or(false);
        b_name_match.cmp(&a_name_match)
    });
    cache_communities(&communities);
    log::info!(
        "Client-side search returned {} communities for '{}'",
        communities.len(),
        query
    );
    Ok(communities)
}

/// Fetch communities with pagination (for infinite scroll)
/// Uses `until` timestamp to fetch older communities.
/// Uses DB-first aggregated fetch for instant cache hits on revisit.
pub async fn fetch_communities_page(
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<Community>, String> {
    let mut filter = Filter::new()
        .kind(Kind::Custom(KIND_COMMUNITY_DEFINITION))
        .limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }
    let events = crate::stores::nostr_client::fetch_events_aggregated(
        filter,
        Duration::from_secs(10),
    )
    .await
    .map_err(|e| format!("Failed to fetch communities page: {}", e))?;
    let events_count = events.len();
    let mut communities: Vec<Community> = events
        .into_iter()
        .filter_map(|e| parse_community_event(&e))
        .collect();
    communities.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    cache_communities(&communities);
    log::info!(
        "fetch_communities_page: events={}, parsed={}, until={:?}",
        events_count,
        communities.len(),
        until
    );
    Ok(communities)
}

/// Fetch approved members list (kind 34551) for a community
pub async fn fetch_approved_members(
    community: &Community,
) -> std::result::Result<HashSet<String>, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let owner_pubkey = PublicKey::from_hex(&community.pubkey)
        .map_err(|e| format!("Invalid community pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_APPROVED_MEMBERS))
        .author(owner_pubkey)
        .identifier(&community.d_tag)
        .limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch approved members: {}", e))?;
    let mut members = HashSet::new();
    if let Some(event) = events.into_iter().next() {
        for tag in event.tags.iter() {
            if tag.kind() == TagKind::p() {
                if let Some(pubkey) = tag.content() {
                    members.insert(pubkey.to_string());
                }
            }
        }
    }
    APPROVED_MEMBERS_CACHE
        .write()
        .insert(community.a_tag.clone(), members.clone());
    log::info!(
        "Fetched {} approved members for {}",
        members.len(),
        community.a_tag
    );
    Ok(members)
}

/// Fetch user's pending join requests across all communities
pub async fn fetch_user_join_requests(
    user_pubkey: &str,
) -> std::result::Result<Vec<JoinRequest>, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let pubkey = PublicKey::from_hex(user_pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_JOIN_REQUEST))
        .author(pubkey)
        .limit(100);
    let events = client
        .fetch_events(filter, Duration::from_secs(7))
        .await
        .map_err(|e| format!("Failed to fetch join requests: {}", e))?;
    let requests: Vec<JoinRequest> = events
        .into_iter()
        .filter_map(|e| parse_join_request(&e))
        .collect();
    let mut cache = USER_PENDING_REQUESTS.write();
    for request in &requests {
        cache.insert(request.community_a_tag.clone(), request.clone());
    }
    log::info!("Fetched {} join requests for user", requests.len());
    Ok(requests)
}

/// Fetch pending join requests for a community (for moderators)
pub async fn fetch_community_join_requests(
    community: &Community,
) -> std::result::Result<Vec<JoinRequest>, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_JOIN_REQUEST))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::A), &community.a_tag)
        .limit(200);
    let events = client
        .fetch_events(filter, Duration::from_secs(7))
        .await
        .map_err(|e| format!("Failed to fetch community join requests: {}", e))?;
    let requests: Vec<JoinRequest> = events
        .into_iter()
        .filter_map(|e| parse_join_request(&e))
        .collect();
    let approved = APPROVED_MEMBERS_CACHE.read();
    let declined = DECLINED_MEMBERS_CACHE.read();
    let banned = BANNED_MEMBERS_CACHE.read();
    let approved_set = approved.get(&community.a_tag);
    let declined_set = declined.get(&community.a_tag);
    let banned_set = banned.get(&community.a_tag);
    let pending_requests: Vec<JoinRequest> = requests
        .into_iter()
        .filter(|r| {
            let is_approved = approved_set
                .map(|s| s.contains(&r.user_pubkey))
                .unwrap_or(false);
            let is_declined = declined_set
                .map(|s| s.contains(&r.user_pubkey))
                .unwrap_or(false);
            let is_banned = banned_set
                .map(|s| s.contains(&r.user_pubkey))
                .unwrap_or(false);
            !is_approved && !is_declined && !is_banned
        })
        .collect();
    PENDING_JOIN_REQUESTS_CACHE
        .write()
        .insert(community.a_tag.clone(), pending_requests.clone());
    log::info!(
        "Fetched {} pending join requests for {}",
        pending_requests.len(),
        community.a_tag
    );
    Ok(pending_requests)
}

/// Fetch declined members list for a community
pub async fn fetch_declined_members(
    community: &Community,
) -> std::result::Result<HashSet<String>, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let owner_pubkey = PublicKey::from_hex(&community.pubkey)
        .map_err(|e| format!("Invalid community pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_DECLINED_MEMBERS))
        .author(owner_pubkey)
        .identifier(&community.d_tag)
        .limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch declined members: {}", e))?;
    let mut members = HashSet::new();
    if let Some(event) = events.into_iter().next() {
        for tag in event.tags.iter() {
            if tag.kind() == TagKind::p() {
                if let Some(pubkey) = tag.content() {
                    members.insert(pubkey.to_string());
                }
            }
        }
    }
    DECLINED_MEMBERS_CACHE
        .write()
        .insert(community.a_tag.clone(), members.clone());
    log::info!(
        "Fetched {} declined members for {}",
        members.len(),
        community.a_tag
    );
    Ok(members)
}

/// Fetch banned members list for a community
pub async fn fetch_banned_members(
    community: &Community,
) -> std::result::Result<HashSet<String>, String> {
    let client = crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let owner_pubkey = PublicKey::from_hex(&community.pubkey)
        .map_err(|e| format!("Invalid community pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_BANNED_MEMBERS))
        .author(owner_pubkey)
        .identifier(&community.d_tag)
        .limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to fetch banned members: {}", e))?;
    let mut members = HashSet::new();
    if let Some(event) = events.into_iter().next() {
        for tag in event.tags.iter() {
            if tag.kind() == TagKind::p() {
                if let Some(pubkey) = tag.content() {
                    members.insert(pubkey.to_string());
                }
            }
        }
    }
    BANNED_MEMBERS_CACHE
        .write()
        .insert(community.a_tag.clone(), members.clone());
    log::info!(
        "Fetched {} banned members for {}",
        members.len(),
        community.a_tag
    );
    Ok(members)
}
