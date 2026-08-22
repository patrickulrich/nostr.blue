use crate::components::icons::LockIcon;
use crate::components::{
    ArticleCard, ClientInitializing, NoteCard, PhotoCard, PollCard, VideoCard,
    VoiceMessageCard,
};
use crate::error::NostrBlueError;
use crate::hooks::{use_infinite_scroll_with_generation, use_mute_block_cache};
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client, notifications as notif_store, profiles};
use crate::utils::bolt11::parse_bolt11_amount;
use crate::utils::debounced_collector::DebouncedCollector;
use crate::utils::nips::dip03;
use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, Filter, Kind, TagStandard, Timestamp};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;
use futures::join;
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
enum NotificationType {
    Mention(NostrEvent),
    Reply(NostrEvent),
    Reaction(NostrEvent),
    Repost(NostrEvent),
    Quote(NostrEvent),
    Zap(NostrEvent),
}
#[derive(Clone, Copy, PartialEq)]
enum NotificationFilter {
    All,
    Replies,
    Mentions,
    Reactions,
    Reposts,
    Zaps,
}
impl NotificationFilter {
    fn label(&self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Replies => "Replies",
            Self::Mentions => "Mentions",
            Self::Reactions => "Reactions",
            Self::Reposts => "Reposts",
            Self::Zaps => "Zaps",
        }
    }
    fn matches(&self, notification: &NotificationType) -> bool {
        match self {
            Self::All => true,
            Self::Replies => matches!(notification, NotificationType::Reply(_)),
            Self::Mentions => matches!(notification, NotificationType::Mention(_)),
            Self::Reactions => matches!(notification, NotificationType::Reaction(_)),
            Self::Reposts => matches!(
                notification,
                NotificationType::Repost(_) | NotificationType::Quote(_)
            ),
            Self::Zaps => matches!(notification, NotificationType::Zap(_)),
        }
    }
}
struct DailySummary {
    replies: usize,
    reactions: usize,
    reposts: usize,
    mentions: usize,
    zap_sats: u64,
}

fn route_for_event(event: &NostrEvent) -> Route {
    crate::utils::route_for_kind::route_for_event(event)
}

#[component]
fn EventCard(
    event: NostrEvent,
    #[props(default = true)] collapsible: bool,
    #[props(default = None)] cached_muted_posts: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_blocked_users: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_muted_words: Option<Rc<HashSet<String>>>,
) -> Element {
    match event.kind.as_u16() {
        20 => rsx! { PhotoCard { event } },
        21 | 22 => rsx! { VideoCard { event } },
        1040 => rsx! { VoiceMessageCard { event } },
        1068 => rsx! { PollCard { event } },
        30023 => rsx! { ArticleCard { event } },
        _ => rsx! {
            NoteCard {
                event,
                collapsible,
                cached_muted_posts,
                cached_blocked_users,
                cached_muted_words,
            }
        },
    }
}

