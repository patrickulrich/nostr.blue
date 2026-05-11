use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::collections::{HashMap, HashSet};
use std::time:: Duration;

use crate::components::{ClientInitializing, EditProposalCard, NoteCard, ThreadedComment, VoiceMessageCard};
use crate::hooks::{use_mute_block_cache, use_relay_subscription};
use crate::routes::Route;
use crate::services::aggregation::{
    fetch_interaction_counts_batch, InteractionCounts,
};
use crate::stores::back_navigation;
use crate::stores::nostr_client;
use crate::stores::nostr_client::fetching::{fetch_event_targeted, parse_event_id};
use crate::stores::relay;
use crate::stores::relay::coverage::RelayPurpose;
use crate::utils::{build_thread_tree, event::is_voice_message};

async fn fetch_main_note(note_id: &str) -> std::result::Result<NostrEvent, String> {
    let parsed = parse_event_id(note_id).ok_or("Invalid note ID")?;
    fetch_event_targeted(parsed, Duration::from_secs(10))
        .await?
        .ok_or("Event not found".to_string())
}

fn extract_parent_ids(note: &NostrEvent) -> Vec<EventId> {
    let mut ids: Vec<EventId> = note.tags.event_ids().cloned().collect();
    let upper_e = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
    for tag in note.tags.iter() {
        if tag.kind() == nostr_sdk::TagKind::SingleLetter(upper_e) {
            if let Some(content) = tag.content() {
                if let Ok(id) = EventId::from_hex(content) {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
    }
    ids
}

fn extract_relay_hints(note: &NostrEvent) -> Vec<(EventId, Option<String>)> {
    note.tags
        .iter()
        .filter_map(|tag| {
            if let Some(tag_std) = tag.as_standardized() {
                match tag_std {
                    TagStandard::Event {
                        event_id,
                        relay_url,
                        ..
                    } => {
                        let url = relay_url.as_ref().map(|u| u.to_string());
                        Some((*event_id, url))
                    }
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect()
}

fn extract_author_from_etags(note: &NostrEvent) -> HashMap<EventId, PublicKey> {
    let mut author_map = HashMap::new();
    for tag in note.tags.iter() {
        if let Some(TagStandard::Event {
            event_id,
            public_key: Some(pk),
            ..
        }) = tag.as_standardized()
        {
            author_map.insert(*event_id, *pk);
        }
    }
    author_map
}

async fn fetch_parents_with_hints(
    initial_ids: Vec<EventId>,
    clicked_note: &NostrEvent,
    max_depth: usize,
) -> std::result::Result<Vec<NostrEvent>, String> {
    let mut all_parents = Vec::new();
    let mut fetched_ids: HashSet<EventId> = HashSet::new();
    fetched_ids.insert(clicked_note.id);

    let hints = extract_relay_hints(clicked_note);
    let hint_map: HashMap<EventId, String> = hints
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

        let mut new_events = Vec::new();

        // Phase 1: Fetch from hinted relays (e-tag relay_url)
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

            // Add unfetched hinted IDs to unhinted so they get fallback attempts
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
            // Phase 2: Author-targeted fetch for parents where we know the author
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

            // Phase 3: Try clicked note author's relays for remaining parents
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

            // Phase 4: Final fallback — outbox fetch for anything still missing
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

        // Build author hints from newly fetched events for the next iteration
        let mut next_author_hints = HashMap::new();
        for event in &new_events {
            let event_authors = extract_author_from_etags(event);
            next_author_hints.extend(event_authors);
            // Record 10002 events eagerly
            relay::coverage::record_relay_list_from_event(event);
        }

        ids_to_fetch = new_events
            .iter()
            .flat_map(extract_parent_ids)
            .filter(|id| !fetched_ids.contains(id))
            .collect();

        all_parents.extend(new_events);
    }

    Ok(dedup_replies(all_parents))
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

fn merge_new_replies(
    new_events: Vec<NostrEvent>,
    mut replies: Signal<Vec<NostrEvent>>,
    mut reply_ids: Signal<HashSet<EventId>>,
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
    }
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
            Kind::VoiceMessage,
            Kind::VoiceMessageReply,
            Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT),
        ])
        .event(event_id)
        .limit(100);

    let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
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

    Ok(dedup_replies(db_replies))
}

async fn fetch_replies_from_relays(
    event_id: EventId,
    root_author_pubkey: Option<PublicKey>,
) -> std::result::Result<Vec<NostrEvent>, String> {
    let event_id_hex = event_id.to_hex();

    let filter_lower = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::VoiceMessage,
            Kind::VoiceMessageReply,
            Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT),
        ])
        .event(event_id)
        .limit(100);

    let upper_e_tag = nostr_sdk::SingleLetterTag::uppercase(nostr_sdk::Alphabet::E);
    let filter_upper = Filter::new()
        .kinds(vec![
            Kind::VoiceMessage,
            Kind::VoiceMessageReply,
            Kind::Comment,
        ])
        .custom_tag(upper_e_tag, event_id_hex)
        .limit(100);

    let (lower_result, upper_result, author_result) = tokio::join!(
        nostr_client::fetch_events_from_connected_relays(filter_lower, Duration::from_secs(5)),
        nostr_client::fetch_events_from_connected_relays(filter_upper, Duration::from_secs(5)),
        async {
            match root_author_pubkey {
                Some(pk) => fetch_author_relays_replies(event_id, &pk).await,
                None => Vec::new(),
            }
        }
    );

    let mut all_replies = Vec::new();
    if let Ok(replies) = lower_result {
        all_replies.extend(replies);
    }
    if let Ok(replies) = upper_result {
        all_replies.extend(replies);
    }
    all_replies.extend(author_result);

    Ok(dedup_replies(all_replies))
}

