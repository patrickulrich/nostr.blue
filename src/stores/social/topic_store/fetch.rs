use super::*;
use std::time::Duration;

pub async fn fetch_topic_metadata(topic: &str) -> Option<TopicMetadata> {
    if let Some(cached) = TOPIC_METADATA_CACHE.read().peek(topic).cloned() {
        return Some(cached);
    }

    let d_tag = topic_metadata_d_tag(topic);
    let filter = Filter::new()
        .kind(Kind::ApplicationSpecificData)
        .identifier(&d_tag)
        .limit(10);

    let result =
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(10)).await;

    match result {
        Ok(events) => {
            let nostr_events: Vec<NostrEvent> = events;
            let meta = parse_topic_metadata(&nostr_events, topic);
            if let Some(ref m) = meta {
                TOPIC_METADATA_CACHE
                    .write()
                    .put(topic.to_string(), m.clone());
            }
            meta
        }
        Err(e) => {
            log::warn!("Failed to fetch topic metadata for {}: {}", topic, e);
            None
        }
    }
}

pub async fn fetch_topic_pins(topic: &str, creator_pubkey: &str) -> Vec<String> {
    if let Some(cached) = TOPIC_PINS_CACHE.read().peek(topic).cloned() {
        return cached;
    }

    let d_tag = topic_pins_d_tag(topic);
    let pk = match PublicKey::from_hex(creator_pubkey) {
        Ok(pk) => pk,
        Err(_) => return Vec::new(),
    };
    let filter = Filter::new()
        .kind(Kind::ApplicationSpecificData)
        .identifier(&d_tag)
        .author(pk)
        .limit(1);

    let result =
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(10)).await;

    match result {
        Ok(events) => {
            let pins: Vec<String> = events
                .first()
                .map(|e| {
                    e.tags
                        .iter()
                        .filter(|t| t.kind() == TagKind::e())
                        .filter_map(|t| t.content().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            TOPIC_PINS_CACHE
                .write()
                .put(topic.to_string(), pins.clone());
            pins
        }
        Err(_) => Vec::new(),
    }
}

/// Search topic posts (NIP-50 relay search + client-side fallback)
pub async fn search_topic_posts(
    query: &str,
    topic: Option<&str>,
    limit: usize,
) -> std::result::Result<(Vec<TopicPost>, SearchMode), String> {
    if query.trim().is_empty() {
        return Ok((Vec::new(), SearchMode::Local));
    }

    let mut filter = Filter::new()
        .kind(Kind::Comment)
        .search(query)
        .custom_tag(SingleLetterTag::uppercase(Alphabet::K), "#".to_string())
        .limit(limit);

    if let Some(t) = topic {
        let hashtag = format!("#{}", t);
        filter = filter.custom_tags(SingleLetterTag::uppercase(Alphabet::I), [hashtag]);
    }

    let result =
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(8)).await;

    let mut mode = SearchMode::Relay;
    let mut events = match result {
        Ok(e) => e,
        Err(_) => {
            mode = SearchMode::Local;
            let fallback_filter = recent_topic_posts_filter(500, None, None);
            crate::stores::nostr_client::fetch_topic_events(fallback_filter, Duration::from_secs(10))
                .await
                .unwrap_or_default()
        }
    };

    if mode == SearchMode::Local {
        let query_lower = query.to_lowercase();
        let terms: Vec<&str> = query_lower.split_whitespace().collect();
        events.retain(|e| {
            let content_lower = e.content.to_lowercase();
            terms.iter().all(|t| content_lower.contains(t))
        });
        events.truncate(limit);
    }

    let mut posts: Vec<TopicPost> = events.iter().filter_map(parse_topic_post).collect();
    posts.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    cache_topic_posts(&posts);

    Ok((posts, mode))
}

/// Fetch posts for a specific topic
pub async fn fetch_topic_posts(
    topic: &str,
    limit: usize,
    until: Option<u64>,
) -> std::result::Result<Vec<TopicPost>, String> {
    *LOADING_TOPIC_POSTS.write() = true;
    let filter = topic_posts_filter(topic, limit, until);
    let result =
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(15)).await;
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
    let filter = recent_topic_posts_filter(limit, until, None);

    let result =
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(15)).await;
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

    let topic_hashtags: Vec<String> = topics.iter().map(|t| format!("#{}", t)).collect();
    let mut filter = Filter::new()
        .kind(Kind::Comment)
        .custom_tags(SingleLetterTag::uppercase(Alphabet::I), topic_hashtags)
        .custom_tag(SingleLetterTag::uppercase(Alphabet::K), "#".to_string())
        .limit(limit);
    if let Some(ts) = until {
        filter = filter.until(Timestamp::from(ts));
    }

    let result =
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(15)).await;
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
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(10))
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
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(10))
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
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(10))
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
    let filter = recent_topic_posts_filter(500, None, None);
    let events =
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(15))
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

