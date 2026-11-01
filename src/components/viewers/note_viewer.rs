use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use nostr::event::Event as NostrEvent;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::components::{ClientInitializing, EditProposalCard, NoteCard, ThreadedComment, VoiceMessageCard};
use crate::hooks::{use_mute_block_cache, use_relay_subscription};
use crate::routes::Route;
use crate::services::aggregation::{
    fetch_interaction_counts_batch_partitioned, fetch_local_db_counts,
    invalidate_interaction_counts_batch, merge_counts_batch_into, stream_interaction_counts_since,
    InteractionCounts, InteractionStreamHandle, StreamDeltaRecorder,
};
use crate::stores::back_navigation;
use crate::stores::nostr_client;
use crate::stores::nostr_client::fetching::{fetch_event_targeted, parse_event_id};
use crate::stores::relay;
use crate::stores::relay::coverage::RelayPurpose;
use crate::utils::{
    build_thread_tree, event::is_voice_message, filter_replies_to_descendants,
    resolve_thread_root_id, ThreadNode,
};

async fn fetch_main_note(note_id: &str) -> std::result::Result<NostrEvent, String> {
    let parsed = parse_event_id(note_id).ok_or("Invalid note ID")?;
    fetch_event_targeted(parsed, Duration::from_secs(12))
        .await?
        .ok_or("Event not found".to_string())
}

fn extract_relay_hints(note: &NostrEvent) -> Vec<(EventId, Option<String>)> {
    note.tags
        .iter()
        .filter_map(|tag| {
            if let Ok(Nip10Tag::Event { id, relay_hint, .. }) = Nip10Tag::try_from(tag) {
                let url = relay_hint.as_ref().map(|u| u.to_string());
                Some((id, url))
            } else {
                None
            }
        })
        .collect()
}

fn extract_author_from_etags(note: &NostrEvent) -> HashMap<EventId, PublicKey> {
    let mut author_map = HashMap::new();
    for tag in note.tags.iter() {
        if let Ok(Nip10Tag::Event {
            id,
            public_key: Some(pk),
            ..
        }) = Nip10Tag::try_from(tag)
        {
            author_map.insert(id, pk);
        }
    }
    author_map
}

struct ParentFetchResult {
    parents: Vec<NostrEvent>,
    missing_ids: HashSet<EventId>,
}

async fn fetch_parents_with_hints(
    initial_ids: Vec<EventId>,
    clicked_note: &NostrEvent,
    max_depth: usize,
) -> std::result::Result<ParentFetchResult, String> {
    let mut all_parents = Vec::new();
    let mut fetched_ids: HashSet<EventId> = HashSet::new();
    fetched_ids.insert(clicked_note.id);

    let hints = extract_relay_hints(clicked_note);
    let mut hint_map: HashMap<EventId, String> = hints
        .into_iter()
        .filter_map(|(id, url)| url.map(|u| (id, u)))
        .collect();

    let clicked_author_hints = extract_author_from_etags(clicked_note);

    let mut ids_to_fetch: Vec<EventId> = initial_ids
        .into_iter()
        .filter(|id| !fetched_ids.contains(id))
        .collect();

    let client = nostr_client::get_client().unwrap();

    for _ in 0..max_depth {
        if ids_to_fetch.is_empty() {
            break;
        }

        let mut new_events = Vec::new();

        // DB-first drain: pull cached ancestors before hitting relays
        if !ids_to_fetch.is_empty() {
            let db_filter = Filter::new()
                .ids(ids_to_fetch.clone())
                .kinds(vec![
                    Kind::TextNote,
                    Kind::VoiceMessage,
                    Kind::VoiceMessageReply,
                    Kind::Comment,
                ]);
            if let Ok(db_events) = client.database().query(db_filter).await {
                for e in db_events {
                    fetched_ids.insert(e.id);
                    new_events.push(e);
                }
                ids_to_fetch.retain(|id| !fetched_ids.contains(id));
            }
        }
        // Native nostrdb bridge (direct per-id lookup, bypasses SDK filter translation)
        #[cfg(feature = "native")]
        if !ids_to_fetch.is_empty() {
            let mut still_missing = Vec::new();
            for id in &ids_to_fetch {
                if let Some(event) = crate::stores::ndb::get_cached_event(&id.to_bytes()) {
                    fetched_ids.insert(event.id);
                    new_events.push(event);
                } else {
                    still_missing.push(*id);
                }
            }
            ids_to_fetch = still_missing;
        }

        let mut hinted_grouped: HashMap<String, Vec<EventId>> = HashMap::new();
        let mut unhinted_ids = Vec::new();

        for id in &ids_to_fetch {
            if let Some(url) = hint_map.get(id) {
                hinted_grouped
                    .entry(url.clone())
                    .or_default()
                    .push(*id);
            } else {
                unhinted_ids.push(*id);
            }
        }

        if !hinted_grouped.is_empty() {
            let hint_urls: Vec<String> = hinted_grouped.keys().cloned().collect();
            let ephemeral =
                relay::coverage::connect_ephemeral_relays(&client, &hint_urls).await;
            if !ephemeral.connected.is_empty() {
                for (relay_url, ids) in &hinted_grouped {
                    if !ephemeral.connected.contains(relay_url) {
                        continue;
                    }
                    let filter = Filter::new().ids(ids.clone()).kinds(vec![
                        Kind::TextNote,
                        Kind::VoiceMessage,
                        Kind::VoiceMessageReply,
                        Kind::Comment,
                    ]);
                    match relay::connection::fetch_events_from_relays(
                        &client,
                        filter,
                        vec![relay_url.clone()],
                        Duration::from_secs(5),
                    )
                    .await
                    {
                        Ok(events) => new_events.extend(events),
                        Err(e) => log::warn!("Hinted relay fetch failed for {}: {}", relay_url, e),
                    }
                }
            }
            relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;

            let fetched_ids_this_round: HashSet<EventId> =
                new_events.iter().map(|e| e.id).collect();
            for ids in hinted_grouped.values() {
                for id in ids {
                    if !fetched_ids_this_round.contains(id) {
                        unhinted_ids.push(*id);
                    }
                }
            }
        }

        if !unhinted_ids.is_empty() {
            let mut remaining_unhinted = Vec::new();
            let mut author_targeted: HashMap<String, Vec<EventId>> = HashMap::new();

            for id in &unhinted_ids {
                if let Some(author) = clicked_author_hints.get(id) {
                    let relay_urls = relay::coverage::resolve_user_relays(
                        &author.to_hex(),
                        RelayPurpose::Write,
                    )
                    .await;
                    if !relay_urls.is_empty() {
                        for url in &relay_urls {
                            author_targeted
                                .entry(url.clone())
                                .or_default()
                                .push(*id);
                        }
                        continue;
                    }
                }
                remaining_unhinted.push(*id);
            }

            if !author_targeted.is_empty() {
                let author_urls: Vec<String> = author_targeted.keys().cloned().collect();
                let ephemeral =
                    relay::coverage::connect_ephemeral_relays(&client, &author_urls).await;
                if !ephemeral.connected.is_empty() {
                    for (relay_url, ids) in &author_targeted {
                        if !ephemeral.connected.contains(relay_url) {
                            continue;
                        }
                        let filter = Filter::new().ids(ids.clone()).kinds(vec![
                            Kind::TextNote,
                            Kind::VoiceMessage,
                            Kind::VoiceMessageReply,
                            Kind::Comment,
                        ]);
                        match relay::connection::fetch_events_from_relays(
                            &client,
                            filter,
                            vec![relay_url.clone()],
                            Duration::from_secs(5),
                        )
                        .await
                        {
                            Ok(events) => new_events.extend(events),
                            Err(e) => {
                                log::warn!("Author relay fetch failed for {}: {}", relay_url, e)
                            }
                        }
                    }
                }
                relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
            }

            let still_missing: Vec<EventId> = remaining_unhinted
                .iter()
                .filter(|id| !new_events.iter().any(|e| e.id == **id))
                .copied()
                .collect();

            if !still_missing.is_empty() {
                let clicked_relays = relay::coverage::resolve_user_relays(
                    &clicked_note.pubkey.to_hex(),
                    RelayPurpose::Write,
                )
                .await;
                if !clicked_relays.is_empty() {
                    let ephemeral =
                        relay::coverage::connect_ephemeral_relays(&client, &clicked_relays).await;
                    if !ephemeral.connected.is_empty() {
                        let filter = Filter::new().ids(still_missing.clone()).kinds(vec![
                            Kind::TextNote,
                            Kind::VoiceMessage,
                            Kind::VoiceMessageReply,
                            Kind::Comment,
                        ]);
                        match relay::connection::fetch_events_from_relays(
                            &client,
                            filter,
                            ephemeral.connected.clone(),
                            Duration::from_secs(5),
                        )
                        .await
                        {
                            Ok(events) => new_events.extend(events),
                            Err(e) => {
                                log::warn!("Clicked-author relay fetch failed: {}", e)
                            }
                        }
                    }
                    relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added)
                        .await;
                }
            }

            let final_missing: Vec<EventId> = remaining_unhinted
                .iter()
                .filter(|id| !new_events.iter().any(|e| e.id == **id))
                .copied()
                .collect();

            if !final_missing.is_empty() {
                let filter = Filter::new().ids(final_missing).kinds(vec![
                    Kind::TextNote,
                    Kind::VoiceMessage,
                    Kind::VoiceMessageReply,
                    Kind::Comment,
                ]);
                if let Ok(events) =
                    nostr_client::fetch_events_aggregated_outbox(filter, Duration::from_secs(10))
                        .await
                {
                    new_events.extend(events);
                }
            }
        }

        for id in &ids_to_fetch {
            fetched_ids.insert(*id);
        }

        for event in &new_events {
            relay::coverage::record_relay_list_from_event(event);
        }

        for parent in &new_events {
            for (id, url_opt) in extract_relay_hints(parent) {
                if let Some(url) = url_opt {
                    hint_map.entry(id).or_insert(url);
                }
            }
        }

        ids_to_fetch = new_events
            .iter()
            .filter_map(crate::utils::thread_tree::get_parent_id)
            .filter(|id| !fetched_ids.contains(id))
            .collect();

        all_parents.extend(new_events);
    }

    let missing_ids: HashSet<EventId> = ids_to_fetch
        .into_iter()
        .filter(|id| !fetched_ids.contains(id))
        .collect();

    Ok(ParentFetchResult {
        parents: dedup_replies(all_parents),
        missing_ids,
    })
}

