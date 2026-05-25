use super::*;

pub async fn send_group_reaction(
    relay_url: &str,
    group_id: &str,
    target_event_id: &str,
    target_author: &str,
    content: &str,
) -> std::result::Result<String, String> {
    use nostr::nips::nip25::ReactionTarget;
    let target_event_id = nostr::EventId::from_hex(target_event_id)
        .map_err(|e| format!("Invalid event ID: {}", e))?;
    let target_pubkey = nostr::PublicKey::parse(target_author)
        .map_err(|e| format!("Invalid pubkey: {}", e))?;
    let target = ReactionTarget {
        event_id: target_event_id,
        public_key: target_pubkey,
        coordinate: None,
        kind: None,
        relay_hint: None,
    };
    let builder = nostr::EventBuilder::reaction(target, content)
        .tag(Tag::custom(
            TagKind::Custom("h".into()),
            vec![group_id.to_string()],
        ));
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign reaction: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}

pub async fn send_group_message(
    relay_url: &str,
    group_id: &str,
    content: &str,
    reply_to: Option<&str>,
) -> std::result::Result<String, String> {
    let mut tags: Vec<Tag> = vec![Tag::custom(
        TagKind::Custom("h".into()),
        vec![group_id.to_string()],
    )];
    if let Some(reply) = reply_to {
        tags.push(Tag::custom(
            TagKind::Custom("q".into()),
            vec![reply.to_string()],
        ));
    }
    let previous_refs = super::get_previous_refs(relay_url, group_id, 5);
    for prev in &previous_refs {
        tags.push(Tag::custom(
            TagKind::Custom("previous".into()),
            vec![prev.clone()],
        ));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_CHAT_MESSAGE), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign message: {}", e))?;
    let event_id = event.id.to_hex();
    super::track_previous_event(relay_url, group_id, &event_id);
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Group message sent: {}", event_id);
    Ok(event_id)
}

