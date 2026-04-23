use super::*;
use nostr::nips::nip22::CommentTarget;
use nostr::nips::nip25::ReactionTarget;
use nostr::nips::nip73::ExternalContentId;
use std::borrow::Cow;

pub async fn create_topic_post(topic: &str, content: &str) -> std::result::Result<String, String> {
    let hashtag = ExternalContentId::Hashtag(topic.to_string());
    let target = CommentTarget::external(Cow::Owned(hashtag), None);

    let builder = EventBuilder::comment(content, target, None);

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign topic post: {}", e))?;

    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Topic,
        None,
        std::collections::HashMap::new(),
    ).await;

    log::info!(
        "Topic post published in #{}: {}",
        topic,
        event_id
    );
    Ok(event_id)
}

pub async fn reply_to_topic_post(
    parent: &TopicPost,
    content: &str,
) -> std::result::Result<String, String> {
    let parent_id =
        EventId::from_hex(&parent.id).map_err(|e| format!("Invalid parent event ID: {}", e))?;
    let parent_pk =
        PublicKey::from_hex(&parent.pubkey).map_err(|e| format!("Invalid parent pubkey: {}", e))?;

    let comment_to = CommentTarget::event(parent_id, Kind::Comment, Some(parent_pk), None);

    let hashtag = ExternalContentId::Hashtag(parent.topic.clone());
    let root = CommentTarget::external(Cow::Owned(hashtag), None);

    let builder = EventBuilder::comment(content, comment_to, Some(root));

    let event = crate::stores::publish_queue::signing::sign_event_builder(builder)
        .await
        .map_err(|e| format!("Failed to sign reply: {}", e))?;

    let event_id = event.id.to_hex();
    crate::stores::publish_queue::enqueue(
        event,
        crate::stores::publish_queue::types::QueueEventType::Topic,
        None,
        std::collections::HashMap::new(),
    ).await;

    log::info!("Topic reply published: {}", event_id);
    Ok(event_id)
}

pub async fn vote_on_post(
    post: &TopicPost,
    direction: VoteDirection,
) -> std::result::Result<String, String> {
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
    cache_votes(&post.id, counts);

    log::info!("Vote {} on post {}", reaction, event_id);
    Ok(event_id)
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
