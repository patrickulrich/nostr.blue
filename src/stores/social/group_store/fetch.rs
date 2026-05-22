use super::*;

async fn ensure_group_relay(relay_url: &str) -> std::result::Result<std::sync::Arc<Client>, String> {
    let client =
        crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let _ = client.add_relay(relay_url).await;
    let _ = client.connect_relay(relay_url).await;
    crate::stores::relay::connection::ensure_relays_ready(&client).await;
    Ok(client)
}

pub async fn fetch_groups_from_relay(
    relay_url: &str,
) -> std::result::Result<Vec<Group>, String> {
    let client = ensure_group_relay(relay_url).await?;
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    let filter = all_groups_filter();
    let events = client
        .fetch_events_from(vec![url], filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch groups: {}", e))?;
    let groups: Vec<Group> = events
        .into_iter()
        .filter_map(|e| parse_group_metadata(&e, relay_url))
        .collect();
    cache_groups(&groups);
    log::info!("Fetched {} groups from {}", groups.len(), relay_url);
    Ok(groups)
}

pub async fn fetch_group_full(
    relay_url: &str,
    group_id: &str,
) -> std::result::Result<Group, String> {
    if let Some(cached) = get_cached_group(relay_url, group_id) {
        return Ok(cached);
    }
    let client = ensure_group_relay(relay_url).await?;
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    let filter = group_metadata_filter(group_id);
    let events = client
        .fetch_events_from(vec![url], filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch group metadata: {}", e))?;
    let mut group_opt: Option<Group> = None;
    for event in events.into_iter() {
        match event.kind.as_u16() {
            KIND_GROUP_METADATA => {
                if let Some(g) = parse_group_metadata(&event, relay_url) {
                    group_opt = Some(g);
                }
            }
            KIND_GROUP_ADMINS => {
                let admins = parse_group_admins(&event);
                cache_admins(relay_url, group_id, admins);
            }
            KIND_GROUP_MEMBERS => {
                let members: HashSet<String> =
                    parse_group_members(&event).into_iter().collect();
                cache_members(relay_url, group_id, members);
            }
            _ => {}
        }
    }
    let group = group_opt.ok_or_else(|| "Group metadata not found".to_string())?;
    cache_group(&group);
    Ok(group)
}

pub async fn fetch_group_messages(
    relay_url: &str,
    group_id: &str,
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<GroupMessage>, String> {
    let client = ensure_group_relay(relay_url).await?;
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    let filter = group_messages_filter(group_id, limit, until);
    let events = client
        .fetch_events_from(vec![url], filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch messages: {}", e))?;
    let mut messages: Vec<GroupMessage> = events
        .into_iter()
        .filter_map(|e| parse_group_message(&e))
        .collect();
    messages.sort_by_key(|m| std::cmp::Reverse(m.created_at));
    cache_messages(&messages);
    for msg in &messages {
        track_previous_event(relay_url, group_id, &msg.id);
    }
    log::info!(
        "Fetched {} messages for group {}",
        messages.len(),
        group_id
    );
    Ok(messages)
}

pub async fn fetch_group_members(
    relay_url: &str,
    group_id: &str,
) -> std::result::Result<HashSet<String>, String> {
    let client = ensure_group_relay(relay_url).await?;
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_GROUP_MEMBERS))
        .identifier(group_id)
        .limit(1);
    let events = client
        .fetch_events_from(vec![url], filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch members: {}", e))?;
    let members: HashSet<String> = events
        .into_iter()
        .flat_map(|e| parse_group_members(&e))
        .collect();
    cache_members(relay_url, group_id, members.clone());
    log::info!("Fetched {} members for group {}", members.len(), group_id);
    Ok(members)
}

pub async fn fetch_group_admins(
    relay_url: &str,
    group_id: &str,
) -> std::result::Result<Vec<GroupAdmin>, String> {
    let client = ensure_group_relay(relay_url).await?;
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_GROUP_ADMINS))
        .identifier(group_id)
        .limit(1);
    let events = client
        .fetch_events_from(vec![url], filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch admins: {}", e))?;
    let admins: Vec<GroupAdmin> = events
        .into_iter()
        .flat_map(|e| parse_group_admins(&e))
        .collect();
    cache_admins(relay_url, group_id, admins.clone());
    log::info!("Fetched {} admins for group {}", admins.len(), group_id);
    Ok(admins)
}

pub async fn check_membership_status(
    relay_url: &str,
    group_id: &str,
    user_pubkey: &str,
) -> std::result::Result<GroupMembershipStatus, String> {
    let admins = get_cached_admins(relay_url, group_id);
    if let Some(admin) = admins.iter().find(|a| a.pubkey == user_pubkey) {
        let status = GroupMembershipStatus::Admin {
            role: admin.role.clone(),
        };
        cache_membership(relay_url, group_id, status.clone());
        return Ok(status);
    }
    let members = get_cached_members(relay_url, group_id);
    if members.contains(user_pubkey) {
        cache_membership(relay_url, group_id, GroupMembershipStatus::Member);
        return Ok(GroupMembershipStatus::Member);
    }
    let client = ensure_group_relay(relay_url).await?;
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    let pubkey =
        PublicKey::from_hex(user_pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kinds(vec![
            Kind::Custom(KIND_PUT_USER),
            Kind::Custom(KIND_REMOVE_USER),
        ])
        .author(pubkey)
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), group_id)
        .limit(2);
    let events = client
        .fetch_events_from(vec![url], filter, Duration::from_secs(5))
        .await
        .map_err(|e| format!("Failed to check membership: {}", e))?;
    if events.is_empty() {
        let members = get_cached_members(relay_url, group_id);
        if members.contains(user_pubkey) {
            cache_membership(relay_url, group_id, GroupMembershipStatus::NotInGroupButKnown);
            return Ok(GroupMembershipStatus::NotInGroupButKnown);
        }
        cache_membership(relay_url, group_id, GroupMembershipStatus::NotInGroup);
        return Ok(GroupMembershipStatus::NotInGroup);
    }
    let latest = events
        .into_iter()
        .max_by_key(|e| e.created_at)
        .unwrap();
    let status = if latest.kind.as_u16() == KIND_PUT_USER {
        GroupMembershipStatus::Member
    } else {
        GroupMembershipStatus::NotInGroup
    };
    cache_membership(relay_url, group_id, status.clone());
    Ok(status)
}

pub async fn fetch_user_groups_list(
) -> std::result::Result<Vec<(String, String)>, String> {
    let client =
        crate::stores::nostr_client::get_client().ok_or("Client not initialized")?;
    let pubkey = crate::stores::auth_store::get_pubkey()
        .ok_or("Not logged in")?;
    let pk = PublicKey::from_hex(&pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_USER_GROUPS_LIST))
        .author(pk)
        .limit(1);
    let events = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch groups list: {}", e))?;
    if let Some(event) = events.into_iter().next() {
        let groups = parse_groups_list_from_event(&event);
        return Ok(groups);
    }
    Ok(Vec::new())
}