const MAX_EPHEMERAL_RELAYS: usize = 5;

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

    let Ok(relay_maps) = client.database().relay_lists(pubkeys.clone()).await else {
        return;
    };

    let mut combined_relays: Vec<String> = Vec::new();
    let mut authors_with_relays = HashSet::new();

    for (pk, relays_map) in &relay_maps {
        if !relays_map.is_empty() {
            let urls: Vec<String> = relays_map
                .iter()
                .filter(|(_, m)| m.is_none() || matches!(m, Some(RelayMetadata::Write)))
                .map(|(u, _)| u.to_string())
                .collect();
            combined_relays.extend(urls);
            authors_with_relays.insert(*pk);
        }
        relay::coverage::record_relay_list_from_event_by_map(pk, relays_map);
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
                merge_new_replies(events, replies_signal, reply_ids_signal);
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
                    if let Ok(events) = c.fetch_events(filter, Duration::from_secs(5)).await {
                        for event in events {
                            relay::coverage::record_relay_list_from_event(&event);
                        }
                    }
                }
            }
        });
    }
}

#[component]
pub fn Note(note_id: String, from_voice: Option<String>) -> Element {
    let initial_is_voice = from_voice.as_ref().is_some_and(|v| v == "true");
    let mut note_data: Signal<Option<NostrEvent>> = use_signal(|| None);
    let mut parent_events = use_signal(Vec::<NostrEvent>::new);
    let mut replies = use_signal(Vec::<NostrEvent>::new);
    let mut loading = use_signal(|| true);
    let mut loading_parents = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut interaction_counts: Signal<HashMap<String, InteractionCounts>> =
        use_signal(HashMap::new);
    let (cached_muted_posts, cached_blocked_users) = use_mute_block_cache();
    let mut load_generation = use_signal(|| 0u32);
    let mut reply_ids: Signal<HashSet<EventId>> = use_signal(HashSet::new);

    use_effect(use_reactive!(|note_id| {
        let note_id_str = note_id.clone();

        let this_generation = load_generation.peek().wrapping_add(1);
        load_generation.set(this_generation);

        note_data.set(None);
        replies.set(Vec::new());
        parent_events.set(Vec::new());
        interaction_counts.set(HashMap::new());
        reply_ids.set(HashSet::new());
        loading.set(true);
        loading_parents.set(true);
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
            loading_parents.set(false);
            return;
        }

        let event_id = match parse_event_id(&note_id_str) {
            Some(p) => p.event_id,
            None => match EventId::from_hex(&note_id_str) {
                Ok(id) => id,
                Err(e) => {
                    error.set(Some(format!("Invalid note ID: {}", e)));
                    loading.set(false);
                    loading_parents.set(false);
                    return;
                }
            },
        };

        let mut replies_early = replies;
        let mut reply_ids_early = reply_ids;
        let lg = load_generation;
        let gen = this_generation;

        spawn(async move {
            // Phase 0: DB query for replies (instant, ~1ms)
            if let Ok(db_replies) = fetch_replies_db(event_id).await {
                if *lg.peek() != gen { return; }
                let db_ids: HashSet<EventId> = db_replies.iter().map(|e| e.id).collect();
                let db_count = db_replies.len();
                reply_ids_early.set(db_ids);
                replies_early.set(db_replies);
                loading_parents.set(false);
                log::info!("Phase 0: loaded {} replies from DB cache", db_count);
            }
        });

        spawn(async move {
            // Phase 1: Fetch main note (DB-first, then relays)
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
                    extract_parent_ids(event)
                }
                Err(e) => {
                    error.set(Some(e.clone()));
                    loading.set(false);
                    loading_parents.set(false);
                    return;
                }
            };

            let clicked_note = note_result.as_ref().unwrap().clone();
            let root_author = clicked_note.pubkey;

            // Phase 2: Fetch parents + relay replies concurrently
            let (parents_result, relay_replies_result) = tokio::join!(
                fetch_parents_with_hints(parent_ids, &clicked_note, 5),
                fetch_replies_from_relays(event_id, Some(root_author))
            );

            if *load_generation.peek() != this_generation {
                return;
            }

            if let Ok(mut parents) = parents_result {
                parents.sort_by_key(|a| a.created_at);
                back_navigation::set_active_note_back_context(
                    note_id_str.clone(),
                    parents.iter().map(|event| event.id.to_hex()).collect(),
                    note_data.peek().as_ref().is_some_and(is_voice_message),
                );
                parent_events.set(parents);
            }

            if let Ok(relay_replies) = relay_replies_result {
                let relay_count = relay_replies.len();
                let reply_authors: HashSet<PublicKey> =
                    relay_replies.iter().map(|e| e.pubkey).collect();

                merge_new_replies(relay_replies, replies, reply_ids);
                log::info!(
                    "Phase 1: merged {} relay replies (total now {})",
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

            use crate::utils::profile_prefetch;
            let mut all_events = Vec::new();
            if let Some(note) = note_data.peek().as_ref() {
                all_events.push(note.clone());
            }
            all_events.extend(parent_events.peek().iter().cloned());
            all_events.extend(replies.peek().iter().cloned());
            let mut ic = interaction_counts;
            let ids_for_counts: Vec<EventId> = all_events.iter().map(|e| e.id).collect();
            let ids_clone = ids_for_counts.clone();
            spawn(async move {
                profile_prefetch::prefetch_event_authors(&all_events).await;
            });
            if !ids_clone.is_empty() {
                spawn(async move {
                    if let Ok(counts) =
                        fetch_interaction_counts_batch(ids_clone, Duration::from_secs(5))
                            .await
                    {
                        if *load_generation.peek() != this_generation {
                            return;
                        }
                        ic.set(counts);
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
        use_relay_subscription(reply_filter, move |event: &nostr::Event| {
            if reply_ids.write().insert(event.id) {
                log::info!(
                    "New reply received via streaming: {}",
                    event.id.to_hex()
                );
                replies.write().push(event.clone());
            }
        });
    }

    use_drop(move || {
        back_navigation::clear_active_note_back_context(&note_id);
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
                    div { class: "sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border",
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
                if *loading_parents.read() && parent_events.read().is_empty() && replies.read().is_empty() {
                    div { class: "flex items-center justify-center py-10",
                        div { class: "text-center",
                            div { class: "animate-spin text-4xl mb-2", "⚡" }
                            p { class: "text-muted-foreground", "Loading..." }
                        }
                    }
                } else {{
                    let reply_vec = replies.read().clone();
                    let edit_kind = Kind::Custom(crate::stores::nostr_client::edits::KIND_NOTE_EDIT);
                    let (proposals, actual_replies): (Vec<NostrEvent>, Vec<NostrEvent>) = reply_vec
                        .into_iter()
                        .partition(|e| e.kind == edit_kind && e.pubkey != event.pubkey);
                    let root_event_id = event.id;
                    let original_for_proposals = event.clone();
                    let has_content = !actual_replies.is_empty() || !proposals.is_empty();
                    if !has_content && !*loading_parents.read() {
                        rsx! {
                            div { class: "flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground",
                                p { "No replies yet" }
                                p { class: "text-sm", "Be the first to reply!" }
                            }
                        }
                    } else {
                        let thread_tree = build_thread_tree(actual_replies, &event.id);
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
