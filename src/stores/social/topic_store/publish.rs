use super::*;
use nostr::nips::nip22::CommentTarget;
use nostr::nips::nip25::ReactionTarget;
use nostr::nips::nip73::ExternalContentId;
use std::borrow::Cow;

pub async fn create_topic_metadata(
    topic: &str,
    description: &str,
    rules: &str,
) -> std::result::Result<String, String> {
    let d_tag = topic_metadata_d_tag(topic);
    let content = serde_json::json!({
        "description": description,
        "rules": rules,
    })
    .to_string();

    let builder = EventBuilder::new(Kind::ApplicationSpecificData, content)
        .tag(Tag::identifier(&d_tag));

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign topic metadata: {}", e))?;

    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Topic,
        None,
        std::collections::HashMap::new(),
    )
    .await;

    TOPIC_METADATA_CACHE.write().put(
        topic.to_string(),
        TopicMetadata {
            name: topic.to_string(),
            description: description.to_string(),
            rules: rules.to_string(),
            created_at: 0,
            creator_pubkey: String::new(),
        },
    );

    log::info!("Topic metadata published for #{}: {}", topic, event_id);
    Ok(event_id)
}

pub async fn pin_post(
    topic: &str,
    event_id: &str,
    current_pins: &[String],
) -> std::result::Result<String, String> {
    if current_pins.len() >= MAX_PINS {
        return Err(format!("Maximum {} pinned posts reached", MAX_PINS));
    }
    if current_pins.contains(&event_id.to_string()) {
        return Err("Post already pinned".to_string());
    }

    let mut new_pins = current_pins.to_vec();
    new_pins.push(event_id.to_string());

    let d_tag = topic_pins_d_tag(topic);
    let tags: Vec<Tag> = std::iter::once(Tag::identifier(&d_tag))
        .chain(new_pins.iter().map(|id| Tag::event(EventId::from_hex(id).unwrap_or_else(|_| EventId::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap()))))
        .collect();

    let builder = EventBuilder::new(Kind::ApplicationSpecificData, "").tags(tags);

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign pin event: {}", e))?;

    let id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Topic,
        None,
        std::collections::HashMap::new(),
    )
    .await;

    TOPIC_PINS_CACHE
        .write()
        .put(topic.to_string(), new_pins);
    log::info!("Pinned post {} in topic #{}", event_id, topic);
    Ok(id)
}

pub async fn unpin_post(
    topic: &str,
    event_id: &str,
    current_pins: &[String],
) -> std::result::Result<String, String> {
    let new_pins: Vec<String> = current_pins
        .iter()
        .filter(|id| *id != event_id)
        .cloned()
        .collect();

    let d_tag = topic_pins_d_tag(topic);
    let tags: Vec<Tag> = std::iter::once(Tag::identifier(&d_tag))
        .chain(
            new_pins
                .iter()
                .filter_map(|id| EventId::from_hex(id).ok())
                .map(Tag::event),
        )
        .collect();

    let builder = EventBuilder::new(Kind::ApplicationSpecificData, "").tags(tags);

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign unpin event: {}", e))?;

    let id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Topic,
        None,
        std::collections::HashMap::new(),
    )
    .await;

    TOPIC_PINS_CACHE
        .write()
        .put(topic.to_string(), new_pins);
    log::info!("Unpinned post {} in topic #{}", event_id, topic);
    Ok(id)
}

pub async fn create_topic_post_with_media(
    topic: &str,
    content: &str,
    media_urls: Vec<String>,
) -> std::result::Result<String, String> {
    let content_id = ExternalContentId::Hashtag(topic.to_string());
    let target = CommentTarget::external(Cow::Owned(content_id.clone()), None);
    let root = CommentTarget::external(Cow::Owned(content_id), None);

    let mut builder = EventBuilder::comment(content, target, Some(root));

    for url in &media_urls {
        let mut imeta_fields = vec![format!("url {}", url)];
        if let Some(mime) =
            crate::stores::nostr_client::detect_mime_type(url)
        {
            imeta_fields.push(format!("m {}", mime));
        }
        builder = builder.tag(Tag::custom(
            nostr::TagKind::custom("imeta"),
            imeta_fields,
        ));
    }

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign topic post: {}", e))?;

    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Topic,
        None,
        std::collections::HashMap::new(),
    )
    .await;

    log::info!("Topic post with media published in #{}: {}", topic, event_id);
    Ok(event_id)
}

pub async fn create_topic_post(topic: &str, content: &str) -> std::result::Result<String, String> {
    create_topic_post_with_media(topic, content, Vec::new()).await
}

