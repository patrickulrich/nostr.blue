use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::routes::Route;
use crate::components::{NoteCard, NoteComposer, ArticleCard, ClientInitializing};
use crate::hooks::use_infinite_scroll;
use crate::utils::{DataState, FeedItem, extract_reposted_event};
use crate::services::aggregation::{InteractionCounts, fetch_interaction_counts_batch};
use nostr_sdk::{Filter, Kind, Timestamp, PublicKey};
use std::time::Duration;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Debug)]
enum FeedType {
    Following,          // Top level posts only
    FollowingWithReplies, // All posts including replies
}

impl FeedType {
    fn label(&self) -> &'static str {
        match self {
            FeedType::Following => "Following",
            FeedType::FollowingWithReplies => "Following + Replies",
        }
    }
}

#[component]
pub fn Home() -> Element {
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
    let mut interaction_counts = use_signal(|| HashMap::<String, InteractionCounts>::new());

    // Buffer for real-time events (Twitter/X pattern: "Show N new posts")
    let mut pending_posts = use_signal(|| Vec::<FeedItem>::new());
    let mut pending_count = use_signal(|| 0);

    // Load feed on mount and when refresh is triggered or feed type changes
    use_effect(move || {
        // Watch refresh trigger and feed type
        let _ = refresh_trigger.read();
        let current_feed_type = *feed_type.read();

        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load feed if both authenticated AND client is initialized
        if is_authenticated && client_initialized {
            feed_state.set(DataState::Loading);
            oldest_timestamp.set(None);
            has_more.set(true);

            // Clear pending posts buffer on refresh
            pending_posts.set(Vec::new());
            pending_count.set(0);

            // Clear profile cache to prevent stale author metadata on refresh
            crate::stores::profiles::PROFILE_CACHE.write().clear();

            spawn(async move {
                match current_feed_type {
                    FeedType::Following => {
                        match load_following_feed(None).await {
                            Ok((feed_items, raw_count)) => {
                                // Track oldest timestamp for pagination
                                if let Some(last_item) = feed_items.last() {
                                    oldest_timestamp.set(Some(last_item.sort_timestamp().as_secs()));
                                }

                                // Determine if there are more events based on RAW count before filtering
                                has_more.set(raw_count >= 100);

                                // Display feed immediately (NoteCard shows fallback until metadata loads)
                                feed_state.set(DataState::Loaded(feed_items.clone()));

                                // Batch fetch interaction counts for all events (99% query reduction!)
                                let items_for_counts = feed_items.clone();
                                spawn(async move {
                                    let event_ids: Vec<_> = items_for_counts.iter().map(|item| item.event().id).collect();
                                    if let Ok(counts) = fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await {
                                        interaction_counts.set(counts);
                                    }
                                });

                                // Spawn non-blocking background prefetch for metadata
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                            Err(e) => {
                                feed_state.set(DataState::Error(e));
                            }
                        }
                    }
                    FeedType::FollowingWithReplies => {
                        match load_following_with_replies(None).await {
                            Ok(feed_items) => {
                                // Track oldest timestamp for pagination
                                if let Some(last_item) = feed_items.last() {
                                    oldest_timestamp.set(Some(last_item.sort_timestamp().as_secs()));
                                }

                                // Determine if there are more events to load
                                has_more.set(feed_items.len() >= 150);

                                // Display feed immediately (NoteCard shows fallback until metadata loads)
                                feed_state.set(DataState::Loaded(feed_items.clone()));

                                // Batch fetch interaction counts for all events (99% query reduction!)
                                let items_for_counts = feed_items.clone();
                                spawn(async move {
                                    let event_ids: Vec<_> = items_for_counts.iter().map(|item| item.event().id).collect();
                                    if let Ok(counts) = fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await {
                                        interaction_counts.set(counts);
                                    }
                                });

                                // Spawn non-blocking background prefetch for metadata
                                spawn(async move {
                                    prefetch_author_metadata(&feed_items).await;
                                });
                            }
                            Err(e) => {
                                feed_state.set(DataState::Error(e));
                            }
                        }
                    }
                }
            });
        }
    });

    // Real-time subscription for live feed updates (starts AFTER initial load)
    use_effect(move || {
        let current_feed_type = *feed_type.read();
        let current_state = feed_state.read().clone();
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only subscribe if authenticated, initialized, AND feed is loaded
        // This prevents race conditions during initial load
        if !is_authenticated || !client_initialized {
            return;
        }

        // Wait until feed is loaded before starting real-time subscription
        if !matches!(current_state, DataState::Loaded(_)) {
            return;
        }

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
            let num_batches = (total_authors + BATCH_SIZE - 1) / BATCH_SIZE;

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
                        TimeoutFuture::new((batch_idx as u32 * BATCH_DELAY_MS as u32) as u32).await;
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
                    .since(Timestamp::now())
                    .limit(0); // limit=0 means only new events

                log::info!("Subscribing to batch {}/{} ({} authors)",
                    batch_num, num_batches, batch_authors.len());

                match client.subscribe(filter, None).await {
                    Ok(output) => {
                        let subscription_id = output.val;
                        log::info!("Batch {}/{} subscribed: {:?}", batch_num, num_batches, subscription_id);

                        // Handle incoming events for this batch
                        let client_for_notifications = client.clone();
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
                                        let should_add = match current_feed_type {
                                            FeedType::Following => {
                                                // Only top-level posts (no e tags)
                                                !event.tags.iter().any(|tag| tag.kind() == nostr_sdk::TagKind::e())
                                            }
                                            FeedType::FollowingWithReplies => {
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

                                        let already_in_feed = if let DataState::Loaded(current_items) = feed_state.read().clone() {
                                            current_items.iter().any(|item| item.event().id == event_id)
                                        } else {
                                            false
                                        };

                                        if !already_buffered && !already_in_feed {
                                            // Add to pending buffer
                                            pending_posts.write().push(feed_item);
                                            let new_count = *pending_count.read() + 1;
                                            pending_count.set(new_count);
                                            log::info!("Buffered new post, total pending: {}", new_count);
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
            let current_feed_type = *feed_type.read();

            log::info!("load_more spawn executing - until: {:?}, feed_type: {:?}", until, current_feed_type);

            match current_feed_type {
                FeedType::Following => {
                    match load_following_feed(until).await {
                        Ok((mut new_items, raw_count)) => {
                            // Track oldest timestamp from new items
                            if let Some(last_item) = new_items.last() {
                                oldest_timestamp.set(Some(last_item.sort_timestamp().as_secs()));
                            }

                            // Determine if there are more events based on RAW count before filtering
                            has_more.set(raw_count >= 100);

                            // Append and show new items immediately
                            let prefetch_items = new_items.clone();
                            let current_state = feed_state.read().clone();
                            if let DataState::Loaded(mut current) = current_state {
                                current.append(&mut new_items);
                                feed_state.set(DataState::Loaded(current));
                            }
                            pagination_loading.set(false);

                            // Spawn non-blocking background prefetch for missing metadata
                            spawn(async move {
                                prefetch_author_metadata(&prefetch_items).await;
                            });
                        }
                        Err(e) => {
                            log::error!("Failed to load more events: {}", e);
                            pagination_loading.set(false);
                        }
                    }
                }
                FeedType::FollowingWithReplies => {
                    match load_following_with_replies(until).await {
                        Ok(mut new_items) => {
                            // Track oldest timestamp from new items
                            if let Some(last_item) = new_items.last() {
                                oldest_timestamp.set(Some(last_item.sort_timestamp().as_secs()));
                            }

                            // Determine if there are more events to load
                            has_more.set(new_items.len() >= 150);

                            // Append and show new items immediately
                            let prefetch_items = new_items.clone();
                            let current_state = feed_state.read().clone();
                            if let DataState::Loaded(mut current) = current_state {
                                current.append(&mut new_items);
                                feed_state.set(DataState::Loaded(current));
                            }
                            pagination_loading.set(false);

                            // Spawn non-blocking background prefetch for missing metadata
                            spawn(async move {
                                prefetch_author_metadata(&prefetch_items).await;
                            });
                        }
                        Err(e) => {
                            log::error!("Failed to load more events: {}", e);
                            pagination_loading.set(false);
                        }
                    }
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

    // Handler to merge pending posts into feed (Twitter/X pattern)
    let show_new_posts = move |_| {
        let mut pending = pending_posts.read().clone();

        if !pending.is_empty() {
            let pending_len = pending.len();

            // Sort pending posts by timestamp (newest first)
            pending.sort_by(|a, b| b.sort_timestamp().cmp(&a.sort_timestamp()));

            // Get current items and clone to avoid borrow checker issues
            let current_items = {
                if let DataState::Loaded(items) = feed_state.read().clone() {
                    Some(items)
                } else {
                    None
                }
            };

            if let Some(current_items) = current_items {
                // Prepend pending posts to feed
                let mut new_items = pending;
                new_items.extend(current_items);

                feed_state.set(DataState::Loaded(new_items));

                // Clear pending buffer
                pending_posts.set(Vec::new());
                pending_count.set(0);

                log::info!("Merged {} new posts into feed", pending_len);
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
                            onclick: move |_| {
                                let current = *refresh_trigger.read();
                                refresh_trigger.set(current + 1);
                            },
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
                    // Show loading animation
                    ClientInitializing {}
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
                                        onclick: show_new_posts,
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
                                            event: event.clone(),
                                            repost_info: repost_info,
                                            precomputed_counts: interaction_counts.read().get(&event.id.to_hex()).cloned()
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
    let mut nsec_input = use_signal(|| String::new());
    let mut npub_input = use_signal(|| String::new());
    let mut bunker_uri_input = use_signal(|| String::new());
    let mut error = use_signal(|| None::<String>);
    let mut show_advanced = use_signal(|| false);
    let mut show_help_modal = use_signal(|| false);
    let mut connecting_bunker = use_signal(|| false);

    // Login handlers
    let login_with_nsec = move |_| {
        let nsec = nsec_input.read().clone();
        spawn(async move {
            match auth_store::login_with_nsec(&nsec).await {
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
                                class: "w-full px-4 py-2.5 bg-green-600 hover:bg-green-700 text-white rounded-lg font-medium transition shadow-sm",
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
                                class: "w-full px-4 py-2.5 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition shadow-sm disabled:opacity-50 disabled:cursor-not-allowed",
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

                        // Private Key (nsec)
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
                                div {
                                    class: "flex gap-2",
                                    button {
                                        class: "flex-1 px-3 py-2 text-sm bg-gray-700 hover:bg-gray-800 dark:bg-gray-600 dark:hover:bg-gray-700 text-white rounded-lg transition",
                                        onclick: login_with_nsec,
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
                            nav.push(Route::Home {});
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

// Helper function to load following feed
// Returns (feed_items, raw_count_before_filtering) tuple
async fn load_following_feed(until: Option<u64>) -> Result<(Vec<FeedItem>, usize), String> {
    // TODO: Consider implementing progressive loading with client.stream_events() for better UX
    // This would display events as they arrive instead of waiting for all results

    // Get current user's pubkey
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    log::info!("Loading following feed for {} (until: {:?})", pubkey_str, until);

    // Fetch the user's contact list (people they follow)
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            let global = load_global_feed(until).await?;
            let count = global.len();
            return Ok((global, count));
        }
    };

    // If user doesn't follow anyone, show global feed
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        let global = load_global_feed(until).await?;
        let count = global.len();
        return Ok((global, count));
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
        let count = global.len();
        return Ok((global, count));
    }

    // Create filter for posts AND reposts from followed users
    let mut filter = Filter::new()
        .kinds(vec![Kind::TextNote, Kind::Repost])
        .authors(authors)
        .limit(100);

    // Add until for pagination, no since filter to allow going back indefinitely
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    log::info!("Fetching events from {} followed accounts", filter.authors.as_ref().map(|a| a.len()).unwrap_or(0));

    // Fetch events using aggregated pattern (database-first)
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let raw_count = events.len();
            log::info!("Loaded {} events (including reposts) from following feed", raw_count);

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
            feed_items.sort_by(|a, b| b.sort_timestamp().cmp(&a.sort_timestamp()));

            log::info!("After processing: {} feed items (raw: {})", feed_items.len(), raw_count);

            // If no events found, fall back to global feed
            if feed_items.is_empty() {
                log::info!("No posts from followed users, showing global feed");
                let global = load_global_feed(until).await?;
                let count = global.len();
                return Ok((global, count));
            }

            Ok((feed_items, raw_count))
        }
        Err(e) => {
            log::error!("Failed to fetch following feed: {}, falling back to global", e);
            let global = load_global_feed(until).await?;
            let count = global.len();
            Ok((global, count))
        }
    }
}

// Helper function to load following feed with replies
async fn load_following_with_replies(until: Option<u64>) -> Result<Vec<FeedItem>, String> {
    // Get current user's pubkey
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    log::info!("Loading following feed with replies for {} (until: {:?})", pubkey_str, until);

    // Fetch the user's contact list (people they follow)
    let contacts = match nostr_client::fetch_contacts(pubkey_str.clone()).await {
        Ok(contacts) => contacts,
        Err(e) => {
            log::warn!("Failed to fetch contacts: {}, falling back to global feed", e);
            return load_global_feed(until).await;
        }
    };

    // If user doesn't follow anyone, show global feed
    if contacts.is_empty() {
        log::info!("User doesn't follow anyone, showing global feed");
        return load_global_feed(until).await;
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
        return load_global_feed(until).await;
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

    // Fetch events using aggregated pattern (database-first)
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} events (including replies and reposts) from following feed", events.len());

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
            feed_items.sort_by(|a, b| b.sort_timestamp().cmp(&a.sort_timestamp()));

            // If no events found, fall back to global feed
            if feed_items.is_empty() {
                log::info!("No events from followed users, showing global feed");
                return load_global_feed(until).await;
            }

            Ok(feed_items)
        }
        Err(e) => {
            log::error!("Failed to fetch following feed with replies: {}, falling back to global", e);
            load_global_feed(until).await
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
            feed_items.sort_by(|a, b| b.sort_timestamp().cmp(&a.sort_timestamp()));

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