fn dedup_replies(all_replies: Vec<NostrEvent>) -> Vec<NostrEvent> {
    let mut seen_ids = std::collections::HashSet::new();
    all_replies
        .into_iter()
        .filter(|event| seen_ids.insert(event.id))
        .collect()
}

async fn fetch_author_relays_replies(
    event_id: EventId,
    root_author_pubkey: &PublicKey,
) -> Vec<NostrEvent> {
    let author_relays =
        crate::stores::relay::coverage::get_relays_for_pubkey(&root_author_pubkey.to_hex());
    if author_relays.is_empty() {
        return Vec::new();
    }
    let reply_filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Comment])
        .event(event_id)
        .limit(100);
    let Some(client) = nostr_client::get_client() else {
        return Vec::new();
    };
    let ephemeral = relay::coverage::connect_ephemeral_relays(&client, &author_relays).await;
    if ephemeral.connected.is_empty() {
        return Vec::new();
    }
    let result = relay::connection::fetch_events_from_relays(
        &client,
        reply_filter,
        ephemeral.connected.clone(),
        Duration::from_secs(5),
    )
    .await
    .unwrap_or_default();
    relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
    result
}

async fn fetch_replies_from_root_inbox(
    root_event_id: EventId,
    root_author: &PublicKey,
) -> Vec<NostrEvent> {
    let kinds = vec![
        Kind::TextNote,
        Kind::Comment,
        Kind::VoiceMessage,
        Kind::VoiceMessageReply,
        Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT),
        Kind::EventDeletion,
    ];
    let Some(client) = nostr_client::get_client() else {
        return Vec::new();
    };

    // 1. DB-first: surface cached root replies instantly
    let db_filter = Filter::new()
        .kinds(kinds.clone())
        .event(root_event_id)
        .limit(500);
    let mut result: Vec<NostrEvent> = client
        .database()
        .query(db_filter)
        .await
        .map(|events| events.into_iter().collect())
        .unwrap_or_default();

    // 2. Native nostrdb bridge (direct query, bypasses SDK filter translation)
    #[cfg(feature = "native")]
    {
        let bridge = crate::stores::ndb::get_cached_replies(&root_event_id, &kinds);
        result.extend(bridge);
    }

    // 3. Network: root author's NIP-65 read relays (freshness + completeness)
    let read_relays = crate::stores::relay::coverage::resolve_user_relays(
        &root_author.to_hex(),
        crate::stores::relay::coverage::RelayPurpose::Read,
    )
    .await;
    if !read_relays.is_empty() {
        let net_filter = Filter::new()
            .kinds(kinds)
            .event(root_event_id)
            .limit(500);
        let net = relay::connection::fetch_events_from_relays(
            &client,
            net_filter,
            read_relays,
            Duration::from_secs(5),
        )
        .await
        .unwrap_or_default();
        result.extend(net);
    }

    // 4. Dedup + client-side guard (drop events not referencing the root)
    let root_hex = root_event_id.to_hex();
    dedup_replies(result)
        .into_iter()
        .filter(|e| {
            e.id == root_event_id
                || e.tags.iter().any(|tag| {
                    let slice = tag.as_slice();
                    slice.first().map(|s| s.as_str()) == Some("e")
                        && slice.get(1).map(|s| s.as_str()) == Some(root_hex.as_str())
                })
        })
        .collect()
}

fn merge_new_replies(
    new_events: Vec<NostrEvent>,
    mut replies: Signal<Vec<NostrEvent>>,
    mut reply_ids: Signal<HashSet<EventId>>,
    root_event_id: EventId,
) {
    let mut added = false;
    for event in new_events {
        if reply_ids.write().insert(event.id) {
            replies.write().push(event);
            added = true;
        }
    }
    if added {
        replies.write().sort_by_key(|a| a.created_at);
        crate::utils::thread_tree::invalidate_thread_tree_cache(&root_event_id);
    }
}

async fn fetch_parents_db(parent_ids: &[EventId]) -> Vec<NostrEvent> {
    if parent_ids.is_empty() {
        return Vec::new();
    }

    let Some(client) = nostr_client::get_client() else {
        return Vec::new();
    };

    let filter = Filter::new()
        .ids(parent_ids.iter().copied())
        .kinds(vec![
            Kind::TextNote,
            Kind::VoiceMessage,
            Kind::VoiceMessageReply,
            Kind::Comment,
        ]);

    let mut db_parents = Vec::new();

    if let Ok(events) = client.database().query(filter).await {
        db_parents.extend(events);
    }

    #[cfg(feature = "native")]
    {
        let found_ids: HashSet<EventId> = db_parents.iter().map(|e| e.id).collect();
        for id in parent_ids {
            if !found_ids.contains(id) {
                if let Some(event) = crate::stores::ndb::get_cached_event(&id.to_bytes()) {
                    db_parents.push(event);
                }
            }
        }
    }

    dedup_replies(db_parents)
}

async fn fetch_replies_db(
    event_id: EventId,
) -> std::result::Result<Vec<NostrEvent>, String> {
    let Some(client) = nostr_client::get_client() else {
        return Err("Client not initialized".to_string());
    };
    let event_id_hex = event_id.to_hex();

    let filter_lower = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Comment,
            Kind::VoiceMessage,
            Kind::VoiceMessageReply,
            Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT),
        ])
        .event(event_id)
        .limit(100);

    let upper_e_tag = SingleLetterTag::from_char('E').unwrap();
    let filter_upper = Filter::new()
        .kinds(vec![
            Kind::VoiceMessage,
            Kind::VoiceMessageReply,
            Kind::Comment,
        ])
        .custom_tag(upper_e_tag, event_id_hex)
        .limit(100);

    let (lower_db, upper_db) = tokio::join!(
        client.database().query(filter_lower),
        client.database().query(filter_upper),
    );

    let mut db_replies = Vec::new();
    if let Ok(events) = lower_db {
        db_replies.extend(events);
    }
    if let Ok(events) = upper_db {
        db_replies.extend(events);
    }

    #[cfg(feature = "native")]
    {
        let reply_kinds = vec![
            Kind::TextNote,
            Kind::VoiceMessage,
            Kind::VoiceMessageReply,
            Kind::Comment,
            Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT),
        ];
        let bridge_replies = crate::stores::ndb::get_cached_replies(&event_id, &reply_kinds);
        log::debug!("fetch_replies_db: bridge cache found {} replies for {:?}", bridge_replies.len(), event_id.to_hex());
        db_replies.extend(bridge_replies);
    }

    Ok(dedup_replies(db_replies))
}