#[component]
pub fn Notifications() -> Element {
    let mut notifications = use_signal(Vec::<NotificationType>::new);
    let mut loading = use_signal(|| true);
    let mut refreshing = use_signal(|| false);
    let mut error = use_signal(|| None::<NostrBlueError>);
    let mut active_filter = use_signal(|| NotificationFilter::All);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut load_generation = use_signal(|| 0u64);
    let mut feed_reset_generation = use_signal(|| 0u64);
    // Both a pull-to-refresh (clears the list) and switching `active_filter`
    // (can empty the filtered view) unmount the sentinel while `has_more`
    // stays true. Bump the generation so the observer re-attaches.
    use_effect(move || {
        let _ = *active_filter.read();
        feed_reset_generation += 1;
    });
    let mut active_task: Signal<Option<dioxus_core::Task>> = use_signal(|| None);
    let (cached_muted_posts, cached_blocked_users, cached_muted_words) = use_mute_block_cache();
    use_effect(move || {
        // Cancel any previously spawned load task to prevent concurrent streams
        if let Some(task) = active_task.write().take() {
            task.cancel();
        }

        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        let has_signer = *nostr_client::HAS_SIGNER.read();
        let auth_state = auth_store::AUTH_STATE.read();
        let is_authenticated = auth_state.is_authenticated;
        let login_method = auth_state.login_method.clone();
        drop(auth_state); // Release lock before async operations

        // Wait for client initialization
        if !client_initialized {
            return;
        }

        // Notifications require authentication
        if !is_authenticated {
            return;
        }

        // For authenticated users with signing capability, wait for signer restoration
        // This prevents race condition where CLIENT_INITIALIZED is true but
        // restore_session_async() hasn't attached the signer yet
        // ReadOnly (npub) users don't need a signer and should bypass this guard
        let requires_signer = matches!(
            login_method,
            Some(auth_store::LoginMethod::BrowserExtension)
                | Some(auth_store::LoginMethod::PrivateKey)
                | Some(auth_store::LoginMethod::RemoteSigner)
        ) || {
            #[cfg(feature = "mobile_platform")]
            {
                matches!(login_method, Some(auth_store::LoginMethod::AndroidSigner))
            }
            #[cfg(not(feature = "mobile_platform"))]
            {
                false
            }
        };
        if requires_signer && !has_signer {
            log::debug!("Waiting for signer restoration before loading notifications...");
            return;
        }

        // Increment generation for stale detection (matches polls/home.rs pattern)
        load_generation.with_mut(|v| *v = v.wrapping_add(1));
        let current_gen = *load_generation.peek();

        let now = Timestamp::now().as_secs() as i64;
        notif_store::set_checked_at(now);
        loading.set(true);
        error.set(None);
        crate::stores::notification_event_cache::clear_notification_event_cache();

        log::debug!("Notifications effect spawning stream (gen={current_gen})");

        // Use streaming for progressive loading
        let task = spawn(async move {
            let initial_pubkey = auth_store::get_pubkey();

            // Wait for relays before fetching (NIP-46 timing)
            nostr_client::wait_for_user_relays(
                Duration::from_secs(5),
                "notifications_initial_load",
            )
            .await;

            // Stale check after relay wait
            if *load_generation.peek() != current_gen {
                log::debug!("Notification load stale after relay wait, aborting");
                return;
            }

            // Abort if user changed during relay wait
            if auth_store::get_pubkey() != initial_pubkey {
                log::debug!("Pubkey changed during relay wait, aborting notification load");
                loading.set(false);
                return;
            }

            let mut seen_ids: HashSet<nostr_sdk::EventId> = HashSet::new();

            let collector = DebouncedCollector::<NotificationType>::new(50);
            let result = stream_notifications(None, |notif| {
                // Stale check in stream callback
                if *load_generation.peek() != current_gen {
                    return;
                }
                let event_id = get_event_id(&notif);

                // SDK pattern: insert() returns true if newly inserted (atomic check-and-insert)
                if seen_ids.insert(event_id) {
                    collector.extend([notif], {
                        let mut notifications = notifications;
                        move |batch| {
                            // Stale check in flush callback
                            if *load_generation.peek() != current_gen {
                                return;
                            }
                            let mut current = notifications.peek().clone();
                            // Defense-in-depth: dedup against existing items
                            let mut existing: HashSet<nostr_sdk::EventId> =
                                current.iter().map(get_event_id).collect();
                            current.extend(
                                batch
                                    .into_iter()
                                    .filter(|n| existing.insert(get_event_id(n))),
                            );
                            current.sort_by_key(|n| std::cmp::Reverse(get_timestamp(n)));
                            notifications.set(current);
                        }
                    });
                }
            })
            .await;

            // Stale check before tail flush
            if *load_generation.peek() != current_gen {
                return;
            }

            // Flush tail items buffered after the last debounce window.
            let tail = collector.drain();
            if !tail.is_empty() {
                let mut current = notifications.peek().clone();
                let mut existing: HashSet<nostr_sdk::EventId> =
                    current.iter().map(get_event_id).collect();
                current.extend(tail.into_iter().filter(|n| existing.insert(get_event_id(n))));
                current.sort_by_key(|n| std::cmp::Reverse(get_timestamp(n)));
                notifications.set(current);
            }

            match result {
                Ok(count) => {
                    if *load_generation.peek() != current_gen {
                        return;
                    }
                    log::debug!("Notifications stream completed: {count} events (gen={current_gen})");
                    if count > 0 {
                        // Dioxus pattern: peek() in async, not read()
                        let notifs = notifications.peek().clone();
                        let oldest = notifs.iter().map(get_timestamp).min();
                        oldest_timestamp.set(oldest);
                        has_more.set(count >= 100);
                        spawn(async move {
                            join!(
                                prefetch_notification_authors(&notifs),
                                prefetch_notification_posts(&notifs),
                            );
                        });
                    } else {
                        has_more.set(false);
                    }
                }
                Err(e) => {
                    // SDK pattern: log error but continue gracefully
                    log::error!("Failed to stream notifications: {:?}", e);
                    error.set(Some(e));
                    has_more.set(false);
                }
            }
            if *load_generation.peek() == current_gen {
                loading.set(false);
            }
        });
        active_task.set(Some(task));
    });
    let handle_refresh = move |_| {
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        if !is_authenticated || *refreshing.read() {
            return;
        }
        // Cancel any active task and bump generation
        if let Some(task) = active_task.write().take() {
            task.cancel();
        }
        load_generation.with_mut(|v| *v = v.wrapping_add(1));
        let current_gen = *load_generation.peek();

        refreshing.set(true);
        // Clear existing notifications for fresh load
        notifications.set(Vec::new());
        feed_reset_generation += 1;

        let task = spawn(async move {
            let mut seen_ids: HashSet<nostr_sdk::EventId> = HashSet::new();

            let collector = DebouncedCollector::<NotificationType>::new(50);
            let result = stream_notifications(None, |notif| {
                if *load_generation.peek() != current_gen {
                    return;
                }
                let event_id = get_event_id(&notif);
                if seen_ids.insert(event_id) {
                    collector.extend([notif], {
                        let mut notifications = notifications;
                        move |batch| {
                            if *load_generation.peek() != current_gen {
                                return;
                            }
                            let mut current = notifications.peek().clone();
                            let mut existing: HashSet<nostr_sdk::EventId> =
                                current.iter().map(get_event_id).collect();
                            current.extend(
                                batch
                                    .into_iter()
                                    .filter(|n| existing.insert(get_event_id(n))),
                            );
                            current.sort_by_key(|n| std::cmp::Reverse(get_timestamp(n)));
                            notifications.set(current);
                        }
                    });
                }
            })
            .await;
            if *load_generation.peek() != current_gen {
                return;
            }
            // Flush tail items buffered after the last debounce window.
            let tail = collector.drain();
            if !tail.is_empty() {
                let mut current = notifications.peek().clone();
                let mut existing: HashSet<nostr_sdk::EventId> =
                    current.iter().map(get_event_id).collect();
                current.extend(tail.into_iter().filter(|n| existing.insert(get_event_id(n))));
                current.sort_by_key(|n| std::cmp::Reverse(get_timestamp(n)));
                notifications.set(current);
            }

            match result {
                Ok(count) => {
                    if *load_generation.peek() != current_gen {
                        return;
                    }
                    if count > 0 {
                        let notifs = notifications.peek().clone();
                        let oldest = notifs.iter().map(get_timestamp).min();
                        oldest_timestamp.set(oldest);
                        has_more.set(count >= 100);
                        spawn(async move {
                            join!(
                                prefetch_notification_authors(&notifs),
                                prefetch_notification_posts(&notifs),
                            );
                        });
                    }
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            if *load_generation.peek() == current_gen {
                refreshing.set(false);
            }
        });
        active_task.set(Some(task));
    };
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }
        // Bump generation to invalidate any concurrent loads
        load_generation.with_mut(|v| *v = v.wrapping_add(1));
        let current_gen = *load_generation.peek();

        let until = *oldest_timestamp.read();
        loading.set(true);

        let task = spawn(async move {
            // Get existing IDs to avoid duplicates when loading more
            let existing_ids: HashSet<nostr_sdk::EventId> =
                notifications.peek().iter().map(get_event_id).collect();
            let mut seen_ids = existing_ids.clone();
            let initial_count = seen_ids.len();

            let collector = DebouncedCollector::<NotificationType>::new(50);
            let result = stream_notifications(until, |notif| {
                if *load_generation.peek() != current_gen {
                    return;
                }
                let event_id = get_event_id(&notif);
                if seen_ids.insert(event_id) {
                    collector.extend([notif], {
                        let mut notifications = notifications;
                        move |batch| {
                            if *load_generation.peek() != current_gen {
                                return;
                            }
                            let mut current = notifications.peek().clone();
                            let mut existing: HashSet<nostr_sdk::EventId> =
                                current.iter().map(get_event_id).collect();
                            current.extend(
                                batch
                                    .into_iter()
                                    .filter(|n| existing.insert(get_event_id(n))),
                            );
                            current.sort_by_key(|n| std::cmp::Reverse(get_timestamp(n)));
                            notifications.set(current);
                        }
                    });
                }
            })
            .await;
            if *load_generation.peek() != current_gen {
                return;
            }
            // Flush tail items buffered after the last debounce window.
            let tail = collector.drain();
            if !tail.is_empty() {
                let mut current = notifications.peek().clone();
                let mut existing: HashSet<nostr_sdk::EventId> =
                    current.iter().map(get_event_id).collect();
                current.extend(tail.into_iter().filter(|n| existing.insert(get_event_id(n))));
                current.sort_by_key(|n| std::cmp::Reverse(get_timestamp(n)));
                notifications.set(current);
            }

            match result {
                Ok(count) => {
                    if *load_generation.peek() != current_gen {
                        return;
                    }
                    let new_count = seen_ids.len() - initial_count;
                    if new_count > 0 {
                        let notifs = notifications.peek().clone();
                        let oldest = notifs.iter().map(get_timestamp).min();
                        oldest_timestamp.set(oldest);
                        has_more.set(count >= 100);
                        let new_notifs: Vec<_> = notifs
                            .iter()
                            .filter(|n| !existing_ids.contains(&get_event_id(n)))
                            .cloned()
                            .collect::<Vec<_>>();
                        spawn(async move {
                            join!(
                                prefetch_notification_authors(&new_notifs),
                                prefetch_notification_posts(&new_notifs),
                            );
                        });
                    } else {
                        has_more.set(false);
                    }
                }
                Err(_) => {
                    has_more.set(false);
                }
            }
            if *load_generation.peek() == current_gen {
                loading.set(false);
            }
        });
        active_task.set(Some(task));
    };
    let sentinel_id =
        use_infinite_scroll_with_generation(load_more, has_more, loading, feed_reset_generation);
    let auth = auth_store::AUTH_STATE.read();
    let filtered_notifications: Vec<NotificationType> = notifications
        .read()
        .iter()
        .filter(|n| active_filter.read().matches(n))
        .cloned()
        .collect();
    let summary = {
        let all = notifications.read();
        let now_secs = crate::platform::timestamp::now_secs();
        let today_start = now_secs - (now_secs % 86400);
        let today: Vec<_> = all
            .iter()
            .filter(|n| get_timestamp(n) >= today_start)
            .collect();
        let replies = today
            .iter()
            .filter(|n| matches!(n, NotificationType::Reply(_)))
            .count();
        let reactions = today
            .iter()
            .filter(|n| matches!(n, NotificationType::Reaction(_)))
            .count();
        let reposts = today
            .iter()
            .filter(|n| {
                matches!(
                    n,
                    NotificationType::Repost(_) | NotificationType::Quote(_)
                )
            })
            .count();
        let mentions = today
            .iter()
            .filter(|n| matches!(n, NotificationType::Mention(_)))
            .count();
        let zap_sats: u64 = today
            .iter()
            .filter_map(|n| match n {
                NotificationType::Zap(e) => extract_zap_amount(e),
                _ => None,
            })
            .sum();
        DailySummary { replies, reactions, reposts, mentions, zap_sats }
    };
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    h2 { class: "text-xl font-bold", "🔔 Notifications" }
                    if auth.is_authenticated {
                        button {
                            class: "p-2 hover:bg-accent rounded-lg transition",
                            onclick: handle_refresh,
                            disabled: *refreshing.read(),
                            span { class: if *refreshing.read() { "inline-block animate-spin" } else { "" },
                                "🔄"
                            }
                        }
                    }
                }
                if auth.is_authenticated {
                    div { class: "px-4 pb-2 overflow-x-auto",
                        div { class: "flex gap-2 min-w-max",
                            for filter in [
                                NotificationFilter::All,
                                NotificationFilter::Replies,
                                NotificationFilter::Mentions,
                                NotificationFilter::Reactions,
                                NotificationFilter::Reposts,
                                NotificationFilter::Zaps,
                            ]
                            {
                                {
                                    let is_active = *active_filter.read() == filter;
                                    rsx! {
                                        button {
                                            key: "{filter.label()}",
                                            class: "px-4 py-2 text-sm rounded-lg transition relative",
                                            class: if is_active { "font-semibold" } else { "text-muted-foreground hover:bg-accent/50" },
                                            onclick: move |_| {
                                                active_filter.set(filter);
                                            },
                                            span { "{filter.label()}" }
                                            if is_active {
                                                div { class: "absolute bottom-0 left-0 right-0 h-0.5 bg-primary rounded-full" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    {
                        let has_any = summary.replies + summary.reactions + summary.reposts + summary.mentions + (summary.zap_sats as usize) > 0;
                        if has_any {
                            rsx! {
                                div { class: "px-4 py-2",
                                    div { class: "bg-muted rounded-lg p-3 flex items-center gap-3 text-sm overflow-x-auto",
                                        if summary.replies > 0 {
                                            button {
                                                class: "flex items-center gap-1 hover:bg-accent rounded px-2 py-1 transition whitespace-nowrap",
                                                onclick: move |_| active_filter.set(NotificationFilter::Replies),
                                                span { "💬 {summary.replies}" }
                                            }
                                        }
                                        if summary.reactions > 0 {
                                            button {
                                                class: "flex items-center gap-1 hover:bg-accent rounded px-2 py-1 transition whitespace-nowrap",
                                                onclick: move |_| active_filter.set(NotificationFilter::Reactions),
                                                span { "❤️ {summary.reactions}" }
                                            }
                                        }
                                        if summary.zap_sats > 0 {
                                            button {
                                                class: "flex items-center gap-1 hover:bg-accent rounded px-2 py-1 transition whitespace-nowrap",
                                                onclick: move |_| active_filter.set(NotificationFilter::Zaps),
                                                span { "⚡ {summary.zap_sats} sats" }
                                            }
                                        }
                                        if summary.reposts > 0 {
                                            button {
                                                class: "flex items-center gap-1 hover:bg-accent rounded px-2 py-1 transition whitespace-nowrap",
                                                onclick: move |_| active_filter.set(NotificationFilter::Reposts),
                                                span { "🔁 {summary.reposts}" }
                                            }
                                        }
                                        if summary.mentions > 0 {
                                            button {
                                                class: "flex items-center gap-1 hover:bg-accent rounded px-2 py-1 transition whitespace-nowrap",
                                                onclick: move |_| active_filter.set(NotificationFilter::Mentions),
                                                span { "@ {summary.mentions}" }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {}
                        }
                    }
                }
            }
            if !auth.is_authenticated {
                div { class: "text-center py-12",
                    div { class: "text-6xl mb-4", "🔐" }
                    h3 { class: "text-xl font-semibold mb-2", "Sign in to view notifications" }
                    p { class: "text-muted-foreground",
                        "Connect your account to see mentions, replies, and reactions"
                    }
                }
            } else {
                if let Some(err) = error.read().as_ref() {
                    div { class: "p-4",
                        div { class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                            "❌ {err}"
                        }
                    }
                }
                if !*nostr_client::CLIENT_INITIALIZED.read()
                    || (*loading.read() && notifications.read().is_empty())
                {
                    ClientInitializing {}
                }
                if !*loading.read() || !notifications.read().is_empty() {
                    if filtered_notifications.is_empty() {
                        div { class: "text-center py-12",
                            div { class: "text-6xl mb-4", "🔕" }
                            h3 { class: "text-xl font-semibold mb-2",
                                if *active_filter.read() == NotificationFilter::All {
                                    "No notifications yet"
                                } else {
                                    "No {active_filter.read().label().to_lowercase()}"
                                }
                            }
                            p { class: "text-muted-foreground",
                                if *active_filter.read() == NotificationFilter::All {
                                    "When someone mentions or replies to you, it'll show up here"
                                } else {
                                    "No {active_filter.read().label().to_lowercase()} found"
                                }
                            }
                        }
                    } else {
                        div { class: "divide-y divide-border",
                            for notification in filtered_notifications.iter() {
                                {
                                    render_notification(
                                        notification,
                                        cached_muted_posts.read().clone(),
                                        cached_blocked_users.read().clone(),
                                        cached_muted_words.read().clone(),
                                    )
                                }
                            }
                            if *has_more.read() {
                                div {
                                    id: "{sentinel_id}",
                                    class: "py-8 flex justify-center",
                                    if *loading.read() {
                                        div { class: "animate-spin text-2xl", "🔄" }
                                    }
                                }
                            } else if !filtered_notifications.is_empty() {
                                div { class: "py-8 text-center text-sm text-muted-foreground",
                                    "You've reached the end"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
fn render_notification(
    notification: &NotificationType,
    cached_muted_posts: Option<Rc<HashSet<String>>>,
    cached_blocked_users: Option<Rc<HashSet<String>>>,
    cached_muted_words: Option<Rc<HashSet<String>>>,
) -> Element {
    match notification {
        NotificationType::Mention(event) | NotificationType::Reply(event) => {
            rsx! {
                div {
                    key: "{event.id}",
                    class: "p-4 hover:bg-accent/50 transition",
                    div { class: "flex items-center gap-2 mb-2 text-sm text-muted-foreground",
                        span {
                            if matches!(notification, NotificationType::Mention(_)) {
                                "💬 mentioned you"
                            } else {
                                "↩️ replied to you"
                            }
                        }
                    }
                    EventCard {
                        event: event.clone(),
                        collapsible: true,
                        cached_muted_posts: cached_muted_posts.clone(),
                        cached_blocked_users: cached_blocked_users.clone(),
                        cached_muted_words: cached_muted_words.clone(),
                    }
                }
            }
        }
        NotificationType::Reaction(event) => {
            rsx! {
                ReactionNotification {
                    key: "{event.id}",
                    event: event.clone(),
                    cached_muted_posts: cached_muted_posts.clone(),
                    cached_blocked_users: cached_blocked_users.clone(),
                    cached_muted_words: cached_muted_words.clone(),
                }
            }
        }
        NotificationType::Repost(event) => {
            rsx! {
                RepostNotification {
                    key: "{event.id}",
                    event: event.clone(),
                    cached_muted_posts: cached_muted_posts.clone(),
                    cached_blocked_users: cached_blocked_users.clone(),
                    cached_muted_words: cached_muted_words.clone(),
                }
            }
        }
        NotificationType::Quote(event) => {
            rsx! {
                QuoteNotification {
                    key: "{event.id}",
                    event: event.clone(),
                    cached_muted_posts: cached_muted_posts.clone(),
                    cached_blocked_users: cached_blocked_users.clone(),
                    cached_muted_words: cached_muted_words.clone(),
                }
            }
        }
        NotificationType::Zap(event) => {
            rsx! {
                ZapNotification {
                    key: "{event.id}",
                    event: event.clone(),
                    cached_muted_posts: cached_muted_posts.clone(),
                    cached_blocked_users: cached_blocked_users.clone(),
                    cached_muted_words: cached_muted_words.clone(),
                }
            }
        }
    }
}
#[component]
fn ReactionNotification(
    event: NostrEvent,
    #[props(default = None)] cached_muted_posts: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_blocked_users: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_muted_words: Option<Rc<HashSet<String>>>,
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut reacted_post = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut hidden = use_signal(|| false);
    let reactor_pubkey = event.pubkey.to_string();
    let custom_emoji_url = if event.content.starts_with(':') && event.content.ends_with(':') {
        let shortcode = event.content.trim_matches(':');
        event.tags.iter().find_map(|tag| {
            let slice = tag.as_slice();
            if slice.first().map(|k| k == "emoji").unwrap_or(false)
                && slice.get(1).map(|s| s == shortcode).unwrap_or(false)
            {
                slice.get(2).cloned()
            } else {
                None
            }
        })
    } else {
        None
    };
    let reaction_emoji = if event.content.is_empty() || event.content == "+" {
        "❤️".to_string()
    } else if event.content == "-" {
        "👎".to_string()
    } else {
        event.content.clone()
    };
    let reacted_event_id = event
        .tags
        .iter()
        .find(|tag| {
            tag.kind()
                == nostr_sdk::TagKind::SingleLetter(nostr_sdk::SingleLetterTag::lowercase(
                    nostr_sdk::Alphabet::E,
                ))
        })
        .and_then(|tag| tag.content())
        .map(|s| s.to_string());
    let reactor_pubkey_for_effect = reactor_pubkey.clone();
    let reactor_pubkey_for_display = reactor_pubkey.clone();
    let reactor_pubkey_for_avatar = reactor_pubkey.clone();
    let reactor_pubkey_for_link = reactor_pubkey.clone();
    let reacted_eid_for_link = reacted_event_id.clone();
    let validated_reacted_eid = reacted_eid_for_link
        .as_ref()
        .filter(|eid| nostr_sdk::EventId::from_hex(eid).is_ok())
        .cloned();
    let my_pubkey_for_verify = auth_store::get_pubkey().unwrap_or_default();
    use_effect(move || {
        let pubkey = reactor_pubkey_for_effect.clone();
        let event_id = reacted_event_id.clone();
        let my_pk = my_pubkey_for_verify.clone();
        spawn(async move {
            let profile_fut = async {
                if let Ok(p) = profiles::fetch_profile(pubkey).await {
                    profile.set(Some(p));
                }
            };
            let post_fut = async {
                if let Some(eid) = event_id {
                    if let Some(cached) =
                        crate::stores::notification_event_cache::get_cached_referenced_event(&eid)
                    {
                        if cached.pubkey.to_hex() != my_pk {
                            hidden.set(true);
                        } else {
                            reacted_post.set(Some(cached));
                        }
                    } else if let Ok(event_id) = nostr_sdk::EventId::from_hex(&eid) {
                        let filter = Filter::new().id(event_id).limit(1);
                        match nostr_client::fetch_events_aggregated(
                            filter,
                            Duration::from_secs(10),
                        )
                        .await
                        {
                            Ok(events) => {
                                if let Some(original_event) = events.into_iter().next() {
                                    if original_event.pubkey.to_hex() != my_pk {
                                        hidden.set(true);
                                    } else {
                                        reacted_post.set(Some(original_event));
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to fetch referenced event: {}", e)
                            }
                        }
                    }
                }
            };
            join!(profile_fut, post_fut);
            loading.set(false);
        });
    });
    if *hidden.read() {
        return rsx! {};
    }
    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| crate::utils::format::truncate_pubkey(&reactor_pubkey_for_display));
    let avatar_url = profile
        .read()
        .as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| {
            format!(
                "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                reactor_pubkey_for_avatar,
            )
        });
    let post_route = reacted_post
        .read()
        .as_ref()
        .map(route_for_event);
    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition",
            div { class: "flex items-center gap-3 mb-2",
                Link {
                    to: Route::AddressViewer {
                        address: crate::utils::nip19_urls::profile_route_id(&reactor_pubkey_for_link),
                    },
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    img {
                        src: "{avatar_url}",
                        alt: "{display_name}",
                        class: "w-10 h-10 rounded-full object-cover shrink-0",
                    }
                }
                div { class: "flex items-center gap-2 text-sm",
                    if let Some(emoji_url) = custom_emoji_url {
                        img {
                            src: "{emoji_url}",
                            alt: "{reaction_emoji}",
                            class: "w-6 h-6 inline-block",
                        }
                    } else {
                        span { class: "text-2xl", "{reaction_emoji}" }
                    }
                    Link {
                        to: Route::AddressViewer {
                            address: crate::utils::nip19_urls::profile_route_id(&reactor_pubkey_for_link),
                        },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        class: "font-semibold hover:underline",
                        "{display_name}"
                    }
                    span { class: "text-muted-foreground", "reacted to" }
                    if let Some(route) = post_route {
                        Link {
                            to: route,
                            class: "text-muted-foreground hover:underline",
                            "your post"
                        }
                    } else if validated_reacted_eid.is_some() {
                        Link {
                            to: Route::AddressViewer {
                                address: crate::utils::nip19_urls::note_route_id(&validated_reacted_eid.clone().unwrap(), None),
                            },
                            class: "text-muted-foreground hover:underline",
                            "your post"
                        }
                    } else {
                        span { class: "text-muted-foreground", "your post" }
                    }
                }
            }
            if let Some(post) = reacted_post.read().as_ref() {
                div { class: "ml-13 mt-2",
                    EventCard {
                        event: post.clone(),
                        collapsible: true,
                        cached_muted_posts: cached_muted_posts.clone(),
                        cached_blocked_users: cached_blocked_users.clone(),
                        cached_muted_words: cached_muted_words.clone(),
                    }
                }
            } else if *loading.read() {
                div { class: "ml-13 mt-2 text-sm text-muted-foreground", "Loading post..." }
            }
        }
    }
}
#[component]
fn RepostNotification(
    event: NostrEvent,
    #[props(default = None)] cached_muted_posts: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_blocked_users: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_muted_words: Option<Rc<HashSet<String>>>,
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut reposted_post = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut hidden = use_signal(|| false);
    let reposter_pubkey = event.pubkey.to_string();
    let reposted_event_id = event
        .tags
        .iter()
        .find(|tag| {
            tag.kind()
                == nostr_sdk::TagKind::SingleLetter(nostr_sdk::SingleLetterTag::lowercase(
                    nostr_sdk::Alphabet::E,
                ))
        })
        .and_then(|tag| tag.content())
        .map(|s| s.to_string());
    let reposter_pubkey_for_effect = reposter_pubkey.clone();
    let reposter_pubkey_for_display = reposter_pubkey.clone();
    let reposter_pubkey_for_avatar = reposter_pubkey.clone();
    let reposter_pubkey_for_link = reposter_pubkey.clone();
    let reposted_eid_for_link = reposted_event_id.clone();
    let validated_reposted_eid = reposted_eid_for_link
        .as_ref()
        .filter(|eid| nostr_sdk::EventId::from_hex(eid).is_ok())
        .cloned();
    let my_pubkey_for_verify = auth_store::get_pubkey().unwrap_or_default();
    use_effect(move || {
        let pubkey = reposter_pubkey_for_effect.clone();
        let event_id = reposted_event_id.clone();
        let my_pk = my_pubkey_for_verify.clone();
        spawn(async move {
            let profile_fut = async {
                if let Ok(p) = profiles::fetch_profile(pubkey).await {
                    profile.set(Some(p));
                }
            };
            let post_fut = async {
                if let Some(eid) = event_id {
                    if let Some(cached) =
                        crate::stores::notification_event_cache::get_cached_referenced_event(&eid)
                    {
                        if cached.pubkey.to_hex() != my_pk {
                            hidden.set(true);
                        } else {
                            reposted_post.set(Some(cached));
                        }
                    } else if let Ok(event_id) = nostr_sdk::EventId::from_hex(&eid) {
                        let filter = Filter::new().id(event_id).limit(1);
                        match nostr_client::fetch_events_aggregated(
                            filter,
                            Duration::from_secs(10),
                        )
                        .await
                        {
                            Ok(events) => {
                                if let Some(original_event) = events.into_iter().next() {
                                    if original_event.pubkey.to_hex() != my_pk {
                                        hidden.set(true);
                                    } else {
                                        reposted_post.set(Some(original_event));
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to fetch referenced event: {}", e)
                            }
                        }
                    }
                }
            };
            join!(profile_fut, post_fut);
            loading.set(false);
        });
    });
    if *hidden.read() {
        return rsx! {};
    }
    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| crate::utils::format::truncate_pubkey(&reposter_pubkey_for_display));
    let avatar_url = profile
        .read()
        .as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| {
            format!(
                "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                reposter_pubkey_for_avatar,
            )
        });
    let post_route = reposted_post
        .read()
        .as_ref()
        .map(route_for_event);
    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition",
            div { class: "flex items-center gap-3 mb-2",
                Link {
                    to: Route::AddressViewer {
                        address: crate::utils::nip19_urls::profile_route_id(&reposter_pubkey_for_link),
                    },
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    img {
                        src: "{avatar_url}",
                        alt: "{display_name}",
                        class: "w-10 h-10 rounded-full object-cover shrink-0",
                    }
                }
                div { class: "flex items-center gap-2 text-sm",
                    span { class: "text-green-500 text-2xl", "🔁" }
                    Link {
                        to: Route::AddressViewer {
                            address: crate::utils::nip19_urls::profile_route_id(&reposter_pubkey_for_link),
                        },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        class: "font-semibold hover:underline",
                        "{display_name}"
                    }
                    span { class: "text-muted-foreground", "reposted" }
                    if let Some(route) = post_route {
                        Link {
                            to: route,
                            class: "text-muted-foreground hover:underline",
                            "your post"
                        }
                    } else if validated_reposted_eid.is_some() {
                        Link {
                            to: Route::AddressViewer {
                                address: crate::utils::nip19_urls::note_route_id(&validated_reposted_eid.clone().unwrap(), None),
                            },
                            class: "text-muted-foreground hover:underline",
                            "your post"
                        }
                    } else {
                        span { class: "text-muted-foreground", "your post" }
                    }
                }
            }
            if let Some(post) = reposted_post.read().as_ref() {
                div { class: "ml-13 mt-2",
                    EventCard {
                        event: post.clone(),
                        collapsible: true,
                        cached_muted_posts: cached_muted_posts.clone(),
                        cached_blocked_users: cached_blocked_users.clone(),
                        cached_muted_words: cached_muted_words.clone(),
                    }
                }
            } else if *loading.read() {
                div { class: "ml-13 mt-2 text-sm text-muted-foreground", "Loading post..." }
            }
        }
    }
}
#[component]
fn ZapNotification(
    event: NostrEvent,
    #[props(default = None)] cached_muted_posts: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_blocked_users: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_muted_words: Option<Rc<HashSet<String>>>,
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut zapped_post = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut hidden = use_signal(|| false);
    let private_zap_resolved = use_signal(|| None::<dip03::DecryptedPrivateZap>);
    let private_zap_failed = use_signal(|| false);
    let zap_request_event = dip03::parse_description_event(&event);
    let anon_kind = zap_request_event
        .as_ref()
        .map(dip03::classify_anon)
        .unwrap_or(dip03::AnonKind::None);
    let is_private_zap = matches!(anon_kind, dip03::AnonKind::Private(_));
    let is_anonymous_zap = matches!(anon_kind, dip03::AnonKind::Anonymous);
    // For anon/private zaps the description pubkey is an ephemeral key — never
    // treat it as the sender identity (empty string keeps downstream links inert).
    let zapper_pubkey = if is_private_zap || is_anonymous_zap {
        String::new()
    } else {
        extract_zapper_pubkey(&event).unwrap_or_else(|| event.pubkey.to_string())
    };
    let zap_amount_sats = extract_zap_amount(&event);
    let zapped_event_id = event
        .tags
        .iter()
        .find(|tag| {
            tag.kind()
                == nostr_sdk::TagKind::SingleLetter(nostr_sdk::SingleLetterTag::lowercase(
                    nostr_sdk::Alphabet::E,
                ))
        })
        .and_then(|tag| tag.content())
        .map(|s| s.to_string());
    let is_profile_zap = zapped_event_id.is_none();
    let zapper_pubkey_for_effect = zapper_pubkey.clone();
    let zapper_pubkey_for_display = zapper_pubkey.clone();
    let zapper_pubkey_for_avatar = zapper_pubkey.clone();
    let zapper_pubkey_for_link = zapper_pubkey.clone();
    let zapped_eid_for_link = zapped_event_id.clone();
    let validated_zapped_eid = zapped_eid_for_link
        .as_ref()
        .filter(|eid| nostr_sdk::EventId::from_hex(eid).is_ok())
        .cloned();
    let my_pubkey_for_verify = auth_store::get_pubkey().unwrap_or_default();
    // Anon/private zaps carry an ephemeral description pubkey — fetching its
    // profile would pollute the indexer queue with a throwaway key.
    let fetch_zapper_profile = !is_private_zap && !is_anonymous_zap;
    use_effect(move || {
        let pubkey = zapper_pubkey_for_effect.clone();
        let event_id = zapped_event_id.clone();
        let my_pk = my_pubkey_for_verify.clone();
        spawn(async move {
            let profile_fut = async {
                if fetch_zapper_profile {
                    if let Ok(p) = profiles::fetch_profile(pubkey).await {
                        profile.set(Some(p));
                    }
                }
            };
            let post_fut = async {
                if let Some(eid) = event_id {
                    if let Some(cached) =
                        crate::stores::notification_event_cache::get_cached_referenced_event(&eid)
                    {
                        if cached.pubkey.to_hex() != my_pk {
                            hidden.set(true);
                        } else {
                            zapped_post.set(Some(cached));
                        }
                    } else if let Ok(event_id) = nostr_sdk::EventId::from_hex(&eid) {
                        let filter = Filter::new().id(event_id).limit(1);
                        match nostr_client::fetch_events_aggregated(
                            filter,
                            Duration::from_secs(10),
                        )
                        .await
                        {
                            Ok(events) => {
                                if let Some(original_event) = events.into_iter().next() {
                                    if original_event.pubkey.to_hex() != my_pk {
                                        hidden.set(true);
                                    } else {
                                        zapped_post.set(Some(original_event));
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to fetch referenced event: {}", e)
                            }
                        }
                    }
                }
            };
            join!(profile_fut, post_fut);
            loading.set(false);
        });
    });
    // DIP-03 private zap: decrypt the anon payload to recover the sender
    // identity + private message. Runs once per notification; the dip03
    // cache prevents repeated signer prompts across re-renders.
    {
        let zap_request_for_decrypt = zap_request_event.clone();
        let mut resolved_sig = private_zap_resolved;
        let mut failed_sig = private_zap_failed;
        let mut profile_sig = profile;
        use_effect(move || {
            let Some(zap_request) = zap_request_for_decrypt.clone() else {
                return;
            };
            if !matches!(dip03::classify_anon(&zap_request), dip03::AnonKind::Private(_)) {
                return;
            }
            if resolved_sig.peek().is_some() || *failed_sig.peek() {
                return;
            }
            spawn(async move {
                match dip03::decrypt_private_zap(&zap_request).await {
                    Ok(decrypted) => {
                        let sender_pubkey = decrypted.sender_pubkey.to_string();
                        resolved_sig.set(Some(decrypted));
                        if let Ok(p) = profiles::fetch_profile(sender_pubkey).await {
                            profile_sig.set(Some(p));
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to decrypt private zap: {}", e);
                        failed_sig.set(true);
                    }
                }
            });
        });
    }
    if *hidden.read() {
        return rsx! {};
    }
    let private_resolved = private_zap_resolved.read().is_some();
    // Named mode is only reached for private zaps once the decrypt resolved,
    // so the profile links must target the *revealed* sender — the
    // pre-decrypt `zapper_pubkey` placeholder is an empty string for
    // anon/private zaps and would navigate to a broken route.
    let zapper_link_pubkey = private_zap_resolved
        .read()
        .as_ref()
        .map(|dec| dec.sender_pubkey.to_hex())
        .unwrap_or_else(|| zapper_pubkey_for_link.clone());
    enum ZapperDisplay {
        Named,
        Anonymous,
        PendingPrivate,
    }
    let display_mode = if is_anonymous_zap || (is_private_zap && *private_zap_failed.read()) {
        ZapperDisplay::Anonymous
    } else if is_private_zap && !private_resolved {
        ZapperDisplay::PendingPrivate
    } else {
        ZapperDisplay::Named
    };
    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| crate::utils::format::truncate_pubkey(&zapper_pubkey_for_display));
    let avatar_url = profile
        .read()
        .as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| {
            format!(
                "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                zapper_pubkey_for_avatar,
            )
        });
    let post_route = zapped_post
        .read()
        .as_ref()
        .map(route_for_event);
    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition",
            div { class: "flex items-center gap-3 mb-2",
                {
                    match display_mode {
                        ZapperDisplay::Named => rsx! {
                            Link {
                                to: Route::AddressViewer {
                                    address: crate::utils::nip19_urls::profile_route_id(&zapper_link_pubkey),
                                },
                                onclick: move |e: MouseEvent| e.stop_propagation(),
                                img {
                                    src: "{avatar_url}",
                                    alt: "{display_name}",
                                    class: "w-10 h-10 rounded-full object-cover shrink-0",
                                }
                            }
                        },
                        ZapperDisplay::PendingPrivate => rsx! {
                            div {
                                class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center shrink-0 text-muted-foreground",
                                LockIcon { class: "w-5 h-5".to_string() }
                            }
                        },
                        ZapperDisplay::Anonymous => rsx! {
                            div {
                                class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center shrink-0 text-sm font-bold text-muted-foreground",
                                "?"
                            }
                        },
                    }
                }
                div { class: "flex items-center gap-2 text-sm flex-wrap",
                    span { class: "text-yellow-500 text-2xl", "⚡" }
                    {
                        match display_mode {
                            ZapperDisplay::Named => rsx! {
                                Link {
                                    to: Route::AddressViewer {
                                        address: crate::utils::nip19_urls::profile_route_id(&zapper_link_pubkey),
                                    },
                                    onclick: move |e: MouseEvent| e.stop_propagation(),
                                    class: "font-semibold hover:underline",
                                    "{display_name}"
                                }
                            },
                            ZapperDisplay::PendingPrivate => rsx! {
                                span { class: "font-semibold", "Private zap" }
                                span {
                                    class: "inline-flex items-center gap-1 text-xs px-1.5 py-0.5 rounded bg-accent text-muted-foreground",
                                    LockIcon { class: "w-3 h-3".to_string() }
                                    "Decrypting..."
                                }
                            },
                            ZapperDisplay::Anonymous => rsx! {
                                span { class: "font-semibold", "Anonymous" }
                                if is_private_zap {
                                    span {
                                        class: "inline-flex items-center gap-1 text-xs px-1.5 py-0.5 rounded bg-accent text-muted-foreground",
                                        LockIcon { class: "w-3 h-3".to_string() }
                                        "Private"
                                    }
                                }
                            },
                        }
                    }
                    span { class: "text-muted-foreground", "zapped" }
                    if is_profile_zap {
                        span { class: "text-muted-foreground", "you" }
                    } else if let Some(route) = post_route {
                        Link {
                            to: route,
                            class: "text-muted-foreground hover:underline",
                            "your post"
                        }
                    } else if validated_zapped_eid.is_some() {
                        Link {
                            to: Route::AddressViewer {
                                address: crate::utils::nip19_urls::note_route_id(&validated_zapped_eid.clone().unwrap(), None),
                            },
                            class: "text-muted-foreground hover:underline",
                            "your post"
                        }
                    } else {
                        span { class: "text-muted-foreground", "your post" }
                    }
                    if let Some(amount) = zap_amount_sats {
                        span { class: "text-yellow-600 dark:text-yellow-400 font-bold",
                            "{amount} sats"
                        }
                    }
                }
            }
            if let Some(decrypted) = private_zap_resolved.read().as_ref() {
                if let Some(message) = &decrypted.message {
                    div {
                        class: "ml-13 mt-1 text-sm text-muted-foreground border-l-2 border-border pl-2",
                        "{message}"
                    }
                }
            }
            if !is_profile_zap {
                if let Some(post) = zapped_post.read().as_ref() {
                    div { class: "ml-13 mt-2",
                        EventCard {
                            event: post.clone(),
                            collapsible: true,
                            cached_muted_posts: cached_muted_posts.clone(),
                            cached_blocked_users: cached_blocked_users.clone(),
                            cached_muted_words: cached_muted_words.clone(),
                        }
                    }
                } else if *loading.read() {
                    div { class: "ml-13 mt-2 text-sm text-muted-foreground", "Loading post..." }
                }
            }
        }
    }
}
#[component]
fn QuoteNotification(
    event: NostrEvent,
    #[props(default = None)] cached_muted_posts: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_blocked_users: Option<Rc<HashSet<String>>>,
    #[props(default = None)] cached_muted_words: Option<Rc<HashSet<String>>>,
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut quoted_post = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut hidden = use_signal(|| false);
    let quoter_pubkey = event.pubkey.to_string();
    let quoted_event_id = event.tags.iter().find_map(|tag| {
        if let Some(TagStandard::Quote { event_id, .. }) = tag.as_standardized() {
            Some(event_id.to_hex())
        } else {
            None
        }
    });
    let quoter_pubkey_for_effect = quoter_pubkey.clone();
    let quoter_pubkey_for_display = quoter_pubkey.clone();
    let quoter_pubkey_for_avatar = quoter_pubkey.clone();
    let quoter_pubkey_for_link = quoter_pubkey.clone();
    let validated_quoted_eid = quoted_event_id
        .as_ref()
        .filter(|eid| nostr_sdk::EventId::from_hex(eid).is_ok())
        .cloned();
    let my_pubkey_for_verify = auth_store::get_pubkey().unwrap_or_default();
    use_effect(move || {
        let pubkey = quoter_pubkey_for_effect.clone();
        let event_id = quoted_event_id.clone();
        let my_pk = my_pubkey_for_verify.clone();
        spawn(async move {
            let profile_fut = async {
                if let Ok(p) = profiles::fetch_profile(pubkey).await {
                    profile.set(Some(p));
                }
            };
            let post_fut = async {
                if let Some(eid) = event_id {
                    if let Some(cached) =
                        crate::stores::notification_event_cache::get_cached_referenced_event(&eid)
                    {
                        if cached.pubkey.to_hex() != my_pk {
                            hidden.set(true);
                        } else {
                            quoted_post.set(Some(cached));
                        }
                    } else if let Ok(event_id) = nostr_sdk::EventId::from_hex(&eid) {
                        let filter = Filter::new().id(event_id).limit(1);
                        match nostr_client::fetch_events_aggregated(
                            filter,
                            Duration::from_secs(10),
                        )
                        .await
                        {
                            Ok(events) => {
                                if let Some(original_event) = events.into_iter().next() {
                                    if original_event.pubkey.to_hex() != my_pk {
                                        hidden.set(true);
                                    } else {
                                        quoted_post.set(Some(original_event));
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to fetch referenced event: {}", e)
                            }
                        }
                    }
                }
            };
            join!(profile_fut, post_fut);
            loading.set(false);
        });
    });
    if *hidden.read() {
        return rsx! {};
    }
    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| crate::utils::format::truncate_pubkey(&quoter_pubkey_for_display));
    let avatar_url = profile
        .read()
        .as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| {
            format!(
                "https://api.dicebear.com/7.x/identicon/svg?seed={}",
                quoter_pubkey_for_avatar,
            )
        });
    let post_route = quoted_post
        .read()
        .as_ref()
        .map(route_for_event);
    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition",
            div { class: "flex items-center gap-3 mb-2",
                Link {
                    to: Route::AddressViewer {
                        address: crate::utils::nip19_urls::profile_route_id(&quoter_pubkey_for_link),
                    },
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    img {
                        src: "{avatar_url}",
                        alt: "{display_name}",
                        class: "w-10 h-10 rounded-full object-cover shrink-0",
                    }
                }
                div { class: "flex items-center gap-2 text-sm",
                    span { class: "text-blue-500 text-2xl", "📝" }
                    Link {
                        to: Route::AddressViewer {
                            address: crate::utils::nip19_urls::profile_route_id(&quoter_pubkey_for_link),
                        },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        class: "font-semibold hover:underline",
                        "{display_name}"
                    }
                    span { class: "text-muted-foreground", "quoted" }
                    if let Some(route) = post_route {
                        Link {
                            to: route,
                            class: "text-muted-foreground hover:underline",
                            "your post"
                        }
                    } else if validated_quoted_eid.is_some() {
                        Link {
                            to: Route::AddressViewer {
                                address: crate::utils::nip19_urls::note_route_id(&validated_quoted_eid.clone().unwrap(), None),
                            },
                            class: "text-muted-foreground hover:underline",
                            "your post"
                        }
                    } else {
                        span { class: "text-muted-foreground", "your post" }
                    }
                }
            }
            div { class: "ml-13 mt-2",
                NoteCard {
                    event: event.clone(),
                    collapsible: true,
                    cached_muted_posts: cached_muted_posts.clone(),
                    cached_blocked_users: cached_blocked_users.clone(),
                    cached_muted_words: cached_muted_words.clone(),
                }
            }
            if let Some(post) = quoted_post.read().as_ref() {
                div { class: "ml-13 mt-2",
                    EventCard {
                        event: post.clone(),
                        collapsible: true,
                        cached_muted_posts: cached_muted_posts.clone(),
                        cached_blocked_users: cached_blocked_users.clone(),
                        cached_muted_words: cached_muted_words.clone(),
                    }
                }
            } else if *loading.read() {
                div { class: "ml-13 mt-2 text-sm text-muted-foreground", "Loading post..." }
            }
        }
    }
}
/// Helper to extract the actual zapper's pubkey from a zap receipt event (kind 9735)
/// The event.pubkey is the Lightning node's pubkey, the actual zapper is in the description
fn extract_zapper_pubkey(event: &NostrEvent) -> Option<String> {
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        tag.as_slice()
            .first()
            .map(|k| k == "description")
            .unwrap_or(false)
    }) {
        if let Some(description) = description_tag.as_slice().get(1) {
            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(description) {
                if let Some(pubkey_str) = zap_request.get("pubkey").and_then(|p| p.as_str()) {
                    return Some(pubkey_str.to_string());
                }
            }
        }
    }
    None
}
/// Helper to extract zap amount in sats from a zap receipt event (kind 9735)
fn extract_zap_amount(event: &NostrEvent) -> Option<u64> {
    if let Some(bolt11_tag) = event.tags.iter().find(|tag| {
        tag.as_slice()
            .first()
            .map(|k| k == "bolt11")
            .unwrap_or(false)
    }) {
        if let Some(bolt11) = bolt11_tag.as_slice().get(1) {
            return parse_bolt11_amount(bolt11);
        }
    }
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        tag.as_slice()
            .first()
            .map(|k| k == "description")
            .unwrap_or(false)
    }) {
        if let Some(description) = description_tag.as_slice().get(1) {
            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(description) {
                if let Some(amount_msat) = zap_request.get("amount").and_then(|a| a.as_u64()) {
                    return Some(amount_msat / 1000);
                }
            }
        }
    }
    None
}
/// Helper to get timestamp from notification
fn get_timestamp(notification: &NotificationType) -> u64 {
    match notification {
        NotificationType::Mention(e)
        | NotificationType::Reply(e)
        | NotificationType::Reaction(e)
        | NotificationType::Repost(e)
        | NotificationType::Quote(e)
        | NotificationType::Zap(e) => e.created_at.as_secs(),
    }
}

/// Helper to get event ID from notification
fn get_event_id(notification: &NotificationType) -> nostr_sdk::EventId {
    match notification {
        NotificationType::Mention(e)
        | NotificationType::Reply(e)
        | NotificationType::Reaction(e)
        | NotificationType::Repost(e)
        | NotificationType::Quote(e)
        | NotificationType::Zap(e) => e.id,
    }
}

/// Classify an event into a notification type
fn classify_notification(event: &NostrEvent, my_pubkey: &str) -> Option<NotificationType> {
    // Skip self-notifications
    if event.pubkey.to_hex() == my_pubkey {
        return None;
    }

    match event.kind {
        Kind::TextNote => {
            let has_quote = event.tags.iter().any(|tag| {
                matches!(tag.as_standardized(), Some(TagStandard::Quote { .. }))
            });
            if has_quote {
                return Some(NotificationType::Quote(event.clone()));
            }
            let is_reply = event.tags.iter().any(|tag| {
                matches!(
                    tag.as_standardized(),
                    Some(nostr_sdk::TagStandard::Event {
                        marker: Some(nostr_sdk::nips::nip10::Marker::Root),
                        ..
                    }) | Some(nostr_sdk::TagStandard::Event {
                        marker: Some(nostr_sdk::nips::nip10::Marker::Reply),
                        ..
                    })
                )
            });
            if is_reply {
                Some(NotificationType::Reply(event.clone()))
            } else {
                Some(NotificationType::Mention(event.clone()))
            }
        }
        Kind::Reaction => Some(NotificationType::Reaction(event.clone())),
        Kind::Repost => Some(NotificationType::Repost(event.clone())),
        Kind::ZapReceipt => Some(NotificationType::Zap(event.clone())),
        _ => None,
    }
}

/// Build the notifications query filter for the signed-in user.
///
/// Initial load (`until = None`) is bounded to a 7-day window: without a
/// `since`, the fetched mention/reply events are persisted into the shared
/// event database regardless of age, where unbounded paginated home-feed
/// reads can surface them. Pagination (`until = Some`) stays unbounded so
/// the page can still scroll back through full history.
fn build_notifications_filter(pubkey: nostr_sdk::PublicKey, until: Option<u64>) -> Filter {
    let mut filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Repost,
            Kind::Reaction,
            Kind::ZapReceipt,
        ])
        .pubkey(pubkey)
        .limit(100);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    } else {
        let since_secs = Timestamp::now().as_secs().saturating_sub(7 * 86400);
        filter = filter.since(Timestamp::from(since_secs));
    }
    filter
}

/// Stream notifications with progressive loading
/// Calls on_notification for each notification as it arrives
async fn stream_notifications<F>(
    until: Option<u64>,
    mut on_notification: F,
) -> Result<usize, NostrBlueError>
where
    F: FnMut(NotificationType),
{
    let pubkey_str = auth_store::get_pubkey().ok_or(NostrBlueError::NotAuthenticated)?;
    log::info!(
        "Streaming notifications for {} (until: {:?})",
        pubkey_str,
        until
    );
    let pubkey = nostr_sdk::PublicKey::parse(&pubkey_str)
        .map_err(|e| NostrBlueError::Other(format!("Invalid pubkey: {}", e)))?;

    let filter = build_notifications_filter(pubkey, until);

    let mut count = 0;
    let pubkey_for_classify = pubkey_str.clone();

    let stream_result =
        nostr_client::stream_events_immediate(filter, Duration::from_secs(10), |event| {
            if let Some(notif) = classify_notification(&event, &pubkey_for_classify) {
                on_notification(notif);
                count += 1;
            }
        })
        .await;

    if let Err(e) = stream_result {
        log::error!("Failed to stream notifications: {:?}", e);
        return Err(e);
    }

    log::info!("Streamed {} notifications", count);
    Ok(count)
}

#[allow(dead_code)]
async fn load_notifications(until: Option<u64>) -> Result<Vec<NotificationType>, NostrBlueError> {
    let client = nostr_client::NOSTR_CLIENT
        .read()
        .as_ref()
        .ok_or(NostrBlueError::Other("Client not initialized".into()))?
        .clone();
    nostr_client::ensure_relays_ready(&client).await;
    let pubkey_str = auth_store::get_pubkey().ok_or(NostrBlueError::NotAuthenticated)?;
    log::info!(
        "Loading notifications for {} (until: {:?})",
        pubkey_str,
        until
    );
    let pubkey = nostr_sdk::PublicKey::parse(&pubkey_str)
        .map_err(|e| NostrBlueError::Other(format!("Invalid pubkey: {}", e)))?;
    let mut all_notifications = Vec::new();
    let mut filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,
            Kind::Repost,
            Kind::Reaction,
            Kind::ZapReceipt,
        ])
        .pubkey(pubkey)
        .limit(100);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let events: Vec<_> = client
        .fetch_events(filter, Duration::from_secs(10))
        .await
        .map_err(|e| {
            log::error!("Failed to fetch notifications: {}", e);
            e
        })?
        .into_iter()
        .collect();

    for event in events {
        if event.pubkey.to_hex() == pubkey_str {
            continue;
        }
        match event.kind {
            Kind::TextNote => {
                let has_quote = event.tags.iter().any(|tag| {
                    matches!(tag.as_standardized(), Some(TagStandard::Quote { .. }))
                });
                if has_quote {
                    all_notifications.push(NotificationType::Quote(event));
                } else {
                    let is_reply = event.tags.iter().any(|tag| {
                        matches!(
                            tag.as_standardized(),
                            Some(nostr_sdk::TagStandard::Event {
                                marker: Some(nostr_sdk::nips::nip10::Marker::Root),
                                ..
                            }) | Some(nostr_sdk::TagStandard::Event {
                                marker: Some(nostr_sdk::nips::nip10::Marker::Reply),
                                ..
                            })
                        )
                    });
                    if is_reply {
                        all_notifications.push(NotificationType::Reply(event));
                    } else {
                        all_notifications.push(NotificationType::Mention(event));
                    }
                }
            }
            Kind::Reaction => {
                all_notifications.push(NotificationType::Reaction(event));
            }
            Kind::Repost => {
                all_notifications.push(NotificationType::Repost(event));
            }
            Kind::ZapReceipt => {
                all_notifications.push(NotificationType::Zap(event));
            }
            _ => {}
        }
    }
    all_notifications.sort_by_key(|n| std::cmp::Reverse(get_timestamp(n)));
    log::info!("Loaded {} notifications", all_notifications.len());
    Ok(all_notifications)
}
/// Batch prefetch author metadata for notification authors
async fn prefetch_notification_authors(notifications: &[NotificationType]) {
    use crate::utils::profile_prefetch;
    if notifications.is_empty() {
        return;
    }
    let pubkeys = profile_prefetch::extract_pubkeys(notifications, |notif| match notif {
        NotificationType::Mention(e) => e.pubkey,
        NotificationType::Reply(e) => e.pubkey,
        NotificationType::Reaction(e) => e.pubkey,
        NotificationType::Repost(e) => e.pubkey,
        NotificationType::Quote(e) => e.pubkey,
        NotificationType::Zap(e) => e.pubkey,
    });
    profile_prefetch::prefetch_pubkeys(pubkeys).await;
}

async fn prefetch_notification_posts(notifications: &[NotificationType]) {
    if notifications.is_empty() {
        return;
    }
    let mut event_ids: Vec<nostr_sdk::EventId> = Vec::new();
    let mut seen: HashSet<nostr_sdk::EventId> = HashSet::new();
    for notif in notifications {
        let event = match notif {
            NotificationType::Reaction(e)
            | NotificationType::Repost(e)
            | NotificationType::Quote(e)
            | NotificationType::Zap(e) => e,
            _ => continue,
        };
        for tag in event.tags.iter() {
            if let Some(nostr_sdk::TagStandard::Event { event_id, .. })
            | Some(nostr_sdk::TagStandard::Quote { event_id, .. }) = tag.as_standardized()
            {
                if seen.insert(*event_id) {
                    event_ids.push(*event_id);
                }
            }
        }
    }
    if event_ids.is_empty() {
        return;
    }
    event_ids.truncate(100);
    let filter = Filter::new()
        .ids(event_ids.clone())
        .limit(event_ids.len());
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(15)).await {
        Ok(events) => {
            log::info!(
                "Pre-fetched {} referenced posts for notifications",
                events.len()
            );
            crate::stores::notification_event_cache::cache_referenced_events(events);
        }
        Err(e) => {
            log::warn!("Failed to prefetch notification posts: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_filter_initial_load_is_bounded_to_7d() {
        let pubkey = nostr_sdk::Keys::generate().public_key();
        let before = Timestamp::now().as_secs();

        let filter = build_notifications_filter(pubkey, None);

        let after = Timestamp::now().as_secs();
        assert_eq!(filter.limit, Some(100));
        assert_eq!(filter.until, None);
        let since = filter.since.expect("initial load must set a since bound");
        let since_secs = since.as_secs();
        // Bounded to ~7 days ago (small slack for the before/after capture).
        assert!(since_secs <= before - 7 * 86400 + 2);
        assert!(since_secs >= after.saturating_sub(7 * 86400));
    }

    #[test]
    fn notifications_filter_pagination_is_unbounded() {
        let pubkey = nostr_sdk::Keys::generate().public_key();

        let filter = build_notifications_filter(pubkey, Some(1_234_567));

        assert_eq!(filter.until, Some(Timestamp::from(1_234_567)));
        assert_eq!(filter.since, None, "pagination must scroll back unbounded");
    }
}