pub async fn fetch_user_groups() -> std::result::Result<Vec<Group>, String> {
    *GROUPS_LOADING.write() = true;
    let group_list = fetch_user_groups_list().await?;
    let mut groups = Vec::new();
    for (relay_url, group_id) in &group_list {
        if let Ok(group) = fetch_group_full(relay_url, group_id).await {
            groups.push(group);
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.created_at));
    cache_groups(&groups);
    *GROUPS_LOADING.write() = false;
    *GROUP_INITIALIZED.write() = true;
    log::info!("Fetched {} user groups", groups.len());
    Ok(groups)
}

fn parse_groups_list_from_event(event: &NostrEvent) -> Vec<(String, String)> {
    event
        .tags
        .iter()
        .filter(|t| {
            let slice = t.as_slice();
            slice.first().map(|s| s.as_str()) == Some("group")
        })
        .filter_map(|t| {
            let slice = t.as_slice();
            let group_id = slice.get(1)?.to_string();
            let relay_url = slice.get(2)?.to_string();
            Some((relay_url, group_id))
        })
        .collect()
}

#[allow(dead_code)]
pub async fn search_groups(
    relay_urls: &[&str],
    query: &str,
) -> std::result::Result<Vec<Group>, String> {
    let query_lower = query.to_lowercase();
    let mut all_groups = Vec::new();
    for relay_url in relay_urls {
        if let Ok(groups) = fetch_groups_from_relay(relay_url).await {
            all_groups.extend(groups);
        }
    }
    let filtered: Vec<Group> = all_groups
        .into_iter()
        .filter(|g| {
            g.name
                .as_ref()
                .map(|n| n.to_lowercase().contains(&query_lower))
                .unwrap_or(false)
                || g.about
                    .as_ref()
                    .map(|a| a.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
                || g.id.to_lowercase().contains(&query_lower)
        })
        .collect();
    cache_groups(&filtered);
    log::info!("Search '{}' found {} groups", query, filtered.len());
    Ok(filtered)
}

pub async fn fetch_join_requests(
    relay_url: &str,
    group_id: &str,
) -> std::result::Result<Vec<JoinRequest>, String> {
    let client = ensure_group_relay(relay_url).await?;
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_JOIN_REQUEST))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), group_id)
        .limit(50);
    let events = client
        .fetch_events_from(vec![url], filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch join requests: {}", e))?;
    let requests: Vec<JoinRequest> = events
        .into_iter()
        .filter_map(|e| parse_join_request(&e))
        .collect();
    cache_join_requests(relay_url, group_id, requests.clone());
    log::info!("Fetched {} join requests for group {}", requests.len(), group_id);
    Ok(requests)
}

pub async fn fetch_group_notes(
    relay_url: &str,
    group_id: &str,
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<GroupNote>, String> {
    let client = ensure_group_relay(relay_url).await?;
    let url = RelayUrl::parse(relay_url).map_err(|e| format!("Invalid relay URL: {}", e))?;
    let mut filter = Filter::new()
        .kinds(vec![
            Kind::Custom(KIND_GROUP_NOTE),
            Kind::Custom(KIND_GROUP_NOTE_REPLY),
        ])
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), group_id)
        .limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }
    let events = client
        .fetch_events_from(vec![url], filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch notes: {}", e))?;
    let notes: Vec<GroupNote> = events
        .into_iter()
        .filter_map(|e| parse_group_note(&e))
        .collect();
    {
        let mut cache = GROUP_NOTES_CACHE.write();
        for note in &notes {
            cache.put(note.id.clone(), note.clone());
        }
    }
    log::info!("Fetched {} notes for group {}", notes.len(), group_id);
    Ok(notes)
}
