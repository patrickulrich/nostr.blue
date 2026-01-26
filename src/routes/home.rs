use dioxus::prelude::*;
use crate::stores::{auth_store, feed_cache, nostr_client, subscription_manager};
use crate::stores::feed_cache::FeedCacheKey;
use crate::routes::Route;
use crate::components::{NoteCard, NoteCardSkeleton, NoteComposer, ArticleCard, ClientInitializing};
use crate::hooks::{use_infinite_scroll, use_user_lists, UserList};
use crate::utils::{DataState, FeedItem, extract_reposted_event, get_item_count};
use crate::utils::list_kinds::NAMED_PEOPLE;
use crate::utils::list_encryption::get_all_list_members;
use crate::services::aggregation::{InteractionCounts, fetch_interaction_counts_batch, sync_interaction_counts};
use nostr_sdk::{Filter, Kind, Timestamp, PublicKey};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Clone, PartialEq, Debug)]
enum FeedType {
    Following,              // Top level posts only
    FollowingWithReplies,   // All posts including replies
    Global,                 // Global feed from all users
    PeopleList(Box<UserList>),   // Feed from a specific people list (boxed to reduce enum size)
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
    // State for feed items using type-state machine pattern
    let mut feed_state = use_signal(|| DataState::<Vec<FeedItem>>::Pending);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);

    // Pagination state for infinite scroll
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut pagination_loading = use_signal(|| false);

    // Interaction counts cache (event_id -> counts) for batch optimization
    let mut interaction_counts = use_signal(HashMap::<String, InteractionCounts>::new);

    // Track if this is the first interaction count load (for negentropy optimization)
    // First load: full fetch (no local data to reconcile)
    // Subsequent refreshes: use negentropy sync for incremental updates
    let mut interactions_loaded = use_signal(|| false);

    // Cached mute/block lists for N+1 optimization (fetch once, pass to all NoteCards)
    let mut cached_muted_posts: Signal<Option<HashSet<String>>> = use_signal(|| None);
    let mut cached_blocked_users: Signal<Option<HashSet<String>>> = use_signal(|| None);

    // Buffer for real-time events (Twitter/X pattern: "Show N new posts")
    let mut pending_posts = use_signal(Vec::<FeedItem>::new);

    // Derive pending count from pending_posts to avoid race conditions
    let pending_count = use_memo(move || pending_posts.read().len());

    // Track whether real-time subscription is active to prevent duplicate subscriptions
    let mut realtime_started = use_signal(|| false);

    // Track active subscription IDs for cleanup
    let mut subscription_ids = use_signal(Vec::<nostr_sdk::SubscriptionId>::new);

    // Request ID for preventing stale results when feed type changes rapidly
    let mut request_id = use_signal(|| 0u32);

    // Track last intentional load trigger to guard against spurious re-triggers
    // (refresh_trigger, feed_type) - only reload if these changed or no data
    let mut last_loaded_trigger = use_signal(|| (0u32, FeedType::Following));

    // Fetch user's people lists for the dropdown
    let (all_lists, _lists_loading, _lists_error, _) = use_user_lists();

    // Filter to only people lists (kind 30000)
    let people_lists = use_memo(move || {
        all_lists.read()
            .iter()
            .filter(|list| list.kind == NAMED_PEOPLE)
            .cloned()
            .collect::<Vec<_>>()
    });

    // Handle list URL parameter for deep linking from /lists page
    use_effect({
        let list_param = list.clone();
        move || {
            // Only process non-empty list parameter
            if list_param.is_empty() {
                return;
            }

            // Wait for people lists to load
            let lists = people_lists.read();
            if lists.is_empty() {
                return;
            }

            // Find the list with matching identifier
            if let Some(matching_list) = lists.iter().find(|l| l.identifier == list_param) {
                log::info!("Deep link: Setting feed to list '{}'", matching_list.name);
                feed_type.set(FeedType::PeopleList(Box::new(matching_list.clone())));
            } else {
                log::warn!("Deep link: List with identifier '{}' not found", list_param);
            }
        }
    });

    // Fetch mute/block lists once when authenticated (N+1 optimization)
    // Single fetch for both muted posts and blocked users
    use_effect(move || {
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Clear caches on logout to prevent stale data on re-login
        if !is_authenticated {
            cached_muted_posts.set(None);
            cached_blocked_users.set(None);
            return;
        }

        if !client_initialized {
            return;
        }

        // Only fetch if not already loaded
        if cached_muted_posts.peek().is_some() && cached_blocked_users.peek().is_some() {
            return;
        }

        spawn(async move {
            // Single fetch for both - avoids double fetch_mute_list() call
            // Only set caches on success; leave as None on error so we can retry
            if let Ok(data) = nostr_client::get_mute_list_data().await {
                cached_muted_posts.set(Some(data.muted_posts));
                cached_blocked_users.set(Some(data.blocked_users));
            }
        });
    });

    // Load feed on mount and when refresh is triggered or feed type changes
    use_effect(move || {
        // Watch refresh trigger and feed type
        let refresh = *refresh_trigger.read();
        let current_feed_type = feed_type.read().clone();

        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load feed if both authenticated AND client is initialized
        if !is_authenticated || !client_initialized {
            return;
        }

        // Guard: Only reload if intentional change or no data
        // This prevents spurious re-triggers from wiping visible data
        let (last_refresh, last_feed) = last_loaded_trigger.peek().clone();
        // Check state flags using a scoped borrow to avoid lifetime issues
        let (is_loading, has_data) = {
            let current_state = &*feed_state.peek();
            let loading = matches!(current_state, DataState::Loading);
            let data = matches!(current_state, DataState::Loaded(items) if !items.is_empty());
            (loading, data)
        };

        let feed_type_changed = current_feed_type != last_feed;
        let refresh_changed = refresh != last_refresh;

        // Skip if already loading and no intentional change (prevents duplicate requests)
        // This is critical: if CLIENT_INITIALIZED or other signals change during loading,
        // we must NOT start a new request or the current one gets discarded as stale
        if is_loading && !feed_type_changed && !refresh_changed {
            log::debug!("Skipping feed re-load: already loading, no intentional change");
            return;
        }

        if has_data && !feed_type_changed && !refresh_changed {
            log::debug!("Skipping feed re-load: data already present, no intentional change");
            return;
        }

        // Update last loaded trigger
        last_loaded_trigger.set((refresh, current_feed_type.clone()));

        // Generate request ID for stale request prevention
        let current_id = *request_id.peek() + 1;
        request_id.set(current_id);

        // KEY: Only show loading skeleton if no data exists
        // This keeps existing posts visible during refresh
        if !has_data {
            feed_state.set(DataState::Loading);
        }
        oldest_timestamp.set(None);
        has_more.set(true);

        // Cleanup existing subscriptions before refresh to prevent subscription leaks
        // Use peek() instead of read() to avoid subscribing to subscription_ids changes
        let ids = subscription_ids.peek().clone();
        if !ids.is_empty() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    log::info!("Cleaning up {} real-time subscriptions due to manual refresh", ids.len());
                    subscription_manager::unsubscribe_all(&client, &ids).await;
                }
            });
        }
        subscription_ids.write().clear();

        // Clear pending posts buffer on refresh
        pending_posts.set(Vec::new());

        // Reset real-time subscription flag to allow fresh subscription
        realtime_started.set(false);

        // Capture is_first_load BEFORE resetting the flag
        // Include feed_type_changed so switching feeds triggers full fetch, not sync
        let is_first_load = !*interactions_loaded.peek() || feed_type_changed;

        // Reset interactions_loaded so new feed type gets full fetch (not sync)
        interactions_loaded.set(false);

        // Note: Profile cache NOT cleared - 5-min TTL handles staleness
        // Clearing was causing slow avatar loading on page navigation

        spawn(async move {
            // Check if this request is still current (discard stale results)
            if *request_id.peek() != current_id {
                log::debug!("Discarding stale feed request {}", current_id);
                return;
            }

            // Helper to check staleness after any await point
            let is_stale = || *request_id.peek() != current_id;

            match current_feed_type {
                FeedType::Following => {
                    // Get pubkey for cache key
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let cache_key = FeedCacheKey::Following { pubkey: pubkey_str };

                    // STEP 1: Load from cache instantly
                    let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                        .await
                        .unwrap_or_default();

                    // Check staleness after cache load
                    if is_stale() { return; }

                    // Show cached items immediately if available
                    let mut accumulated_items = if !cached_items.is_empty() {
                        log::info!("Loaded {} items from cache for Following feed", cached_items.len());
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                        cached_items
                    } else {
                        Vec::new()
                    };

                    // STEP 2: Stream from network, merge progressively
                    // Capture request ID for staleness check (Dioxus signals are Copy)
                    let stream_req_id = request_id;
                    let stream_curr_id = current_id;
                    let result = load_following_feed_streaming(None, |batch_items| {
                        // Guard against stale batches (Dioxus task cancellation pattern)
                        if *stream_req_id.peek() != stream_curr_id {
                            log::debug!("Discarding stale streaming batch");
                            return;
                        }

                        // Progressive update: merge new batch with accumulated items
                        accumulated_items = feed_cache::merge_feed_items(accumulated_items.clone(), batch_items);

                        // Update UI with merged items
                        feed_state.set(DataState::Loaded(accumulated_items.clone()));
                    }).await;

                    // Check staleness after network load
                    if is_stale() { return; }

                    match result {
                        Ok((feed_items, did_fallback)) => {
                            // Compute effective cache key based on fallback
                            let effective_cache_key = if did_fallback {
                                log::info!("No contacts, switched to Global feed");
                                feed_type.set(FeedType::Global);
                                FeedCacheKey::Global
                            } else {
                                cache_key.clone()
                            };

                            // Track oldest timestamp for pagination
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp.set(Some(last_item.sort_timestamp().as_secs()));
                            }

                            // Always assume there's more content on initial load
                            has_more.set(true);

                            // Final state update with sorted items
                            feed_state.set(DataState::Loaded(feed_items.clone()));

                            // STEP 3: Store to cache in background using effective key
                            if !is_stale() {
                                let cache_key_for_store = effective_cache_key;
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    if let Err(e) = feed_cache::store_feed_items(&cache_key_for_store, &items_for_cache).await {
                                        log::warn!("Failed to store feed to cache: {}", e);
                                    }
                                    if let Err(e) = feed_cache::run_eviction_if_needed().await {
                                        log::warn!("Failed to run cache eviction: {}", e);
                                    }
                                });
                            }

                            // Batch fetch interaction counts for all events
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let req_id = request_id;
                                let curr_id = current_id;
                                spawn(async move {
                                    let event_ids: Vec<_> = items_for_counts.iter().map(|item| item.event().id).collect();
                                    let counts = if is_first_load {
                                        fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await
                                    } else {
                                        sync_interaction_counts(event_ids, Duration::from_secs(5)).await
                                    };
                                    // Re-check staleness after await
                                    if *req_id.peek() != curr_id { return; }
                                    if let Ok(counts) = counts {
                                        interaction_counts.set(counts);
                                        interactions_loaded.set(true);
                                    }
                                });
                            }

                            // Spawn non-blocking background prefetch for metadata
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            // On error, keep cached data visible if we have it
                            if accumulated_items.is_empty() {
                                feed_state.set(DataState::Error(e));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
                FeedType::FollowingWithReplies => {
                    // Get pubkey for cache key
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let cache_key = FeedCacheKey::FollowingWithReplies { pubkey: pubkey_str };

                    // STEP 1: Load from cache instantly
                    let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                        .await
                        .unwrap_or_default();

                    // Check staleness after cache load
                    if is_stale() { return; }

                    if !cached_items.is_empty() {
                        log::info!("Loaded {} items from cache for FollowingWithReplies feed", cached_items.len());
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                    }

                    // STEP 2: Load from network
                    let result = load_following_with_replies(None).await;

                    // Check staleness after network load
                    if is_stale() { return; }

                    match result {
                        Ok((feed_items, did_fallback)) => {
                            // Compute effective cache key based on fallback
                            let effective_cache_key = if did_fallback {
                                log::info!("No contacts, switched to Global feed");
                                feed_type.set(FeedType::Global);
                                FeedCacheKey::Global
                            } else {
                                cache_key.clone()
                            };

                            // Track oldest timestamp for pagination
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp.set(Some(last_item.sort_timestamp().as_secs()));
                            }

                            // Always assume there's more content on initial load
                            has_more.set(true);

                            // Display feed immediately
                            feed_state.set(DataState::Loaded(feed_items.clone()));

                            // STEP 3: Store to cache using effective key
                            if !is_stale() {
                                let cache_key_for_store = effective_cache_key;
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    let _ = feed_cache::store_feed_items(&cache_key_for_store, &items_for_cache).await;
                                    let _ = feed_cache::run_eviction_if_needed().await;
                                });
                            }

                            // Batch fetch interaction counts for all events
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let req_id = request_id;
                                let curr_id = current_id;
                                spawn(async move {
                                    let event_ids: Vec<_> = items_for_counts.iter().map(|item| item.event().id).collect();
                                    let counts = if is_first_load {
                                        fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await
                                    } else {
                                        sync_interaction_counts(event_ids, Duration::from_secs(5)).await
                                    };
                                    // Re-check staleness after await
                                    if *req_id.peek() != curr_id { return; }
                                    if let Ok(counts) = counts {
                                        interaction_counts.set(counts);
                                        interactions_loaded.set(true);
                                    }
                                });
                            }

                            // Spawn non-blocking background prefetch for metadata
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            // On error, keep cached data visible if we have it
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

                    // STEP 1: Load from cache instantly
                    let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                        .await
                        .unwrap_or_default();

                    // Check staleness after cache load
                    if is_stale() { return; }

                    if !cached_items.is_empty() {
                        log::info!("Loaded {} items from cache for Global feed", cached_items.len());
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                    }

                    // STEP 2: Load from network
                    let result = load_global_feed(None).await;

                    // Check staleness after network load
                    if is_stale() { return; }

                    match result {
                        Ok(feed_items) => {
                            // Track oldest timestamp for pagination
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp.set(Some(last_item.sort_timestamp().as_secs()));
                            }

                            // Always assume there's more content on initial load
                            has_more.set(true);

                            // Display feed immediately
                            feed_state.set(DataState::Loaded(feed_items.clone()));

                            // STEP 3: Store to cache
                            if !is_stale() {
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    let _ = feed_cache::store_feed_items(&FeedCacheKey::Global, &items_for_cache).await;
                                    let _ = feed_cache::run_eviction_if_needed().await;
                                });
                            }

                            // Batch fetch interaction counts for all events
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let is_first_load = !*interactions_loaded.peek();
                                let req_id = request_id;
                                let curr_id = current_id;
                                spawn(async move {
                                    let event_ids: Vec<_> = items_for_counts.iter().map(|item| item.event().id).collect();
                                    let counts = if is_first_load {
                                        fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await
                                    } else {
                                        sync_interaction_counts(event_ids, Duration::from_secs(5)).await
                                    };
                                    // Re-check staleness after await
                                    if *req_id.peek() != curr_id { return; }
                                    if let Ok(counts) = counts {
                                        interaction_counts.set(counts);
                                        interactions_loaded.set(true);
                                    }
                                });
                            }

                            // Spawn non-blocking background prefetch for metadata
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            // On error, keep cached data visible if we have it
                            if cached_items.is_empty() {
                                feed_state.set(DataState::Error(e));
                            } else {
                                log::warn!("Network error but showing cached data: {}", e);
                            }
                        }
                    }
                }
                FeedType::PeopleList(list) => {
                    // Get pubkey for cache key
                    let pubkey_str = auth_store::get_pubkey().unwrap_or_default();
                    let cache_key = FeedCacheKey::PeopleList {
                        pubkey: pubkey_str,
                        list_id: list.identifier.clone(),
                    };

                    // STEP 1: Load from cache instantly
                    let cached_items = feed_cache::load_cached_feed(&cache_key, 100)
                        .await
                        .unwrap_or_default();

                    // Check staleness after cache load
                    if is_stale() { return; }

                    if !cached_items.is_empty() {
                        log::info!("Loaded {} items from cache for PeopleList feed", cached_items.len());
                        feed_state.set(DataState::Loaded(cached_items.clone()));
                    }

                    // STEP 2: Load from network
                    let result = load_people_list_feed(&list, None).await;

                    // Check staleness after network load
                    if is_stale() { return; }

                    match result {
                        Ok(feed_items) => {
                            // Track oldest timestamp for pagination
                            if let Some(last_item) = feed_items.last() {
                                oldest_timestamp.set(Some(last_item.sort_timestamp().as_secs()));
                            }

                            // Always assume there's more content on initial load
                            has_more.set(true);

                            // Display feed immediately
                            feed_state.set(DataState::Loaded(feed_items.clone()));

                            // STEP 3: Store to cache
                            if !is_stale() {
                                let cache_key_for_store = cache_key.clone();
                                let items_for_cache = feed_items.clone();
                                spawn(async move {
                                    let _ = feed_cache::store_feed_items(&cache_key_for_store, &items_for_cache).await;
                                    let _ = feed_cache::run_eviction_if_needed().await;
                                });
                            }

                            // Batch fetch interaction counts for all events
                            if !is_stale() {
                                let items_for_counts = feed_items.clone();
                                let req_id = request_id;
                                let curr_id = current_id;
                                spawn(async move {
                                    let event_ids: Vec<_> = items_for_counts.iter().map(|item| item.event().id).collect();
                                    let counts = if is_first_load {
                                        fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await
                                    } else {
                                        sync_interaction_counts(event_ids, Duration::from_secs(5)).await
                                    };
                                    // Re-check staleness after await
                                    if *req_id.peek() != curr_id { return; }
                                    if let Ok(counts) = counts {
                                        interaction_counts.set(counts);
                                        interactions_loaded.set(true);
                                    }
                                });
                            }

                            // Spawn non-blocking background prefetch for metadata
                            if !is_stale() {
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                        }
                        Err(e) => {
                            // On error, keep cached data visible if we have it
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

    // Reset real-time subscription when feed type changes
    use_effect(move || {
        let _ = feed_type.read(); // watch for changes

        // Cleanup existing subscriptions before resetting
        // Use peek() instead of read() to avoid subscribing to subscription_ids changes
        let ids = subscription_ids.peek().clone();
        if !ids.is_empty() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    log::info!("Cleaning up {} real-time subscriptions due to feed type change", ids.len());
                    subscription_manager::unsubscribe_all(&client, &ids).await;
                }
            });
        }
        subscription_ids.write().clear();
        realtime_started.set(false);
    });

    // Real-time subscription for live feed updates (starts AFTER initial load)
    use_effect(move || {
        let current_feed_type = feed_type.read().clone();
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only subscribe if authenticated, initialized, AND feed is loaded
        // This prevents race conditions during initial load
        if !is_authenticated || !client_initialized {
            return;
        }

        // Check if subscription is already active to prevent duplicate subscriptions
        if *realtime_started.read() {
            return;
        }

        // Wait until feed is loaded before starting real-time subscription
        // Use reference pattern matching to avoid cloning the entire feed
        // NOTE: Use peek() instead of read() to avoid creating an effect dependency on feed_state
        // Using read() would cause this effect to re-trigger whenever feed_state changes,
        // creating a race condition that wipes the displayed posts
        let since_timestamp = match &*feed_state.peek() {
            DataState::Loaded(ref items) => {
                // Compute since timestamp from the latest (first) event in the feed
                // This prevents gaps between feed load and subscription start
                if let Some(latest_item) = items.first() {
                    latest_item.sort_timestamp()
                } else {
                    // Empty feed, use current time
                    Timestamp::now()
                }
            }
            _ => {
                // Feed not loaded yet, wait
                return;
            }
        };

        // Mark subscription as started
        realtime_started.set(true);

        spawn(async move {
            // Get user's pubkey
            let pubkey_str = match auth_store::get_pubkey() {
                Some(pk) => pk,
                None => return,
            };

            // Fetch contacts to subscribe to their posts
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

            // Parse contact pubkeys
            let authors: Vec<PublicKey> = contacts.iter()
                .filter_map(|contact| PublicKey::parse(contact).ok())
                .collect();

            if authors.is_empty() {
                return;
            }

            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => return,
            };

            // Batch subscriptions for large contact lists to avoid overwhelming relays
            // Split into chunks of 50 authors per subscription
            const BATCH_SIZE: usize = 50;
            const BATCH_DELAY_MS: u64 = 100; // 100ms delay between batches

            let total_authors = authors.len();
            let num_batches = total_authors.div_ceil(BATCH_SIZE);

            log::info!("Starting batched real-time subscription for {} followed users in {} batches using gossip",
                contacts.len(), num_batches);

            // Subscribe to batches with staggered timing
            for (batch_idx, author_batch) in authors.chunks(BATCH_SIZE).enumerate() {
                let batch_authors = author_batch.to_vec();
                let client = client.clone();
                let batch_num = batch_idx + 1;

                // Stagger batches to avoid spike
                if batch_idx > 0 {
                    #[cfg(target_arch = "wasm32")]
                    {
                        use gloo_timers::future::TimeoutFuture;
                        TimeoutFuture::new(batch_idx as u32 * BATCH_DELAY_MS as u32).await;
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        use tokio::time::{sleep, Duration as TokioDuration};
                        sleep(TokioDuration::from_millis(batch_idx as u64 * BATCH_DELAY_MS)).await;
                    }
                }

                let filter = Filter::new()
                    .kinds(vec![Kind::TextNote, Kind::Repost])
                    .authors(batch_authors.clone())
                    .since(since_timestamp)
                    .limit(0); // limit=0 means only new events

                log::info!("Subscribing to batch {}/{} ({} authors)",
                    batch_num, num_batches, batch_authors.len());

                match client.subscribe(filter, None).await {
                    Ok(output) => {
                        let subscription_id = output.val;
                        log::info!("Batch {}/{} subscribed: {:?}", batch_num, num_batches, subscription_id);

                        // Store subscription ID for cleanup
                        subscription_ids.write().push(subscription_id.clone());

                        // Handle incoming events for this batch
                        let client_for_notifications = client.clone();
                        // Clone feed type for this batch's async task (UserList isn't Copy)
                        let batch_feed_type = current_feed_type.clone();
                        // Create author set for client-side filtering (defense-in-depth)
                        let batch_author_set: std::collections::HashSet<_> = batch_authors.iter().cloned().collect();
                        spawn(async move {
                            let mut notifications = client_for_notifications.notifications();

                            while let Ok(notification) = notifications.recv().await {
                                if let nostr_sdk::RelayPoolNotification::Event {
                                    subscription_id: event_sub_id,
                                    event,
                                    ..
                                } = notification
                                {
                                    // Only process events from this batch's subscription
                                    if event_sub_id != subscription_id {
                                        continue;
                                    }

                                    // Client-side author filtering (defense-in-depth against misbehaving relays)
                                    if !batch_author_set.contains(&event.pubkey) {
                                        log::debug!("Filtered out event from non-followed author: {}", event.pubkey);
                                        continue;
                                    }

                                    // Process event into FeedItem and check if it matches feed type
                                    let feed_item_opt = if event.kind == Kind::Repost {
                                        // Parse repost to extract original event
                                        match extract_reposted_event(&event) {
                                            Ok(original) => {
                                                // Always include reposts (regardless of feed type)
                                                Some(FeedItem::Repost {
                                                    original,
                                                    reposted_by: event.pubkey,
                                                    repost_timestamp: event.created_at,
                                                })
                                            }
                                            Err(e) => {
                                                log::warn!("Failed to parse repost event {}: {}", event.id, e);
                                                None
                                            }
                                        }
                                    } else if event.kind == Kind::TextNote {
                                        // Check if this matches our feed type
                                        let should_add = match &batch_feed_type {
                                            FeedType::Following => {
                                                // Only top-level posts (no e tags)
                                                !event.tags.iter().any(|tag| tag.kind() == nostr_sdk::TagKind::e())
                                            }
                                            FeedType::FollowingWithReplies | FeedType::Global | FeedType::PeopleList(_) => {
                                                // All posts including replies
                                                true
                                            }
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
                                        log::info!("New post received in real-time from batch {}", batch_num);

                                        // Buffer new posts instead of direct insertion (Twitter/X pattern)
                                        // Check if event already exists in buffer or feed (avoid duplicates)
                                        let event_id = feed_item.event().id;

                                        let already_buffered = pending_posts.read().iter()
                                            .any(|item| item.event().id == event_id);

                                        // Use reference pattern matching to avoid cloning the entire feed
                                        // NOTE: Use peek() to avoid creating effect dependency on feed_state
                                        let already_in_feed = match &*feed_state.peek() {
                                            DataState::Loaded(ref current_items) => {
                                                current_items.iter().any(|item| item.event().id == event_id)
                                            }
                                            _ => false,
                                        };

                                        if !already_buffered && !already_in_feed {
                                            // Prefetch author metadata so it's ready when "Show new posts" is clicked
                                            let author_pk = feed_item.event().pubkey.to_hex();
                                            spawn(async move {
                                                let _ = crate::stores::profiles::fetch_profile(author_pk).await;
                                            });

                                            // If repost, also prefetch original author's metadata
                                            if let FeedItem::Repost { ref original, .. } = feed_item {
                                                let original_author_pk = original.pubkey.to_hex();
                                                spawn(async move {
                                                    let _ = crate::stores::profiles::fetch_profile(original_author_pk).await;
                                                });
                                            }

                                            // Add to pending buffer
                                            pending_posts.write().push(feed_item);
                                            log::info!("Buffered new post, total pending: {}", pending_posts.read().len());
                                        }
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to subscribe batch {}/{}: {}", batch_num, num_batches, e);
                    }
                }
            }
        });
    });

    // Load more function for infinite scroll
    let load_more = move || {
        log::info!("load_more called - pagination_loading: {}, has_more: {}",
                   *pagination_loading.peek(), *has_more.peek());

        if *pagination_loading.peek() || !*has_more.peek() {
            log::info!("load_more blocked by guards");
            return;
        }

        log::info!("load_more setting pagination_loading to true and spawning");
        pagination_loading.set(true);

        spawn(async move {
            // Read signals fresh on each invocation to avoid stale closure bug
            let until = *oldest_timestamp.read();
            let current_feed_type = feed_type.read().clone();

            log::info!("load_more spawn executing - until: {:?}, feed_type: {:?}", until, current_feed_type);

            // Fetch items based on feed type
            // Check did_fallback during pagination - if fallback occurs, treat as error
            // to avoid silently injecting Global items into Following feed
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
                    ).await;
                }
                Err(e) => {
                    log::error!("Failed to load more events: {}", e);
                    pagination_loading.set(false);
                }
            }
        });
    };

    // Set up infinite scroll
    let sentinel_id = use_infinite_scroll(
        load_more,
        has_more,
        pagination_loading
    );

    // Helper to refresh feed and scroll to top
    // Used by both the refresh button and "Show N new posts" banner
    let mut refresh_and_scroll_to_top = move || {
        // Trigger feed refresh (clears pending posts, fetches fresh data from network)
        let current = *refresh_trigger.read();
        refresh_trigger.set(current + 1);

        // Scroll to top of page
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                window.scroll_to_with_x_and_y(0.0, 0.0);
            }
        }
    };

    // Read auth state for rendering
    let auth = auth_store::AUTH_STATE.read();

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center justify-between",

                    // Feed type selector (dropdown)
                    if auth.is_authenticated {
                        div {
                            class: "relative",
                            button {
                                class: "text-xl font-bold flex items-center gap-2 hover:bg-accent px-3 py-1 rounded-lg transition",
                                onclick: move |_| {
                                    let current = *show_dropdown.read();
                                    show_dropdown.set(!current);
                                },
                                "{feed_type.read().label()}"
                                span {
                                    class: "text-sm",
                                    if *show_dropdown.read() { "▲" } else { "▼" }
                                }
                            }

                            // Dropdown menu
                            if *show_dropdown.read() {
                                div {
                                    class: "absolute top-full left-0 mt-2 bg-card border border-border rounded-lg shadow-lg min-w-[200px] overflow-hidden z-30",

                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::Following);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div {
                                                class: "font-medium",
                                                "Following"
                                            }
                                            div {
                                                class: "text-xs text-muted-foreground",
                                                "Top level posts only"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::Following {
                                            span { "✓" }
                                        }
                                    }

                                    div {
                                        class: "border-t border-border"
                                    }

                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::FollowingWithReplies);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div {
                                                class: "font-medium",
                                                "Following + Replies"
                                            }
                                            div {
                                                class: "text-xs text-muted-foreground",
                                                "All posts including replies"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::FollowingWithReplies {
                                            span { "✓" }
                                        }
                                    }

                                    div {
                                        class: "border-t border-border"
                                    }

                                    button {
                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                        onclick: move |_| {
                                            feed_type.set(FeedType::Global);
                                            show_dropdown.set(false);
                                        },
                                        div {
                                            div {
                                                class: "font-medium",
                                                "Global"
                                            }
                                            div {
                                                class: "text-xs text-muted-foreground",
                                                "Posts from everyone"
                                            }
                                        }
                                        if *feed_type.read() == FeedType::Global {
                                            span { "✓" }
                                        }
                                    }

                                    // People Lists Section
                                    if !people_lists.read().is_empty() {
                                        div {
                                            class: "border-t border-border"
                                        }
                                        div {
                                            class: "px-4 py-2 text-xs font-semibold text-muted-foreground uppercase tracking-wide",
                                            "Your Lists"
                                        }

                                        for list in people_lists.read().iter() {
                                            {
                                                let list_for_select = list.clone();
                                                let list_for_check = list.clone();
                                                let item_count = get_item_count(&list.tags);
                                                rsx! {
                                                    button {
                                                        key: "{list.id}",
                                                        class: "w-full px-4 py-3 text-left hover:bg-accent transition flex items-center justify-between",
                                                        onclick: move |_| {
                                                            feed_type.set(FeedType::PeopleList(Box::new(list_for_select.clone())));
                                                            show_dropdown.set(false);
                                                        },
                                                        div {
                                                            div {
                                                                class: "font-medium flex items-center gap-2",
                                                                "👥 {list.name}"
                                                                if list.has_private_content {
                                                                    span {
                                                                        class: "text-sm",
                                                                        title: "Has private members",
                                                                        "🔒"
                                                                    }
                                                                }
                                                            }
                                                            div {
                                                                class: "text-xs text-muted-foreground",
                                                                "{item_count} members"
                                                            }
                                                        }
                                                        if matches!(feed_type.read().clone(), FeedType::PeopleList(ref l) if l.id == list_for_check.id) {
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
                        h2 {
                            class: "text-xl font-bold",
                            "Home"
                        }
                    }

                    // Refresh button
                    if auth.is_authenticated {
                        button {
                            class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                            disabled: feed_state.read().is_loading(),
                            onclick: move |_| refresh_and_scroll_to_top(),
                            title: "Refresh feed",
                            if feed_state.read().is_loading() {
                                span {
                                    class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin"
                                }
                            } else {
                                "🔄"
                            }
                        }
                    }
                }
            }

            // Post Composer (only shown when authenticated)
            if auth.is_authenticated {
                NoteComposer {}
            }

            // Login prompt if not authenticated
            if !auth.is_authenticated {
                div {
                    class: "border-b border-border p-6 bg-blue-50 dark:bg-blue-900/20",
                    div {
                        class: "max-w-md mx-auto text-center",
                        h3 {
                            class: "text-lg font-bold mb-2",
                            "Welcome to nostr.blue"
                        }
                        p {
                            class: "text-muted-foreground mb-4",
                            "Connect your account to see your feed"
                        }
                    }
                }
            }

            // Feed Content
            div {

                if !auth.is_authenticated {
                    // Show login section
                    LoginSection {}
                } else if !*nostr_client::CLIENT_INITIALIZED.read() {
                    // Show client initializing animation during client initialization
                    ClientInitializing {}
                } else if feed_state.read().is_pending() || feed_state.read().is_loading() {
                    // Show skeleton loaders for better UX (prevents layout shift)
                    div {
                        class: "divide-y divide-border",
                        for i in 0..5 {
                            NoteCardSkeleton { key: "{i}" }
                        }
                    }
                } else if let Some(err) = feed_state.read().error() {
                    // Show error message
                    div {
                        class: "p-6 text-center",
                        div {
                            class: "max-w-md mx-auto",
                            div {
                                class: "text-4xl mb-2",
                                "⚠️"
                            }
                            p {
                                class: "text-red-600 dark:text-red-400",
                                "Error loading feed: {err}"
                            }
                        }
                    }
                } else if let Some(feed_items) = feed_state.read().data() {
                    if feed_items.is_empty() {
                        // Empty state
                        div {
                            class: "p-6 text-center text-gray-500 dark:text-gray-400",
                            div {
                                class: "max-w-md mx-auto space-y-4",
                                div {
                                    class: "text-4xl mb-2",
                                    "📝"
                                }
                                h3 {
                                    class: "text-lg font-semibold text-gray-700 dark:text-gray-300",
                                    "No posts yet"
                                }
                                p {
                                    class: "text-sm",
                                    "Posts from the network will appear here"
                                }
                            }
                        }
                    } else {
                        // "Show N new posts" banner (Twitter/X pattern)
                        if *pending_count.read() > 0 {
                            {
                                let count = *pending_count.read();
                                let post_text = if count == 1 { "post" } else { "posts" };
                                rsx! {
                                    div {
                                        class: "sticky top-[57px] z-10 border-b border-border bg-blue-500 hover:bg-blue-600 transition-colors cursor-pointer",
                                        onclick: move |_| refresh_and_scroll_to_top(),
                                        div {
                                            class: "px-4 py-3 text-center",
                                            span {
                                                class: "text-white font-medium",
                                                "Show {count} new {post_text}"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Show feed items (with conditional rendering for articles and reposts)
                        for feed_item in feed_items.iter() {
                            {
                                // Get the underlying event and repost info
                                let event = feed_item.event();
                                let repost_info = feed_item.repost_info();

                                // Check if this is a long-form article (NIP-23)
                                if event.kind == Kind::LongFormTextNote {
                                    rsx! {
                                        ArticleCard {
                                            key: "{event.id}",
                                            event: event.clone()
                                        }
                                    }
                                } else {
                                    rsx! {
                                        NoteCard {
                                            key: "{event.id}",
                                            event: event.clone(),
                                            repost_info: repost_info,
                                            precomputed_counts: interaction_counts.read().get(&event.id.to_hex()).cloned(),
                                            collapsible: true,
                                            cached_muted_posts: cached_muted_posts.read().clone(),
                                            cached_blocked_users: cached_blocked_users.read().clone()
                                        }
                                    }
                                }
                            }
                        }

                        // Infinite scroll sentinel / loading indicator
                        if *has_more.read() {
                            div {
                                id: "{sentinel_id}",
                                class: "p-8 flex justify-center",
                                if *pagination_loading.read() {
                                    span {
                                        class: "flex items-center gap-2 text-muted-foreground",
                                        span {
                                            class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin"
                                        }
                                        "Loading more..."
                                    }
                                }
                            }
                        } else {
                            div {
                                class: "p-8 text-center text-muted-foreground",
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
        // Modal overlay
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center p-4 z-50",
            onclick: move |_| on_close.call(()),

            // Modal content
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-2xl w-full max-h-[90vh] overflow-y-auto",
                onclick: move |e| e.stop_propagation(),

                // Header
                div {
                    class: "sticky top-0 bg-white dark:bg-gray-800 border-b border-gray-200 dark:border-gray-700 px-6 py-4 flex items-center justify-between",
                    h3 {
                        class: "text-xl font-bold text-gray-900 dark:text-white",
                        "About Nostr Sign-In Methods"
                    }
                    button {
                        class: "text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 text-2xl",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }

                // Content
                div {
                    class: "px-6 py-4 space-y-6",

                    // What is Nostr
                    div {
                        h4 {
                            class: "font-semibold text-gray-900 dark:text-white mb-2",
                            "What is Nostr?"
                        }
                        p {
                            class: "text-sm text-gray-600 dark:text-gray-400",
                            "Nostr is a decentralized social protocol where you own your identity and data. Instead of relying on a company, your identity is based on cryptographic keys that only you control."
                        }
                    }

                    // Browser Extension
                    div {
                        h4 {
                            class: "font-semibold text-gray-900 dark:text-white mb-2 flex items-center gap-2",
                            "🔌 Browser Extension (NIP-07)"
                            span {
                                class: "px-2 py-0.5 text-xs bg-green-600 text-white rounded-full",
                                "RECOMMENDED"
                            }
                        }
                        p {
                            class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                            "Browser extensions like Alby, nos2x, and Flamingo store your keys securely and sign events on your behalf. Your private key never leaves the extension."
                        }
                        ul {
                            class: "text-sm text-gray-600 dark:text-gray-400 list-disc list-inside space-y-1",
                            li { "Keys stored securely in the extension" }
                            li { "Websites can't access your private key" }
                            li { "Works across all Nostr apps" }
                            li { "You control which actions to approve" }
                        }
                    }

                    // Remote Signer
                    div {
                        h4 {
                            class: "font-semibold text-gray-900 dark:text-white mb-2 flex items-center gap-2",
                            "🔐 Remote Signer (NIP-46)"
                            span {
                                class: "px-2 py-0.5 text-xs bg-blue-600 text-white rounded-full",
                                "RECOMMENDED"
                            }
                        }
                        p {
                            class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                            "Remote signers let you keep your keys on a separate device (like your phone with Amber) or a dedicated service (like nsecBunker). This app connects to your signer and requests signatures remotely."
                        }
                        ul {
                            class: "text-sm text-gray-600 dark:text-gray-400 list-disc list-inside space-y-1",
                            li { "Keys stay on your signing device" }
                            li { "Approve each action on your phone" }
                            li { "Compatible signers: Amber (Android), nsecBunker" }
                            li { "Most secure for untrusted devices" }
                        }
                        p {
                            class: "text-xs text-blue-600 dark:text-blue-400 mt-2",
                            "To use: Get a bunker:// URI from your signing app and paste it above."
                        }
                    }

                    // Private Key Warning
                    div {
                        h4 {
                            class: "font-semibold text-gray-900 dark:text-white mb-2 flex items-center gap-2",
                            "🔑 Private Key (nsec)"
                            span {
                                class: "px-2 py-0.5 text-xs bg-yellow-600 text-white rounded-full",
                                "USE WITH CAUTION"
                            }
                        }
                        p {
                            class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                            "Entering your private key (nsec) directly gives this app full access to your account. Your key is stored in browser localStorage."
                        }
                        ul {
                            class: "text-sm text-gray-600 dark:text-gray-400 list-disc list-inside space-y-1",
                            li { "⚠️ Only use on devices you fully trust" }
                            li { "⚠️ Never share your nsec with anyone" }
                            li { "⚠️ Stored in browser (cleared if you clear data)" }
                            li { "Can be compromised if device is compromised" }
                        }
                    }

                    // Public Key
                    div {
                        h4 {
                            class: "font-semibold text-gray-900 dark:text-white mb-2",
                            "👁️ Public Key (npub) - Read Only"
                        }
                        p {
                            class: "text-sm text-gray-600 dark:text-gray-400",
                            "Using just your public key (npub) lets you browse and view content, but you cannot post, react, or send messages. Perfect for exploring Nostr without committing."
                        }
                    }

                    // Security Best Practices
                    div {
                        h4 {
                            class: "font-semibold text-gray-900 dark:text-white mb-2",
                            "🛡️ Security Best Practices"
                        }
                        ul {
                            class: "text-sm text-gray-600 dark:text-gray-400 list-disc list-inside space-y-1",
                            li { "Always prefer browser extensions or remote signers" }
                            li { "Never enter your nsec on untrusted websites" }
                            li { "Backup your keys securely (offline)" }
                            li { "Use different keys for testing and main account" }
                        }
                    }
                }

                // Footer
                div {
                    class: "sticky bottom-0 bg-gray-50 dark:bg-gray-900 border-t border-gray-200 dark:border-gray-700 px-6 py-4",
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

    // State management
    let mut nsec_input = use_signal(String::new);
    let mut npub_input = use_signal(String::new);
    let mut bunker_uri_input = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut show_advanced = use_signal(|| false);
    let mut show_help_modal = use_signal(|| false);
    let mut connecting_bunker = use_signal(|| false);

    // NIP-49 password fields for nsec encryption
    let mut nsec_password = use_signal(String::new);
    let mut nsec_confirm_password = use_signal(String::new);
    let mut show_nsec_password = use_signal(|| false);

    // Login handlers
    let login_with_nsec = move |_| {
        let nsec = nsec_input.read().clone();
        let password = nsec_password.read().clone();
        let confirm = nsec_confirm_password.read().clone();

        // Validate passwords match
        if password != confirm {
            error.set(Some("Passwords do not match".to_string()));
            return;
        }

        // Validate password strength
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
        div {
            class: "p-6 max-w-lg mx-auto",

            // Header with Learn More button
            div {
                class: "flex items-center justify-between mb-6",
                h3 {
                    class: "text-2xl font-bold text-gray-900 dark:text-white",
                    "Welcome to Nostr"
                }
                button {
                    class: "px-3 py-1.5 text-sm bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300 hover:bg-blue-200 dark:hover:bg-blue-800 rounded-lg transition",
                    onclick: move |_| show_help_modal.set(true),
                    "Learn More"
                }
            }

            p {
                class: "text-gray-600 dark:text-gray-400 mb-6",
                "Choose a secure sign-in method to get started with the decentralized social network."
            }

            // Error display
            if let Some(err) = error.read().as_ref() {
                div {
                    class: "mb-4 p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg text-sm",
                    "❌ {err}"
                }
            }

            // RECOMMENDED METHODS SECTION
            div {
                class: "mb-6",
                h4 {
                    class: "text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wide mb-3",
                    "Recommended (Secure)"
                }

                div {
                    class: "space-y-3",

                    // Browser Extension
                    if has_extension {
                        div {
                            class: "p-4 bg-gradient-to-r from-green-50 to-emerald-50 dark:from-green-900/20 dark:to-emerald-900/20 rounded-lg border-2 border-green-300 dark:border-green-700",
                            div {
                                class: "flex items-start gap-3 mb-3",
                                div {
                                    class: "text-2xl",
                                    "🔌"
                                }
                                div {
                                    class: "flex-1",
                                    div {
                                        class: "flex items-center gap-2 mb-1",
                                        span {
                                            class: "font-semibold text-gray-900 dark:text-white",
                                            "Browser Extension"
                                        }
                                        span {
                                            class: "px-2 py-0.5 text-xs bg-green-600 text-white rounded-full",
                                            "RECOMMENDED"
                                        }
                                    }
                                    p {
                                        class: "text-sm text-gray-600 dark:text-gray-400",
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

                    // Remote Signer (NIP-46)
                    div {
                        class: "p-4 bg-gradient-to-r from-blue-50 to-indigo-50 dark:from-blue-900/20 dark:to-indigo-900/20 rounded-lg border-2 border-blue-300 dark:border-blue-700",
                        div {
                            class: "flex items-start gap-3 mb-3",
                            div {
                                class: "text-2xl",
                                "🔐"
                            }
                            div {
                                class: "flex-1",
                                div {
                                    class: "flex items-center gap-2 mb-1",
                                    span {
                                        class: "font-semibold text-gray-900 dark:text-white",
                                        "Remote Signer"
                                    }
                                    span {
                                        class: "px-2 py-0.5 text-xs bg-blue-600 text-white rounded-full",
                                        "RECOMMENDED"
                                    }
                                }
                                p {
                                    class: "text-sm text-gray-600 dark:text-gray-400",
                                    "Use Amber, nsecBunker, or other NIP-46 signers. Keys never leave your device."
                                }
                            }
                        }
                        div {
                            class: "space-y-2",
                            input {
                                class: "w-full px-3 py-2 text-sm border border-blue-300 dark:border-blue-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                                r#type: "text",
                                placeholder: "bunker://...",
                                value: "{bunker_uri_input}",
                                oninput: move |evt| bunker_uri_input.set(evt.value()),
                                disabled: *connecting_bunker.read()
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
                                p {
                                    class: "text-xs text-blue-700 dark:text-blue-400 text-center",
                                    "Waiting for approval on your signing device (up to 2 minutes)..."
                                }
                            }
                        }
                    }
                }
            }

            // ADVANCED OPTIONS SECTION (Collapsible)
            div {
                class: "border-t border-gray-200 dark:border-gray-700 pt-6",

                button {
                    class: "w-full flex items-center justify-between p-3 bg-gray-100 dark:bg-gray-700 hover:bg-gray-200 dark:hover:bg-gray-600 rounded-lg transition",
                    onclick: move |_| {
                        let current = *show_advanced.read();
                        show_advanced.set(!current);
                    },
                    div {
                        class: "flex items-center gap-2",
                        span {
                            class: "text-yellow-600 dark:text-yellow-400",
                            "⚠️"
                        }
                        span {
                            class: "font-medium text-gray-900 dark:text-white",
                            "Advanced Options"
                        }
                    }
                    span {
                        class: "text-gray-500",
                        if *show_advanced.read() { "▼" } else { "▶" }
                    }
                }

                if *show_advanced.read() {
                    div {
                        class: "mt-4 p-4 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-700 rounded-lg space-y-4",

                        div {
                            class: "p-3 bg-yellow-100 dark:bg-yellow-900/30 rounded-lg",
                            p {
                                class: "text-sm text-yellow-800 dark:text-yellow-300 font-medium",
                                "⚠️ Security Warning"
                            }
                            p {
                                class: "text-xs text-yellow-700 dark:text-yellow-400 mt-1",
                                "These methods store keys in your browser. Only use on devices you fully trust."
                            }
                        }

                        // Private Key (nsec) with NIP-49 encryption
                        div {
                            h5 {
                                class: "font-medium text-gray-900 dark:text-white mb-2 text-sm",
                                "🔑 Private Key (nsec)"
                            }
                            div {
                                class: "space-y-2",
                                input {
                                    class: "w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white",
                                    r#type: "password",
                                    placeholder: "nsec1...",
                                    value: "{nsec_input}",
                                    oninput: move |evt| nsec_input.set(evt.value())
                                }

                                // Password fields for NIP-49 encryption
                                div {
                                    class: "relative",
                                    input {
                                        class: "w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white pr-10",
                                        r#type: if *show_nsec_password.read() { "text" } else { "password" },
                                        placeholder: "Set encryption password",
                                        value: "{nsec_password}",
                                        oninput: move |evt| nsec_password.set(evt.value())
                                    }
                                    button {
                                        class: "absolute right-2 top-1/2 -translate-y-1/2 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200",
                                        r#type: "button",
                                        onclick: move |_| {
                                            let current = *show_nsec_password.read();
                                            show_nsec_password.set(!current);
                                        },
                                        if *show_nsec_password.read() { "🙈" } else { "👁️" }
                                    }
                                }
                                input {
                                    class: "w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white",
                                    r#type: if *show_nsec_password.read() { "text" } else { "password" },
                                    placeholder: "Confirm password",
                                    value: "{nsec_confirm_password}",
                                    oninput: move |evt| nsec_confirm_password.set(evt.value())
                                }

                                // Password info
                                p {
                                    class: "text-xs text-amber-600 dark:text-amber-400",
                                    "🔐 Your key will be encrypted with this password"
                                }

                                div {
                                    class: "flex gap-2",
                                    button {
                                        class: "flex-1 px-3 py-2 text-sm bg-gray-700 hover:bg-gray-800 dark:bg-gray-600 dark:hover:bg-gray-700 text-white rounded-lg transition disabled:opacity-50 disabled:cursor-not-allowed",
                                        onclick: login_with_nsec,
                                        disabled: nsec_input.read().is_empty() || nsec_password.read().is_empty() || nsec_confirm_password.read().is_empty(),
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

                        // Public Key (npub)
                        div {
                            h5 {
                                class: "font-medium text-gray-900 dark:text-white mb-2 text-sm",
                                "👁️ Public Key (npub) - Read Only"
                            }
                            div {
                                class: "space-y-2",
                                input {
                                    class: "w-full px-3 py-2 text-sm border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white",
                                    r#type: "text",
                                    placeholder: "npub1...",
                                    value: "{npub_input}",
                                    oninput: move |evt| npub_input.set(evt.value())
                                }
                                button {
                                    class: "w-full px-3 py-2 text-sm bg-gray-700 hover:bg-gray-800 dark:bg-gray-600 dark:hover:bg-gray-700 text-white rounded-lg transition",
                                    onclick: login_with_npub,
                                    "View Profile (Read-Only)"
                                }
                                p {
                                    class: "text-xs text-gray-600 dark:text-gray-400",
                                    "ℹ️ You can browse but cannot post or interact."
                                }
                            }
                        }
                    }
                }
            }

            // Help Modal
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
        div {
            class: "bg-white dark:bg-gray-800 rounded-lg shadow-lg p-6",
            div {
                class: "flex justify-between items-start mb-4",
                h3 {
                    class: "text-xl font-semibold text-gray-900 dark:text-white",
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

            div {
                class: "space-y-3",
                div {
                    class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                    p {
                        class: "text-sm text-gray-600 dark:text-gray-400 mb-1",
                        "Public Key"
                    }
                    if let Some(pubkey) = &auth.pubkey {
                        Link {
                            to: Route::Profile { pubkey: pubkey.clone() },
                            class: "font-mono text-sm text-blue-600 dark:text-blue-400 hover:underline break-all",
                            "{pubkey}"
                        }
                    }
                }
                div {
                    class: "p-3 bg-gray-50 dark:bg-gray-700 rounded-lg",
                    p {
                        class: "text-sm text-gray-600 dark:text-gray-400 mb-1",
                        "Login Method"
                    }
                    p {
                        class: "text-gray-900 dark:text-white",
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
    // If no items returned at all, we've reached the end
    if new_items.is_empty() {
        log::info!("No more items from relay, reached end of feed");
        has_more.set(false);
        pagination_loading.set(false);
        return;
    }

    // Get current feed to deduplicate
    let current_state = feed_state.read().clone();
    if let DataState::Loaded(current) = current_state {
        // Build set of existing event IDs for O(1) lookup
        let existing_ids: std::collections::HashSet<_> = current.iter()
            .map(|item| item.event().id)
            .collect();

        // Filter out duplicates
        let unique_items: Vec<_> = new_items.iter()
            .filter(|item| !existing_ids.contains(&item.event().id))
            .cloned()
            .collect();

        log::info!("Deduplication: {} total, {} unique items after filtering",
            new_items.len(), unique_items.len());

        // Always update oldest_timestamp from ALL fetched items (not just unique)
        // to ensure we make progress even if all items were duplicates
        // Subtract 1 to avoid re-fetching posts at the exact boundary
        if let Some(last_item) = new_items.last() {
            let ts = last_item.sort_timestamp().as_secs().saturating_sub(1);
            oldest_timestamp.set(Some(ts));
        }

        // Keep has_more true as long as relay returns items
        // Only the empty check above should disable pagination
        // (duplicates just mean overlap, not end of feed)

        // Append unique items
        if !unique_items.is_empty() {
            let prefetch_items = unique_items.clone();
            let items_for_counts = unique_items.clone();
            let mut updated = current;
            updated.extend(unique_items);
            feed_state.set(DataState::Loaded(updated));

            // Spawn non-blocking background prefetch for missing metadata
            spawn(async move {
                prefetch_author_metadata(&prefetch_items).await;
            });

            // Fetch interaction counts for new items and merge with existing
            let mut counts_signal = *interaction_counts;
            spawn(async move {
                let event_ids: Vec<_> = items_for_counts.iter().map(|item| item.event().id).collect();
                if let Ok(new_counts) = fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await {
                    // Merge new counts with existing using Dioxus's WritableHashMapExt for in-place update
                    counts_signal.extend(new_counts);
                    log::info!("Fetched interaction counts for {} paginated items", items_for_counts.len());
                }
            });
        }
    }
    pagination_loading.set(false);
}

/// Load following feed (non-streaming version, used for pagination)
/// Returns (feed_items, did_fallback) where did_fallback indicates if we fell back to global.
async fn load_following_feed(until: Option<u64>) -> Result<(Vec<FeedItem>, bool), String> {
    // Get current user's pubkey
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    log::info!("Loading following feed for {} (until: {:?})", pubkey_str, until);

    // OPTIMIZATION: Fetch contacts AND global feed in parallel
    // If contacts fails or is empty, global feed is already ready
    let contacts_future = nostr_client::fetch_contacts(pubkey_str.clone());
    let global_future = load_global_feed(until);

    let (contacts_result, global_result) = futures::join!(contacts_future, global_future);

    // Handle contacts fetch result
    let contacts = match contacts_result {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            // Global feed was fetched in parallel, use it
            let global = global_result?;
            return Ok((global, true));
        }
    };

    // If user doesn't follow anyone, show global feed (already fetched)
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        let global = global_result?;
        return Ok((global, true));
    }

    log::info!("User follows {} accounts", contacts.len());

    // Parse contact pubkeys
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

    // Create filter for posts AND reposts from followed users
    // SDK gossip automatically fetches NIP-65 relay lists on demand for routing
    let mut filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .authors(authors)
        .limit(100);

    // Add until for pagination, no since filter to allow going back indefinitely
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    log::info!("Fetching events from {} followed accounts", filter.authors.as_ref().map(|a| a.len()).unwrap_or(0));

    // Use fast fetch (bypasses gossip) for pagination - avoids 30+ second timeouts
    // The initial feed load already populated the connected relays
    match nostr_client::fetch_events_from_connected_relays(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let raw_count = events.len();
            log::info!("Loaded {} events (including reposts) from following feed via outbox", raw_count);

            // Process events into FeedItems
            let mut feed_items: Vec<FeedItem> = Vec::new();

            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    // Parse repost to extract original event
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            // Include all reposts (regardless of whether original was a reply)
                            feed_items.push(FeedItem::Repost {
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
                    // Filter out replies - only include top-level posts
                    use nostr_sdk::TagKind;
                    let is_reply = event.tags.iter().any(|tag| tag.kind() == TagKind::e());
                    if !is_reply {
                        feed_items.push(FeedItem::OriginalPost(event));
                    }
                }
            }

            // Sort by timestamp (repost time for reposts, created_at for originals)
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));

            log::info!("After processing: {} feed items (raw: {})", feed_items.len(), raw_count);

            // If no events found, return empty (valid result - user's contacts just haven't posted)
            // Don't fall back to global here - empty following is different from no contacts
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
            // Parse repost to extract original event
            match extract_reposted_event(&event) {
                Ok(original) => {
                    feed_items.push(FeedItem::Repost {
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
            // Filter out replies - only include top-level posts
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

    // Get current user's pubkey
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    log::info!("Loading following feed (streaming) for {} (until: {:?})", pubkey_str, until);

    // Fetch contacts (need to wait for this before we can stream posts)
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            let global = load_global_feed(until).await?;
            return Ok((global, true));
        }
    };

    // If user doesn't follow anyone, show global feed
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        let global = load_global_feed(until).await?;
        return Ok((global, true));
    }

    log::info!("User follows {} accounts, streaming posts", contacts.len());

    // Parse contact pubkeys
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

    // Create filter for posts AND reposts from followed users
    // SDK gossip automatically fetches NIP-65 relay lists on demand for routing
    let mut filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .authors(authors)
        .limit(100);

    // Add until for pagination
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    // Track all items and seen IDs for deduplication
    let mut all_items: Vec<FeedItem> = Vec::new();
    let mut seen_ids: HashSet<nostr_sdk::EventId> = HashSet::new();

    // Stream events in batches of 10 for progressive UI updates
    // Use fast path that bypasses gossip discovery for initial feed load
    let stream_result = nostr_client::stream_events_from_connected_relays_batched(
        filter,
        Duration::from_secs(10),  // Shorter timeout since no gossip wait
        10, // batch size
        |batch| {
            // Process batch into FeedItems
            let mut batch_items = process_events_to_feed_items(batch);

            // Deduplicate against already seen items
            batch_items.retain(|item| seen_ids.insert(item.event().id));

            if !batch_items.is_empty() {
                // Sort this batch
                batch_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));

                // Add to total collection
                all_items.extend(batch_items.clone());

                // Call the callback to update UI
                on_batch(batch_items);
            }
        },
    ).await;

    if let Err(e) = stream_result {
        log::error!("Failed to stream following feed: {}, falling back to global", e);
        let global = load_global_feed(until).await?;
        return Ok((global, true));
    }

    // If no events found, return empty (valid result - user's contacts just haven't posted)
    // Don't fall back to global here - empty following is different from no contacts
    if all_items.is_empty() {
        log::info!("No posts from followed users via streaming");
        return Ok((Vec::new(), false));
    }

    // Final sort of all collected items
    all_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));

    log::info!("Streaming complete: {} feed items", all_items.len());
    Ok((all_items, false))
}

/// Load following feed with replies
/// Returns (feed_items, did_fallback) where did_fallback indicates if we fell back to global.
async fn load_following_with_replies(until: Option<u64>) -> Result<(Vec<FeedItem>, bool), String> {
    // Get current user's pubkey
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    log::info!("Loading following feed with replies for {} (until: {:?})", pubkey_str, until);

    // Fetch the user's contact list (people they follow)
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            let global = load_global_feed(until).await?;
            return Ok((global, true));
        }
    };

    // If user doesn't follow anyone, show global feed
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        let global = load_global_feed(until).await?;
        return Ok((global, true));
    }

    log::info!("User follows {} accounts", contacts.len());

    // Parse contact pubkeys
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

    // Create filter for all posts AND reposts from followed users (including replies)
    // Unlike load_following_feed, we include ALL posts (even replies)
    let mut filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .authors(authors)
        .limit(150); // Increased limit since we're getting more content

    // Add until for pagination, or since for initial load
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    } else {
        let since = Timestamp::now() - Duration::from_secs(86400); // 24 hours ago
        filter = filter.since(since);
    }

    log::info!("Fetching all events (including replies and reposts) from {} followed accounts", filter.authors.as_ref().map(|a| a.len()).unwrap_or(0));

    // Use fast fetch (bypasses gossip) for following feed - avoids 30+ second timeouts
    match nostr_client::fetch_events_from_connected_relays(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} events (including replies and reposts) from following feed via outbox", events.len());

            // Process events into FeedItems
            let mut feed_items: Vec<FeedItem> = Vec::new();

            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    // Parse repost to extract original event
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            // Include all reposts (regardless of whether original was a reply)
                            feed_items.push(FeedItem::Repost {
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
                    // Include ALL posts (including replies)
                    feed_items.push(FeedItem::OriginalPost(event));
                }
            }

            // Sort by timestamp (repost time for reposts, created_at for originals)
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));

            // If no events found, return empty (valid result - user's contacts just haven't posted)
            // Don't fall back to global here - empty following is different from no contacts
            if feed_items.is_empty() {
                log::info!("No events from followed users");
                return Ok((Vec::new(), false));
            }

            Ok((feed_items, false))
        }
        Err(e) => {
            log::error!("Failed to fetch following feed with replies: {}, falling back to global", e);
            let global = load_global_feed(until).await?;
            Ok((global, true))
        }
    }
}

// Helper function to load global feed
async fn load_global_feed(until: Option<u64>) -> Result<Vec<FeedItem>, String> {
    log::info!("Loading global feed (until: {:?})...", until);

    // Create filter for recent text notes and reposts (kind 1 and kind 6)
    let mut filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .limit(50);

    // Add until for pagination, or since for initial load
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    } else {
        let since = Timestamp::now() - Duration::from_secs(86400); // 24 hours ago
        filter = filter.since(since);
    }

    log::info!("Fetching events with filter: {:?}", filter);

    // Fetch events using aggregated pattern (database-first)
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} events", events.len());

            // Process events into FeedItems
            let mut feed_items: Vec<FeedItem> = Vec::new();

            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    // Parse repost to extract original event
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            feed_items.push(FeedItem::Repost {
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
                    feed_items.push(FeedItem::OriginalPost(event));
                }
            }

            // Sort by timestamp (repost time for reposts, created_at for originals)
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));

            Ok(feed_items)
        }
        Err(e) => {
            log::error!("Failed to fetch events: {}", e);
            Err(format!("Failed to load feed: {}", e))
        }
    }
}

/// Batch prefetch author metadata for all feed items
/// This checks the database first and only fetches missing metadata
/// For reposts, it fetches both the original author AND the reposter
async fn prefetch_author_metadata(feed_items: &[FeedItem]) {
    use crate::utils::profile_prefetch;

    // Extract all unique pubkeys (original authors + reposters)
    let mut pubkeys = Vec::new();
    for item in feed_items {
        match item {
            FeedItem::OriginalPost(event) => {
                pubkeys.push(event.pubkey);
            }
            FeedItem::Repost { original, reposted_by, .. } => {
                pubkeys.push(original.pubkey); // Original author
                pubkeys.push(*reposted_by);     // Reposter
            }
        }
    }

    // Deduplicate pubkeys
    pubkeys.sort();
    pubkeys.dedup();

    // Use optimized prefetch utility
    profile_prefetch::prefetch_pubkeys(pubkeys).await;
}

/// Helper function to load feed from a people list
/// Fetches posts from all members (public + private) of the list
async fn load_people_list_feed(list: &UserList, until: Option<u64>) -> Result<Vec<FeedItem>, String> {
    log::info!("Loading people list feed for '{}' (until: {:?})", list.name, until);

    // Get all members from the list (both public and private via NIP-44 decryption)
    let members = get_all_list_members(&list.event).await.map_err(|e| {
        log::error!("Failed to get list members: {}", e);
        format!("Failed to decrypt list members: {}", e)
    })?;

    if members.is_empty() {
        log::info!("People list '{}' has no members", list.name);
        return Ok(Vec::new());
    }

    log::info!("People list '{}' has {} members", list.name, members.len());

    // Create filter for posts AND reposts from list members
    let mut filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .authors(members)
        .limit(100);

    // Add until for pagination
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    // Fetch events using gossip/outbox routing for automatic relay selection based on authors
    // This routes queries to each list member's preferred write relays for better discovery
    match nostr_client::fetch_events_aggregated_outbox(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} events from people list '{}' via outbox", events.len(), list.name);

            // Process events into FeedItems
            let mut feed_items: Vec<FeedItem> = Vec::new();

            for event in events.into_iter() {
                if event.kind == Kind::Repost {
                    // Parse repost to extract original event
                    match extract_reposted_event(&event) {
                        Ok(original) => {
                            feed_items.push(FeedItem::Repost {
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
                    // Include all posts (including replies) for people list feeds
                    feed_items.push(FeedItem::OriginalPost(event));
                }
            }

            // Sort by timestamp (repost time for reposts, created_at for originals)
            feed_items.sort_by_key(|item| std::cmp::Reverse(item.sort_timestamp()));

            Ok(feed_items)
        }
        Err(e) => {
            log::error!("Failed to fetch events for people list '{}': {}", list.name, e);
            Err(format!("Failed to load feed: {}", e))
        }
    }
}
