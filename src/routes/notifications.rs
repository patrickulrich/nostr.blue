use crate::components::{ClientInitializing, NoteCard};
use crate::error::NostrBlueError;
use crate::hooks::{use_infinite_scroll, use_mute_block_cache};
use crate::routes::Route;
use crate::stores::{auth_store, nostr_client, notifications as notif_store, profiles};
use crate::utils::bolt11::parse_bolt11_amount;
use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, Filter, Kind, Timestamp};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
enum NotificationType {
    Mention(NostrEvent),
    Reply(NostrEvent),
    Reaction(NostrEvent),
    Repost(NostrEvent),
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
            Self::Reposts => matches!(notification, NotificationType::Repost(_)),
            Self::Zaps => matches!(notification, NotificationType::Zap(_)),
        }
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
    let (cached_muted_posts, cached_blocked_users) = use_mute_block_cache();
    use_effect(move || {
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
            #[cfg(feature = "mobile")]
            {
                matches!(login_method, Some(auth_store::LoginMethod::AndroidSigner))
            }
            #[cfg(not(feature = "mobile"))]
            {
                false
            }
        };
        if requires_signer && !has_signer {
            log::debug!("Waiting for signer restoration before loading notifications...");
            return;
        }

        let now = Timestamp::now().as_secs() as i64;
        notif_store::set_checked_at(now);
        loading.set(true);
        error.set(None);

        // Use streaming for progressive loading
        spawn(async move {
            let initial_pubkey = auth_store::get_pubkey();

            // Wait for relays before fetching (NIP-46 timing)
            nostr_client::wait_for_user_relays(
                Duration::from_secs(5),
                "notifications_initial_load",
            )
            .await;

            // Abort if user changed during relay wait
            if auth_store::get_pubkey() != initial_pubkey {
                log::debug!("Pubkey changed during relay wait, aborting notification load");
                loading.set(false);
                return;
            }

            let mut seen_ids: HashSet<nostr_sdk::EventId> = HashSet::new();

            let result = stream_notifications(None, |notif| {
                let event_id = get_event_id(&notif);

                // SDK pattern: insert() returns true if newly inserted (atomic check-and-insert)
                if seen_ids.insert(event_id) {
                    // Dioxus pattern: use peek() in async contexts (non-subscribing read)
                    let mut current = notifications.peek().clone();
                    current.push(notif);
                    current.sort_by_key(|n| std::cmp::Reverse(get_timestamp(n)));
                    notifications.set(current);
                }
            })
            .await;

            match result {
                Ok(count) => {
                    if count > 0 {
                        // Dioxus pattern: peek() in async, not read()
                        let notifs = notifications.peek().clone();
                        let oldest = notifs.iter().map(get_timestamp).min();
                        oldest_timestamp.set(oldest);
                        has_more.set(count >= 100);
                        spawn(async move {
                            prefetch_notification_authors(&notifs).await;
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
            loading.set(false);
        });
    });
    let handle_refresh = move |_| {
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        if !is_authenticated || *refreshing.read() {
            return;
        }
        refreshing.set(true);
        // Clear existing notifications for fresh load
        notifications.set(Vec::new());

        spawn(async move {
            let mut seen_ids: HashSet<nostr_sdk::EventId> = HashSet::new();

            let result = stream_notifications(None, |notif| {
                let event_id = get_event_id(&notif);
                if seen_ids.insert(event_id) {
                    let mut current = notifications.peek().clone();
                    current.push(notif);
                    current.sort_by_key(|n| std::cmp::Reverse(get_timestamp(n)));
                    notifications.set(current);
                }
            })
            .await;

            match result {
                Ok(count) => {
                    if count > 0 {
                        let notifs = notifications.peek().clone();
                        let oldest = notifs.iter().map(get_timestamp).min();
                        oldest_timestamp.set(oldest);
                        has_more.set(count >= 100);
                        spawn(async move {
                            prefetch_notification_authors(&notifs).await;
                        });
                    }
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            refreshing.set(false);
        });
    };
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }
        let until = *oldest_timestamp.read();
        loading.set(true);

        spawn(async move {
            // Get existing IDs to avoid duplicates when loading more
            let existing_ids: HashSet<nostr_sdk::EventId> =
                notifications.peek().iter().map(get_event_id).collect();
            let mut seen_ids = existing_ids.clone();
            let initial_count = seen_ids.len();

            let result = stream_notifications(until, |notif| {
                let event_id = get_event_id(&notif);
                if seen_ids.insert(event_id) {
                    let mut current = notifications.peek().clone();
                    current.push(notif);
                    current.sort_by_key(|n| std::cmp::Reverse(get_timestamp(n)));
                    notifications.set(current);
                }
            })
            .await;

            match result {
                Ok(count) => {
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
                            prefetch_notification_authors(&new_notifs).await;
                        });
                    } else {
                        has_more.set(false);
                    }
                }
                Err(_) => {
                    has_more.set(false);
                }
            }
            loading.set(false);
        });
    };
    let sentinel_id = use_infinite_scroll(load_more, has_more, loading);
    let auth = auth_store::AUTH_STATE.read();
    let filtered_notifications: Vec<NotificationType> = notifications
        .read()
        .iter()
        .filter(|n| active_filter.read().matches(n))
        .cloned()
        .collect();
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
                    NoteCard {
                        event: event.clone(),
                        collapsible: true,
                        cached_muted_posts: cached_muted_posts.clone(),
                        cached_blocked_users: cached_blocked_users.clone(),
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
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut reacted_post = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
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
        .filter(|eid| nostr_sdk::EventId::from_hex(eid).is_ok());
    use_effect(move || {
        let pubkey = reactor_pubkey_for_effect.clone();
        let event_id = reacted_event_id.clone();
        spawn(async move {
            if let Ok(p) = profiles::fetch_profile(pubkey).await {
                profile.set(Some(p));
            }
            if let Some(eid) = event_id {
                if let Ok(event_id) = nostr_sdk::EventId::from_hex(&eid) {
                    let filter = Filter::new().id(event_id).limit(1);
                    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
                        .await
                    {
                        Ok(events) => {
                            if let Some(original_event) = events.into_iter().next() {
                                reacted_post.set(Some(original_event));
                            }
                        }
                        Err(e) => log::error!("Failed to fetch referenced event: {}", e),
                    }
                }
            }
            loading.set(false);
        });
    });
    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| format!("{}...", &reactor_pubkey_for_display[..16]));
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
    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition",
            div { class: "flex items-center gap-3 mb-2",
                Link {
                    to: Route::Profile {
                        pubkey: reactor_pubkey_for_link.clone(),
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
                        to: Route::Profile {
                            pubkey: reactor_pubkey_for_link.clone(),
                        },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        class: "font-semibold hover:underline",
                        "{display_name}"
                    }
                    span { class: "text-muted-foreground", "reacted to" }
                    if let Some(eid) = validated_reacted_eid {
                        Link {
                            to: Route::Note { note_id: eid.clone(), from_voice: None },
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
                    NoteCard {
                        event: post.clone(),
                        collapsible: true,
                        cached_muted_posts: cached_muted_posts.clone(),
                        cached_blocked_users: cached_blocked_users.clone(),
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
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut reposted_post = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
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
        .filter(|eid| nostr_sdk::EventId::from_hex(eid).is_ok());
    use_effect(move || {
        let pubkey = reposter_pubkey_for_effect.clone();
        let event_id = reposted_event_id.clone();
        spawn(async move {
            if let Ok(p) = profiles::fetch_profile(pubkey).await {
                profile.set(Some(p));
            }
            if let Some(eid) = event_id {
                if let Ok(event_id) = nostr_sdk::EventId::from_hex(&eid) {
                    let filter = Filter::new().id(event_id).limit(1);
                    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
                        .await
                    {
                        Ok(events) => {
                            if let Some(original_event) = events.into_iter().next() {
                                reposted_post.set(Some(original_event));
                            }
                        }
                        Err(e) => log::error!("Failed to fetch referenced event: {}", e),
                    }
                }
            }
            loading.set(false);
        });
    });
    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| format!("{}...", &reposter_pubkey_for_display[..16]));
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
    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition",
            div { class: "flex items-center gap-3 mb-2",
                Link {
                    to: Route::Profile {
                        pubkey: reposter_pubkey_for_link.clone(),
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
                        to: Route::Profile {
                            pubkey: reposter_pubkey_for_link.clone(),
                        },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        class: "font-semibold hover:underline",
                        "{display_name}"
                    }
                    span { class: "text-muted-foreground", "reposted" }
                    if let Some(eid) = validated_reposted_eid {
                        Link {
                            to: Route::Note { note_id: eid.clone(), from_voice: None },
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
                    NoteCard {
                        event: post.clone(),
                        collapsible: true,
                        cached_muted_posts: cached_muted_posts.clone(),
                        cached_blocked_users: cached_blocked_users.clone(),
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
) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut zapped_post = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let zapper_pubkey = extract_zapper_pubkey(&event).unwrap_or_else(|| event.pubkey.to_string());
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
    let zapper_pubkey_for_effect = zapper_pubkey.clone();
    let zapper_pubkey_for_display = zapper_pubkey.clone();
    let zapper_pubkey_for_avatar = zapper_pubkey.clone();
    let zapper_pubkey_for_link = zapper_pubkey.clone();
    let zapped_eid_for_link = zapped_event_id.clone();
    let validated_zapped_eid = zapped_eid_for_link
        .as_ref()
        .filter(|eid| nostr_sdk::EventId::from_hex(eid).is_ok());
    use_effect(move || {
        let pubkey = zapper_pubkey_for_effect.clone();
        let event_id = zapped_event_id.clone();
        spawn(async move {
            if let Ok(p) = profiles::fetch_profile(pubkey).await {
                profile.set(Some(p));
            }
            if let Some(eid) = event_id {
                if let Ok(event_id) = nostr_sdk::EventId::from_hex(&eid) {
                    let filter = Filter::new().id(event_id).limit(1);
                    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
                        .await
                    {
                        Ok(events) => {
                            if let Some(original_event) = events.into_iter().next() {
                                zapped_post.set(Some(original_event));
                            }
                        }
                        Err(e) => log::error!("Failed to fetch referenced event: {}", e),
                    }
                }
            }
            loading.set(false);
        });
    });
    let display_name = profile
        .read()
        .as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| format!("{}...", &zapper_pubkey_for_display[..16]));
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
    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition",
            div { class: "flex items-center gap-3 mb-2",
                Link {
                    to: Route::Profile {
                        pubkey: zapper_pubkey_for_link.clone(),
                    },
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    img {
                        src: "{avatar_url}",
                        alt: "{display_name}",
                        class: "w-10 h-10 rounded-full object-cover shrink-0",
                    }
                }
                div { class: "flex items-center gap-2 text-sm",
                    span { class: "text-yellow-500 text-2xl", "⚡" }
                    Link {
                        to: Route::Profile {
                            pubkey: zapper_pubkey_for_link.clone(),
                        },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        class: "font-semibold hover:underline",
                        "{display_name}"
                    }
                    span { class: "text-muted-foreground", "zapped" }
                    if let Some(eid) = validated_zapped_eid {
                        Link {
                            to: Route::Note { note_id: eid.clone(), from_voice: None },
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
            if let Some(post) = zapped_post.read().as_ref() {
                div { class: "ml-13 mt-2",
                    NoteCard {
                        event: post.clone(),
                        collapsible: true,
                        cached_muted_posts: cached_muted_posts.clone(),
                        cached_blocked_users: cached_blocked_users.clone(),
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
            // NIP-10: Only e tags with root/reply markers indicate thread replies
            // Unmarked e tags are mentions in the preferred scheme
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
                // NIP-10: Only e tags with root/reply markers indicate thread replies
                // Unmarked e tags are mentions in the preferred scheme
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
        NotificationType::Zap(e) => e.pubkey,
    });
    profile_prefetch::prefetch_pubkeys(pubkeys).await;
}