async fn fetch_replies_from_relays(
    event_id: EventId,
    root_author_pubkey: Option<PublicKey>,
) -> std::result::Result<Vec<NostrEvent>, String> {
    let root_id = match resolve_root_from_self_or_tags(event_id).await {
        Some(id) => id,
        None => event_id,
    };
    fetch_replies_bfs(root_id, root_author_pubkey, 3).await
}

async fn resolve_root_from_self_or_tags(event_id: EventId) -> Option<EventId> {
    let client = nostr_client::get_client()?;
    if let Ok(Some(event)) = client.database().event_by_id(&event_id).await {
        let root = resolve_thread_root_id(&event);
        if root.is_some() {
            return root;
        }
    }
    None
}

async fn fetch_replies_bfs(
    root_event_id: EventId,
    root_author_pubkey: Option<PublicKey>,
    max_rounds: usize,
) -> std::result::Result<Vec<NostrEvent>, String> {
    let mut all_replies = Vec::new();
    let mut seen: HashSet<EventId> = HashSet::new();
    seen.insert(root_event_id);
    let mut ids_to_query: Vec<EventId> = vec![root_event_id];

    let reply_kinds = vec![
        Kind::TextNote,
        Kind::Comment,
        Kind::VoiceMessage,
        Kind::VoiceMessageReply,
        Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT),
        Kind::EventDeletion,
    ];
    let comment_kinds = vec![
        Kind::VoiceMessage,
        Kind::VoiceMessageReply,
        Kind::Comment,
    ];
    let upper_e_tag = SingleLetterTag::from_char('E').unwrap();

    for round in 0..max_rounds {
        if ids_to_query.is_empty() {
            break;
        }
        log::info!(
            "BFS round {}: querying {} parent IDs",
            round + 1,
            ids_to_query.len()
        );

        let ids_hex: Vec<String> = ids_to_query.iter().map(|id| id.to_hex()).collect();

        // 500/round (Amethyst parity: their engagement REQs use limit 1000
        // per relay): 200/round truncated busy threads to a fraction of
        // the badge's claimed reply count.
        let filter_lower = Filter::new()
            .kinds(reply_kinds.clone())
            .events(ids_to_query.clone())
            .limit(500);

        let filter_upper = Filter::new()
            .kinds(comment_kinds.clone())
            .custom_tags(upper_e_tag, ids_hex.clone())
            .limit(500);

        let (lower_result, upper_result, author_result) = tokio::join!(
            nostr_client::fetch_events_from_connected_relays(
                filter_lower,
                Duration::from_secs(5),
            ),
            nostr_client::fetch_events_from_connected_relays(
                filter_upper,
                Duration::from_secs(5),
            ),
            async {
                match root_author_pubkey {
                    Some(pk) if round == 0 => {
                        fetch_author_relays_replies(root_event_id, &pk).await
                    }
                    _ => Vec::new(),
                }
            }
        );

        let mut round_new = Vec::new();
        if let Ok(replies) = lower_result {
            round_new.extend(replies);
        }
        if let Ok(replies) = upper_result {
            round_new.extend(replies);
        }
        round_new.extend(author_result);

        ids_to_query = Vec::new();
        for event in round_new {
            if seen.insert(event.id) {
                ids_to_query.push(event.id);
                all_replies.push(event);
            }
        }

        log::info!(
            "BFS round {}: found {} new replies (total {})",
            round + 1,
            ids_to_query.len(),
            all_replies.len()
        );
    }

    Ok(dedup_replies(all_replies))
}

async fn fetch_replies_phase2(
    event_id: EventId,
    reply_authors: HashSet<PublicKey>,
    replies_signal: Signal<Vec<NostrEvent>>,
    reply_ids_signal: Signal<HashSet<EventId>>,
    load_generation: Signal<u32>,
    this_generation: u32,
) {
    if reply_authors.is_empty() {
        return;
    }
    let Some(client) = nostr_client::get_client() else {
        return;
    };
    let pubkeys: Vec<PublicKey> = reply_authors.into_iter().collect();

    let filter = Filter::new()
        .kind(Kind::RelayList)
        .authors(pubkeys.clone());
    let Ok(events) = client.database().query(filter).await else {
        return;
    };

    // Keep only the latest relay list per author (kind 10002 is replaceable)
    let mut latest_list: HashMap<PublicKey, &nostr::event::Event> = HashMap::new();
    for event in &events {
        match latest_list.entry(event.pubkey) {
            std::collections::hash_map::Entry::Occupied(mut occupied) => {
                if occupied.get().created_at < event.created_at {
                    occupied.insert(event);
                }
            }
            std::collections::hash_map::Entry::Vacant(vacant) => {
                vacant.insert(event);
            }
        }
    }

    let mut combined_relays: Vec<String> = Vec::new();
    let mut authors_with_relays = HashSet::new();

    for (pk, event) in &latest_list {
        let relays_map: HashMap<RelayUrl, Option<RelayMetadata>> =
            nostr::nips::nip65::extract_relay_list(event).collect();
        if !relays_map.is_empty() {
            let urls: Vec<String> = relays_map
                .iter()
                .filter(|(_, m)| m.is_none() || matches!(m, Some(RelayMetadata::Write)))
                .map(|(u, _)| u.to_string())
                .collect();
            combined_relays.extend(urls);
            authors_with_relays.insert(*pk);
        }
        relay::coverage::record_relay_list_from_event_by_map(pk, &relays_map);
    }

    let missing: Vec<String> = pubkeys
        .iter()
        .filter(|p| !authors_with_relays.contains(p))
        .map(|p| p.to_hex())
        .collect();

    let mut seen = HashSet::new();
    combined_relays.retain(|r| seen.insert(r.clone()));
    combined_relays.truncate(MAX_EPHEMERAL_RELAYS);

    if !combined_relays.is_empty() {
        let ephemeral = relay::coverage::connect_ephemeral_relays(&client, &combined_relays).await;
        if !ephemeral.connected.is_empty() {
            let reply_filter = Filter::new()
                .kinds(vec![
                    Kind::TextNote,
                    Kind::Comment,
                    Kind::VoiceMessage,
                    Kind::VoiceMessageReply,
                ])
                .event(event_id)
                .limit(100);
            if let Ok(events) = relay::connection::fetch_events_from_relays(
                &client,
                reply_filter,
                ephemeral.connected.clone(),
                Duration::from_secs(5),
            )
            .await
            {
                if *load_generation.peek() != this_generation {
                    relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
                    return;
                }
                log::info!("Phase 2: merging {} additional replies", events.len());
                merge_new_replies(events, replies_signal, reply_ids_signal, event_id);
            }
            relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
        }
    }

    if !missing.is_empty() {
        dioxus::prelude::spawn(async move {
            for chunk in missing.chunks(50) {
                let chunk_pks: Vec<PublicKey> = chunk
                    .iter()
                    .filter_map(|p| PublicKey::from_hex(p).ok())
                    .collect();
                if chunk_pks.is_empty() {
                    continue;
                }
                let filter = Filter::new()
                    .authors(chunk_pks)
                    .kind(Kind::RelayList);
                if let Some(c) = nostr_client::get_client() {
                    if let Ok(events) = nostr_client::fetch_with_client(
                        &c,
                        nostr_client::FetchTarget::Auto(vec![filter]),
                        nostr_client::FetchProfile::Ui,
                    )
                    .await
                    {
                        for event in events {
                            relay::coverage::record_relay_list_from_event(&event);
                        }
                    }
                }
            }
        });
    }
}

const MAX_EPHEMERAL_RELAYS: usize = 5;

/// Cap on how many thread authors' inbox relays the interaction sweep
/// resolves — each cache miss costs a kind 10002 lookup (memory → DB →
/// indexers), so the author list is bounded to keep the sweep's worst-case
/// latency bounded.
const THREAD_COUNT_AUTHOR_CAP: usize = 10;

