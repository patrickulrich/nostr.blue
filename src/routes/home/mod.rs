pub mod types;
mod feed_loaders;
pub mod login;
mod engagement;

use crate::components::{
    ArticleCard, ClientInitializing, NoteCard, NoteCardSkeleton, NoteComposer,
};
use crate::components::note_composer::NoteMode;
use crate::error::NostrBlueError;
use crate::hooks::{use_infinite_scroll, use_user_lists};
use crate::services::aggregation::{InteractionCounts, InteractionStreamHandle};
use crate::stores::feed_cache::{self, FeedCacheKey};
use crate::stores::relay;
use crate::stores::{auth_store, nostr_client, subscription_manager};
use crate::stores::ui::scroll_restore;
use crate::utils::list_kinds::NAMED_RELAYS;
use crate::utils::{get_item_count, DataState, FeedItem};
use dioxus::prelude::*;
use engagement::{fetch_and_stream_interactions, fetch_paginated_interactions};
use feed_loaders::{
    exclusive_pagination_cursor, feed_kinds, merge_paginated_feed_items, prefetch_author_metadata,
    prefetch_author_metadata_with_relays,
    load_following_feed, load_following_feed_streaming, load_following_with_replies,
    load_global_feed, load_paginated_global_feed, load_people_list_feed, load_relay_feed,
    sync_following_feed_page, FEED_LIMIT,
};
use login::LoginSection;
use nostr_sdk::{Filter, Kind, PublicKey, Timestamp};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use types::{login_method_requires_signer, FeedType};

