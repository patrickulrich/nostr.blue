use super::*;

pub async fn create_community(
    identifier: &str,
    name: &str,
    description: Option<&str>,
    image: Option<&str>,
    rules: Option<&str>,
    moderators: Vec<String>,
) -> std::result::Result<String, String> {
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(identifier),
        Tag::custom(TagKind::Custom("name".into()), vec![name]),
    ];
    if let Some(desc) = description {
        tags.push(Tag::custom(
            TagKind::Custom("description".into()),
            vec![desc],
        ));
    }
    if let Some(img) = image {
        tags.push(Tag::custom(TagKind::Custom("image".into()), vec![img]));
    }
    if let Some(r) = rules {
        tags.push(Tag::custom(TagKind::Custom("rules".into()), vec![r]));
    }
    for mod_pubkey in moderators {
        tags.push(Tag::custom(
            TagKind::p(),
            vec![mod_pubkey, "".to_string(), "moderator".to_string()],
        ));
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_COMMUNITY_DEFINITION), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign community: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Community,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Community created: {}", event_id);
    Ok(event_id)
}

pub async fn post_to_community(
    community: &Community,
    content: &str,
) -> std::result::Result<String, String> {
    let coord = Coordinate::new(
        Kind::Custom(KIND_COMMUNITY_DEFINITION),
        PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?,
    )
    .identifier(&community.d_tag);
    let target = CommentTarget::coordinate(Cow::Owned(coord), None);
    let builder = EventBuilder::comment(content, target, None);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign post: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Community,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Community post published: {}", event_id);
    Ok(event_id)
}

pub async fn reply_to_post(
    community: &Community,
    parent_post: &CommunityPost,
    content: &str,
) -> std::result::Result<String, String> {
    let community_coord = Coordinate::new(
        Kind::Custom(KIND_COMMUNITY_DEFINITION),
        PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?,
    )
    .identifier(&community.d_tag);
    let parent_id = EventId::from_hex(&parent_post.id)
        .map_err(|e| format!("Invalid parent event ID: {}", e))?;
    let parent_pubkey = PublicKey::from_hex(&parent_post.pubkey)
        .map_err(|e| format!("Invalid parent pubkey: {}", e))?;
    let root_target = CommentTarget::coordinate(Cow::Owned(community_coord), None);
    let parent_target = CommentTarget::event(parent_id, Kind::Comment, Some(parent_pubkey), None);
    let builder = EventBuilder::comment(content, parent_target, Some(root_target));
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign reply: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Community,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Community reply published: {}", event_id);
    Ok(event_id)
}

pub async fn approve_post(
    community: &Community,
    post: &CommunityPost,
) -> std::result::Result<String, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !can_moderate(&current_pubkey, community) {
        return Err("You are not a moderator of this community".to_string());
    }
    let post_json = serde_json::to_string(&post.event).unwrap_or_default();
    let coord = Coordinate::new(
        Kind::Custom(KIND_COMMUNITY_DEFINITION),
        PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?,
    )
    .identifier(&community.d_tag);
    let tags: Vec<Tag> = vec![
        Tag::coordinate(coord, None),
        Tag::event(EventId::from_hex(&post.id).map_err(|e| e.to_string())?),
        Tag::public_key(PublicKey::from_hex(&post.pubkey).map_err(|e| e.to_string())?),
        Tag::custom(TagKind::Custom("k".into()), vec![post.kind.to_string()]),
    ];
    let builder = EventBuilder::new(Kind::Custom(KIND_APPROVAL), &post_json).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign approval: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Community,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Post approved: {}", event_id);
    Ok(event_id)
}

pub async fn remove_post(
    community: &Community,
    post: &CommunityPost,
    reason: Option<&str>,
) -> std::result::Result<String, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !can_moderate(&current_pubkey, community) {
        return Err("You are not a moderator of this community".to_string());
    }
    let content = reason.unwrap_or("");
    let coord = Coordinate::new(
        Kind::Custom(KIND_COMMUNITY_DEFINITION),
        PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?,
    )
    .identifier(&community.d_tag);
    let tags: Vec<Tag> = vec![
        Tag::coordinate(coord, None),
        Tag::event(EventId::from_hex(&post.id).map_err(|e| e.to_string())?),
        Tag::public_key(PublicKey::from_hex(&post.pubkey).map_err(|e| e.to_string())?),
        Tag::custom(TagKind::Custom("k".into()), vec![post.kind.to_string()]),
    ];
    let builder = EventBuilder::new(Kind::Custom(KIND_REMOVAL), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign removal: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Community,
        None,
        std::collections::HashMap::new(),
    ).await;
    log::info!("Post removed: {}", event_id);
    Ok(event_id)
}