pub async fn reply_to_topic_post_with_media(
    parent: &TopicPost,
    content: &str,
    media_urls: Vec<String>,
) -> std::result::Result<String, String> {
    let parent_id =
        EventId::from_hex(&parent.id).map_err(|e| format!("Invalid parent event ID: {}", e))?;
    let parent_pk =
        PublicKey::from_hex(&parent.pubkey).map_err(|e| format!("Invalid parent pubkey: {}", e))?;

    let comment_to = CommentTarget::event(parent_id, Kind::Comment, Some(parent_pk), None);

    let root = extract_root_external_content(&parent.event)
        .map(|cid| CommentTarget::external(Cow::Owned(cid), None));

    let mut builder = EventBuilder::comment(content, comment_to, root);

    for url in &media_urls {
        let mut imeta_fields = vec![format!("url {}", url)];
        if let Some(mime) = crate::stores::nostr_client::detect_mime_type(url) {
            imeta_fields.push(format!("m {}", mime));
        }
        builder = builder.tag(Tag::custom(
            nostr::TagKind::custom("imeta"),
            imeta_fields,
        ));
    }

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign reply: {}", e))?;

    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Topic,
        None,
        std::collections::HashMap::new(),
    )
    .await;

    log::info!("Topic reply with media published: {}", event_id);
    Ok(event_id)
}

pub async fn reply_to_topic_post(
    parent: &TopicPost,
    content: &str,
) -> std::result::Result<String, String> {
    reply_to_topic_post_with_media(parent, content, Vec::new()).await
}

pub async fn vote_on_post(
    post: &TopicPost,
    direction: VoteDirection,
) -> std::result::Result<(String, VoteCounts), String> {
    let event_id = EventId::from_hex(&post.id).map_err(|e| format!("Invalid event ID: {}", e))?;
    let pubkey = PublicKey::from_hex(&post.pubkey).map_err(|e| format!("Invalid pubkey: {}", e))?;

    let target = ReactionTarget {
        event_id,
        public_key: pubkey,
        coordinate: None,
        kind: Some(Kind::Comment),
        relay_hint: None,
    };

    let reaction = match direction {
        VoteDirection::Up => "+",
        VoteDirection::Down => "-",
    };

    let builder = EventBuilder::reaction(target, reaction);

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign vote: {}", e))?;

    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Topic,
        None,
        std::collections::HashMap::new(),
    ).await;

    let mut counts = get_cached_votes(&post.id).unwrap_or_default();
    if let Some(prev) = counts.user_vote {
        match prev {
            VoteDirection::Up => counts.upvotes = counts.upvotes.saturating_sub(1),
            VoteDirection::Down => counts.downvotes = counts.downvotes.saturating_sub(1),
        }
    }
    match direction {
        VoteDirection::Up => counts.upvotes += 1,
        VoteDirection::Down => counts.downvotes += 1,
    }
    counts.user_vote = Some(direction);
    cache_votes(&post.id, counts.clone());

    log::info!("Vote {} on post {}", reaction, event_id);
    Ok((event_id, counts))
}

pub async fn subscribe_to_topic(topic: &str) -> std::result::Result<(), String> {
    let mut current = get_subscribed_topic_names();
    if !current.contains(&topic.to_string()) {
        current.push(topic.to_string());
    }
    update_subscriptions(&current).await?;
    SUBSCRIBED_TOPICS.write().put(topic.to_string(), true);
    log::info!("Subscribed to topic #{}", topic);
    Ok(())
}

pub async fn unsubscribe_from_topic(topic: &str) -> std::result::Result<(), String> {
    let current: Vec<String> = get_subscribed_topic_names()
        .into_iter()
        .filter(|t| t != topic)
        .collect();
    update_subscriptions(&current).await?;
    SUBSCRIBED_TOPICS.write().pop(topic);
    log::info!("Unsubscribed from topic #{}", topic);
    Ok(())
}

pub async fn update_subscriptions(topics: &[String]) -> std::result::Result<(), String> {
    let tags: Vec<Tag> = topics
        .iter()
        .map(|topic| {
            Tag::from_standardized(TagStandard::ExternalContent {
                content: ExternalContentId::Hashtag(topic.clone()),
                hint: None,
                uppercase: true,
            })
        })
        .collect();

    let builder = EventBuilder::new(Kind::Custom(KIND_TOPIC_SUBSCRIPTION), "").tags(tags);

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign subscriptions: {}", e))?;

    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Topic,
        None,
        std::collections::HashMap::new(),

    ).await;

    log::info!("Updated topic subscriptions ({} topics)", topics.len());
    Ok(())
}
