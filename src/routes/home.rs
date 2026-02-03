use crate::components::{
    ArticleCard, ClientInitializing, NoteCard, NoteCardSkeleton, NoteComposer,
};
use crate::hooks::{use_infinite_scroll, use_user_lists, UserList};
use crate::routes::Route;
use crate::services::aggregation::{
    fetch_interaction_counts_batch, stream_interaction_counts, sync_interaction_counts,
    InteractionCounts, InteractionStreamHandle,
};
use crate::stores::feed_cache::FeedCacheKey;
use crate::stores::{auth_store, feed_cache, nostr_client, subscription_manager};
use crate::utils::list_encryption::get_all_list_members;
use crate::utils::list_kinds::NAMED_PEOPLE;
use crate::utils::{extract_reposted_event, get_item_count, DataState, FeedItem};
use dioxus::prelude::*;
use nostr_sdk::{Filter, Kind, PublicKey, Timestamp};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;
#[derive(Clone, PartialEq, Debug)]
enum FeedType {
    Following,
    FollowingWithReplies,
    Global,
    PeopleList(Box<UserList>),
}
impl FeedType {
    fn label(&self) -> String {
        match self {
            FeedType::Following => "Following".to_string(),
            FeedType::FollowingWithReplies => "Following + Replies".to_string(),
            FeedType::Global => "Global".to_string(),
            FeedType::PeopleList(list) => list.name.clone(),
        }
    }
}
#[component]
pub fn Home(list: String) -> Element {
    let mut feed_state = use_signal(|| DataState::<Vec<FeedItem>>::Pending);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut pagination_loading = use_signal(|| false);
    let mut interaction_counts = use_signal(HashMap::<String, InteractionCounts>::new);
    let mut interactions_loaded = use_signal(|| false);
    let mut cached_muted_posts: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| {
        None
    });
    let mut cached_blocked_users: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| {
        None
    });
    let mut pending_posts = use_signal(Vec::<FeedItem>::new);
    let pending_count = use_memo(move || pending_posts.read().len());
    let mut realtime_started = use_signal(|| false);
    let mut subscription_ids = use_signal(Vec::<nostr_sdk::SubscriptionId>::new);
    let mut interaction_stream_handle: Signal<Option<InteractionStreamHandle>> = use_signal(||
    None);
    let mut request_id = use_signal(|| 0u32);
    let mut last_loaded_trigger = use_signal(|| (0u32, FeedType::Following));
    let (all_lists, _lists_loading, _lists_error, _) = use_user_lists();
    let people_lists = use_memo(move || {
        all_lists
            .read()
            .iter()
            .filter(|list| list.kind == NAMED_PEOPLE)
            .cloned()
            .collect::<Vec<_>>()
    });
    use_effect({
        let list_param = list.clone();
        move || {
            if list_param.is_empty() {
                return;
            }
            let lists = people_lists.read();
            if lists.is_empty() {
                return;
            }
            if let Some(matching_list) = lists
                .iter()
                .find(|l| l.identifier == list_param)
            {
                log::info!("Deep link: Setting feed to list '{}'", matching_list.name);
                feed_type.set(FeedType::PeopleList(Box::new(matching_list.clone())));
            } else {
                log::warn!("Deep link: List with identifier '{}' not found", list_param);
            }
        }
    });
    let mut last_mute_pubkey: Signal<Option<String>> = use_signal(|| None);
    let mut last_invalidate_token: Signal<u32> = use_signal(|| 0);
    use_effect(move || {
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let current_pubkey = auth_store::AUTH_STATE.read().pubkey.clone();
        let current_token = *nostr_client::MUTE_BLOCK_INVALIDATE.read();
        if current_token != *last_invalidate_token.peek() {
            log::debug!(
                "Mute/block invalidation detected in home feed, clearing caches"
            );
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            last_invalidate_token.set(current_token);
        }
        if !is_authenticated {
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            last_mute_pubkey.set(None);
            return;
        }
        if let (Some(ref last), Some(ref current)) = (
            last_mute_pubkey.peek().as_ref(),
            current_pubkey.as_ref(),
        ) {
            if last != current {
                log::debug!(
                    "Account switch detected in home feed, clearing mute/block cache"
                );
                cached_muted_posts.set(None);
                cached_blocked_users.set(None);
            }
        }
        last_mute_pubkey.set(current_pubkey.clone());
        if !client_initialized {
            return;
        }
        if current_pubkey.is_none() {
            log::debug!("Skipping mute list fetch - pubkey not yet available");
            return;
        }
        if cached_muted_posts.peek().is_some() && cached_blocked_users.peek().is_some() {
            return;
        }
        let auth_pubkey_snapshot = current_pubkey.clone();
        let invalidate_token_snapshot = current_token;
        spawn(async move {
            match nostr_client::get_mute_list_data().await {
                Ok(data) => {
                    let current_pubkey = auth_store::AUTH_STATE.peek().pubkey.clone();
                    let current_invalidate = *nostr_client::MUTE_BLOCK_INVALIDATE.peek();
                    if current_pubkey == auth_pubkey_snapshot
                        && auth_pubkey_snapshot.is_some()
                        && current_invalidate == invalidate_token_snapshot
                    {
                        cached_muted_posts.set(Some(Rc::new(data.muted_posts)));
                        cached_blocked_users.set(Some(Rc::new(data.blocked_users)));
                    } else {
                        log::debug!(
                            "Discarding stale mute list fetch (pubkey or token changed)"
                        );
                    }
                }
                Err(e) => {
                    let snapshot_short = auth_pubkey_snapshot
                        .as_ref()
                        .map(|s| &s[..8.min(s.len())]);
                    let current_short = auth_store::AUTH_STATE
                        .peek()
                        .pubkey
                        .as_ref()
                        .map(|s| s[..8.min(s.len())].to_string());
                    log::error!(
                        "Failed to fetch mute list: {} (snapshot={:?}, current={:?})", e,
                        snapshot_short, current_short
                    );
                }
            }
        });
    });
    use_effect(move || {
        let refresh = *refresh_trigger.read();
        let current_feed_type = feed_type.read().clone();
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !is_authenticated || !client_initialized {
            return;
        }
        let (last_refresh, last_feed) = last_loaded_trigger.peek().clone();
        let (is_loading, has_data) = {
            let current_state = &*feed_state.peek();
            let loading = matches!(current_state, DataState::Loading);
            let data = matches!(
                current_state,
                DataState::Loaded(items)
                if !items.is_empty()
            );
            (loading, data)
        };
        let feed_type_changed = current_feed_type != last_feed;
        let refresh_changed = refresh != last_refresh;
        if is_loading && !feed_type_changed && !refresh_changed {
            log::debug!("Skipping feed re-load: already loading, no intentional change");
            return;
        }
        if has_data && !feed_type_changed && !refresh_changed {
            log::debug!(
                "Skipping feed re-load: data already present, no intentional change"
            );
            return;
        }
        last_loaded_trigger.set((refresh, current_feed_type.clone()));
        let current_id = *request_id.peek() + 1;
        request_id.set(current_id);
        if !has_data || feed_type_changed {
            feed_state.set(DataState::Loading);
        }
        oldest_timestamp.set(None);
        has_more.set(true);
        let ids = subscription_ids.peek().clone();
        if !ids.is_empty() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    log::info!(
                        "Cleaning up {} real-time subscriptions due to manual refresh",
                        ids.len()
                    );
                    subscription_manager::unsubscribe_all(&client, &ids).await;
                }
            });
        }
        subscription_ids.write().clear();
        if let Some(handle) = interaction_stream_handle.peek().clone() {
            spawn(async move {
                log::info!("Cleaning up interaction stream due to refresh");
                handle.unsubscribe().await;
            });
        }
        interaction_stream_handle.set(None);
        pending_posts.set(Vec::new());
        realtime_started.set(false);
        let is_first_load = !*interactions_loaded.peek() || feed_type_changed;
        interactions_loaded.set(false);
        spawn(async move {
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale feed request {}", current_id);
                return;
            }
            let is_stale = || *request_id.peek() != current_id;
            match current_feed_type {
                FeedType::Following => {
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let cache_key = FeedCacheKey::Following {
                        pubkey: pubkey_str.clone(),
                    };
                    let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                        .await
                        .unwrap_or_default();
                    if is_stale() {
                        return;
                    }
                    let mut accumulated_items = if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for Following feed", cached_items
                            .len()
                        );
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                        cached_items
                    } else {
                        // No cache: show quick global posts while contacts load
                        log::info!("No cache, fetching quick global posts while loading contacts...");
                        if let Ok(quick_posts) = fetch_quick_global_posts(20).await {
                            if !quick_posts.is_empty() && !is_stale() {
                                log::info!("Showing {} quick global posts while contacts load", quick_posts.len());
                                feed_state.set(DataState::Loaded(quick_posts));
                            }
                        }
                        Vec::new()
                    };
                    let stream_req_id = request_id;
                    let stream_curr_id = current_id;
                    let result = load_following_feed_streaming(
                            None,
                            |batch_items| {
                                if *stream_req_id.peek() != stream_curr_id {
                                    log::debug!("Discarding stale streaming batch");
                                    return;
                                }
                                accumulated_items = feed_cache::merge_feed_items(
                                    accumulated_items.clone(),
                                    batch_items,
                                );
                                feed_state
                                    .set(DataState::Loaded(accumulated_items.clone()));
                            },
                        )
                        .await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok((feed_items, did_fallback)) => {
                            let effective_cache_key = if did_fallback {
                                log::info!("No contacts, switched to Global feed");
                                feed_type.set(FeedType::Global);
                                FeedCacheKey::Global
                            } else {
                                cache_key.clone()
                            };
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp
                                    .set(Some(last_item.sort_timestamp().as_secs()));
                            }
                            has_more.set(true);
                            feed_state.set(DataState::Loaded(feed_items.clone()));
                            if !is_stale() {
                                let cache_key_for_store = effective_cache_key;
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    if let Err(e) = feed_cache::store_feed_items(
                                            &cache_key_for_store,
                                            &items_for_cache,
                                        )
                                        .await
                                    {
                                        log::warn!("Failed to store feed to cache: {}", e);
                                    }
                                    if let Err(e) = feed_cache::run_eviction_if_needed().await {
                                        log::warn!("Failed to run cache eviction: {}", e);
                                    }
                                });
                            }
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let req_id = request_id;
                                let curr_id = current_id;
                                spawn(async move {
                                    let event_ids: Vec<_> = items_for_counts
                                        .iter()
                                        .map(|item| item.event().id)
                                        .collect();
                                    if event_ids.is_empty() {
                                        interactions_loaded.set(true);
                                        return;
                                    }
                                    let counts = if is_first_load {
                                        fetch_interaction_counts_batch(
                                                event_ids.clone(),
                                                Duration::from_secs(5),
                                            )
                                            .await
                                    } else {
                                        sync_interaction_counts(
                                                event_ids.clone(),
                                                Duration::from_secs(5),
                                            )
                                            .await
                                    };
                                    if *req_id.peek() != curr_id {
                                        return;
                                    }
                                    if let Ok(counts) = counts {
                                        interaction_counts.set(counts);
                                        interactions_loaded.set(true);
                                        match stream_interaction_counts(
                                                event_ids.clone(),
                                                interaction_counts,
                                                Some(600),
                                            )
                                            .await
                                        {
                                            Ok(handle) => {
                                                if *req_id.peek() != curr_id {
                                                    log::debug!("Discarding stale interaction stream handle");
                                                    handle.unsubscribe().await;
                                                    return;
                                                }
                                                interaction_stream_handle.set(Some(handle));
                                            }
                                            Err(e) => {
                                                log::error!(
                                                    "Failed to start interaction stream: {} (event_count={}, cached_count={})",
                                                    e, event_ids.len(), interaction_counts.peek().len()
                                                );
                                            }
                                        }
                                    }
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            if accumulated_items.is_empty() {
                                feed_state.set(DataState::Error(e));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
                FeedType::FollowingWithReplies => {
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let cache_key = FeedCacheKey::FollowingWithReplies {
                        pubkey: pubkey_str,
                    };
                    let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                        .await
                        .unwrap_or_default();
                    if is_stale() {
                        return;
                    }
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for FollowingWithReplies feed",
                            cached_items.len()
                        );
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                    }
                    let result = load_following_with_replies(None).await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok((feed_items, did_fallback)) => {
                            let effective_cache_key = if did_fallback {
                                log::info!("No contacts, switched to Global feed");
                                feed_type.set(FeedType::Global);
                                FeedCacheKey::Global
                            } else {
                                cache_key.clone()
                            };
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp
                                    .set(Some(last_item.sort_timestamp().as_secs()));
                            }
                            has_more.set(true);
                            feed_state.set(DataState::Loaded(feed_items.clone()));
                            if !is_stale() {
                                let cache_key_for_store = effective_cache_key;
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    let _ = feed_cache::store_feed_items(
                                            &cache_key_for_store,
                                            &items_for_cache,
                                        )
                                        .await;
                                    let _ = feed_cache::run_eviction_if_needed().await;
                                });
                            }
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let req_id = request_id;
                                let curr_id = current_id;
                                spawn(async move {
                                    let event_ids: Vec<_> = items_for_counts
                                        .iter()
                                        .map(|item| item.event().id)
                                        .collect();
                                    if event_ids.is_empty() {
                                        interactions_loaded.set(true);
                                        return;
                                    }
                                    let counts = if is_first_load {
                                        fetch_interaction_counts_batch(
                                                event_ids.clone(),
                                                Duration::from_secs(5),
                                            )
                                            .await
                                    } else {
                                        sync_interaction_counts(
                                                event_ids.clone(),
                                                Duration::from_secs(5),
                                            )
                                            .await
                                    };
                                    if *req_id.peek() != curr_id {
                                        return;
                                    }
                                    if let Ok(counts) = counts {
                                        interaction_counts.set(counts);
                                        interactions_loaded.set(true);
                                        match stream_interaction_counts(
                                                event_ids.clone(),
                                                interaction_counts,
                                                Some(600),
                                            )
                                            .await
                                        {
                                            Ok(handle) => {
                                                if *req_id.peek() != curr_id {
                                                    log::debug!("Discarding stale interaction stream handle");
                                                    handle.unsubscribe().await;
                                                    return;
                                                }
                                                interaction_stream_handle.set(Some(handle));
                                            }
                                            Err(e) => {
                                                log::error!(
                                                    "Failed to start interaction stream: {} (event_count={}, cached_count={})",
                                                    e, event_ids.len(), interaction_counts.peek().len()
                                                );
                                            }
                                        }
                                    }
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            if cached_items.is_empty() {
                                feed_state.set(DataState::Error(e));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
                FeedType::Global => {
                    let cache_key = FeedCacheKey::Global;
                    let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                        .await
                        .unwrap_or_default();
                    if is_stale() {
                        return;
                    }
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for Global feed", cached_items
                            .len()
                        );
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                    }
                    let result = load_global_feed(None).await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok(feed_items) => {
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp
                                    .set(Some(last_item.sort_timestamp().as_secs()));
                            }
                            has_more.set(true);
                            feed_state.set(DataState::Loaded(feed_items.clone()));
                            if !is_stale() {
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    let _ = feed_cache::store_feed_items(
                                            &FeedCacheKey::Global,
                                            &items_for_cache,
                                        )
                                        .await;
                                    let _ = feed_cache::run_eviction_if_needed().await;
                                });
                            }
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let req_id = request_id;
                                let curr_id = current_id;
                                spawn(async move {
                                    let event_ids: Vec<_> = items_for_counts
                                        .iter()
                                        .map(|item| item.event().id)
                                        .collect();
                                    if event_ids.is_empty() {
                                        interactions_loaded.set(true);
                                        return;
                                    }
                                    let counts = if is_first_load {
                                        fetch_interaction_counts_batch(
                                                event_ids.clone(),
                                                Duration::from_secs(5),
                                            )
                                            .await
                                    } else {
                                        sync_interaction_counts(
                                                event_ids.clone(),
                                                Duration::from_secs(5),
                                            )
                                            .await
                                    };
                                    if *req_id.peek() != curr_id {
                                        return;
                                    }
                                    if let Ok(counts) = counts {
                                        interaction_counts.set(counts);
                                        interactions_loaded.set(true);
                                        match stream_interaction_counts(
                                                event_ids.clone(),
                                                interaction_counts,
                                                Some(600),
                                            )
                                            .await
                                        {
                                            Ok(handle) => {
                                                if *req_id.peek() != curr_id {
                                                    log::debug!("Discarding stale interaction stream handle");
                                                    handle.unsubscribe().await;
                                                    return;
                                                }
                                                interaction_stream_handle.set(Some(handle));
                                            }
                                            Err(e) => {
                                                log::error!(
                                                    "Failed to start interaction stream: {} (event_count={}, cached_count={})",
                                                    e, event_ids.len(), interaction_counts.peek().len()
                                                );
                                            }
                                        }
                                    }
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            if cached_items.is_empty() {
                                feed_state.set(DataState::Error(e));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
                FeedType::PeopleList(list) => {
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let cache_key = FeedCacheKey::PeopleList {
                        pubkey: pubkey_str,
                        list_id: list.identifier.clone(),
                    };
                    let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                        .await
                        .unwrap_or_default();
                    if is_stale() {
                        return;
                    }
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for PeopleList feed",
                            cached_items.len()
                        );
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                    }
                    let result = load_people_list_feed(&list, None).await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok(feed_items) => {
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp
                                    .set(Some(last_item.sort_timestamp().as_secs()));
                            }
                            has_more.set(true);
                            feed_state.set(DataState::Loaded(feed_items.clone()));
                            if !is_stale() {
                                let cache_key_for_store = cache_key.clone();
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    let _ = feed_cache::store_feed_items(
                                            &cache_key_for_store,
                                            &items_for_cache,
                                        )
                                        .await;
                                    let _ = feed_cache::run_eviction_if_needed().await;
                                });
                            }
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let req_id = request_id;
                                let curr_id = current_id;
                                spawn(async move {
                                    let event_ids: Vec<_> = items_for_counts
                                        .iter()
                                        .map(|item| item.event().id)
                                        .collect();
                                    if event_ids.is_empty() {
                                        interactions_loaded.set(true);
                                        return;
                                    }
                                    let counts = if is_first_load {
                                        fetch_interaction_counts_batch(
                                                event_ids.clone(),
                                                Duration::from_secs(5),
                                            )
                                            .await
                                    } else {
                                        sync_interaction_counts(
                                                event_ids.clone(),
                                                Duration::from_secs(5),
                                            )
                                            .await
                                    };
                                    if *req_id.peek() != curr_id {
                                        return;
                                    }
                                    if let Ok(counts) = counts {
                                        interaction_counts.set(counts);
                                        interactions_loaded.set(true);
                                        match stream_interaction_counts(
                                                event_ids.clone(),
                                                interaction_counts,
                                                Some(600),
                                            )
                                            .await
                                        {
                                            Ok(handle) => {
                                                if *req_id.peek() != curr_id {
                                                    log::debug!("Discarding stale interaction stream handle");
                                                    handle.unsubscribe().await;
                                                    return;
                                                }
                                                interaction_stream_handle.set(Some(handle));
                                            }
                                            Err(e) => {
                                                log::error!(
                                                    "Failed to start interaction stream: {} (event_count={}, cached_count={})",
                                                    e, event_ids.len(), interaction_counts.peek().len()
                                                );
                                            }
                                        }
                                    }
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            if cached_items.is_empty() {
                                feed_state.set(DataState::Error(e));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
            }
        });
    });
    use_effect(move || {
        let _ = feed_type.read();
        let ids = subscription_ids.peek().clone();
        if !ids.is_empty() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    log::info!(
                        "Cleaning up {} real-time subscriptions due to feed type change",
                        ids.len()
                    );
                    subscription_manager::unsubscribe_all(&client, &ids).await;
                }
            });
        }
        subscription_ids.write().clear();
        realtime_started.set(false);
        if let Some(handle) = interaction_stream_handle.peek().clone() {
            spawn(async move {
                log::info!("Cleaning up interaction stream due to feed type change");
                handle.unsubscribe().await;
            });
        }
        interaction_stream_handle.set(None);
    });
    use_effect(move || {
        let current_feed_type = feed_type.read().clone();
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !is_authenticated || !client_initialized {
            return;
        }
        if *realtime_started.read() {
            return;
        }
        let since_timestamp = match &*feed_state.peek() {
            DataState::Loaded(ref items) => {
                if let Some(latest_item) = items.first() {
                    latest_item.sort_timestamp()
                } else {
                    Timestamp::now()
                }
            }
            _ => {
                return;
            }
        };
        realtime_started.set(true);
        spawn(async move {
            let pubkey_str = match auth_store::get_pubkey() {
                Some(pk) => pk,
                None => return,
            };
            let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
                Ok(c) => c,
                Err(e) => {
                    log::error!(
                        "Failed to fetch contacts for real-time subscription: {}", e
                    );
                    return;
                }
            };
            if contacts.is_empty() {
                log::info!("No contacts to subscribe to for real-time updates");
                return;
            }
            let authors: Vec<PublicKey> = contacts
                .iter()
                .filter_map(|contact| PublicKey::parse(contact).ok())
                .collect();
            if authors.is_empty() {
                return;
            }
            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => return,
            };
            const BATCH_SIZE: usize = 50;
            const BATCH_DELAY_MS: u64 = 100;
            let total_authors = authors.len();
            let num_batches = total_authors.div_ceil(BATCH_SIZE);
            log::info!(
                "Starting batched real-time subscription for {} followed users in {} batches using gossip",
                contacts.len(), num_batches
            );
            for (batch_idx, author_batch) in authors.chunks(BATCH_SIZE).enumerate() {
                let batch_authors = author_batch.to_vec();
                let client = client.clone();
                let batch_num = batch_idx + 1;
                if batch_idx > 0 {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use gloo_timers::future::TimeoutFuture;
                        TimeoutFuture::new(batch_idx as u32 * BATCH_DELAY_MS as u32)
                            .await;
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        use tokio::time::{sleep, Duration as TokioDuration};
                        sleep(
                                TokioDuration::from_millis(
                                    batch_idx as u64 * BATCH_DELAY_MS,
                                ),
                            )
                            .await;
                    }
                }
                let filter = Filter::new()
                    .kinds(vec![Kind::TextNote, Kind::Repost])
                    .authors(batch_authors.clone())
                    .since(since_timestamp)
                    .limit(0);
                log::info!(
                    "Subscribing to batch {}/{} ({} authors)", batch_num, num_batches,
                    batch_authors.len()
                );
                match client.subscribe(filter, None).await {
                    Ok(output) => {
                        let subscription_id = output.val;
                        log::info!(
                            "Batch {}/{} subscribed: {:?}", batch_num, num_batches,
                            subscription_id
                        );
                        subscription_ids.write().push(subscription_id.clone());
                        let client_for_notifications = client.clone();
                        let batch_feed_type = current_feed_type.clone();
                        let batch_author_set: std::collections::HashSet<_> = batch_authors
                            .iter()
                            .cloned()
                            .collect();
                        spawn(async move {
                            let mut notifications = client_for_notifications
                                .notifications();
                            while let Ok(notification) = notifications.recv().await {
                                if let nostr_sdk::RelayPoolNotification::Event {
                                    subscription_id: event_sub_id,
                                    event,
                                    ..
                                } = notification {
                                    if event_sub_id != subscription_id {
                                        continue;
                                    }
                                    if !batch_author_set.contains(&event.pubkey) {
                                        log::debug!(
                                            "Filtered out event from non-followed author: {}", event
                                            .pubkey
                                        );
                                        continue;
                                    }
                                    let feed_item_opt = if event.kind == Kind::Repost {
                                        match extract_reposted_event(&event) {
                                            Ok(original) => {
                                                Some(FeedItem::Repost {
                                                    original,
                                                    reposted_by: event.pubkey,
                                                    repost_timestamp: event.created_at,
                                                })
                                            }
                                            Err(e) => {
                                                log::warn!(
                                                    "Failed to parse repost event {}: {}", event.id, e
                                                );
                                                None
                                            }
                                        }
                                    } else if event.kind == Kind::TextNote {
                                        let should_add = match &batch_feed_type {
                                            FeedType::Following => {
                                                !event
                                                    .tags
                                                    .iter()
                                                    .any(|tag| { tag.kind() == nostr_sdk::TagKind::e() })
                                            }
                                            FeedType::FollowingWithReplies
                                            | FeedType::Global
                                            | FeedType::PeopleList(_) => true,
                                        };
                                        if should_add {
                                            Some(FeedItem::OriginalPost((*event).clone()))
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };
                                    if let Some(feed_item) = feed_item_opt {
                                        log::info!(
                                            "New post received in real-time from batch {}", batch_num
                                        );
                                        let event_id = feed_item.event().id;
                                        let already_buffered = pending_posts
                                            .read()
                                            .iter()
                                            .any(|item| item.event().id == event_id);
                                        let already_in_feed = match &*feed_state.peek() {
                                            DataState::Loaded(ref current_items) => {
                                                current_items.iter().any(|item| item.event().id == event_id)
                                            }
                                            _ => false,
                                        };
                                        if !already_buffered && !already_in_feed {
                                            let author_pk = feed_item.event().pubkey.to_hex();
                                            spawn(async move {
                                                let _ = crate::stores::profiles::fetch_profile(author_pk)
                                                    .await;
                                            });
                                            if let FeedItem::Repost { ref original, .. } = feed_item {
                                                let original_author_pk = original.pubkey.to_hex();
                                                spawn(async move {
                                                    let _ = crate::stores::profiles::fetch_profile(
                                                            original_author_pk,
                                                        )
                                                        .await;
                                                });
                                            }
                                            pending_posts.write().push(feed_item);
                                            log::info!(
                                                "Buffered new post, total pending: {}", pending_posts.read()
                                                .len()
                                            );
                                        }
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to subscribe batch {}/{}: {}", batch_num,
                            num_batches, e
                        );
                    }
                }
            }
        });
    });
    let load_more = move || {
        log::info!(
            "load_more called - pagination_loading: {}, has_more: {}", *
            pagination_loading.peek(), * has_more.peek()
        );
        if *pagination_loading.peek() || !*has_more.peek() {
            log::info!("load_more blocked by guards");
            return;
        }
        log::info!("load_more setting pagination_loading to true and spawning");
        pagination_loading.set(true);
        spawn(async move {
            let until = *oldest_timestamp.read();
            let current_feed_type = feed_type.read().clone();
            log::info!(
                "load_more spawn executing - until: {:?}, feed_type: {:?}", until,
                current_feed_type
            );
            let fetch_result: Result<Vec<FeedItem>, String> = match current_feed_type {
                FeedType::Following => {
                    match load_following_feed(until).await {
                        Ok((items, did_fallback)) => {
                            if did_fallback {
                                Err("Contact fetch failed during pagination".to_string())
                            } else {
                                Ok(items)
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
                FeedType::FollowingWithReplies => {
                    match load_following_with_replies(until).await {
                        Ok((items, did_fallback)) => {
                            if did_fallback {
                                Err("Contact fetch failed during pagination".to_string())
                            } else {
                                Ok(items)
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
                FeedType::Global => load_global_feed(until).await,
                FeedType::PeopleList(list) => load_people_list_feed(&list, until).await,
            };
            match fetch_result {
                Ok(new_items) => {
                    append_paginated_items(
                            new_items,
                            &mut feed_state,
                            &mut oldest_timestamp,
                            &mut has_more,
                            &mut pagination_loading,
                            &mut interaction_counts,
                        )
                        .await;
                }
                Err(e) => {
                    log::error!("Failed to load more events: {}", e);
                    pagination_loading.set(false);
                }
            }
        });
    };
    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);
    let mut refresh_and_scroll_to_top = move || {
        let current = *refresh_trigger.read();
        refresh_trigger.set(current + 1);
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                window.scroll_to_with_x_and_y(0.0, 0.0);
            }
        }
    };
    let auth = auth_store::AUTH_STATE.read();
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    if auth.is_authenticated {
                        div { class: "relative",
                            button {
                                class: "text-xl font-bold flex items-center gap-2 hover:bg-accent px-3 py-1 rounded-lg transition",
                                onclick: move |_| {
                                    let current = *show_dropdown.read();
                                    show_dropdown.set(!current);
                                },
                                "{feed_type.read().label()}"
                                span { class: "text-sm",
                                    if *show_dropdown.read() {
                                        "▲"
                                    } else {
                                        "▼"
                                    }
                                }
                            }
                            if *show_dropdown.read() {
                                div { class: "absolute top-full left-0 mt-2 bg-card border border-border rounded-lg shadow-lg min-w-[200px] overflow-hidden z-30",
                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::Following);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div { class: "font-medium", "Following" }
                                            div { class: "text-xs text-muted-foreground",
                                                "Top level posts only"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::Following {
                                            span { "✓" }
                                        }
                                    }
                                    div { class: "border-t border-border" }
                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::FollowingWithReplies);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div { class: "font-medium", "Following + Replies" }
                                            div { class: "text-xs text-muted-foreground",
                                                "All posts including replies"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::FollowingWithReplies {
                                            span { "✓" }
                                        }
                                    }
                                    div { class: "border-t border-border" }
                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::Global);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div { class: "font-medium", "Global" }
                                            div { class: "text-xs text-muted-foreground",
                                                "Posts from everyone"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::Global {
                                            span { "✓" }
                                        }
                                    }
                                    if !people_lists.read().is_empty() {
                                        div { class: "border-t border-border" }
                                        div { class: "px-4 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wide",
                                            "Your Lists"
                                        }
                                        for list in people_lists.read().iter() {
                                            {
                                                let list_for_select = list.clone();
                                                let list_for_check = list.clone();
                                                let display_count = match list.total_member_count {
                                                    Some(count) => count.to_string(),
                                                    None if list.has_private_content => {
                                                        format!("{}+", get_item_count(&list.tags))
                                                    }
                                                    None => get_item_count(&list.tags).to_string(),
                                                };
                                                rsx! {
                                                    button {
                                                        key: "{list.id}",
                                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                                        onclick: move |_| {
                                                            feed_type.set(FeedType::PeopleList(Box::new(list_for_select.clone())));
                                                            show_dropdown.set(false);
                                                        },
                                                        div {
                                                            div { class: "font-medium flex items-center gap-2",
                                                                "👥 {list.name}"
                                                                if list.has_private_content {
                                                                    span { class: "text-sm", title: "Has private members", "🔒" }
                                                                }
                                                            }
                                                            div { class: "text-xs text-muted-foreground", "{display_count} members" }
                                                        }
                                                        if matches!(
                                                            feed_type.read().clone(),
                                                            FeedType::PeopleList(ref l)
                                                            if l.id == list_for_check.id
                                                        )
                                                        {
                                                            span { "✓" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        h2 { class: "text-xl font-bold", "Home" }
                    }
                    if auth.is_authenticated {
                        button {
                            class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                            disabled: feed_state.read().is_loading(),
                            onclick: move |_| refresh_and_scroll_to_top(),
                            title: "Refresh feed",
                            if feed_state.read().is_loading() {
                                span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                            } else {
                                "🔄"
                            }
                        }
                    }
                }
            }
            if auth.is_authenticated {
                NoteComposer {}
            }
            if !auth.is_authenticated {
                div { class: "border-b border-border p-6 bg-blue-50 dark:bg-blue-900/20",
                    div { class: "max-w-md mx-auto text-center",
                        h3 { class: "text-lg font-bold mb-2", "Welcome to nostr.blue" }
                        p { class: "text-muted-foreground mb-4",
                            "Connect your account to see your feed"
                        }
                    }
                }
            }
            div {
                if !auth.is_authenticated {
                    LoginSection {}
                } else if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if feed_state.read().is_pending() || feed_state.read().is_loading() {
                    div { class: "divide-y divide-border",
                        for i in 0..5 {
                            NoteCardSkeleton { key: "{i}" }
                        }
                    }
                } else if let Some(err) = feed_state.read().error() {
                    div { class: "p-6 text-center",
                        div { class: "max-w-md mx-auto",
                            div { class: "text-4xl mb-2", "⚠️" }
                            p { class: "text-red-600 dark:text-red-400", "Error loading feed: {err}" }
                        }
                    }
                } else if let Some(feed_items) = feed_state.read().data() {
                    if feed_items.is_empty() {
                        div { class: "p-6 text-center text-gray-500 dark:text-gray-400",
                            div { class: "max-w-md mx-auto space-y-4",
                                div { class: "text-4xl mb-2", "📝" }
                                h3 { class: "text-lg font-semibold text-gray-700 dark:text-gray-300",
                                    "No posts yet"
                                }
                                p { class: "text-sm", "Posts from the network will appear here" }
                            }
                        }
                    } else {
                        if *pending_count.read() > 0 {
                            {
                                let count = *pending_count.read();
                                let post_text = if count == 1 { "post" } else { "posts" };
                                rsx! {
                                    div {
                                        class: "sticky top-[57px] z-10 border-b border-border bg-blue-500 hover:bg-blue-600 transition-colors cursor-pointer",
                                        onclick: move |_| refresh_and_scroll_to_top(),
                                        div { class: "px-4 py-3 text-center",
                                            span { class: "text-white font-medium", "Show {count} new {post_text}" }
                                        }
                                    }
                                }
                            }
                        }
                        for feed_item in feed_items.iter() {
                            {
                                let event = feed_item.event();
                                let repost_info = feed_item.repost_info();
                                if event.kind == Kind::LongFormTextNote {
                                    rsx! {
                                        ArticleCard { key: "{event.id}", event: event.clone() }
                                    }
                                } else {
                                    rsx! {
                                        NoteCard {
                                            key: "{event.id}",
                                            event: event.clone(),
                                            repost_info,
                                            precomputed_counts: interaction_counts.read().get(&event.id.to_hex()).cloned(),
                                            collapsible: true,
                                            cached_muted_posts: cached_muted_posts.read().clone(),
                                            cached_blocked_users: cached_blocked_users.read().clone(),
                                        }
                                    }
                                }
                            }
                        }
                        if *has_more.read() {
                            div {
                                id: "{sentinel_id}",
                                class: "p-8 flex justify-center",
                                if *pagination_loading.read() {
                                    span { class: "flex items-center gap-2 text-muted-foreground",
                                        span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                        "Loading more..."
                                    }
                                }
                            }
                        } else {
                            div { class: "p-8 text-center text-muted-foreground",
                                "You've reached the end"
                            }
                        }
                    }
                }
            }
        }
    }
}
#[component]
fn HelpModal(on_close: EventHandler<()>) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center p-4 z-50",
            onclick: move |_| on_close.call(()),
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),
                div { class: "sticky top-0 bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 px-6 py-4 flex items-center justify-between",
                    h3 { class: "text-xl font-bold text-gray-900 dark:text-white",
                        "About Nostr Sign-In Methods"
                    }
                    button {
                        class: "text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 text-2xl",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }
                div { class: "px-6 py-4 space-y-6",
                    div {
                        h4 { class: "font-semibold text-gray-900 dark:text-white mb-2",
                            "What is Nostr?"
                        }
                        p { class: "text-sm text-gray-600 dark:text-gray-400",
                            "Nostr is a decentralized social protocol where you own your identity and data. Instead of relying on a company, your identity is based on cryptographic keys that only you control."
                        }
                    }
                    div {
                        h4 { class: "font-semibold text-gray-900 dark:text-white mb-2 flex items-center gap-2",
                            "🔌 Browser Extension (NIP-07)"
                            span { class: "px-2 py-0.5 text-xs bg-green-600 text-white rounded-full",
                                "RECOMMENDED"
                            }
                        }
                        p { class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                            "Browser extensions like Alby, nos2x, and Flamingo store your keys securely and sign events on your behalf. Your private key never leaves the extension."
                        }
                        ul { class: "text-sm text-gray-600 dark:text-gray-400 list-disc list-inside space-y-1",
                            li { "Keys stored securely in the extension" }
                            li { "Websites can't access your private key" }
                            li { "Works across all Nostr apps" }
                            li { "You control which actions to approve" }
                        }
                    }
                    div {
                        h4 { class: "font-semibold text-gray-900 dark:text-white mb-2 flex items-center gap-2",
                            "🔐 Remote Signer (NIP-46)"
                            span { class: "px-2 py-0.5 text-xs bg-blue-600 text-white rounded-full",
                                "RECOMMENDED"
                            }
                        }
                        p { class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                            "Remote signers let you keep your keys on a separate device (like your phone with Amber) or a dedicated service (like nsecBunker). This app connects to your signer and requests signatures remotely."
                        }
                        ul { class: "text-sm text-gray-600 dark:text-gray-400 list-disc list-inside space-y-1",
                            li { "Keys stay on your signing device" }
                            li { "Approve each action on your phone" }
                            li { "Compatible signers: Amber (Android), nsecBunker" }
                            li { "Most secure for untrusted devices" }
                        }
                        p { class: "text-xs text-blue-600 dark:text-blue-400 mt-2",
                            "To use: Get a bunker:// URI from your signing app and paste it above."
                        }
                    }
                    div {
                        h4 { class: "font-semibold text-gray-900 dark:text-white mb-2 flex items-center gap-2",
                            "🔑 Private Key (nsec)"
                            span { class: "px-2 py-0.5 text-xs bg-yellow-600 text-white rounded-full",
                                "USE WITH CAUTION"
                            }
                        }
                        p { class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                            "Entering your private key (nsec) directly gives this app full access to your account. Your key is stored in browser localStorage."
                        }
                        ul { class: "text-sm text-gray-600 dark:text-gray-400 list-disc list-inside space-y-1",
                            li { "⚠️ Only use on devices you fully trust" }
                            li { "⚠️ Never share your nsec with anyone" }
                            li { "⚠️ Stored in browser (cleared if you clear data)" }
                            li { "Can be compromised if device is compromised" }
                        }
                    }
                    div {
                        h4 { class: "font-semibold text-gray-900 dark:text-white mb-2",
                            "👁️ Public Key (npub) - Read Only"
                        }
                        p { class: "text-sm text-gray-600 dark:text-gray-400",
                            "Using just your public key (npub) lets you browse and view content, but you cannot post, react, or send messages. Perfect for exploring Nostr without committing."
                        }
                    }
                    div {
                        h4 { class: "font-semibold text-gray-900 dark:text-white mb-2",
                            "🛡️ Security Best Practices"
                        }
                        ul { class: "text-sm text-gray-600 dark:text-gray-400 list-disc list-inside space-y-1",
                            li { "Always prefer browser extensions or remote signers" }
                            li { "Never enter your nsec on untrusted websites" }
                            li { "Backup your keys securely (offline)" }
                            li { "Use different keys for testing and main account" }
                        }
                    }
                }
                div { class: "sticky bottom-0 bg-gray-50 dark:bg-gray-900 border-t border-gray-200 dark:border-gray-700 px-6 py-4",
                    button {
                        class: "w-full px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition",
                        onclick: move |_| on_close.call(()),
                        "Got it!"
                    }
                }
            }
        }
    }
}
#[component]
fn LoginSection() -> Element {
    use nostr::ToBech32;
    let mut nsec_input = use_signal(String::new);
    let mut npub_input = use_signal(String::new);
    let mut bunker_uri_input = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut show_advanced = use_signal(|| false);
    let mut show_help_modal = use_signal(|| false);
    let mut connecting_bunker = use_signal(|| false);
    let mut nsec_password = use_signal(String::new);
    let mut nsec_confirm_password = use_signal(String::new);
    let mut show_nsec_password = use_signal(|| false);
    let login_with_nsec = move |_| {
        let nsec = nsec_input.read().clone();
        let password = nsec_password.read().clone();
        let confirm = nsec_confirm_password.read().clone();
        if password != confirm {
            error.set(Some("Passwords do not match".to_string()));
            return;
        }
        if let Some(err) = crate::utils::nip49::validate_password(&password) {
            error.set(Some(err));
            return;
        }
        spawn(async move {
            match auth_store::login_with_nsec(&nsec, &password).await {
                Ok(_) => error.set(None),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    let login_with_npub = move |_| {
        let npub = npub_input.read().clone();
        spawn(async move {
            match auth_store::login_with_npub(&npub).await {
                Ok(_) => error.set(None),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    let login_with_bunker = move |_| {
        let uri = bunker_uri_input.read().clone();
        connecting_bunker.set(true);
        error.set(None);
        spawn(async move {
            match auth_store::login_with_nostr_connect(&uri).await {
                Ok(_) => {
                    bunker_uri_input.set(String::new());
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            connecting_bunker.set(false);
        });
    };
    let generate_new = move |_| {
        let keys = auth_store::generate_keys();
        let nsec = keys.secret_key().to_bech32().unwrap();
        nsec_input.set(nsec);
    };
    let login_with_extension = move |_| {
        spawn(async move {
            match auth_store::login_with_browser_extension().await {
                Ok(_) => error.set(None),
                Err(e) => error.set(Some(e)),
            }
        });
    };
    let has_extension = auth_store::is_browser_extension_available();
    rsx! {
        div { class: "p-6 max-w-lg mx-auto",
            div { class: "flex items-center justify-between mb-6",
                h3 { class: "text-2xl font-bold text-gray-900 dark:text-white", "Welcome to Nostr" }
                button {
                    class: "px-3 py-1.5 text-sm bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 hover:bg-blue-200 dark:hover:bg-blue-800 rounded-lg transition",
                    onclick: move |_| show_help_modal.set(true),
                    "Learn More"
                }
            }
            p { class: "text-gray-600 dark:text-gray-400 mb-6",
                "Choose a secure sign-in method to get started with the decentralized social network."
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "mb-4 p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg text-sm",
                    "❌ {err}"
                }
            }
            div { class: "mb-6",
                h4 { class: "text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-3",
                    "Recommended (Secure)"
                }
                div { class: "space-y-3",
                    if has_extension {
                        div { class: "p-4 bg-gradient-to-r from-green-50 to-emerald-50 dark:from-green-900/20 dark:to-emerald-900/20 rounded-lg border-2 border-green-300 dark:border-green-700",
                            div { class: "flex items-start gap-3 mb-3",
                                div { class: "text-2xl", "🔌" }
                                div { class: "flex-1",
                                    div { class: "flex items-center gap-2 mb-1",
                                        span { class: "font-semibold text-gray-900 dark:text-white",
                                            "Browser Extension"
                                        }
                                        span { class: "px-2 py-0.5 text-xs bg-green-600 text-white rounded-full",
                                            "RECOMMENDED"
                                        }
                                    }
                                    p { class: "text-sm text-gray-600 dark:text-gray-400",
                                        "Your keys stay in the extension, never exposed to websites."
                                    }
                                }
                            }
                            button {
                                class: "w-full px-4 py-2.5 bg-green-600 hover:bg-green-700 text-white rounded-lg font-medium transition shadow-xs",
                                onclick: login_with_extension,
                                "Connect Extension"
                            }
                        }
                    }
                    div { class: "p-4 bg-gradient-to-r from-blue-50 to-indigo-50 dark:from-blue-900/20 dark:to-indigo-900/20 rounded-lg border-2 border-blue-300 dark:border-blue-700",
                        div { class: "flex items-start gap-3 mb-3",
                            div { class: "text-2xl", "🔐" }
                            div { class: "flex-1",
                                div { class: "flex items-center gap-2 mb-1",
                                    span { class: "font-semibold text-gray-900 dark:text-white",
                                        "Remote Signer"
                                    }
                                    span { class: "px-2 py-0.5 text-xs bg-blue-600 text-white rounded-full",
                                        "RECOMMENDED"
                                    }
                                }
                                p { class: "text-sm text-gray-600 dark:text-gray-400",
                                    "Use Amber, nsecBunker, or other NIP-46 signers. Keys never leave your device."
                                }
                            }
                        }
                        div { class: "space-y-2",
                            input {
                                class: "w-full px-3 py-2 text-sm border border-blue-300 dark:border-blue-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                r#type: "text",
                                placeholder: "bunker://...",
                                value: "{bunker_uri_input}",
                                oninput: move |evt| bunker_uri_input.set(evt.value()),
                                disabled: *connecting_bunker.read(),
                            }
                            button {
                                class: "w-full px-4 py-2.5 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition shadow-xs disabled:opacity-50 disabled:cursor-not-allowed",
                                onclick: login_with_bunker,
                                disabled: bunker_uri_input.read().is_empty() || *connecting_bunker.read(),
                                if *connecting_bunker.read() {
                                    "Connecting..."
                                } else {
                                    "Connect Remote Signer"
                                }
                            }
                            if *connecting_bunker.read() {
                                p { class: "text-xs text-blue-700 dark:text-blue-400 text-center",
                                    "Waiting for approval on your signing device (up to 2 minutes)..."
                                }
                            }
                        }
                    }
                }
            }
            div { class: "border-t border-gray-200 dark:border-gray-700 pt-6",
                button {
                    class: "w-full flex items-center justify-between p-3 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg transition",
                    onclick: move |_| {
                        let current = *show_advanced.read();
                        show_advanced.set(!current);
                    },
                    div { class: "flex items-center gap-2",
                        span { class: "text-yellow-600 dark:text-yellow-400", "⚠️" }
                        span { class: "font-medium text-gray-900 dark:text-white", "Advanced Options" }
                    }
                    span { class: "text-gray-500",
                        if *show_advanced.read() {
                            "▼"
                        } else {
                            "▶"
                        }
                    }
                }
                if *show_advanced.read() {
                    div { class: "mt-4 p-4 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-700 rounded-lg space-y-4",
                        div { class: "p-3 bg-yellow-100 dark:bg-yellow-900/30 rounded-lg",
                            p { class: "text-sm text-yellow-800 dark:text-yellow-300 font-medium",
                                "⚠️ Security Warning"
                            }
                            p { class: "text-xs text-yellow-700 dark:text-yellow-400 mt-1",
                                "These methods store keys in your browser. Only use on devices you fully trust."
                            }
                        }
                        div {
                            h5 { class: "font-medium text-gray-900 dark:text-white mb-2 text-sm",
                                "🔑 Private Key (nsec)"
                            }
                            div { class: "space-y-2",
                                input {
                                    class: "w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white",
                                    r#type: "password",
                                    placeholder: "nsec1...",
                                    value: "{nsec_input}",
                                    oninput: move |evt| nsec_input.set(evt.value()),
                                }
                                div { class: "relative",
                                    input {
                                        class: "w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white pr-10",
                                        r#type: if *show_nsec_password.read() { "text" } else { "password" },
                                        placeholder: "Set encryption password",
                                        value: "{nsec_password}",
                                        oninput: move |evt| nsec_password.set(evt.value()),
                                    }
                                    button {
                                        class: "absolute right-2 top-1/2 -translate-y-1/2 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200",
                                        r#type: "button",
                                        onclick: move |_| {
                                            let current = *show_nsec_password.read();
                                            show_nsec_password.set(!current);
                                        },
                                        if *show_nsec_password.read() {
                                            "🙈"
                                        } else {
                                            "👁️"
                                        }
                                    }
                                }
                                input {
                                    class: "w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white",
                                    r#type: if *show_nsec_password.read() { "text" } else { "password" },
                                    placeholder: "Confirm password",
                                    value: "{nsec_confirm_password}",
                                    oninput: move |evt| nsec_confirm_password.set(evt.value()),
                                }
                                p { class: "text-xs text-amber-600 dark:text-amber-400",
                                    "🔐 Your key will be encrypted with this password"
                                }
                                div { class: "flex gap-2",
                                    button {
                                        class: "flex-1 px-3 py-2 text-sm bg-gray-700 hover:bg-gray-800 dark:bg-gray-600 dark:hover:bg-gray-700 text-white rounded-lg transition disabled:opacity-50 disabled:cursor-not-allowed",
                                        onclick: login_with_nsec,
                                        disabled: nsec_input.read().is_empty() || nsec_password.read().is_empty()
                                            || nsec_confirm_password.read().is_empty(),
                                        "Login"
                                    }
                                    button {
                                        class: "px-3 py-2 text-sm bg-gray-600 hover:bg-gray-700 text-white rounded-lg transition",
                                        onclick: generate_new,
                                        "Generate"
                                    }
                                }
                            }
                        }
                        div {
                            h5 { class: "font-medium text-gray-900 dark:text-white mb-2 text-sm",
                                "👁️ Public Key (npub) - Read Only"
                            }
                            div { class: "space-y-2",
                                input {
                                    class: "w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white",
                                    r#type: "text",
                                    placeholder: "npub1...",
                                    value: "{npub_input}",
                                    oninput: move |evt| npub_input.set(evt.value()),
                                }
                                button {
                                    class: "w-full px-3 py-2 text-sm bg-gray-700 hover:bg-gray-800 dark:bg-gray-600 dark:hover:bg-gray-700 text-white rounded-lg transition",
                                    onclick: login_with_npub,
                                    "View Profile (Read-Only)"
                                }
                                p { class: "text-xs text-gray-600 dark:text-gray-400",
                                    "ℹ️ You can browse but cannot post or interact."
                                }
                            }
                        }
                    }
                }
            }
            if *show_help_modal.read() {
                HelpModal { on_close: move |_| show_help_modal.set(false) }
            }
        }
    }
}
#[component]
fn ProfileSection() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    rsx! {
        div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
            div { class: "flex justify-between items-start mb-4",
                h3 { class: "text-xl font-semibold text-gray-900 dark:text-white",
                    "👤 Your Profile"
                }
                button {
                    class: "px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg font-medium transition",
                    onclick: move |_| {
                        let nav = navigator();
                        spawn(async move {
                            auth_store::logout().await;
                            nav.push(Route::Home { list: String::new() });
                        });
                    },
                    "Logout"
                }
            }
            div { class: "space-y-3",
                div { class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                    p { class: "text-sm text-gray-600 dark:text-gray-400 mb-1", "Public Key" }
                    if let Some(pubkey) = &auth.pubkey {
                        Link {
                            to: Route::Profile {
                                pubkey: pubkey.clone(),
                            },
                            class: "font-mono text-sm text-blue-600 dark:text-blue-400 hover:underline break-all",
                            "{pubkey}"
                        }
                    }
                }
                div { class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                    p { class: "text-sm text-gray-600 dark:text-gray-400 mb-1", "Login Method" }
                    p { class: "text-gray-900 dark:text-white",
                        match auth.login_method {
                            Some(auth_store::LoginMethod::PrivateKey) => "🔑 Private Key",
                            Some(auth_store::LoginMethod::ReadOnly) => "👁️ Read-Only",
                            Some(auth_store::LoginMethod::BrowserExtension) => "🔌 Browser Extension",
                            Some(auth_store::LoginMethod::RemoteSigner) => "🔐 Remote Signer",
                            None => "Unknown",
                        }
                    }
                }
            }
        }
    }
}
/// Helper function to append paginated items to the feed
/// Handles deduplication, timestamp updates, and metadata prefetching
async fn append_paginated_items(
    new_items: Vec<FeedItem>,
    feed_state: &mut Signal<DataState<Vec<FeedItem>>>,
    oldest_timestamp: &mut Signal<Option<u64>>,
    has_more: &mut Signal<bool>,
    pagination_loading: &mut Signal<bool>,
    interaction_counts: &mut Signal<HashMap<String, InteractionCounts>>,
) {
    if new_items.is_empty() {
        log::info!("No more items from relay, reached end of feed");
        has_more.set(false);
        pagination_loading.set(false);
        return;
    }
    let current_state = feed_state.read().clone();
    if let DataState::Loaded(current) = current_state {
        let existing_ids: std::collections::HashSet<_> = current
            .iter()
            .map(|item| item.event().id)
            .collect();
        let unique_items: Vec<_> = new_items
            .iter()
            .filter(|item| !existing_ids.contains(&item.event().id))
            .cloned()
            .collect();
        log::info!(
            "Deduplication: {} total, {} unique items after filtering", new_items.len(),
            unique_items.len()
        );
        if let Some(last_item) = new_items.last() {
            let ts = last_item.sort_timestamp().as_secs().saturating_sub(1);
            oldest_timestamp.set(Some(ts));
        }
        if !unique_items.is_empty() {
            let prefetch_items = unique_items.clone();
            let items_for_counts = unique_items.clone();
            let mut updated = current;
            updated.extend(unique_items);
            feed_state.set(DataState::Loaded(updated));
            spawn(async move {
                prefetch_author_metadata(&prefetch_items).await;
            });
            let mut counts_signal = *interaction_counts;
            spawn(async move {
                let event_ids: Vec<_> = items_for_counts
                    .iter()
                    .map(|item| item.event().id)
                    .collect();
                if let Ok(new_counts) = fetch_interaction_counts_batch(
                        event_ids,
                        Duration::from_secs(5),
                    )
                    .await
                {
                    counts_signal.extend(new_counts);
                    log::info!(
                        "Fetched interaction counts for {} paginated items",
                        items_for_counts.len()
                    );
                }
            });
        }
    }
    pagination_loading.set(false);
}
/// Load following feed (non-streaming version, used for pagination)
/// Returns (feed_items, did_fallback) where did_fallback indicates if we fell back to global.
async fn load_following_feed(
    until: Option<u64>,
) -> Result<(Vec<FeedItem>, bool), String> {
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    log::info!("Loading following feed for {} (until: {:?})", pubkey_str, until);
    let contacts_future = nostr_client::fetch_contacts(pubkey_str.clone());
    let global_future = load_global_feed(until);
    let (contacts_result, global_result) = futures::join!(
        contacts_future, global_future
    );
    let contacts = match contacts_result {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            let global = global_result?;
            return Ok((global, true));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        let global = global_result?;
        return Ok((global, true));
    }
    log::info!("User follows {} accounts", contacts.len());
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }
    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let global = load_global_feed(until).await?;
        return Ok((global, true));
    }
    let mut filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .authors(authors)
        .limit(100);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    log::info!(
        "Fetching events from {} followed accounts", filter.authors.as_ref().map(| a | a
        .len()).unwrap_or(0)
    );
    match nostr_client::fetch_events_from_connected_relays(
            filter,
            Duration::from_secs(10),
        )
        .await
    {
        Ok(events) => {
            let raw_count = events.len();
            log::info!(
                "Loaded {} events (including reposts) from following feed via outbox",
                raw_count
            );
            let mut feed_items: Vec<FeedItem> = Vec::new();
            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            feed_items
                                .push(FeedItem::Repost {
                                    original,
                                    reposted_by: event.pubkey,
                                    repost_timestamp: event.created_at,
                                });
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to parse repost event {}: {}", event.id, e
                            );
                        }
                    }
                } else if event.kind == Kind::TextNote {
                    use nostr_sdk::TagKind;
                    let is_reply = event
                        .tags
                        .iter()
                        .any(|tag| tag.kind() == TagKind::e());
                    if !is_reply {
                        feed_items.push(FeedItem::OriginalPost(event));
                    }
                }
            }
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
            log::info!(
                "After processing: {} feed items (raw: {})", feed_items.len(), raw_count
            );
            if feed_items.is_empty() {
                log::info!("No posts from followed users");
                return Ok((Vec::new(), false));
            }
            Ok((feed_items, false))
        }
        Err(e) => {
            log::error!("Failed to fetch following feed: {}, falling back to global", e);
            let global = load_global_feed(until).await?;
            Ok((global, true))
        }
    }
}
/// Process raw events into FeedItems (extracted for reuse in streaming)
fn process_events_to_feed_items(events: Vec<nostr_sdk::Event>) -> Vec<FeedItem> {
    use nostr_sdk::TagKind;
    let mut feed_items: Vec<FeedItem> = Vec::new();
    for event in events.into_iter() {
        if event.kind == Kind::Repost {
            match extract_reposted_event(&event) {
                Ok(original) => {
                    feed_items
                        .push(FeedItem::Repost {
                            original,
                            reposted_by: event.pubkey,
                            repost_timestamp: event.created_at,
                        });
                }
                Err(e) => {
                    log::warn!("Failed to parse repost event {}: {}", event.id, e);
                }
            }
        } else if event.kind == Kind::TextNote {
            let is_reply = event.tags.iter().any(|tag| tag.kind() == TagKind::e());
            if !is_reply {
                feed_items.push(FeedItem::OriginalPost(event));
            }
        }
    }
    feed_items
}
/// Load following feed with progressive streaming updates
///
/// This function streams events as they arrive and calls the on_batch callback
/// with each batch of FeedItems, enabling progressive UI updates.
/// Returns (feed_items, did_fallback) where did_fallback indicates if we fell back to global.
async fn load_following_feed_streaming<F>(
    until: Option<u64>,
    mut on_batch: F,
) -> Result<(Vec<FeedItem>, bool), String>
where
    F: FnMut(Vec<FeedItem>),
{
    use std::collections::HashSet;
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    log::info!(
        "Loading following feed (streaming) for {} (until: {:?})", pubkey_str, until
    );
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            let global = load_global_feed(until).await?;
            return Ok((global, true));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        let global = load_global_feed(until).await?;
        return Ok((global, true));
    }
    log::info!("User follows {} accounts, streaming posts", contacts.len());
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }
    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let global = load_global_feed(until).await?;
        return Ok((global, true));
    }
    let mut filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .authors(authors)
        .limit(100);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let mut all_items: Vec<FeedItem> = Vec::new();
    let mut seen_ids: HashSet<nostr_sdk::EventId> = HashSet::new();
    // Use immediate streaming for faster time-to-first-post
    let stream_result = nostr_client::stream_events_immediate(
            filter,
            Duration::from_secs(10),
            |event| {
                use nostr_sdk::TagKind;
                // Process single event immediately
                let item = if event.kind == Kind::Repost {
                    match extract_reposted_event(&event) {
                        Ok(original) => Some(FeedItem::Repost {
                            original,
                            reposted_by: event.pubkey,
                            repost_timestamp: event.created_at,
                        }),
                        Err(_) => None,
                    }
                } else if event.kind == Kind::TextNote {
                    // Filter out replies (posts with e tags)
                    let is_reply = event.tags.iter().any(|tag| tag.kind() == TagKind::e());
                    if !is_reply {
                        Some(FeedItem::OriginalPost(event))
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(feed_item) = item {
                    if seen_ids.insert(feed_item.event().id) {
                        all_items.push(feed_item.clone());
                        // Call on_batch with single item for immediate UI update
                        on_batch(vec![feed_item]);
                    }
                }
            },
        )
        .await;
    if let Err(e) = stream_result {
        log::error!("Failed to stream following feed: {}, falling back to global", e);
        let global = load_global_feed(until).await?;
        return Ok((global, true));
    }
    if all_items.is_empty() {
        log::info!("No posts from followed users via streaming");
        return Ok((Vec::new(), false));
    }
    all_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
    log::info!("Streaming complete: {} feed items", all_items.len());
    Ok((all_items, false))
}
/// Load following feed with replies
/// Returns (feed_items, did_fallback) where did_fallback indicates if we fell back to global.
async fn load_following_with_replies(
    until: Option<u64>,
) -> Result<(Vec<FeedItem>, bool), String> {
    let pubkey_str = auth_store::get_pubkey().ok_or("Not authenticated")?;
    log::info!(
        "Loading following feed with replies for {} (until: {:?})", pubkey_str, until
    );
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            let global = load_global_feed(until).await?;
            return Ok((global, true));
        }
    };
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        let global = load_global_feed(until).await?;
        return Ok((global, true));
    }
    log::info!("User follows {} accounts", contacts.len());
    let mut authors = Vec::new();
    for contact in contacts.iter() {
        if let Ok(pk) = PublicKey::parse(contact) {
            authors.push(pk);
        }
    }
    if authors.is_empty() {
        log::warn!("No valid contact pubkeys, falling back to global feed");
        let global = load_global_feed(until).await?;
        return Ok((global, true));
    }
    let mut filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .authors(authors)
        .limit(150);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    } else {
        let since = Timestamp::now() - Duration::from_secs(86400);
        filter = filter.since(since);
    }
    log::info!(
        "Fetching all events (including replies and reposts) from {} followed accounts",
        filter.authors.as_ref().map(| a | a.len()).unwrap_or(0)
    );
    match nostr_client::fetch_events_from_connected_relays(
            filter,
            Duration::from_secs(10),
        )
        .await
    {
        Ok(events) => {
            log::info!(
                "Loaded {} events (including replies and reposts) from following feed via outbox",
                events.len()
            );
            let mut feed_items: Vec<FeedItem> = Vec::new();
            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            feed_items
                                .push(FeedItem::Repost {
                                    original,
                                    reposted_by: event.pubkey,
                                    repost_timestamp: event.created_at,
                                });
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to parse repost event {}: {}", event.id, e
                            );
                        }
                    }
                } else if event.kind == Kind::TextNote {
                    feed_items.push(FeedItem::OriginalPost(event));
                }
            }
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
            if feed_items.is_empty() {
                log::info!("No events from followed users");
                return Ok((Vec::new(), false));
            }
            Ok((feed_items, false))
        }
        Err(e) => {
            log::error!(
                "Failed to fetch following feed with replies: {}, falling back to global",
                e
            );
            let global = load_global_feed(until).await?;
            Ok((global, true))
        }
    }
}
async fn load_global_feed(until: Option<u64>) -> Result<Vec<FeedItem>, String> {
    log::info!("Loading global feed (until: {:?})...", until);
    let mut filter = Filter::new().kinds(vec![Kind::TextNote, Kind::Repost]).limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    } else {
        let since = Timestamp::now() - Duration::from_secs(86400);
        filter = filter.since(since);
    }
    log::info!("Fetching events with filter: {:?}", filter);
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} events", events.len());
            let mut feed_items: Vec<FeedItem> = Vec::new();
            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            feed_items
                                .push(FeedItem::Repost {
                                    original,
                                    reposted_by: event.pubkey,
                                    repost_timestamp: event.created_at,
                                });
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to parse repost event {}: {}", event.id, e
                            );
                        }
                    }
                } else if event.kind == Kind::TextNote {
                    feed_items.push(FeedItem::OriginalPost(event));
                }
            }
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
            Ok(feed_items)
        }
        Err(e) => {
            log::error!("Failed to fetch events: {}", e);
            Err(format!("Failed to load feed: {}", e))
        }
    }
}
/// Fetch a small batch of global posts quickly for immediate display
/// Used while contacts are loading to show something immediately
async fn fetch_quick_global_posts(limit: usize) -> Result<Vec<FeedItem>, String> {
    log::info!("Fetching {} quick global posts...", limit);
    let filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .limit(limit);

    let events = nostr_client::fetch_events_from_connected_relays(
        filter,
        Duration::from_secs(3), // Short timeout for speed
    )
    .await?;

    let mut feed_items = process_events_to_feed_items(events);
    feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));

    log::info!("Got {} quick global posts", feed_items.len());
    Ok(feed_items)
}
/// Batch prefetch author metadata for all feed items
/// This checks the database first and only fetches missing metadata
/// For reposts, it fetches both the original author AND the reposter
async fn prefetch_author_metadata(feed_items: &[FeedItem]) {
    use crate::utils::profile_prefetch;
    let mut pubkeys = Vec::new();
    for item in feed_items {
        match item {
            FeedItem::OriginalPost(event) => {
                pubkeys.push(event.pubkey);
            }
            FeedItem::Repost { original, reposted_by, .. } => {
                pubkeys.push(original.pubkey);
                pubkeys.push(*reposted_by);
            }
        }
    }
    pubkeys.sort();
    pubkeys.dedup();
    profile_prefetch::prefetch_pubkeys(pubkeys).await;
}
/// Helper function to load feed from a people list
/// Fetches posts from all members (public + private) of the list
async fn load_people_list_feed(
    list: &UserList,
    until: Option<u64>,
) -> Result<Vec<FeedItem>, String> {
    log::info!("Loading people list feed for '{}' (until: {:?})", list.name, until);
    let members = get_all_list_members(&list.event)
        .await
        .map_err(|e| {
            log::error!("Failed to get list members: {}", e);
            format!("Failed to decrypt list members: {}", e)
        })?;
    if members.is_empty() {
        log::warn!(
            "People list '{}' has no members - check if private decryption failed", list
            .name
        );
        return Ok(Vec::new());
    }
    log::info!("People list '{}' has {} members", list.name, members.len());
    let mut filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .authors(members)
        .limit(100);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    match nostr_client::fetch_events_from_connected_relays(
            filter,
            Duration::from_secs(10),
        )
        .await
    {
        Ok(events) => {
            log::info!(
                "Loaded {} events from people list '{}'", events.len(), list.name
            );
            let mut feed_items: Vec<FeedItem> = Vec::new();
            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            feed_items
                                .push(FeedItem::Repost {
                                    original,
                                    reposted_by: event.pubkey,
                                    repost_timestamp: event.created_at,
                                });
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to parse repost event {}: {}", event.id, e
                            );
                        }
                    }
                } else if event.kind == Kind::TextNote {
                    feed_items.push(FeedItem::OriginalPost(event));
                }
            }
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));
            Ok(feed_items)
        }
        Err(e) => {
            log::error!("Failed to fetch events for people list '{}': {}", list.name, e);
            Err(format!("Failed to load feed: {}", e))
        }
    }
}