pub async fn update_approved_members(
    community: &Community,
    members: Vec<String>,
) -> std::result::Result<String, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if community.pubkey != current_pubkey {
        return Err("Only the community owner can update approved members".to_string());
    }
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&community.d_tag),
        Tag::custom(TagKind::a(), vec![community.a_tag.clone()]),
    ];
    for member in &members {
        if let Ok(pk) = PublicKey::from_hex(member) {
            tags.push(Tag::public_key(pk));
        }
    }
    let builder = EventBuilder::new(Kind::Custom(KIND_APPROVED_MEMBERS), "").tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign approved members: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Community,
        None,
        std::collections::HashMap::new(),
    ).await;
    APPROVED_MEMBERS_CACHE
        .write()
        .insert(community.a_tag.clone(), members.into_iter().collect());
    log::info!("Updated approved members: {}", event_id);
    Ok(event_id)
}

pub async fn submit_join_request(
    community: &Community,
    reason: Option<&str>,
) -> std::result::Result<String, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    let status = get_membership_status(&current_pubkey, community);
    match status {
        MembershipStatus::Owner | MembershipStatus::Moderator | MembershipStatus::Member => {
            return Err("You are already a member of this community".to_string());
        }
        MembershipStatus::Pending { .. } => {
            return Err("You already have a pending join request".to_string());
        }
        MembershipStatus::Banned { .. } => {
            return Err("You are banned from this community".to_string());
        }
        _ => {}
    }
    let coord = Coordinate::new(
        Kind::Custom(KIND_COMMUNITY_DEFINITION),
        PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?,
    )
    .identifier(&community.d_tag);
    let content = reason.unwrap_or("");
    let tags: Vec<Tag> = vec![
        Tag::coordinate(coord, None),
        Tag::public_key(PublicKey::from_hex(&community.pubkey).map_err(|e| e.to_string())?),
    ];
    let builder = EventBuilder::new(Kind::Custom(KIND_JOIN_REQUEST), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign join request: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Community,
        None,
        std::collections::HashMap::new(),
    ).await;
    let request = JoinRequest {
        id: event_id.clone(),
        community_a_tag: community.a_tag.clone(),
        user_pubkey: current_pubkey,
        reason: reason.map(|s| s.to_string()),
        created_at: crate::platform::timestamp::now_secs(),
        event: None,
    };
    USER_PENDING_REQUESTS
        .write()
        .insert(community.a_tag.clone(), request);
    log::info!("Join request submitted: {}", event_id);
    Ok(event_id)
}

pub async fn approve_join_request(
    community: &Community,
    request: &JoinRequest,
) -> std::result::Result<String, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !can_moderate(&current_pubkey, community) {
        return Err("You are not a moderator of this community".to_string());
    }
    let mut members: Vec<String> = APPROVED_MEMBERS_CACHE
        .read()
        .get(&community.a_tag)
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();
    if !members.contains(&request.user_pubkey) {
        members.push(request.user_pubkey.clone());
    }
    let result = update_approved_members(community, members).await?;
    if let Some(requests) = PENDING_JOIN_REQUESTS_CACHE
        .write()
        .get_mut(&community.a_tag)
    {
        requests.retain(|r| r.id != request.id);
    }
    log::info!(
        "Approved join request {} for user {}",
        request.id,
        request.user_pubkey
    );
    Ok(result)
}

pub async fn decline_join_request(
    community: &Community,
    user_pubkey: &str,
    reason: Option<&str>,
) -> std::result::Result<String, String> {
    let current_pubkey = crate::stores::auth_store::get_pubkey().ok_or("Not logged in")?;
    if !can_moderate(&current_pubkey, community) {
        return Err("You are not a moderator of this community".to_string());
    }
    let mut declined: Vec<String> = DECLINED_MEMBERS_CACHE
        .read()
        .get(&community.a_tag)
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();
    if !declined.contains(&user_pubkey.to_string()) {
        declined.push(user_pubkey.to_string());
    }
    let mut tags: Vec<Tag> = vec![
        Tag::identifier(&community.d_tag),
        Tag::custom(TagKind::a(), vec![community.a_tag.clone()]),
    ];
    for pubkey in &declined {
        if let Ok(pk) = PublicKey::from_hex(pubkey) {
            tags.push(Tag::public_key(pk));
        }
    }
    let content = reason.unwrap_or("");
    let builder = EventBuilder::new(Kind::Custom(KIND_DECLINED_MEMBERS), content).tags(tags);
    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign declined members: {}", e))?;
    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Community,
        None,
        std::collections::HashMap::new(),
    ).await;
    DECLINED_MEMBERS_CACHE
        .write()
        .insert(community.a_tag.clone(), declined.into_iter().collect());
    if let Some(requests) = PENDING_JOIN_REQUESTS_CACHE
        .write()
        .get_mut(&community.a_tag)
    {
        requests.retain(|r| r.user_pubkey != user_pubkey);
    }
    log::info!("Declined join request for user {}", user_pubkey);
    Ok(event_id)
}