/// Concurrent resolution width + overall deadline for the sweep authors'
/// kind-10002 lists (R3 review): resolving serially awaited each author's
/// tier-3 indexer fetch (5s each) back-to-back — ~8 uncached authors ≈ 40s
/// before the sweep even connected, blocking the counts paint. Amethyst's
/// equivalent (`User.inboxRelays()`) is a pure cache read and never blocks
/// thread-open on network resolution.
const SWEEP_RESOLVE_CONCURRENCY: usize = 4;
const SWEEP_RESOLVE_DEADLINE: Duration = Duration::from_secs(8);

/// R3 review: back-and-forth thread↔feed navigation re-ran the full
/// author-inbox sweep (≤5 ephemeral connects + two limit-500 REQs) and
/// unconditionally popped the L2 counts cache on EVERY mount. Sweep
/// results are persisted to the shared SDK database (the relay layer
/// saves every fetched event before emitting it), so a re-open within
/// the TTL only needs `fetch_local_db_counts` + the live stream + the
/// cache-served batch.
static RECENT_THREAD_SWEEPS: std::sync::OnceLock<
    std::sync::Mutex<HashMap<EventId, instant::Instant>>,
> = std::sync::OnceLock::new();
const THREAD_SWEEP_TTL_SECS: u64 = 90;
const THREAD_SWEEP_MAP_CAP: usize = 64;

/// Returns `true` when this thread root was swept within the TTL and the
/// sweep (plus its L2 pop) should be skipped. Marks the root as swept
/// when returning `false`, committing the caller to sweeping now.
fn thread_sweep_cooldown_active(root: EventId) -> bool {
    let map = RECENT_THREAD_SWEEPS.get_or_init(Default::default);
    let mut guard = map.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = instant::Instant::now();
    if guard.len() >= THREAD_SWEEP_MAP_CAP {
        guard.retain(|_, swept_at| now.duration_since(*swept_at).as_secs() < THREAD_SWEEP_TTL_SECS);
    }
    match guard.get(&root) {
        Some(swept_at) if now.duration_since(*swept_at).as_secs() < THREAD_SWEEP_TTL_SECS => true,
        _ => {
            guard.insert(root, now);
            false
        }
    }
}

/// Resolve the authors' read relays concurrently under one overall
/// deadline, preserving partial results when the deadline hits.
async fn resolve_author_read_relays_concurrently(authors: &[PublicKey]) -> Vec<String> {
    use futures::StreamExt as _;

    // `buffer_unordered` keeps at most SWEEP_RESOLVE_CONCURRENCY resolves
    // in flight; `.next()` yields each author's result as it completes so
    // the deadline below keeps whatever resolved before it fired.
    let mut pending = futures::stream::iter(authors.iter().map(|author| {
        let hex = author.to_hex();
        async move { relay::coverage::resolve_user_relays(&hex, RelayPurpose::Read).await }
    }))
    .buffer_unordered(SWEEP_RESOLVE_CONCURRENCY);
    let deadline = instant::Instant::now() + SWEEP_RESOLVE_DEADLINE;
    let mut urls: Vec<String> = Vec::new();
    let mut authors_left = authors.len();
    while authors_left > 0 {
        let time_left = deadline.saturating_duration_since(instant::Instant::now());
        if time_left.is_zero() {
            log::warn!(
                "Thread sweep: relay resolution deadline hit with {} authors unresolved; \
                 proceeding with warm coverage + the connected pool",
                authors_left
            );
            break;
        }
        let timer = crate::platform::timer::sleep_ms(
            u32::try_from(time_left.as_millis()).unwrap_or(u32::MAX),
        );
        futures::pin_mut!(timer);
        match futures::future::select(pending.next(), timer).await {
            futures::future::Either::Left((Some(relays), _)) => {
                authors_left -= 1;
                for url in relays {
                    if !urls.contains(&url) {
                        urls.push(url);
                    }
                }
            }
            futures::future::Either::Left((None, _)) => break,
            futures::future::Either::Right((_, _)) => {
                log::warn!(
                    "Thread sweep: relay resolution deadline hit with {} authors unresolved; \
                     proceeding with warm coverage + the connected pool",
                    authors_left
                );
                break;
            }
        }
    }
    urls
}

/// Authors whose inbox relays the thread interaction sweep should ask, in
/// priority order: parent-chain authors first (`parent_events` is sorted
/// ascending by `created_at`, so the thread root leads — its relays hold
/// the thread's reactions), then the clicked note's author (first when the
/// clicked note IS the root), then reply authors. Deduped and capped.
fn collect_thread_count_authors(
    clicked: Option<&NostrEvent>,
    parents: &[NostrEvent],
    replies: &[NostrEvent],
    cap: usize,
) -> Vec<PublicKey> {
    let mut seen = HashSet::new();
    let mut authors = Vec::new();
    for event in parents.iter().chain(clicked).chain(replies) {
        if seen.insert(event.pubkey) {
            authors.push(event.pubkey);
        }
    }
    authors.truncate(cap);
    authors
}

/// Sweep the relays where a thread's interactions actually land: each
/// thread author's NIP-65 read/inbox relays (NIP-25/57 outbox — reactors
/// and zappers publish to the reacted-to author's read relays), plus
/// coverage-ranked seen-on relays for the same authors. This mirrors
/// Amethyst's `relayUrlsForReactions` (author inbox + seen-on).
///
/// The count backfill's pool leg (3c-8) only reaches ≤3 relays already in
/// the connected READ|WRITE pool and structurally cannot see these
/// targets: relays connected for the reply fetches are GOSSIP-only
/// ephemeral members — invisible to `client.relays()` snapshots — and torn
/// down before the count fetch runs. Fetched events are saved into the
/// shared SDK database on receipt, where the count legs pick them up (the
/// caller pops the L2 counts cache so the batch re-reads it).
///
/// `backfill_until` bounds the sweep REQs strictly below the caller's
/// live-stream partition (R3 review): the stream's +1 increments and this
/// set-recount must observe disjoint event sets or an interaction landing
/// on both would be counted twice.
async fn fetch_thread_interactions_from_author_inbox(
    thread_event_ids: &[EventId],
    authors: &[PublicKey],
    backfill_until: Timestamp,
) {
    const SWEEP_FETCH_LIMIT: usize = 500;
    if thread_event_ids.is_empty() || authors.is_empty() {
        return;
    }
    let Some(client) = nostr_client::get_client() else {
        return;
    };

    let mut urls: Vec<String> = resolve_author_read_relays_concurrently(authors).await;
    // Fill remaining slots with coverage-ranked seen-on/NIP-65 relays for
    // the same authors (no extra network lookups — reads the live map).
    for url in relay::coverage::top_relays_by_author_coverage(
        authors,
        MAX_EPHEMERAL_RELAYS.saturating_mul(2),
    ) {
        if !urls.contains(&url) {
            urls.push(url);
        }
    }
    // Indexers are DISCOVERY-only profile directories — an ephemeral
    // connection there cannot serve interaction events.
    urls.retain(|url| {
        !relay::nip65::DEFAULT_INDEXER_RELAYS
            .iter()
            .any(|indexer| indexer.trim_end_matches('/') == url.trim_end_matches('/'))
    });
    urls.truncate(MAX_EPHEMERAL_RELAYS);

    let ephemeral = relay::coverage::connect_ephemeral_relays(&client, &urls).await;

    // Already-connected READ pool relays are free targets (no ephemeral
    // connect, one bounded REQ each) — Amethyst's engagement set is author
    // inbox ∪ seen-on ∪ the user's own relays, and reshares/reactions by
    // third parties propagate on the user's pool (that's how the feed
    // received the note). The batch's 3c-8 pool leg only reaches ≤3
    // coverage-ranked members, so fold the pool in here.
    let pool_urls: Vec<String> = {
        use nostr_sdk::relay::RelayStatus as PoolRelayStatus;
        client
            .relays()
            .await
            .iter()
            .filter(|(_, r)| r.status() == PoolRelayStatus::Connected && r.capabilities().can_read())
            .map(|(url, _)| url.to_string())
            .collect()
    };
    let mut targets = ephemeral.connected.clone();
    for url in pool_urls {
        if !targets.contains(&url) {
            targets.push(url);
        }
    }

    if !targets.is_empty() {
        let interaction_filter = Filter::new()
            .kinds(vec![
                Kind::Reaction,
                Kind::Repost,
                Kind::GenericRepost,
                Kind::ZapReceipt,
                Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT),
            ])
            .events(thread_event_ids.to_vec())
            .until(backfill_until)
            .limit(SWEEP_FETCH_LIMIT);
        // NIP-18 quote arm: `q`-tag quotes are reshares the `#e` filter can
        // never see (Amethyst parity: `FilterRepliesAndReactionsToNotes`
        // carries the same second filter).
        let quote_filter = Filter::new()
            .kinds(vec![Kind::TextNote, Kind::Comment])
            .custom_tags(
                SingleLetterTag::from_char('q').expect("valid single-letter tag"),
                thread_event_ids.iter().map(|id| id.to_hex()),
            )
            .until(backfill_until)
            .limit(SWEEP_FETCH_LIMIT);

        let mut fetched = 0usize;
        for filter in [interaction_filter, quote_filter] {
            match relay::connection::fetch_events_from_relays(
                &client,
                filter,
                targets.clone(),
                Duration::from_secs(5),
            )
            .await
            {
                Ok(events) => fetched += events.len(),
                Err(e) => log::warn!("Thread interaction sweep failed: {}", e),
            }
        }
        log::info!(
            "Thread interaction sweep: {} events from {} relays ({} ephemeral author-inbox)",
            fetched,
            targets.len(),
            ephemeral.connected.len()
        );
    }
    relay::coverage::cleanup_ephemeral_relays(&client, &ephemeral.newly_added).await;
}

