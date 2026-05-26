use super::*;
use std::time::Duration;

/// Fetch posts for a specific topic
pub async fn fetch_topic_posts(
    topic: &str,
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<TopicPost>, String> {
    *LOADING_TOPIC_POSTS.write() = true;
    let filter = topic_posts_filter(topic, limit, until);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;
    *LOADING_TOPIC_POSTS.write() = false;

    match result {
        Ok(events) => {
            let mut posts: Vec<TopicPost> = events.iter().filter_map(parse_topic_post).collect();
            posts.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            cache_topic_posts(&posts);
            log::info!("Fetched {} posts for topic #{}", posts.len(), topic);
            Ok(posts)
        }
        Err(e) => {
            log::error!("Failed to fetch topic posts: {}", e);
            Err(e)
        }
    }
}

/// Fetch recent posts across all topics (global feed)
pub async fn fetch_recent_posts(
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<TopicPost>, String> {
    *LOADING_TOPIC_POSTS.write() = true;
    let filter = recent_topic_posts_filter(limit, until);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;
    *LOADING_TOPIC_POSTS.write() = false;

    match result {
        Ok(events) => {
            let mut posts: Vec<TopicPost> = events.iter().filter_map(parse_topic_post).collect();
            posts.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            cache_topic_posts(&posts);
            log::info!("Fetched {} recent topic posts", posts.len());
            Ok(posts)
        }
        Err(e) => {
            log::error!("Failed to fetch recent topic posts: {}", e);
            Err(e)
        }
    }
}

/// Fetch posts from subscribed topics (user's personalized feed)
pub async fn fetch_subscribed_feed(
    topics: &[String],
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<TopicPost>, String> {
    if topics.is_empty() {
        return Ok(Vec::new());
    }

    *LOADING_TOPIC_POSTS.write() = true;

    // Build filters for each topic and merge results
    let topic_tags: Vec<String> = topics.iter().map(|t| format!("#{}", t)).collect();
    let mut filter = Filter::new()
        .kind(Kind::Comment)
        .custom_tags(SingleLetterTag::uppercase(Alphabet::I), topic_tags)
        .limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }

    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await;
    *LOADING_TOPIC_POSTS.write() = false;

    match result {
        Ok(events) => {
            let mut posts: Vec<TopicPost> = events.iter().filter_map(parse_topic_post).collect();
            posts.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            cache_topic_posts(&posts);
            log::info!("Fetched {} posts for subscribed feed", posts.len());
            Ok(posts)
        }
        Err(e) => {
            log::error!("Failed to fetch subscribed feed: {}", e);
            Err(e)
        }
    }
}

/// Fetch a user's topic subscriptions (kind 10073)
pub async fn fetch_subscriptions(pubkey: PublicKey) -> std::result::Result<Vec<String>, String> {
    *LOADING_SUBSCRIPTIONS.write() = true;
    let filter = subscriptions_filter(pubkey);
    let result =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await;
    *LOADING_SUBSCRIPTIONS.write() = false;

    match result {
        Ok(events) => {
            let topics = events.first().map(parse_subscriptions).unwrap_or_default();

            // Update the subscriptions cache
            let mut cache = SUBSCRIBED_TOPICS.write();
            cache.clear();
            for topic in &topics {
                cache.put(topic.clone(), true);
            }

            log::info!("Fetched {} topic subscriptions", topics.len());
            Ok(topics)
        }
        Err(e) => {
            log::error!("Failed to fetch topic subscriptions: {}", e);
            Err(e)
        }
    }
}

/// Fetch vote counts for a batch of events
pub async fn fetch_votes_batch(
    event_ids: Vec<EventId>,
    user_pubkey: Option<PublicKey>,
) -> std::result::Result<HashMap<String, VoteCounts>, String> {
    if event_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let filter = votes_filter(event_ids.clone());
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;

    // Aggregate votes, deduplicating by latest per pubkey per post
    let mut vote_map: HashMap<String, HashMap<String, (VoteDirection, u64)>> = HashMap::new();

    for event in &events {
        if let Some((target_id, direction)) = parse_vote(event) {
            let pubkey = event.pubkey.to_hex();
            let created_at = event.created_at.as_secs();
            let post_votes = vote_map.entry(target_id).or_default();
            let entry = post_votes.entry(pubkey).or_insert((direction, created_at));
            // Keep the most recent vote per pubkey
            if created_at > entry.1 {
                *entry = (direction, created_at);
            }
        }
    }

    let user_hex = user_pubkey.map(|pk| pk.to_hex());
    let mut result = HashMap::new();

    for id in &event_ids {
        let id_hex = id.to_hex();
        let mut counts = VoteCounts::default();

        if let Some(post_votes) = vote_map.get(&id_hex) {
            for (pubkey, (direction, _)) in post_votes {
                match direction {
                    VoteDirection::Up => counts.upvotes += 1,
                    VoteDirection::Down => counts.downvotes += 1,
                }
                if user_hex.as_ref() == Some(pubkey) {
                    counts.user_vote = Some(*direction);
                }
            }
        }

        cache_votes(&id_hex, counts.clone());
        result.insert(id_hex, counts);
    }

    Ok(result)
}

/// Fetch a single post by event ID
pub async fn fetch_post_by_id(event_id: &str) -> std::result::Result<Option<TopicPost>, String> {
    if let Some(cached) = get_cached_topic_post(event_id) {
        return Ok(Some(cached));
    }

    let id = EventId::from_hex(event_id).map_err(|e| format!("Invalid event ID: {}", e))?;

    if let Some(client) = crate::stores::nostr_client::fetching::get_client() {
        if let Ok(Some(event)) = client.database().event_by_id(&id).await {
            if let Some(post) = parse_topic_post(&event) {
                cache_topic_post(post.clone());
                return Ok(Some(post));
            }
        }
    }

    let filter = Filter::new().id(id).limit(1);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;

    if let Some(event) = events.first() {
        if let Some(post) = parse_topic_post(event) {
            cache_topic_post(post.clone());
            return Ok(Some(post));
        }
    }
    Ok(None)
}

/// Fetch replies to a specific post
pub async fn fetch_post_replies(
    post_id: &str,
    topic: &str,
    limit: usize,
) -> std::result::Result<Vec<TopicPost>, String> {
    let filter = post_replies_filter(post_id, limit);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
            .await?;

    let mut posts: Vec<TopicPost> = events
        .iter()
        .filter_map(parse_topic_post)
        // Only include replies that belong to the same topic
        .filter(|p| p.topic == topic)
        .collect();
    posts.sort_by_key(|a| a.created_at);
    cache_topic_posts(&posts);
    Ok(posts)
}

/// Discover popular topics by fetching recent kind 1111 events and counting unique topics
pub async fn discover_topics(limit: usize) -> std::result::Result<Vec<TopicInfo>, String> {
    let filter = recent_topic_posts_filter(500, None);
    let events =
        crate::stores::nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15))
            .await?;

    let mut topic_counts: HashMap<String, (usize, u64)> = HashMap::new();
    for event in &events {
        if let Some(topic_name) = extract_topic_name(event) {
            let entry = topic_counts.entry(topic_name).or_insert((0, 0));
            entry.0 += 1;
            let ts = event.created_at.as_secs();
            if ts > entry.1 {
                entry.1 = ts;
            }
        }
    }

    let mut topics: Vec<TopicInfo> = topic_counts
        .into_iter()
        .map(|(name, (count, latest))| TopicInfo {
            name,
            post_count: count,
            latest_post_at: Some(latest),
        })
        .collect();

    // Sort by post count descending
    topics.sort_by_key(|b| std::cmp::Reverse(b.post_count));
    topics.truncate(limit);

    // Cache discovered topics
    let mut cache = DISCOVERED_TOPICS.write();
    for info in &topics {
        cache.put(info.name.clone(), info.clone());
    }

    log::info!("Discovered {} topics", topics.len());
    Ok(topics)
}