#[component]
pub fn Home(list: String) -> Element {
    let mut feed_state = use_signal(|| DataState::<Vec<FeedItem>>::Pending);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut pagination_loading = use_signal(|| false);
    let interaction_counts = use_signal(HashMap::<String, InteractionCounts>::new);
    let mut interactions_loaded = use_signal(|| false);
    let mut cached_muted_posts: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut cached_blocked_users: Signal<Option<Rc<HashSet<String>>>> = use_signal(|| None);
    let mut pending_posts = use_signal(Vec::<FeedItem>::new);
    let pending_count = use_memo(move || pending_posts.read().len());
    let mut realtime_started = use_signal(|| false);
    let mut subscription_ids = use_signal(Vec::<nostr_sdk::SubscriptionId>::new);
    let mut interaction_stream_handle: Signal<Option<InteractionStreamHandle>> =
        use_signal(|| None);
    let mut request_id = use_signal(|| 0u32);
    let mut last_loaded_trigger = use_signal(|| (0u32, FeedType::Following, false));
    let mut relay_feed_sub_id: Signal<Option<nostr_sdk::SubscriptionId>> =
        use_signal(|| None);
    let mut relay_feed_ephemeral_urls = use_signal(Vec::<String>::new);
    let relay_url_input = use_signal(String::new);
    let (all_lists, _lists_loading, _lists_error, _) = use_user_lists();
    let people_lists = use_memo(move || {
        all_lists
            .read()
            .iter()
            .filter(|list| list.kind == crate::utils::list_kinds::NAMED_PEOPLE)
            .cloned()
            .collect::<Vec<_>>()
    });
    let relay_lists = use_memo(move || {
        all_lists
            .read()
            .iter()
            .filter(|list| list.kind == NAMED_RELAYS)
            .cloned()
            .collect::<Vec<_>>()
    });

    // Deep link: set feed type from ?list= parameter
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
            if let Some(matching_list) = lists.iter().find(|l| l.identifier == list_param) {
                log::info!("Deep link: Setting feed to list '{}'", matching_list.name);
                feed_type.set(FeedType::PeopleList(Box::new(matching_list.clone())));
            } else {
                log::warn!("Deep link: List with identifier '{}' not found", list_param);
            }
        }
    });

    // Mute/block cache management
    let mut last_mute_pubkey: Signal<Option<String>> = use_signal(|| None);
    let mut last_invalidate_token: Signal<u32> = use_signal(|| 0);
    use_effect(move || {
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let current_pubkey = auth_store::AUTH_STATE.read().pubkey.clone();
        let current_token = *nostr_client::MUTE_BLOCK_INVALIDATE.read();
        if current_token != *last_invalidate_token.peek() {
            log::debug!("Mute/block invalidation detected in home feed, clearing caches");
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
        if let (Some(ref last), Some(ref current)) =
            (last_mute_pubkey.peek().as_ref(), current_pubkey.as_ref())
        {
            if last != current {
                log::debug!("Account switch detected in home feed, clearing mute/block cache");
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
                        log::debug!("Discarding stale mute list fetch (pubkey or token changed)");
                    }
                }
                Err(e) => {
                    let snapshot_short =
                        auth_pubkey_snapshot.as_ref().map(|s| &s[..8.min(s.len())]);
                    let current_short = auth_store::AUTH_STATE
                        .peek()
                        .pubkey
                        .as_ref()
                        .map(|s| s[..8.min(s.len())].to_string());
                    log::error!(
                        "Failed to fetch mute list: {} (snapshot={:?}, current={:?})",
                        e,
                        snapshot_short,
                        current_short
                    );
                }
            }
        });
    });

    // Main feed loading effect
    use_effect(move || {
        let refresh = *refresh_trigger.read();
        let current_feed_type = feed_type.read().clone();
        let auth_state = auth_store::AUTH_STATE.read();
        let is_authenticated = auth_state.is_authenticated;
        let login_method = auth_state.login_method.clone();
        drop(auth_state);
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = *nostr_client::HAS_SIGNER.read();

        if !client_initialized {
            return;
        }
        let requires_signer = login_method_requires_signer(login_method.as_ref());
        let signer_available = !requires_signer || has_signer;
        let cache_only = is_authenticated && requires_signer && !has_signer;
        let (last_refresh, last_feed, last_signer_available) = last_loaded_trigger.peek().clone();
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
        let signer_changed = signer_available != last_signer_available;
        if is_loading && !feed_type_changed && !refresh_changed && !signer_changed {
            log::debug!("Skipping feed re-load: already loading, no intentional change");
            return;
        }
        if has_data && !feed_type_changed && !refresh_changed && !signer_changed {
            log::debug!("Skipping feed re-load: data already present, no intentional change");
            return;
        }
        last_loaded_trigger.set((refresh, current_feed_type.clone(), signer_available));
        let current_id = *request_id.peek() + 1;
        request_id.set(current_id);
        if !has_data || feed_type_changed {
            feed_state.set(DataState::Loading);
        }
        oldest_timestamp.set(None);
        has_more.set(true);

        // Cleanup existing subscriptions
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
        #[cfg(feature = "native")]
        {
            spawn(async move {
                let _ = crate::stores::ndb::subscriptions::unsubscribe(
                    crate::stores::ndb::subscriptions::SubKey::FollowingFeed,
                ).await;
            });
        }
        if let Some(handle) = interaction_stream_handle.peek().clone() {
            spawn(async move {
                log::info!("Cleaning up interaction stream due to refresh");
                handle.unsubscribe().await;
            });
        }
        interaction_stream_handle.set(None);
        pending_posts.set(Vec::new());
        crate::hooks::use_viewport_engagement::clear_engaged();
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
                    let (cached_items, cached_cursor, cached_count) = {
                        let items = feed_cache::load_cached_feed(&cache_key, FEED_LIMIT)
                            .await
                            .unwrap_or_default();
                        let cursor = feed_cache::load_feed_cursor(&cache_key).await;
                        let count = feed_cache::get_cached_item_count(&cache_key).await;
                        (items, cursor, count)
                    };
                    if is_stale() {
                        return;
                    }
                    let mut accumulated_items = if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for Following feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                        cached_items
                    } else {
                        log::info!("No cache, loading Following feed...");
                        Vec::new()
                    };
                    if cache_only {
                        if accumulated_items.is_empty() {
                            log::info!("Phase 1 cache-only: no cached items for Following, waiting for signer restore");
                        } else {
                            log::info!(
                                "Phase 1 cache-only: showing {} cached items for Following while signer restores",
                                accumulated_items.len()
                            );
                        }
                        has_more.set(false);
                        return;
                    }
                    let stream_req_id = request_id;
                    let stream_curr_id = current_id;
                    let debounce_buffer: Rc<RefCell<Vec<FeedItem>>> =
                        Rc::new(RefCell::new(Vec::new()));
                    let debounce_flush_pending: Rc<RefCell<bool>> =
                        Rc::new(RefCell::new(false));
                    let accumulated_clone: Rc<RefCell<Vec<FeedItem>>> =
                        Rc::new(RefCell::new(accumulated_items.clone()));
                    let feed_state_clone = feed_state;
                    let result = load_following_feed_streaming(None, cached_cursor, cached_count, |batch_items| {
                        if *stream_req_id.peek() != stream_curr_id {
                            log::debug!("Discarding stale streaming batch");
                            return;
                        }
                        debounce_buffer.borrow_mut().extend(batch_items);
                        if !*debounce_flush_pending.borrow() {
                            *debounce_flush_pending.borrow_mut() = true;
                            let buffer = debounce_buffer.clone();
                            let acc = accumulated_clone.clone();
                            let mut fs = feed_state_clone;
                            let pending = debounce_flush_pending.clone();
                            spawn(async move {
                                crate::platform::timer::sleep_ms(50).await;
                                let items: Vec<FeedItem> =
                                    buffer.borrow_mut().drain(..).collect();
                                if !items.is_empty() {
                                    let mut acc_guard = acc.borrow_mut();
                                    *acc_guard =
                                        feed_cache::merge_feed_items(acc_guard.clone(), items);
                                    fs.set(DataState::Loaded(acc_guard.clone()));
                                }
                                *pending.borrow_mut() = false;
                            });
                        }
                    })
                    .await;
                    accumulated_items = accumulated_clone.borrow().clone();
                    if !debounce_buffer.borrow().is_empty() {
                        let items: Vec<FeedItem> =
                            debounce_buffer.borrow_mut().drain(..).collect();
                        accumulated_items =
                            feed_cache::merge_feed_items(accumulated_items.clone(), items);
                        feed_state.set(DataState::Loaded(accumulated_items.clone()));
                    }
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
                            has_more.set(true);
                            accumulated_items = feed_cache::merge_feed_items(accumulated_items, feed_items.clone());
                            if let Some(last_item) = accumulated_items.last() {
                                oldest_timestamp.set(exclusive_pagination_cursor(Some(last_item)));
                            }
                            feed_state.set(DataState::Loaded(accumulated_items.clone()));
                            if !is_stale() {
                                let cache_key_for_store = effective_cache_key;
                                let items_for_cache = accumulated_items.clone();
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
                                let ic = interaction_counts;
                                let il = interactions_loaded;
                                let ish = interaction_stream_handle;
                                spawn(async move {
                                    fetch_and_stream_interactions(
                                        &items_for_counts,
                                        is_first_load,
                                        ic,
                                        req_id,
                                        curr_id,
                                        il,
                                        ish,
                                    )
                                    .await
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata_with_relays(&feed_items).await;
                                });
                            }
                            if !is_stale() && !did_fallback {
                                let pk_for_sync = pubkey_str.clone();
                                spawn(async move {
                                    if let Ok(contacts) = nostr_client::fetch_contacts(pk_for_sync).await {
                                        let authors: Vec<PublicKey> = contacts
                                            .iter()
                                            .filter_map(|c| PublicKey::parse(c).ok())
                                            .collect();
                                        if !authors.is_empty() {
                                            sync_following_feed_page(authors, None).await;
                                        }
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            if accumulated_items.is_empty() {
                                feed_state.set(DataState::Error(e.to_string()));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
                FeedType::FollowingWithReplies => {
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let pk_for_sync = pubkey_str.clone();
                    let cache_key = FeedCacheKey::FollowingWithReplies { pubkey: pubkey_str };
                    let (cached_items, cached_cursor, cached_count) = {
                        let items = feed_cache::load_cached_feed(&cache_key, FEED_LIMIT)
                            .await
                            .unwrap_or_default();
                        let cursor = feed_cache::load_feed_cursor(&cache_key).await;
                        let count = feed_cache::get_cached_item_count(&cache_key).await;
                        (items, cursor, count)
                    };
                    if is_stale() {
                        return;
                    }
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for FollowingWithReplies feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                    }
                    if cache_only {
                        if cached_items.is_empty() {
                            log::info!("Phase 1 cache-only: no cached items for FollowingWithReplies, waiting for signer restore");
                        } else {
                            log::info!(
                                "Phase 1 cache-only: showing {} cached items for FollowingWithReplies while signer restores",
                                cached_items.len()
                            );
                        }
                        has_more.set(false);
                        return;
                    }
                    let result = load_following_with_replies(None, cached_cursor, cached_count).await;
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
                            has_more.set(true);
                            let merged = feed_cache::merge_feed_items(cached_items, feed_items.clone());
                            if let Some(last_item) = merged.last() {
                                oldest_timestamp.set(exclusive_pagination_cursor(Some(last_item)));
                            }
                            feed_state.set(DataState::Loaded(merged.clone()));
                            if !is_stale() {
                                let cache_key_for_store = effective_cache_key;
                                let items_for_cache = merged;
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
                                let ic = interaction_counts;
                                let il = interactions_loaded;
                                let ish = interaction_stream_handle;
                                spawn(async move {
                                    fetch_and_stream_interactions(
                                        &items_for_counts,
                                        is_first_load,
                                        ic,
                                        req_id,
                                        curr_id,
                                        il,
                                        ish,
                                    )
                                    .await
                                });
                            }
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                            if !is_stale() && !did_fallback {
                                let pk_sync = pk_for_sync.clone();
                                spawn(async move {
                                    if let Ok(contacts) = nostr_client::fetch_contacts(pk_sync).await {
                                        let authors: Vec<PublicKey> = contacts
                                            .iter()
                                            .filter_map(|c| PublicKey::parse(c).ok())
                                            .collect();
                                        if !authors.is_empty() {
                                            sync_following_feed_page(authors, None).await;
                                        }
                                    }
                                });
                            }
                        }
                        Err(e) => {
                            if cached_items.is_empty() {
                                feed_state.set(DataState::Error(e.to_string()));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
                FeedType::Global => {
                    let cache_key = FeedCacheKey::Global;
                    let (cached_items, cached_cursor, cached_count) = {
                        let items = feed_cache::load_cached_feed(&cache_key, FEED_LIMIT)
                            .await
                            .unwrap_or_default();
                        let cursor = feed_cache::load_feed_cursor(&cache_key).await;
                        let count = feed_cache::get_cached_item_count(&cache_key).await;
                        (items, cursor, count)
                    };
                    if is_stale() {
                        return;
                    }
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for Global feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                    }
                    let result = load_global_feed(None, cached_cursor, cached_count).await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok(feed_items) => {
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp.set(exclusive_pagination_cursor(Some(last_item)));
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
                                let ic = interaction_counts;
                                let il = interactions_loaded;
                                let ish = interaction_stream_handle;
                                spawn(async move {
                                    fetch_and_stream_interactions(
                                        &items_for_counts,
                                        is_first_load,
                                        ic,
                                        req_id,
                                        curr_id,
                                        il,
                                        ish,
                                    )
                                    .await
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
                                feed_state.set(DataState::Error(e.to_string()));
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
                    let (cached_items, cached_cursor, cached_count) = {
                        let items = feed_cache::load_cached_feed(&cache_key, FEED_LIMIT)
                            .await
                            .unwrap_or_default();
                        let cursor = feed_cache::load_feed_cursor(&cache_key).await;
                        let count = feed_cache::get_cached_item_count(&cache_key).await;
                        (items, cursor, count)
                    };
                    if is_stale() {
                        return;
                    }
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for PeopleList feed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                    }
                    if cache_only {
                        if cached_items.is_empty() {
                            log::info!("Phase 1 cache-only: no cached items for PeopleList, waiting for signer restore");
                        } else {
                            log::info!(
                                "Phase 1 cache-only: showing {} cached items for PeopleList while signer restores",
                                cached_items.len()
                            );
                        }
                        has_more.set(false);
                        return;
                    }
                    let result = load_people_list_feed(&list, None, cached_cursor, cached_count).await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok(feed_items) => {
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp.set(exclusive_pagination_cursor(Some(last_item)));
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
                                let ic = interaction_counts;
                                let il = interactions_loaded;
                                let ish = interaction_stream_handle;
                                spawn(async move {
                                    fetch_and_stream_interactions(
                                        &items_for_counts,
                                        is_first_load,
                                        ic,
                                        req_id,
                                        curr_id,
                                        il,
                                        ish,
                                    )
                                    .await
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
                                feed_state.set(DataState::Error(e.to_string()));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
                FeedType::RelayFeed { .. } | FeedType::RelaySetFeed { .. } => {
                    let urls = current_feed_type.relay_urls();
                    let cache_key = FeedCacheKey::RelayFeed {
                        urls: urls.join(","),
                    };
                    let (cached_items, cached_cursor, cached_count) = {
                        let items = feed_cache::load_cached_feed(&cache_key, FEED_LIMIT)
                            .await
                            .unwrap_or_default();
                        let cursor = feed_cache::load_feed_cursor(&cache_key).await;
                        let count = feed_cache::get_cached_item_count(&cache_key).await;
                        (items, cursor, count)
                    };
                    if is_stale() {
                        return;
                    }
                    if !cached_items.is_empty() {
                        log::info!(
                            "Loaded {} items from cache for RelayFeed (cursor: {:?}, count: {})",
                            cached_items.len(),
                            cached_cursor,
                            cached_count
                        );
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                    }
                    let result = load_relay_feed(urls.clone(), None, cached_cursor, cached_count).await;
                    if is_stale() {
                        return;
                    }
                    match result {
                        Ok(feed_items) => {
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp.set(exclusive_pagination_cursor(Some(last_item)));
                            }
                            has_more.set(true);
                            feed_state.set(DataState::Loaded(feed_items.clone()));
                            if !is_stale() {
                                let cache_key_for_store = cache_key;
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
                                let ic = interaction_counts;
                                let il = interactions_loaded;
                                let ish = interaction_stream_handle;
                                spawn(async move {
                                    fetch_and_stream_interactions(
                                        &items_for_counts,
                                        is_first_load,
                                        ic,
                                        req_id,
                                        curr_id,
                                        il,
                                        ish,
                                    )
                                    .await
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
                                feed_state.set(DataState::Error(e.to_string()));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
            }
        });
    });

    // Cleanup subscriptions on feed type change
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
        #[cfg(feature = "native")]
        {
            spawn(async move {
                let _ = crate::stores::ndb::subscriptions::unsubscribe(
                    crate::stores::ndb::subscriptions::SubKey::FollowingFeed,
                ).await;
            });
        }
        if let Some(handle) = interaction_stream_handle.peek().clone() {
            spawn(async move {
                log::info!("Cleaning up interaction stream due to feed type change");
                handle.unsubscribe().await;
            });
        }
        interaction_stream_handle.set(None);
        if let Some(sub_id) = relay_feed_sub_id.peek().clone() {
            let ephemeral_urls = relay_feed_ephemeral_urls.peek().clone();
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    let _ = client.unsubscribe(&sub_id).await;
                    for url in &ephemeral_urls {
                        let _ = client.force_remove_relay(url).await;
                    }
                }
            });
        }
        relay_feed_sub_id.set(None);
        relay_feed_ephemeral_urls.write().clear();
    });

    // Real-time subscriptions
    use_effect(move || {
        let current_feed_type = feed_type.read().clone();
        let auth_state = auth_store::AUTH_STATE.read();
        let is_authenticated = auth_state.is_authenticated;
        let login_method = auth_state.login_method.clone();
        drop(auth_state);
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = *nostr_client::HAS_SIGNER.read();

        if !client_initialized {
            return;
        }
        let requires_signer = login_method_requires_signer(login_method.as_ref());
        if is_authenticated && requires_signer && !has_signer {
            log::debug!("Waiting for signer restoration before starting realtime...");
            return;
        }
        if *realtime_started.read() {
            return;
        }
        let since_timestamp = match &*feed_state.peek() {
            DataState::Loaded(ref items) => {
                items
                    .first()
                    .map(|i| i.sort_timestamp())
                    .unwrap_or_else(Timestamp::now)
            }
            _ => Timestamp::now(),
        };
        realtime_started.set(true);
        spawn(async move {
            if current_feed_type.is_relay_feed() {
                let urls = current_feed_type.relay_urls();
                let client = match nostr_client::get_client() {
                    Some(c) => c,
                    None => return,
                };
                for url in &urls {
                    let relay_url = match nostr_sdk::RelayUrl::parse(url) {
                        Ok(u) => u,
                        Err(_) => continue,
                    };
                    let relays = client.relays().await;
                    if !relays.contains_key(&relay_url) {
                        drop(relays);
                        let _ = client.add_read_relay(url).await;
                    }
                    let _ = client.connect_relay(url).await;
                }
                let parsed_urls: Vec<nostr_sdk::RelayUrl> = urls
                    .iter()
                    .filter_map(|u| nostr_sdk::RelayUrl::parse(u).ok())
                    .collect();
                let filter = Filter::new().kinds(feed_kinds()).since(since_timestamp);
                let sub_id = match client
                    .subscribe_to(parsed_urls, filter, None)
                    .await
                {
                    Ok(output) => {
                        log::info!("Relay feed real-time subscription: {:?}", output.val);
                        output.val
                    }
                    Err(e) => {
                        log::error!("Failed to subscribe to relay feed: {}", e);
                        return;
                    }
                };
                relay_feed_sub_id.set(Some(sub_id.clone()));
                relay_feed_ephemeral_urls.write().extend(urls.iter().cloned());
                subscription_ids.write().push(sub_id.clone());
                let mut pending = pending_posts;
                let fstate = feed_state;
                let mut notifications = client.notifications();
                while let Ok(notification) = notifications.recv().await {
                    if let nostr_sdk::RelayPoolNotification::Event {
                        subscription_id: event_sub_id,
                        event,
                        ..
                    } = notification
                    {
                        if event_sub_id != sub_id {
                            continue;
                        }
                        let feed_item_opt = if event.kind == Kind::Repost {
                            crate::utils::extract_reposted_event(&event).ok().map(|original| {
                                FeedItem::Repost {
                                    original,
                                    reposted_by: event.pubkey,
                                    repost_timestamp: event.created_at,
                                }
                            })
                        } else if event.kind == Kind::TextNote
                            || event.kind == Kind::Comment
                            || event.kind.as_u16() == crate::utils::nip_bb::KIND_BLOBBI_STATE
                        {
                            Some(FeedItem::OriginalPost((*event).clone()))
                        } else {
                            None
                        };
                        if let Some(feed_item) = feed_item_opt {
                            let event_id = feed_item.event().id;
                            let already_buffered = pending
                                .read()
                                .iter()
                                .any(|item| item.event().id == event_id);
                            let already_in_feed = match &*fstate.peek() {
                                DataState::Loaded(ref current_items) => current_items
                                    .iter()
                                    .any(|item| item.event().id == event_id),
                                _ => false,
                            };
                            if !already_buffered && !already_in_feed {
                                pending.write().push(feed_item);
                            }
                        }
                    }
                }
                return;
            }

            let pubkey_str = match auth_store::get_pubkey() {
                Some(pk) => pk,
                None => return,
            };
            let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Failed to fetch contacts for real-time subscription: {}", e);
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

            {
                let sub_ids = subscription_ids;
                let mut pending = pending_posts;
                let fstate = feed_state;
                let ftype = current_feed_type.clone();
                let client_for_listener = client.clone();
                spawn(async move {
                    let mut notifications = client_for_listener.notifications();
                    while let Ok(notification) = notifications.recv().await {
                        if let nostr_sdk::RelayPoolNotification::Event {
                            subscription_id: event_sub_id,
                            event,
                            ..
                        } = notification
                        {
                            let active_ids = sub_ids.read();
                            if !active_ids.contains(&event_sub_id) {
                                continue;
                            }
                            drop(active_ids);

                            let feed_item_opt = if event.kind == Kind::Repost {
                                match crate::utils::extract_reposted_event(&event) {
                                    Ok(original) => Some(FeedItem::Repost {
                                        original,
                                        reposted_by: event.pubkey,
                                        repost_timestamp: event.created_at,
                                    }),
                                    Err(e) => {
                                        log::warn!(
                                            "Failed to parse repost event {}: {}",
                                            event.id,
                                            e
                                        );
                                        None
                                    }
                                }
                            } else if event.kind == Kind::TextNote {
                                let should_add = match &ftype {
                                    FeedType::Following => !event
                                        .tags
                                        .iter()
                                        .any(|tag| tag.is_reply() || tag.is_root()),
                                    FeedType::FollowingWithReplies
                                    | FeedType::Global
                                    | FeedType::PeopleList(_)
                                    | FeedType::RelayFeed { .. }
                                    | FeedType::RelaySetFeed { .. } => true,
                                };
                                if should_add {
                                    Some(FeedItem::OriginalPost((*event).clone()))
                                } else {
                                    None
                                }
                            } else if event.kind == Kind::Comment {
                                if crate::stores::topic_store::is_topic_post(&event) {
                                    let is_reply = event
                                        .tags
                                        .iter()
                                        .any(|tag| tag.is_reply() || tag.is_root());
                                    if !is_reply {
                                        Some(FeedItem::OriginalPost((*event).clone()))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else if event.kind.as_u16() == crate::utils::nip_bb::KIND_BLOBBI_STATE {
                                Some(FeedItem::OriginalPost((*event).clone()))
                            } else {
                                None
                            };
                            if let Some(feed_item) = feed_item_opt {
                                log::info!("New post received in real-time");
                                let event_id = feed_item.event().id;
                                let already_buffered = pending
                                    .read()
                                    .iter()
                                    .any(|item| item.event().id == event_id);
                                let already_in_feed = match &*fstate.peek() {
                                    DataState::Loaded(ref current_items) => current_items
                                        .iter()
                                        .any(|item| item.event().id == event_id),
                                    _ => false,
                                };
                                if !already_buffered && !already_in_feed {
                                    let author_pk = feed_item.event().pubkey.to_hex();
                                    spawn(async move {
                                        let _ = crate::stores::profiles::fetch_profile(
                                            author_pk,
                                        )
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
                                    pending.write().push(feed_item);
                                    log::info!(
                                        "Buffered new post, total pending: {}",
                                        pending.read().len()
                                    );
                                }
                            }
                }
            }

            #[cfg(feature = "native")]
            {
                let ndb_authors: Vec<PublicKey> = contacts
                    .iter()
                    .filter_map(|c| PublicKey::parse(c).ok())
                    .collect();
                let ndb_filter = Filter::new()
                    .kinds(vec![Kind::TextNote, Kind::Repost, Kind::Comment, Kind::Custom(crate::utils::nip_bb::KIND_BLOBBI_STATE)])
                    .authors(ndb_authors)
                    .since(since_timestamp)
                    .limit(0);
                let filter_jsons = crate::stores::ndb::queries::sdk_filters_to_ndb_jsons(&[ndb_filter]);
                if let Err(e) = crate::stores::ndb::subscriptions::subscribe(
                    crate::stores::ndb::subscriptions::SubKey::FollowingFeed,
                    filter_jsons,
                ).await {
                    log::warn!("NDB subscription failed: {}", e);
                } else {
                    log::info!("NDB subscription active for following feed");
                }
            }
        });
            }

            for (batch_idx, author_batch) in authors.chunks(BATCH_SIZE).enumerate() {
                let batch_authors = author_batch.to_vec();
                let client = client.clone();
                let batch_num = batch_idx + 1;
                if batch_idx > 0 {
                    let delay = (batch_idx as u32) * (BATCH_DELAY_MS as u32);
                    crate::platform::timer::sleep_ms(delay).await;
                }
                let filter = Filter::new()
                    .kinds(vec![Kind::TextNote, Kind::Repost, Kind::Comment, Kind::Custom(crate::utils::nip_bb::KIND_BLOBBI_STATE)])
                    .authors(batch_authors.clone())
                    .since(since_timestamp)
                    .limit(0);
                log::info!(
                    "Subscribing to batch {}/{} ({} authors)",
                    batch_num,
                    num_batches,
                    batch_authors.len()
                );
                match client.subscribe(filter, None).await {
                    Ok(output) => {
                        let subscription_id = output.val;
                        log::info!(
                            "Batch {}/{} subscribed: {:?}",
                            batch_num,
                            num_batches,
                            subscription_id
                        );
                        subscription_ids.write().push(subscription_id);
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to subscribe batch {}/{}: {}",
                            batch_num,
                            num_batches,
                            e
                        );
                    }
                }
            }
        });
    });

    // NDB live events poller (native only) — drains events from nostrdb
    // subscriptions and adds matching new posts to pending_posts
    #[cfg(feature = "native")]
    {
        let mut ndb_pending = pending_posts;
        let fstate = feed_state;
        let ftype = feed_type;
        use_future(move || async move {
            loop {
                crate::platform::timer::sleep_ms(500).await;
                let events = crate::stores::ndb::drain_ndb_live_events();
                if events.is_empty() {
                    continue;
                }
                log::debug!("NDB live: {} new events", events.len());
                let existing_ids: HashSet<nostr_sdk::EventId> = match &*fstate.peek() {
                    DataState::Loaded(items) => items.iter().map(|i| i.event().id).collect(),
                    _ => HashSet::new(),
                };
                let buffered_ids: HashSet<nostr_sdk::EventId> = ndb_pending
                    .read()
                    .iter()
                    .map(|i| i.event().id)
                    .collect();
                for event in events {
                    if existing_ids.contains(&event.id) || buffered_ids.contains(&event.id) {
                        continue;
                    }
                    let feed_item = if event.kind == Kind::Repost {
                        match crate::utils::extract_reposted_event(&event) {
                            Ok(original) => Some(FeedItem::Repost {
                                original,
                                reposted_by: event.pubkey,
                                repost_timestamp: event.created_at,
                            }),
                            Err(_) => None,
                        }
                    } else if event.kind == Kind::TextNote {
                        let is_reply =
                            event.tags.iter().any(|t| t.is_reply() || t.is_root());
                        let include_replies = matches!(
                            &*ftype.read(),
                            FeedType::FollowingWithReplies
                        );
                        if !is_reply || include_replies {
                            Some(FeedItem::OriginalPost(event))
                        } else {
                            None
                        }
                    } else if event.kind == Kind::Comment {
                        if crate::stores::topic_store::is_topic_post(&event) {
                            let is_reply = event.tags.iter().any(|t| t.is_reply() || t.is_root());
                            if !is_reply {
                                Some(FeedItem::OriginalPost(event))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(item) = feed_item {
                        ndb_pending.write().push(item);
                    }
                }
            }
        });
    }

    // Scroll position tracking — polls window.scrollY into a Signal so
    // use_drop can read it synchronously (document::eval is async and
    // would return 0 because WebHistory::push already scrolled to top
    // before use_drop fires).
    let last_scroll_y: Signal<f64> = use_signal(|| 0.0);
    {
        let mut sy = last_scroll_y;
        use_future(move || async move {
            loop {
                crate::platform::timer::sleep_ms(200).await;
                let y = scroll_restore::get_scroll_y().await;
                if y > 0.0 {
                    sy.set(y);
                }
            }
        });
    }

    // Save scroll position on unmount — reads the tracked Signal synchronously
    use_drop(move || {
        let scroll_y = *last_scroll_y.read();
        if scroll_y > 0.0 {
            let mut anchor = scroll_restore::HOME_SCROLL_ANCHOR.write();
            anchor.scroll_y = scroll_y;
            anchor.is_set = true;
            anchor.feed_type_label = feed_type.read().label();
            log::debug!("Saved scroll position (sync): y={}", scroll_y);
        }

        let ids = subscription_ids.peek().clone();
        if !ids.is_empty() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    log::info!(
                        "Cleaning up {} real-time subscriptions on unmount",
                        ids.len()
                    );
                    subscription_manager::unsubscribe_all(&client, &ids).await;
                }
            });
        }
        #[cfg(feature = "native")]
        {
            spawn(async move {
                let _ = crate::stores::ndb::subscriptions::unsubscribe(
                    crate::stores::ndb::subscriptions::SubKey::FollowingFeed,
                )
                .await;
            });
        }
        if let Some(handle) = interaction_stream_handle.peek().clone() {
            spawn(async move {
                log::info!("Cleaning up interaction stream on unmount");
                handle.unsubscribe().await;
            });
        }
        if let Some(sub_id) = relay_feed_sub_id.peek().clone() {
            let ephemeral_urls = relay_feed_ephemeral_urls.peek().clone();
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    let _ = client.unsubscribe(&sub_id).await;
                    for url in &ephemeral_urls {
                        let _ = client.force_remove_relay(url).await;
                    }
                }
            });
        }
    });

    // Restore scroll position after feed content loads (only for popstate/back navigation)
    use_effect(move || {
        if let DataState::Loaded(items) = &*feed_state.read() {
            if !items.is_empty() {
                let anchor = scroll_restore::HOME_SCROLL_ANCHOR.read();
                if anchor.is_set {
                    let label = anchor.feed_type_label.clone();
                    let scroll_y = anchor.scroll_y;
                    drop(anchor);
                    let current_label = feed_type.read().label();
                    if label == current_label {
                        spawn(async move {
                            if scroll_restore::was_popstate_nav().await {
                                crate::platform::timer::sleep_ms(100).await;
                                scroll_restore::set_scroll_y(scroll_y).await;
                                log::debug!("Restored scroll position (popstate): y={}", scroll_y);
                            }
                            scroll_restore::HOME_SCROLL_ANCHOR.write().is_set = false;
                        });
                    }
                }
            }
        }
    });

    // Viewport-aware engagement (Phase 4)
    {
        use crate::hooks::use_viewport_engagement;
        let ic = interaction_counts;
        use_future(move || {
            async move {
                loop {
                    crate::platform::timer::sleep_ms(300).await;
                    let engaged = use_viewport_engagement::ENGAGED_IDS.read().clone();
                    let ic = ic;
                    let script = r#"
                        const els = document.querySelectorAll('[data-event-id]');
                        const vh = window.innerHeight;
                        const results = [];
                        for (const el of els) {
                            const rect = el.getBoundingClientRect();
                            if (rect.bottom >= -200 && rect.top <= vh + 200) {
                                results.push(el.getAttribute('data-event-id'));
                            }
                            if (results.length >= 30) break;
                        }
                        return JSON.stringify(results);
                    "#;
                    let result = match document::eval(script).await {
                        Ok(v) => v.as_str().unwrap_or_default().to_string(),
                        Err(_) => continue,
                    };
                    let visible_ids: Vec<String> = match serde_json::from_str(&result) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let unengaged: Vec<String> = visible_ids
                        .into_iter()
                        .filter(|id| !engaged.contains(id))
                        .collect();
                    if !unengaged.is_empty() {
                        use_viewport_engagement::fetch_counts_for_visible(unengaged, ic).await;
                    }
                }
            }
        });
    }

    // Pagination (load more)
    let load_more = move || {
        log::info!(
            "load_more called - pagination_loading: {}, has_more: {}",
            *pagination_loading.peek(),
            *has_more.peek()
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
                "load_more spawn executing - until: {:?}, feed_type: {:?}",
                until,
                current_feed_type
            );
            let fetch_result: Result<Vec<FeedItem>, NostrBlueError> = match current_feed_type {
                FeedType::Following => match load_following_feed(until, None, 0).await {
                    Ok((items, did_fallback)) => {
                        if did_fallback {
                            Err(NostrBlueError::Other(
                                "Contact fetch failed during pagination".to_string(),
                            ))
                        } else {
                            Ok(items)
                        }
                    }
                    Err(e) => Err(e),
                },
                FeedType::FollowingWithReplies => match load_following_with_replies(until, None, 0).await {
                    Ok((items, did_fallback)) => {
                        if did_fallback {
                            Err(NostrBlueError::Other(
                                "Contact fetch failed during pagination".to_string(),
                            ))
                        } else {
                            Ok(items)
                        }
                    }
                    Err(e) => Err(e),
                },
                FeedType::Global => load_paginated_global_feed(until).await,
                FeedType::PeopleList(list) => load_people_list_feed(&list, until, None, 0).await,
                FeedType::RelayFeed { .. } | FeedType::RelaySetFeed { .. } => {
                    load_relay_feed(current_feed_type.relay_urls(), until, None, 0).await
                }
            };
            match fetch_result {
                Ok(new_items) => {
                    if new_items.is_empty() {
                        log::info!("No more items from relay, reached end of feed");
                        has_more.set(false);
                        pagination_loading.set(false);
                        return;
                    }
                    let fetched_count = new_items.len();
                    let current_state = feed_state.read().clone();
                    if let DataState::Loaded(current) = current_state {
                        let (updated, unique_items, next_cursor) =
                            merge_paginated_feed_items(current, new_items);
                        log::info!(
                            "Deduplication: {} total, {} unique items after filtering",
                            fetched_count,
                            unique_items.len()
                        );
                        if let Some(cursor) = next_cursor {
                            oldest_timestamp.set(Some(cursor));
                        }
                        if !unique_items.is_empty() {
                            let prefetch_items = unique_items.clone();
                            feed_state.set(DataState::Loaded(updated));
                            spawn(async move {
                                prefetch_author_metadata(&prefetch_items).await;
                            });
                            let counts_signal = interaction_counts;
                            spawn(async move {
                                fetch_paginated_interactions(&unique_items, counts_signal).await;
                            });
                        }
                    }
                    pagination_loading.set(false);
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
        spawn(async move {
            scroll_restore::set_scroll_y(0.0).await;
        });
    };

    let mut accept_pending_posts = move || {
        let pending: Vec<FeedItem> = pending_posts.write().drain(..).collect();
        if pending.is_empty() {
            return;
        }
        let current = match feed_state.read().clone() {
            DataState::Loaded(items) => items,
            _ => return,
        };
        let merged = feed_cache::merge_feed_items(current, pending);
        feed_state.set(DataState::Loaded(merged));
        spawn(async move {
            scroll_restore::set_scroll_y(0.0).await;
        });
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
                                    {
                                        let favorite_relays = {
                                            use dioxus::prelude::ReadableExt;
                                            let relays = relay::nip65::FAVORITE_RELAYS.peek().clone();
                                            if relays.is_empty() {
                                                relay::nip65::default_favorite_relays()
                                            } else {
                                                relays
                                            }
                                        };
                                        let rlists = relay_lists.read().clone();
                                        let has_relay_entries = !favorite_relays.is_empty() || !rlists.is_empty();
                                        rsx! {
                                            if has_relay_entries {
                                                div { class: "border-t border-border" }
                                                div { class: "px-4 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wide",
                                                    "Relay Feeds"
                                                }
                                            }
                                            for url in &favorite_relays {
                                                {
                                                    let url_for_click = url.clone();
                                                    let url_for_check = url.clone();
                                                    let domain = url
                                                        .trim_start_matches("wss://")
                                                        .trim_start_matches("ws://")
                                                        .trim_end_matches('/')
                                                        .to_string();
                                                    rsx! {
                                                        button {
                                                            key: "fav-{url}",
                                                            class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                                            onclick: move |_| {
                                                                feed_type.set(FeedType::RelayFeed {
                                                                    url: url_for_click.clone(),
                                                                    name: domain.clone(),
                                                                });
                                                                show_dropdown.set(false);
                                                            },
                                                            div {
                                                                div { class: "font-medium", "📡 {domain}" }
                                                                div { class: "text-xs text-muted-foreground truncate", "{url}" }
                                                            }
                                                            if matches!(
                                                                feed_type.read().clone(),
                                                                FeedType::RelayFeed { ref url, .. } if url == &url_for_check
                                                            ) {
                                                                span { "✓" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            for list in rlists.iter() {
                                                {
                                                    let list_relay_urls: Vec<String> = list.tags.iter()
                                                        .filter_map(|tag| {
                                                            let s = tag.as_slice();
                                                            if s.first().map(|v| v.as_str()) == Some("relay") {
                                                                s.get(1).map(|v| v.to_string())
                                                            } else {
                                                                None
                                                            }
                                                        })
                                                        .collect();
                                                    let list_name_for_set = list.name.clone();
                                                    let list_name_for_check = list.name.clone();
                                                    let list_urls_for_set = list_relay_urls.clone();
                                                    let list_urls_for_check = list_relay_urls.clone();
                                                    let relay_count = list_relay_urls.len();
                                                    rsx! {
                                                        button {
                                                            key: "rlist-{list.id}",
                                                            class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                                            onclick: move |_| {
                                                                if !list_urls_for_set.is_empty() {
                                                                    feed_type.set(FeedType::RelaySetFeed {
                                                                        name: list_name_for_set.clone(),
                                                                        urls: list_urls_for_set.clone(),
                                                                    });
                                                                    show_dropdown.set(false);
                                                                }
                                                            },
                                                            div {
                                                                div { class: "font-medium", "📡 {list.name}" }
                                                                div { class: "text-xs text-muted-foreground", "{relay_count} relays" }
                                                            }
                                                            if matches!(
                                                                feed_type.read().clone(),
                                                                FeedType::RelaySetFeed { ref name, ref urls, .. }
                                                                if name == &list_name_for_check && urls == &list_urls_for_check
                                                            ) {
                                                                span { "✓" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            {
                                                let mut input_url = relay_url_input;
                                                rsx! {
                                                    div { class: "border-t border-border" }
                                                    div { class: "px-4 py-2 flex gap-2",
                                                        input {
                                                            class: "flex-1 text-sm px-3 py-2 bg-muted border border-border rounded-lg focus:outline-none focus:ring-1 focus:ring-primary",
                                                            r#type: "text",
                                                            placeholder: "wss://relay.example.com",
                                                            value: "{input_url}",
                                                            oninput: move |e| {
                                                                input_url.set(e.value());
                                                            },
                                                        }
                                                        button {
                                                            class: "px-3 py-2 text-sm bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                                                            onclick: move |_| {
                                                                let url = input_url.read().clone();
                                                                if !url.is_empty() {
                                                                    let domain = url
                                                                        .trim_start_matches("wss://")
                                                                        .trim_start_matches("ws://")
                                                                        .trim_end_matches('/')
                                                                        .to_string();
                                                                    feed_type.set(FeedType::RelayFeed {
                                                                        url: url.clone(),
                                                                        name: domain,
                                                                    });
                                                                    show_dropdown.set(false);
                                                                    input_url.set(String::new());
                                                                }
                                                            },
                                                            "Go"
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
                            class: "p-2 hover:bg-accent rounded-lg transition disabled:opacity-50",
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
                NoteComposer { mode: NoteMode::Inline }
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
                                        onclick: move |_| accept_pending_posts(),
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
                                } else if event.kind.as_u16() == crate::utils::nip_bb::KIND_BLOBBI_STATE {
                                    rsx! {
                                        crate::components::blobbi::blobbi_card::BlobbiCard { key: "{event.id}", event: event.clone() }
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