async fn retry_missing_parents(
    missing_ids: HashSet<EventId>,
    _thread_root_id: EventId,
    load_generation: Signal<u32>,
    this_generation: u32,
    mut parent_events: Signal<Vec<NostrEvent>>,
) {
    let mut still_missing: Vec<EventId> = missing_ids.into_iter().collect();

    for attempt in 0..3 {
        if still_missing.is_empty() {
            break;
        }
        if *load_generation.peek() != this_generation {
            return;
        }

        crate::platform::timer::sleep_ms(2000).await;

        if *load_generation.peek() != this_generation {
            return;
        }

        log::info!(
            "Parent retry attempt {}: trying {} missing IDs",
            attempt + 1,
            still_missing.len()
        );

        let filter = Filter::new().ids(still_missing.clone()).kinds(vec![
            Kind::TextNote,
            Kind::VoiceMessage,
            Kind::VoiceMessageReply,
            Kind::Comment,
        ]);

        let found = match nostr_client::fetch_events_aggregated_outbox(
            filter,
            Duration::from_secs(10),
        )
        .await
        {
            Ok(events) => events,
            Err(_) => continue,
        };

        if found.is_empty() {
            continue;
        }

        let found_ids: HashSet<EventId> = found.iter().map(|e| e.id).collect();

        let mut current = parent_events.write();
        for event in &found {
            if !current.iter().any(|e| e.id == event.id) {
                current.push(event.clone());
            }
        }
        current.sort_by_key(|a| a.created_at);
        drop(current);

        still_missing.retain(|id| !found_ids.contains(id));

        log::info!(
            "Parent retry attempt {}: found {}, {} still missing",
            attempt + 1,
            found.len(),
            still_missing.len()
        );
    }
}