pub async fn query_topic_posts_from_db(filter: Filter) -> Vec<TopicPost> {
    let Some(client) = crate::stores::nostr_client::fetching::get_client() else {
        return Vec::new();
    };
    match client.database().query(filter).await {
        Ok(events) => {
            let event_vec: Vec<NostrEvent> = events.into_iter().collect();
            let mut posts: Vec<TopicPost> = event_vec.iter().filter_map(parse_topic_post).collect();
            posts.sort_by_key(|b| std::cmp::Reverse(b.created_at));
            log::info!("Topic DB-only: {} posts instantly", posts.len());
            posts
        }
        Err(e) => {
            log::warn!("Topic DB query failed: {}", e);
            Vec::new()
        }
    }
}

pub async fn query_votes_from_db(
    event_ids: Vec<EventId>,
    user_pubkey: Option<PublicKey>,
) -> HashMap<String, VoteCounts> {
    if event_ids.is_empty() {
        return HashMap::new();
    }
    let Some(client) = crate::stores::nostr_client::fetching::get_client() else {
        return HashMap::new();
    };
    let filter = votes_filter(event_ids.clone());
    let events = match client.database().query(filter).await {
        Ok(e) => e,
        Err(e) => {
            log::warn!("Topic votes DB query failed: {}", e);
            return HashMap::new();
        }
    };
    let event_vec: Vec<NostrEvent> = events.into_iter().collect();

    let mut vote_map: HashMap<String, HashMap<String, (VoteDirection, u64)>> = HashMap::new();
    for event in &event_vec {
        if let Some((target_id, direction)) = parse_vote(event) {
            let pubkey = event.pubkey.to_hex();
            let created_at = event.created_at.as_secs();
            let post_votes = vote_map.entry(target_id).or_default();
            let entry = post_votes.entry(pubkey).or_insert((direction, created_at));
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
        result.insert(id_hex, counts);
    }
    result
}

pub async fn query_discover_topics_from_db(limit: usize) -> Vec<TopicInfo> {
    let Some(client) = crate::stores::nostr_client::fetching::get_client() else {
        return Vec::new();
    };
    let filter = recent_topic_posts_filter(500, None, None);
    match client.database().query(filter).await {
        Ok(events) => {
            let event_vec: Vec<NostrEvent> = events.into_iter().collect();
            let mut topic_counts: HashMap<String, (usize, u64)> = HashMap::new();
            for event in &event_vec {
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
            topics.sort_by_key(|b| std::cmp::Reverse(b.post_count));
            topics.truncate(limit);
            log::info!("Topic DB-only: discovered {} topics instantly", topics.len());
            topics
        }
        Err(e) => {
            log::warn!("Topic discover DB query failed: {}", e);
            Vec::new()
        }
    }
}

pub async fn discover_unsubscribed_topics(
    limit: usize,
) -> std::result::Result<Vec<DiscoverTopic>, String> {
    let subscribed: std::collections::HashSet<String> =
        get_subscribed_topic_names().into_iter().collect();

    let since = Timestamp::now().as_secs().saturating_sub(7 * 86400);
    let filter = recent_topic_posts_filter(500, None, Some(since));
    let events =
        crate::stores::nostr_client::fetch_topic_events(filter, Duration::from_secs(15))
            .await?;

    let mut topic_data: HashMap<String, (usize, u64, Option<String>, Option<String>)> =
        HashMap::new();
    for event in &events {
        if let Some(topic_name) = extract_topic_name(event) {
            if subscribed.contains(&topic_name) {
                continue;
            }
            let is_root = !event
                .tags
                .iter()
                .any(|t| t.kind() == TagKind::SingleLetter(SingleLetterTag::lowercase(Alphabet::E)));
            if !is_root {
                continue;
            }
            let entry = topic_data
                .entry(topic_name)
                .or_insert((0, 0, None, None));
            entry.0 += 1;
            let ts = event.created_at.as_secs();
            if ts > entry.1 {
                entry.1 = ts;
                let content = event.content.trim().to_string();
                let preview = if content.len() > 120 {
                    let mut end = 120;
                    while !content.is_char_boundary(end) && end > 0 {
                        end -= 1;
                    }
                    format!("{}...", &content[..end])
                } else {
                    content
                };
                entry.2 = Some(preview);
                entry.3 = Some(event.pubkey.to_hex());
            }
        }
    }

    let mut topics: Vec<DiscoverTopic> = topic_data
        .into_iter()
        .map(|(name, (count, latest, preview, author))| DiscoverTopic {
            info: TopicInfo {
                name,
                post_count: count,
                latest_post_at: Some(latest),
            },
            preview_content: preview,
            preview_author: author,
        })
        .collect();

    topics.sort_by_key(|b| std::cmp::Reverse(b.info.post_count));
    topics.truncate(limit);
    log::info!("Discovered {} unsubscribed topics", topics.len());
    Ok(topics)
}