pub async fn join_group(
    relay_url: &str,
    group_id: &str,
    reason: Option<&str>,
    invite_code: Option<&str>,
) -> std::result::Result<String, String> {
    crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let mut tags: Vec<Tag> = vec![Tag::custom(
        TagKind::Custom("h".into()),
        vec![group_id.to_string()],
    )];
    if let Some(code) = invite_code {
        tags.push(Tag::custom(
            TagKind::Custom("code".into()),
            vec![code.to_string()],
        ));
    }
    let content = reason.unwrap_or("");
    let builder = EventBuilder::new(Kind::Custom(KIND_JOIN_REQUEST), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign join request: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    cache_membership(relay_url, group_id, GroupMembershipStatus::Pending);
    log::info!("Join request sent for group {}", group_id);
    Ok(event_id)
}

pub async fn leave_group(
    relay_url: &str,
    group_id: &str,
    reason: Option<&str>,
) -> std::result::Result<String, String> {
    crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let tags: Vec<Tag> = vec![Tag::custom(
        TagKind::Custom("h".into()),
        vec![group_id.to_string()],
    )];
    let content = reason.unwrap_or("");
    let builder = EventBuilder::new(Kind::Custom(KIND_LEAVE_REQUEST), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign leave request: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    {
        let mut members = GROUP_MEMBERS_CACHE.write();
        if let Some(member_set) = members.get_mut(&group_id_from_parts(relay_url, group_id)) {
            let pk = crate::stores::auth_store::get_pubkey().unwrap_or_default();
            member_set.remove(&pk);
        }
    }
    cache_membership(relay_url, group_id, GroupMembershipStatus::NotInGroup);
    log::info!("Leave request sent for group {}", group_id);
    Ok(event_id)
}

pub async fn add_user_to_group(
    relay_url: &str,
    group_id: &str,
    pubkey: &str,
    roles: Vec<String>,
) -> std::result::Result<String, String> {
    let current_pubkey =
        crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !is_group_admin(relay_url, group_id, &current_pubkey) {
        return Err("You are not an admin of this group".to_string());
    }
    let mut tag_content = vec![pubkey.to_string()];
    tag_content.extend(roles);
    let tags: Vec<Tag> = vec![
        Tag::custom(
            TagKind::Custom("h".into()),
            vec![group_id.to_string()],
        ),
        Tag::custom(TagKind::p(), tag_content),
    ];
    let builder = EventBuilder::new(Kind::Custom(KIND_PUT_USER), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign add user: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Added user {} to group {}", truncate_pubkey(pubkey), group_id);
    Ok(event_id)
}

pub async fn remove_user_from_group(
    relay_url: &str,
    group_id: &str,
    pubkey: &str,
) -> std::result::Result<String, String> {
    let current_pubkey =
        crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !is_group_admin(relay_url, group_id, &current_pubkey) {
        return Err("You are not an admin of this group".to_string());
    }
    let tags: Vec<Tag> = vec![
        Tag::custom(
            TagKind::Custom("h".into()),
            vec![group_id.to_string()],
        ),
        Tag::custom(TagKind::p(), vec![pubkey.to_string()]),
    ];
    let builder = EventBuilder::new(Kind::Custom(KIND_REMOVE_USER), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign remove user: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    log::info!(
        "Removed user {} from group {}",
        truncate_pubkey(pubkey),
        group_id
    );
    Ok(event_id)
}

pub async fn edit_group_metadata(
    relay_url: &str,
    group_id: &str,
    name: Option<&str>,
    about: Option<&str>,
    picture: Option<&str>,
) -> std::result::Result<String, String> {
    let current_pubkey =
        crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !is_group_admin(relay_url, group_id, &current_pubkey) {
        return Err("You are not an admin of this group".to_string());
    }
    let mut tags: Vec<Tag> = vec![Tag::custom(
        TagKind::Custom("h".into()),
        vec![group_id.to_string()],
    )];
    if let Some(n) = name {
        tags.push(Tag::custom(
            TagKind::Custom("name".into()),
            vec![n.to_string()],
        ));
    }
    if let Some(a) = about {
        tags.push(Tag::custom(
            TagKind::Custom("about".into()),
            vec![a.to_string()],
        ));
    }
    if let Some(p) = picture {
        tags.push(Tag::custom(
            TagKind::Custom("picture".into()),
            vec![p.to_string()],
        ));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_EDIT_METADATA), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign metadata edit: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    if let Some(mut cached) = get_cached_group(relay_url, group_id) {
        if let Some(n) = name {
            cached.name = Some(n.to_string());
        }
        if let Some(a) = about {
            cached.about = Some(a.to_string());
        }
        if let Some(p) = picture {
            cached.picture = Some(p.to_string());
        }
        cache_group(&cached);
    }
    log::info!("Edited metadata for group {}", group_id);
    Ok(event_id)
}

pub async fn delete_group_event(
    relay_url: &str,
    event_id: &str,
    group_id: &str,
) -> std::result::Result<String, String> {
    let current_pubkey =
        crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !is_group_admin(relay_url, group_id, &current_pubkey) {
        return Err("You are not an admin of this group".to_string());
    }
    let tags: Vec<Tag> = vec![
        Tag::custom(
            TagKind::Custom("h".into()),
            vec![group_id.to_string()],
        ),
        Tag::custom(
            TagKind::Custom("e".into()),
            vec![event_id.to_string()],
        ),
    ];
    let builder = EventBuilder::new(Kind::Custom(KIND_DELETE_EVENT), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign event deletion: {}", e))?;
    let queue_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Deleted event {} in group {}", event_id, group_id);
    Ok(queue_id)
}

pub async fn add_permission(
    relay_url: &str,
    group_id: &str,
    pubkey: &str,
    permission: &str,
) -> std::result::Result<String, String> {
    let current_pubkey =
        crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !is_group_admin(relay_url, group_id, &current_pubkey) {
        return Err("You are not an admin of this group".to_string());
    }
    let tags: Vec<Tag> = vec![
        Tag::custom(
            TagKind::Custom("h".into()),
            vec![group_id.to_string()],
        ),
        Tag::custom(TagKind::p(), vec![pubkey.to_string(), permission.to_string()]),
    ];
    let builder = EventBuilder::new(Kind::Custom(KIND_ADD_PERMISSION), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign add permission: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Added permission {} for {} in {}", permission, pubkey, group_id);
    Ok(event_id)
}

pub async fn remove_permission(
    relay_url: &str,
    group_id: &str,
    pubkey: &str,
    permission: &str,
) -> std::result::Result<String, String> {
    let current_pubkey =
        crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !is_group_admin(relay_url, group_id, &current_pubkey) {
        return Err("You are not an admin of this group".to_string());
    }
    let tags: Vec<Tag> = vec![
        Tag::custom(
            TagKind::Custom("h".into()),
            vec![group_id.to_string()],
        ),
        Tag::custom(TagKind::p(), vec![pubkey.to_string(), permission.to_string()]),
    ];
    let builder = EventBuilder::new(Kind::Custom(KIND_REMOVE_PERMISSION), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign remove permission: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Removed permission {} from {} in {}", permission, pubkey, group_id);
    Ok(event_id)
}

pub async fn edit_group_status(
    relay_url: &str,
    group_id: &str,
    is_private: bool,
    is_closed: bool,
    is_restricted: bool,
    is_hidden: bool,
) -> std::result::Result<String, String> {
    let current_pubkey =
        crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !is_group_admin(relay_url, group_id, &current_pubkey) {
        return Err("You are not an admin of this group".to_string());
    }
    let mut tags: Vec<Tag> = vec![Tag::custom(
        TagKind::Custom("h".into()),
        vec![group_id.to_string()],
    )];
    if is_private {
        tags.push(Tag::custom(TagKind::Custom("private".into()), vec![String::new()]));
    }
    if is_closed {
        tags.push(Tag::custom(TagKind::Custom("closed".into()), vec![String::new()]));
    }
    if is_restricted {
        tags.push(Tag::custom(
            TagKind::Custom("restricted".into()),
            vec![String::new()],
        ));
    }
    if is_hidden {
        tags.push(Tag::custom(TagKind::Custom("hidden".into()), vec![String::new()]));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_EDIT_GROUP_STATUS), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign status edit: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Edited status for group {}", group_id);
    Ok(event_id)
}

pub async fn delete_group(
    relay_url: &str,
    group_id: &str,
) -> std::result::Result<String, String> {
    let current_pubkey =
        crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !is_group_admin(relay_url, group_id, &current_pubkey) {
        return Err("You are not an admin of this group".to_string());
    }
    let tags: Vec<Tag> = vec![Tag::custom(
        TagKind::Custom("h".into()),
        vec![group_id.to_string()],
    )];
    let builder = EventBuilder::new(Kind::Custom(KIND_DELETE_GROUP), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign delete group: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Deleted group {}", group_id);
    Ok(event_id)
}

pub async fn create_group(
    relay_url: &str,
) -> std::result::Result<String, String> {
    crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let builder = EventBuilder::new(Kind::Custom(KIND_CREATE_GROUP), "");
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign create group: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Group creation request sent to {}", relay_url);
    Ok(event_id)
}

pub async fn create_invite(
    relay_url: &str,
    group_id: &str,
    code: &str,
) -> std::result::Result<String, String> {
    let current_pubkey =
        crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !is_group_admin(relay_url, group_id, &current_pubkey) {
        return Err("You are not an admin of this group".to_string());
    }
    let tags: Vec<Tag> = vec![
        Tag::custom(
            TagKind::Custom("h".into()),
            vec![group_id.to_string()],
        ),
        Tag::custom(
            TagKind::Custom("code".into()),
            vec![code.to_string()],
        ),
    ];
    let builder = EventBuilder::new(Kind::Custom(KIND_CREATE_INVITE), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign invite: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Created invite for group {}", group_id);
    Ok(event_id)
}

pub async fn update_user_groups_list(
    groups: &[(String, String)],
) -> std::result::Result<String, String> {
    let _pubkey =
        crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let mut tags: Vec<Tag> = Vec::new();
    for (relay_url, group_id) in groups {
        tags.push(Tag::custom(
            TagKind::Custom("group".into()),
            vec![group_id.clone(), relay_url.clone()],
        ));
    }
    let builder =
        EventBuilder::new(Kind::Custom(KIND_USER_GROUPS_LIST), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign groups list: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        None,
        std::collections::HashMap::new(),
    )
    .await;
    log::info!("Updated user groups list: {} groups", groups.len());
    Ok(event_id)
}

pub async fn add_group_to_user_list(
    relay_url: &str,
    group_id: &str,
) -> std::result::Result<String, String> {
    let mut current = fetch_user_groups_list().await.unwrap_or_default();
    let exists = current
        .iter()
        .any(|(r, g)| r == relay_url && g == group_id);
    if !exists {
        current.push((relay_url.to_string(), group_id.to_string()));
    }
    update_user_groups_list(&current).await
}

pub async fn remove_group_from_user_list(
    relay_url: &str,
    group_id: &str,
) -> std::result::Result<String, String> {
    let mut current = fetch_user_groups_list().await.unwrap_or_default();
    current.retain(|(r, g)| !(r == relay_url && g == group_id));
    update_user_groups_list(&current).await
}

pub async fn edit_group_message(
    relay_url: &str,
    group_id: &str,
    original_event_id: &str,
    new_content: &str,
) -> std::result::Result<String, String> {
    delete_group_event(relay_url, original_event_id, group_id).await?;
    send_group_message(relay_url, group_id, new_content, None).await
}

#[allow(dead_code)]
pub async fn send_group_note(
    relay_url: &str,
    group_id: &str,
    content: &str,
) -> std::result::Result<String, String> {
    crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let mut tags: Vec<Tag> = vec![Tag::custom(
        TagKind::Custom("h".into()),
        vec![group_id.to_string()],
    )];
    let previous_refs = super::get_previous_refs(relay_url, group_id, 3);
    for prev in &previous_refs {
        tags.push(Tag::custom(
            TagKind::Custom("previous".into()),
            vec![prev.clone()],
        ));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_GROUP_NOTE), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign note: {}", e))?;
    let event_id = event.id.to_hex();
    super::track_previous_event(relay_url, group_id, &event_id);
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}

#[allow(dead_code)]
pub async fn send_group_note_reply(
    relay_url: &str,
    group_id: &str,
    content: &str,
    root_event_id: &str,
    reply_to_event_id: &str,
    reply_to_pubkey: &str,
) -> std::result::Result<String, String> {
    crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let root_id = nostr::EventId::from_hex(root_event_id)
        .map_err(|e| format!("Invalid root event ID: {}", e))?;
    let reply_id = nostr::EventId::from_hex(reply_to_event_id)
        .map_err(|e| format!("Invalid reply event ID: {}", e))?;
    let reply_pk = nostr::PublicKey::parse(reply_to_pubkey)
        .map_err(|e| format!("Invalid reply pubkey: {}", e))?;
    let relay_url_parsed = RelayUrl::parse(relay_url).ok();
    let mut tags: Vec<Tag> = vec![Tag::custom(
        TagKind::Custom("h".into()),
        vec![group_id.to_string()],
    )];
    tags.push(Tag::from_standardized_without_cell(
        nostr::event::tag::TagStandard::Event {
            event_id: root_id,
            relay_url: relay_url_parsed.clone(),
            marker: Some(nostr_sdk::nips::nip10::Marker::Root),
            public_key: None,
            uppercase: false,
        },
    ));
    tags.push(Tag::from_standardized_without_cell(
        nostr::event::tag::TagStandard::Event {
            event_id: reply_id,
            relay_url: relay_url_parsed,
            marker: Some(nostr_sdk::nips::nip10::Marker::Reply),
            public_key: Some(reply_pk),
            uppercase: false,
        },
    ));
    let previous_refs = super::get_previous_refs(relay_url, group_id, 3);
    for prev in &previous_refs {
        tags.push(Tag::custom(
            TagKind::Custom("previous".into()),
            vec![prev.clone()],
        ));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_GROUP_NOTE_REPLY), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign note reply: {}", e))?;
    let event_id = event.id.to_hex();
    super::track_previous_event(relay_url, group_id, &event_id);
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Group,
        Some(vec![relay_url.to_string()]),
        std::collections::HashMap::new(),
    )
    .await;
    Ok(event_id)
}