#[component]
pub fn NoteViewer(
    note_id: String,
    from_voice: Option<String>,
    #[props(default)] prefetched_event: Option<NostrEvent>,
) -> Element {
    let initial_is_voice = from_voice.as_ref().is_some_and(|v| v == "true");
    let mut note_data: Signal<Option<NostrEvent>> = use_signal(|| prefetched_event.clone());
    let mut parent_events = use_signal(Vec::<NostrEvent>::new);
    let mut replies = use_signal(Vec::<NostrEvent>::new);
    let mut loading = use_signal(|| true);
    let mut loading_replies = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut interaction_counts: Signal<HashMap<String, InteractionCounts>> =
        use_signal(HashMap::new);
    // R4 review: post-partition stream-delta ledger. The live stream
    // records every counter increment it applies for events created at/after
    // the partition `T`; when the counts batch's relay-truth rows later
    // REPLACE their painted rows (bounded `until = T-1`), the recorded
    // deltas are re-applied on top so streamed interactions survive the
    // replace instead of flickering away (see `merge_counts_batch_into`).
    // Non-reactive on purpose: nothing renders from it — it is written by
    // the stream task (an `Fn` recorder needs interior mutability) and
    // read once at batch-merge time.
    let stream_deltas: std::sync::Arc<
        std::sync::Mutex<HashMap<String, InteractionCounts>>,
    > = use_hook(|| std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())));
    let mut interaction_stream_handle: Signal<Option<InteractionStreamHandle>> =
        use_signal(|| None);
    let (cached_muted_posts, cached_blocked_users, cached_muted_words) = use_mute_block_cache();
    let mut load_generation = use_signal(|| 0u32);
    let mut reply_ids: Signal<HashSet<EventId>> = use_signal(HashSet::new);

    // Memoized thread tree. The closure reads `note_data` and `replies`
    // signals inside its body, so Dioxus auto-subscribes the memo to both.
    // Returns `Vec::new()` until the active note is loaded. The `PartialEq`
    // short-circuit in `Memo::recompute` skips re-renders when the tree
    // didn't change.
    //
    // `filter_replies_to_descendants` performs a BFS from the active note's
    // id over the reply graph and returns only events that are descendants
    // of the active note. This compensates for the BFS reply fetch (which
    // over-fetches the entire thread root subtree) and the streaming root
    // subscription (which streams events for the whole thread). The tree
    // is now scoped to the active note's branch only.
    let thread_tree_memo = use_memo(move || -> Vec<ThreadNode> {
        let event_id = {
            let guard = note_data.read();
            match guard.as_ref() {
                Some(e) => e.id,
                None => return Vec::new(),
            }
        };
        let reply_vec = replies.read().clone();
        let edit_kind = Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT);
        let filtered = filter_replies_to_descendants(reply_vec, event_id, &[edit_kind]);
        build_thread_tree(filtered, &event_id)
    });

    use_effect(use_reactive!(|note_id| {
        let note_id_str = note_id.clone();

        let this_generation = load_generation.peek().wrapping_add(1);
        load_generation.set(this_generation);

        if let Some(handle) = interaction_stream_handle.write().take() {
            spawn(async move {
                handle.unsubscribe().await;
            });
        }

        // Seed from the prefetched event (if any) instead of blanking to a
        // spinner. The background fetch below still runs to refresh/confirm,
        // but the user sees content immediately when AddressViewer already
        // resolved the event.
        let initial_event = prefetched_event.clone();
        note_data.set(initial_event.clone());
        replies.set(Vec::new());
        parent_events.set(Vec::new());
        interaction_counts.set(HashMap::new());
        // New epoch = new partition: deltas recorded by the previous
        // thread-open's stream are stale (the new batch re-observes
        // everything below the new T). The recorder's generation gate
        // keeps a dying old stream from repopulating this after the clear.
        stream_deltas.lock().unwrap().clear();
        reply_ids.set(HashSet::new());
        loading.set(initial_event.is_none());
        loading_replies.set(true);
        error.set(None);

        back_navigation::set_active_note_back_context(
            note_id_str.clone(),
            Vec::new(),
            initial_is_voice,
        );

        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            log::info!("Waiting for client initialization before loading note...");
            loading.set(false);
            loading_replies.set(false);
            return;
        }

        let event_id = match parse_event_id(&note_id_str) {
            Some(p) => p.event_id,
            None => match EventId::from_hex(&note_id_str) {
                Ok(id) => id,
                Err(e) => {
                    error.set(Some(format!("Invalid note ID: {}", e)));
                    loading.set(false);
                    loading_replies.set(false);
                    return;
                }
            },
        };

        crate::utils::thread_tree::invalidate_thread_tree_cache(&event_id);

        let replies_early = replies;
        let reply_ids_early = reply_ids;
        let lg = load_generation;
        let gen = this_generation;

        spawn(async move {
            if let Ok(db_replies) = fetch_replies_db(event_id).await {
                if *lg.peek() != gen { return; }
                let db_count = db_replies.len();
                merge_new_replies(db_replies, replies_early, reply_ids_early, event_id);
                log::info!("Phase 0: loaded {} replies from DB cache", db_count);
            }
        });

        // The spawned future below must be 'static, so anything it touches
        // is moved out of this FnMut closure — hand the delta ledger over
        // as a fresh Arc clone per epoch run (the original stays owned by
        // the closure).
        let deltas_for_spawn = std::sync::Arc::clone(&stream_deltas);
        spawn(async move {
            let note_result = fetch_main_note(&note_id_str).await;
            if *load_generation.peek() != this_generation {
                return;
            }

            let parent_ids = match &note_result {
                Ok(event) => {
                    note_data.set(Some(event.clone()));
                    back_navigation::set_active_note_back_context(
                        note_id_str.clone(),
                        Vec::new(),
                        is_voice_message(event),
                    );
                    loading.set(false);
                    let mut parent_ids = Vec::new();
                    if let Some(parent) =
                        crate::utils::thread_tree::get_parent_id(event)
                    {
                        parent_ids.push(parent);
                    }
                    if let Some(root) = crate::utils::thread_tree::resolve_thread_root_id(event) {
                        if root != event.id && !parent_ids.contains(&root) {
                            parent_ids.push(root);
                        }
                    }
                    parent_ids
                }
                Err(e) => {
                    error.set(Some(e.clone()));
                    loading.set(false);
                    loading_replies.set(false);
                    return;
                }
            };

            let clicked_note = note_result.as_ref().unwrap().clone();
            let root_author = clicked_note.pubkey;

            let thread_root_id = resolve_thread_root_id(&clicked_note)
                .unwrap_or(clicked_note.id);

            let db_parents = fetch_parents_db(&parent_ids).await;
            if !db_parents.is_empty() {
                let mut sorted = db_parents;
                sorted.sort_by_key(|a| a.created_at);
                parent_events.set(sorted);
            }

            let (parents_result, relay_replies_result, inbox_replies) = tokio::join!(
                fetch_parents_with_hints(parent_ids, &clicked_note, 5),
                fetch_replies_from_relays(event_id, Some(root_author)),
                fetch_replies_from_root_inbox(thread_root_id, &root_author)
            );

            if *load_generation.peek() != this_generation {
                return;
            }

            if let Ok(parent_fetch) = parents_result {
                let parents = parent_fetch.parents;
                let missing = parent_fetch.missing_ids;
                let mut merged: Vec<NostrEvent> = parent_events.peek().clone();
                for p in &parents {
                    if !merged.iter().any(|e| e.id == p.id) {
                        merged.push(p.clone());
                    }
                }
                merged.sort_by_key(|a| a.created_at);
                back_navigation::set_active_note_back_context(
                    note_id_str.clone(),
                    merged.iter().map(|event| event.id.to_hex()).collect(),
                    note_data.peek().as_ref().is_some_and(is_voice_message),
                );
                parent_events.set(merged);

                if !missing.is_empty() {
                    let pe = parent_events;
                    let lg = load_generation;
                    let gen = this_generation;
                    let root_for_retry = thread_root_id;
                    spawn(async move {
                        retry_missing_parents(
                            missing,
                            root_for_retry,
                            lg,
                            gen,
                            pe,
                        )
                        .await;
                    });
                }
            }

            let mut combined_replies = relay_replies_result.unwrap_or_default();
            combined_replies.extend(inbox_replies);
            if !combined_replies.is_empty() {
                let relay_count = combined_replies.len();
                let reply_authors: HashSet<PublicKey> =
                    combined_replies.iter().map(|e| e.pubkey).collect();

                merge_new_replies(combined_replies, replies, reply_ids, event_id);
                log::info!(
                    "Phase 1: merged {} replies (total now {})",
                    relay_count,
                    replies.peek().len()
                );

                if !reply_authors.is_empty() {
                    let replies_bg = replies;
                    let lg = this_generation;
                    let load_gen = load_generation;
                    spawn(async move {
                        fetch_replies_phase2(
                            event_id,
                            reply_authors,
                            replies_bg,
                            reply_ids,
                            load_gen,
                            lg,
                        )
                        .await;
                    });
                }
            }

            loading_replies.set(false);

            use crate::utils::profile_prefetch;
            let mut all_events = Vec::new();
            if let Some(note) = note_data.peek().as_ref() {
                all_events.push(note.clone());
            }
            all_events.extend(parent_events.peek().iter().cloned());
            all_events.extend(replies.peek().iter().cloned());
            let sweep_authors = {
                let note = note_data.peek();
                let parents = parent_events.peek();
                let replies_snapshot = replies.peek();
                collect_thread_count_authors(
                    note.as_ref(),
                    &parents,
                    &replies_snapshot,
                    THREAD_COUNT_AUTHOR_CAP,
                )
            };
            let mut ic = interaction_counts;
            let mut ic_stream = interaction_stream_handle;
            let ids_for_counts: Vec<EventId> = all_events.iter().map(|e| e.id).collect();
            let ids_clone = ids_for_counts.clone();
            let ids_for_stream = ids_for_counts.clone();
            let lg_stream = this_generation;
            // `parent_events` is sorted ascending by `created_at`, so the
            // root leads; without parents the clicked note IS the root.
            let thread_root_id = parent_events
                .peek()
                .first()
                .map(|root| root.id)
                .or_else(|| note_data.peek().as_ref().map(|note| note.id));
            // R3 review: skip the full author-inbox sweep (and its L2 pop)
            // when this thread was swept recently — sweep events are
            // already persisted in the shared DB, so the local-DB paint +
            // the cache-served batch + the live stream cover the re-open.
            let skip_sweep = thread_root_id
                .is_some_and(thread_sweep_cooldown_active);
            if !ids_clone.is_empty() {
                spawn(async move {
                    // R3 review — time partition with the live stream: the
                    // stream (started below, independent of every backfill
                    // leg) owns events created at/after `partition`; the
                    // sweep and the counts batch are bounded strictly below
                    // it. Without the partition an interaction delivered by
                    // BOTH the sweep fetch and the stream subscription
                    // would be counted twice (+1 increment vs set-recount).
                    let partition = Timestamp::now();
                    let backfill_until = partition - 1u64;

                    // R4 review: delta-ledger recorder — generation-gated so
                    // an in-flight event from a dying previous-epoch stream
                    // cannot poison this epoch's ledger after the reset
                    // above (the counts map tolerates stale max-merges; the
                    // ledger does not). Cloned for the recorder; the merge
                    // site below borrows `deltas_for_spawn` directly.
                    let deltas_ledger = std::sync::Arc::clone(&deltas_for_spawn);
                    let ledger_gen = this_generation;
                    let ledger_load_gen = load_generation;
                    // Clippy: the recorder is !Send only because of the
                    // generation-peek Signal (UnsyncStorage); it lives on
                    // the single-threaded Dioxus runtime like every other
                    // signal-capturing closure here.
                    #[allow(clippy::arc_with_non_send_sync)]
                    let delta_ledger: StreamDeltaRecorder =
                        std::sync::Arc::new(move |anchor, delta| {
                            if *ledger_load_gen.peek() == ledger_gen {
                                deltas_ledger
                                    .lock()
                                    .unwrap()
                                    .entry(anchor.to_string())
                                    .or_default()
                                    .add_counters(&delta);
                            }
                        });

                    // Instant paint: whatever the local DB already holds.
                    // Merged (not `set`) because the stream started below
                    // may have applied increments before this lands.
                    let local_counts = fetch_local_db_counts(&ids_clone).await;
                    if !local_counts.is_empty() && *load_generation.peek() == this_generation {
                        let mut map = ic.write();
                        for (id, counts) in local_counts {
                            match map.get_mut(&id) {
                                Some(existing) => existing.merge_max(&counts),
                                None => {
                                    map.insert(id, counts);
                                }
                            }
                        }
                    }

                    // Live interaction stream FIRST (R3 review): it used to
                    // start only inside the batch's success branch, so a
                    // failed batch fetch killed the live stream entirely
                    // and the sweep + serial relay resolution blocked it.
                    // Starting it here (with the partition `since`) keeps
                    // live zaps/reactions flowing regardless of how the
                    // backfill legs fare. A generation change past this
                    // point unsubscribes before returning.
                    match stream_interaction_counts_since(
                        ids_for_stream,
                        ic,
                        Some(600),
                        Some(partition),
                        Some(delta_ledger),
                    )
                    .await
                    {
                        Ok(handle) => {
                            if *load_generation.peek() != lg_stream {
                                handle.unsubscribe().await;
                                return;
                            }
                            ic_stream.set(Some(handle));
                        }
                        Err(e) => {
                            log::warn!("Failed to start interaction stream: {}", e);
                            // Non-fatal: the batch + local DB still paint.
                        }
                    }

                    // Profile prefetch off the critical path: the sweep no
                    // longer waits on it (its kind-10002 side effect only
                    // fired for authors whose PROFILES were missing — the
                    // sweep now resolves relay lists explicitly and
                    // concurrently). Avatars are served by the per-card
                    // profile queue; this keeps the batched head start.
                    spawn(async move {
                        profile_prefetch::prefetch_event_authors(&all_events).await;
                    });

                    if !skip_sweep {
                        // Inbox sweep (see fetch_thread_interactions_from_author_inbox):
                        // reactions/reposts/zaps live on the thread authors'
                        // read relays, which the batch's ≤3-target pool leg
                        // cannot reach. The sweep drops them into the shared
                        // DB; the L2 pop below forces the batch to re-read it
                        // instead of serving pre-sweep zeros from cache.
                        fetch_thread_interactions_from_author_inbox(
                            &ids_clone,
                            &sweep_authors,
                            backfill_until,
                        )
                        .await;
                        if *load_generation.peek() != this_generation {
                            return;
                        }
                        invalidate_interaction_counts_batch(
                            &ids_clone.iter().map(|id| id.to_hex()).collect::<Vec<_>>(),
                        );
                    }

                    match fetch_interaction_counts_batch_partitioned(
                        ids_clone,
                        Duration::from_secs(5),
                        Some(backfill_until),
                    )
                    .await
                    {
                        Ok(batch) => {
                            if *load_generation.peek() != this_generation {
                                return;
                            }
                            // R3 review merge policy — see
                            // `merge_counts_batch_into`: relay-truth rows
                            // may correct counts DOWNWARD (deletions,
                            // un-likes); cache-served/truncated rows
                            // max-merge. R4: this epoch's stream deltas
                            // are re-applied on top of the relay-truth
                            // baselines so post-partition increments
                            // survive the replace.
                            {
                                let deltas_snapshot = deltas_for_spawn.lock().unwrap().clone();
                                let mut map = ic.write();
                                merge_counts_batch_into(&mut map, batch, &deltas_snapshot);
                            }
                        }
                        Err(e) => {
                            log::warn!("Interaction counts batch failed: {}", e);
                        }
                    }
                });
            }
        });
    }));

    {
        let reply_filter = note_data.read().as_ref().map(|event| {
            Filter::new()
                .kinds(vec![
                    Kind::TextNote,
                    Kind::Comment,
                    Kind::VoiceMessageReply,
                    Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT),
                ])
                .event(event.id)
                .since(Timestamp::now())
                .limit(0)
        });
        use_relay_subscription(reply_filter, move |event: &nostr::event::Event| {
            if reply_ids.write().insert(event.id) {
                log::info!(
                    "New reply received via streaming: {}",
                    event.id.to_hex()
                );
                replies.write().push(event.clone());
                if let Some(note) = note_data.peek().as_ref() {
                    crate::utils::thread_tree::invalidate_thread_tree_cache(&note.id);
                }
            }
        });
    }

    {
        let root_filter = note_data.read().as_ref().and_then(|event| {
            let root_id = resolve_thread_root_id(event)?;
            if root_id == event.id {
                return None;
            }
            Some(Filter::new()
                .kinds(vec![
                    Kind::TextNote,
                    Kind::Comment,
                    Kind::VoiceMessage,
                    Kind::VoiceMessageReply,
                ])
                .event(root_id)
                .since(Timestamp::now())
                .limit(0))
        });
        use_relay_subscription(root_filter, move |event: &nostr::event::Event| {
            if reply_ids.write().insert(event.id) {
                log::info!(
                    "Root subscription: new reply {}",
                    event.id.to_hex()
                );
                replies.write().push(event.clone());
                if let Some(note) = note_data.peek().as_ref() {
                    crate::utils::thread_tree::invalidate_thread_tree_cache(&note.id);
                }
            }
        });
    }

    // C1 — Deferred scroll application.
    //
    // `use_effect` runs after the DOM is committed, so by the time this
    // fires the real thread content is laid out (not the AddressViewer
    // spinner the Layout's scroll-to-zero hit). We subscribe to `note_data`
    // (via `.read()`) so this (re)fires whenever the main note becomes
    // available on each note_id change, and `peek` everything else to avoid
    // spurious subscriptions. Tasks spawned in an effect are NOT cancelled on
    // re-run, so the `load_generation` guard drops stale fires if the user
    // navigates to another note during the 16 ms settle window; all writes
    // are idempotent so coexistence is harmless. The `PENDING_SCROLL_TARGET`
    // (set by the Layout) is `Some(0.0)` for a forward push (open at top) and
    // `Some(saved_y)` for a Back navigation (restore prior position).
    use_effect(move || {
        let has_note = note_data.read().is_some();
        let gen = *load_generation.peek();
        if has_note {
            spawn(async move {
                // Yield one frame so DB-parent writes (set a few lines after
                // `note_data` in the main load effect) land before the reset.
                crate::platform::timer::sleep_ms(16).await;
                if *load_generation.peek() != gen {
                    return;
                }
                // Apply only when a target is actually pending. This effect
                // re-fires on EVERY `note_data` write — including the
                // prefetch→confirmed swap in the main load task, which
                // rewrites the same note shortly after the initial render.
                // By then the Layout's target has been consumed (None), and
                // an `unwrap_or(0.0)` fallback would yank a mid-read user
                // back to the top — reintroducing the #348 symptom on the
                // refresh path.
                let Some(target) = *crate::stores::ui::scroll_restore::PENDING_SCROLL_TARGET
                    .peek()
                else {
                    return;
                };
                crate::stores::ui::scroll_restore::set_scroll_y(target).await;
                *crate::stores::ui::scroll_restore::PENDING_SCROLL_TARGET.write() = None;
            });
        }
    });

    // C2a — Suppress browser scroll-anchoring during the initial load.
    //
    // When parent events prepend above the main note the browser's
    // `overflow-anchor` shifts the viewport to keep the main note in view —
    // the "thrown down" effect. Disabling anchoring on `<html>` makes the
    // explicit `scrollTo` above the sole authority on scroll position during
    // load. Re-keyed on `load_generation` so it re-disables on each
    // thread→thread navigation (same component, new note_id).
    use_effect(move || {
        let _gen = *load_generation.read();
        spawn(async move {
            let _ = dioxus::document::eval(
                "document.documentElement.style.overflowAnchor = 'none'",
            )
            .await;
        });
    });

    // C2b — Restore scroll-anchoring once the initial load settles, so
    // streaming replies (appended within the visible area) still benefit from
    // the browser keeping the user's position stable.
    use_effect(move || {
        let replies_done = !*loading_replies.read();
        if replies_done {
            spawn(async move {
                let _ = dioxus::document::eval(
                    "document.documentElement.style.overflowAnchor = ''",
                )
                .await;
            });
        }
    });

    use_drop(move || {
        back_navigation::clear_active_note_back_context(&note_id);
        if let Some(handle) = interaction_stream_handle.write().take() {
            spawn(async move {
                handle.unsubscribe().await;
            });
        }
        // Safety net: if the user navigates away mid-load (before
        // `loading_replies` settles), restore scroll-anchoring. `spawn_forever`
        // (not `spawn`) so the eval survives this scope's teardown.
        dioxus_core::spawn_forever(async move {
            let _ = dioxus::document::eval(
                "document.documentElement.style.overflowAnchor = ''",
            )
            .await;
        });
    });

    rsx! {
        div { class: "min-h-screen",
            {
                let data_is_voice = note_data.read().as_ref().map(is_voice_message);
                let is_voice_note = data_is_voice.unwrap_or(initial_is_voice);
                let back_route = if is_voice_note {
                    Route::VoiceMessages {}
                } else {
                    Route::Home {
                        list: String::new(),
                    }
                };
                let title = if is_voice_note {
                    "Voice Message"
                } else {
                    "Post"
                };
                let nav = use_navigator();
                let fallback_route = back_route.clone();
                rsx! {
                    div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                        div { class: "flex items-center gap-4 p-4",
                            button {
                                class: "hover:bg-accent rounded-full p-2 transition",
                                onclick: move |_| {
                                    if nav.can_go_back() {
                                        nav.go_back();
                                    } else {
                                        nav.push(fallback_route.clone());
                                    }
                                },
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "20",
                                    height: "20",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    path { d: "m15 18-6-6 6-6" }
                                }
                            }
                            h1 { class: "text-xl font-bold", "{title}" }
                        }
                    }
                }
            }
            if !*nostr_client::CLIENT_INITIALIZED.read()
                || (*loading.read() && note_data.read().is_none())
            {
                ClientInitializing {}
            } else if let Some(err) = error.read().as_ref() {
                div { class: "p-6",
                    div { class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg border border-red-200 dark:border-red-800",
                        "{err}"
                    }
                }
            } else if let Some(event) = note_data.read().as_ref() {
                if !parent_events.read().is_empty() {
                    div { class: "border-b-2 border-blue-500/20",
                        for parent in parent_events.read().iter() {
                            div { key: "{parent.id}", class: "relative",
                                if is_voice_message(parent) {
                                    VoiceMessageCard {
                                        key: "{parent.id}",
                                        event: parent.clone(),
                                    }
                                } else {
                                    NoteCard {
                                        key: "{parent.id}",
                                        event: parent.clone(),
                                         precomputed_counts: interaction_counts.read().get(&parent.id.to_hex()).cloned(),
                                         collapsible: true,
                                         cached_muted_posts: cached_muted_posts.read().clone(),
                                         cached_blocked_users: cached_blocked_users.read().clone(),
                                         cached_muted_words: cached_muted_words.read().clone(),
                                     }
                                 }
                                 div { class: "absolute left-[40px] top-[60px] bottom-0 w-0.5 bg-border" }
                            }
                        }
                    }
                }
                if is_voice_message(event) {
                    VoiceMessageCard {
                        key: "{event.id}",
                        event: event.clone(),
                    }
                } else {
                    {
                        let root_event_id = event.id;
                        rsx! {
                            NoteCard {
                                key: "{event.id}",
                                event: event.clone(),
                                root_event: Some(event.clone()),
                                 precomputed_counts: interaction_counts.read().get(&event.id.to_hex()).cloned(),
                                 collapsible: false,
                                 cached_muted_posts: cached_muted_posts.read().clone(),
                                 cached_blocked_users: cached_blocked_users.read().clone(),
                                 cached_muted_words: cached_muted_words.read().clone(),
                                 on_reply: move |reply_event: NostrEvent| {
                                    if reply_ids.write().insert(reply_event.id) {
                                        log::info!("Adding reply optimistically from main note: {}", reply_event.id.to_hex());
                                        replies.write().push(reply_event);
                                        crate::utils::thread_tree::invalidate_thread_tree_cache(&root_event_id);
                                    }
                                },
                            }
                        }
                    }
                }
                div { class: "border-b border-border" }
                if *loading_replies.read() && parent_events.read().is_empty() && replies.read().is_empty() {
                    div { class: "flex items-center justify-center py-10",
                        div { class: "text-center",
                            div { class: "animate-spin text-4xl mb-2", "⚡" }
                            p { class: "text-muted-foreground", "Loading..." }
                        }
                    }
                } else {{
                    // Read the memoized tree. `.cloned()` subscribes the current
                    // render to the memo's signal and returns an owned Vec. The
                    // memo handles the parent-chain filter, the sibling filter,
                    // the edit-kind filter, and the tree build.
                    let thread_tree: Vec<ThreadNode> = thread_tree_memo.cloned();
                    // Proposals are cheap to partition (O(n) linear scan), so we
                    // compute them inline rather than extending the memo's
                    // return type. (Memo would also need its return type to be
                    // `PartialEq`, which `Vec<NostrEvent>` is not.)
                    let edit_kind = Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT);
                    let proposals: Vec<NostrEvent> = replies
                        .read()
                        .iter()
                        .filter(|e| e.kind == edit_kind && e.pubkey != event.pubkey)
                        .cloned()
                        .collect();
                    let root_event_id = event.id;
                    let original_for_proposals = event.clone();
                    let has_content = !thread_tree.is_empty() || !proposals.is_empty();
                    if !has_content && !*loading_replies.read() {
                        rsx! {
                            div { class: "flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground",
                                p { "No replies yet" }
                                p { class: "text-sm", "Be the first to reply!" }
                            }
                        }
                    } else {
                        rsx! {
                            div { class: "divide-y divide-border",
                                for proposal in proposals {
                                    EditProposalCard {
                                        key: "{proposal.id}",
                                        event: proposal,
                                        original_event: original_for_proposals.clone(),
                                    }
                                }
                                for node in thread_tree {
                                    ThreadedComment {
                                        key: "{node.event.id}",
                                        node: node.clone(),
                                        depth: 0,
                                        root_event: Some(event.clone()),
                                         precomputed_counts: interaction_counts.read().get(&node.event.id.to_hex()).cloned(),
                                         cached_muted_posts: cached_muted_posts.read().clone(),
                                         cached_blocked_users: cached_blocked_users.read().clone(),
                                         cached_muted_words: cached_muted_words.read().clone(),
                                         on_reply: move |reply_event: NostrEvent| {
                                            if reply_ids.write().insert(reply_event.id) {
                                                log::info!("Adding reply optimistically: {}", reply_event.id.to_hex());
                                                replies.write().push(reply_event);
                                                crate::utils::thread_tree::invalidate_thread_tree_cache(&root_event_id);
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::{EventBuilder, FinalizeEvent};
    use nostr::key::Keys;

    fn note_by(pubkey: &Keys, created_at: u64) -> NostrEvent {
        EventBuilder::new(Kind::TextNote, "test")
            .custom_created_at(Timestamp::from(created_at))
            .finalize(pubkey)
            .unwrap()
    }

    /// Parent-chain authors lead (ascending `created_at` order puts the
    /// thread root first), the clicked note's author comes next, reply
    /// authors last. The clicked note's author is not duplicated.
    #[test]
    fn collect_thread_count_authors_prioritizes_root_chain() {
        let root_author = Keys::generate();
        let mid_author = Keys::generate();
        let clicked_author = Keys::generate();
        let reply_author = Keys::generate();

        let root = note_by(&root_author, 1_000);
        let mid = note_by(&mid_author, 1_100);
        let clicked = note_by(&clicked_author, 1_200);
        let reply = note_by(&reply_author, 1_300);

        let authors = collect_thread_count_authors(
            Some(&clicked),
            &[mid, root], // unsorted input: dedup/order comes from call-site sort
            &[reply],
            THREAD_COUNT_AUTHOR_CAP,
        );

        assert_eq!(
            authors,
            vec![
                mid_author.public_key(),
                root_author.public_key(),
                clicked_author.public_key(),
                reply_author.public_key(),
            ]
        );
    }

    /// With no parents the clicked note IS the root: its author leads.
    #[test]
    fn collect_thread_count_authors_clicked_first_when_no_parents() {
        let clicked_author = Keys::generate();
        let reply_author = Keys::generate();
        let clicked = note_by(&clicked_author, 1_000);
        let reply = note_by(&reply_author, 1_100);

        let authors = collect_thread_count_authors(
            Some(&clicked),
            &[],
            &[reply],
            THREAD_COUNT_AUTHOR_CAP,
        );

        assert_eq!(
            authors,
            vec![clicked_author.public_key(), reply_author.public_key()]
        );
    }

    /// Duplicate authors across the chain/replies collapse to one entry.
    #[test]
    fn collect_thread_count_authors_dedups() {
        let author = Keys::generate();
        let other = Keys::generate();
        let root = note_by(&author, 1_000);
        let clicked = note_by(&author, 1_100);
        let reply = note_by(&other, 1_200);
        let own_reply = note_by(&author, 1_300);

        let authors = collect_thread_count_authors(
            Some(&clicked),
            &[root],
            &[reply, own_reply],
            THREAD_COUNT_AUTHOR_CAP,
        );

        assert_eq!(authors, vec![author.public_key(), other.public_key()]);
    }

    /// The author list is capped so the sweep's kind 10002 resolutions
    /// stay bounded; the cap keeps insertion (priority) order.
    #[test]
    fn collect_thread_count_authors_caps_in_priority_order() {
        let keys: Vec<Keys> = (0..4).map(|_| Keys::generate()).collect();
        let clicked = note_by(&keys[0], 1_000);
        let parents: Vec<NostrEvent> = keys[1..]
            .iter()
            .enumerate()
            .map(|(i, k)| note_by(k, 1_100 + i as u64))
            .collect();

        let authors = collect_thread_count_authors(Some(&clicked), &parents, &[], 3);

        // parents first (keys[1], keys[2], keys[3]), then clicked — capped at 3.
        assert_eq!(
            authors,
            vec![keys[1].public_key(), keys[2].public_key(), keys[3].public_key()]
        );
    }
}
