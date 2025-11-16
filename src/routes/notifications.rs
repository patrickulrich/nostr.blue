use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client, notifications as notif_store, profiles};
use crate::components::{NoteCard, ClientInitializing};
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;
use nostr_sdk::{Event as NostrEvent, Filter, Kind, Timestamp};
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
    let mut notifications = use_signal(|| Vec::<NotificationType>::new());
    let mut loading = use_signal(|| false);
    let mut refreshing = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut active_filter = use_signal(|| NotificationFilter::All);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Load initial notifications (limit: 100 for historical data)
    use_effect(move || {
        let is_authenticated = auth_store::is_authenticated();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load notifications if both authenticated AND client is initialized
        if !is_authenticated || !client_initialized {
            return;
        }

        // Mark notifications as checked at current time (updates localStorage and clears badge)
        let now = Timestamp::now().as_secs() as i64;
        notif_store::set_checked_at(now);

        loading.set(true);
        error.set(None);

        spawn(async move {
            match load_notifications(None).await {
                Ok(notifs) => {
                    if !notifs.is_empty() {
                        let oldest = notifs.iter().map(|n| get_timestamp(n)).min();
                        oldest_timestamp.set(oldest);
                        let len = notifs.len();
                        notifications.set(notifs.clone());
                        has_more.set(len >= 100);

                        // Spawn non-blocking background prefetch for notification authors
                        spawn(async move {
                            prefetch_notification_authors(&notifs).await;
                        });
                    } else {
                        has_more.set(false);
                    }
                }
                Err(e) => {
                    error.set(Some(e));
                    has_more.set(false);
                }
            }
            loading.set(false);
        });
    });

    // Refresh handler
    let handle_refresh = move |_| {
        let is_authenticated = auth_store::is_authenticated();
        if !is_authenticated || *refreshing.read() {
            return;
        }

        refreshing.set(true);
        spawn(async move {
            match load_notifications(None).await {
                Ok(notifs) => {
                    if !notifs.is_empty() {
                        let oldest = notifs.iter().map(|n| get_timestamp(n)).min();
                        oldest_timestamp.set(oldest);
                        let len = notifs.len();
                        notifications.set(notifs.clone());
                        has_more.set(len >= 100);

                        // Spawn non-blocking background prefetch for notification authors
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

    // Load more handler for infinite scroll
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }

        let until = *oldest_timestamp.read();
        loading.set(true);

        spawn(async move {
            match load_notifications(until).await {
                Ok(new_notifs) => {
                    if !new_notifs.is_empty() {
                        let oldest = new_notifs.iter().map(|n| get_timestamp(n)).min();
                        oldest_timestamp.set(oldest);

                        let mut current = notifications.read().clone();
                        current.extend(new_notifs.clone());
                        notifications.set(current);

                        has_more.set(new_notifs.len() >= 100);

                        // Spawn non-blocking background prefetch for notification authors
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

    // Setup infinite scroll (callback, has_more, loading)
    let sentinel_id = use_infinite_scroll(load_more, has_more, loading);

    let auth = auth_store::AUTH_STATE.read();

    // Filter notifications based on active filter
    let filtered_notifications: Vec<NotificationType> = notifications.read()
        .iter()
        .filter(|n| active_filter.read().matches(n))
        .cloned()
        .collect();

    rsx! {
        div {
            class: "min-h-screen",

            // Header with title and refresh button
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center justify-between",
                    h2 {
                        class: "text-xl font-bold",
                        "🔔 Notifications"
                    }
                    if auth.is_authenticated {
                        button {
                            class: "p-2 hover:bg-accent rounded-lg transition",
                            onclick: handle_refresh,
                            disabled: *refreshing.read(),
                            span {
                                class: if *refreshing.read() { "inline-block animate-spin" } else { "" },
                                "🔄"
                            }
                        }
                    }
                }

                // Tab bar
                if auth.is_authenticated {
                    div {
                        class: "px-4 pb-2 overflow-x-auto",
                        div {
                            class: "flex gap-2 min-w-max",
                            for filter in [
                                NotificationFilter::All,
                                NotificationFilter::Replies,
                                NotificationFilter::Mentions,
                                NotificationFilter::Reactions,
                                NotificationFilter::Reposts,
                                NotificationFilter::Zaps
                            ] {
                                {
                                    let is_active = *active_filter.read() == filter;
                                    rsx! {
                                        button {
                                            key: "{filter.label()}",
                                            class: "px-4 py-2 text-sm rounded-lg transition relative",
                                            class: if is_active {
                                                "font-semibold"
                                            } else {
                                                "text-muted-foreground hover:bg-accent/50"
                                            },
                                            onclick: move |_| {
                                                active_filter.set(filter);
                                            },
                                            span { "{filter.label()}" }
                                            if is_active {
                                                div {
                                                    class: "absolute bottom-0 left-0 right-0 h-0.5 bg-primary rounded-full"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Not authenticated
            if !auth.is_authenticated {
                div {
                    class: "text-center py-12",
                    div {
                        class: "text-6xl mb-4",
                        "🔐"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2",
                        "Sign in to view notifications"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Connect your account to see mentions, replies, and reactions"
                    }
                }
            } else {
                // Error state
                if let Some(err) = error.read().as_ref() {
                    div {
                        class: "p-4",
                        div {
                            class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                            "❌ {err}"
                        }
                    }
                }

                // Loading state
                if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && notifications.read().is_empty()) {
                    // Show client initializing animation during:
                    // 1. Client initialization
                    // 2. Initial notifications load (loading + no notifications, regardless of error state)
                    ClientInitializing {}
                }

                // Notifications list
                if !*loading.read() || !notifications.read().is_empty() {
                    if filtered_notifications.is_empty() {
                        div {
                            class: "text-center py-12",
                            div {
                                class: "text-6xl mb-4",
                                "🔕"
                            }
                            h3 {
                                class: "text-xl font-semibold mb-2",
                                if *active_filter.read() == NotificationFilter::All {
                                    "No notifications yet"
                                } else {
                                    "No {active_filter.read().label().to_lowercase()}"
                                }
                            }
                            p {
                                class: "text-muted-foreground",
                                if *active_filter.read() == NotificationFilter::All {
                                    "When someone mentions or replies to you, it'll show up here"
                                } else {
                                    "No {active_filter.read().label().to_lowercase()} found"
                                }
                            }
                        }
                    } else {
                        div {
                            class: "divide-y divide-border",
                            for notification in filtered_notifications.iter() {
                                {render_notification(notification)}
                            }

                            // Infinite scroll sentinel
                            if *has_more.read() {
                                div {
                                    id: "{sentinel_id}",
                                    class: "py-8 flex justify-center",
                                    if *loading.read() {
                                        div {
                                            class: "animate-spin text-2xl",
                                            "🔄"
                                        }
                                    }
                                }
                            } else if !filtered_notifications.is_empty() {
                                div {
                                    class: "py-8 text-center text-sm text-muted-foreground",
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

fn render_notification(notification: &NotificationType) -> Element {
    match notification {
        NotificationType::Mention(event) | NotificationType::Reply(event) => {
            rsx! {
                div {
                    class: "p-4 hover:bg-accent/50 transition",
                    div {
                        class: "flex items-center gap-2 mb-2 text-sm text-muted-foreground",
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
                        collapsible: true
                    }
                }
            }
        }
        NotificationType::Reaction(event) => {
            rsx! {
                ReactionNotification {
                    event: event.clone()
                }
            }
        }
        NotificationType::Repost(event) => {
            rsx! {
                RepostNotification {
                    event: event.clone()
                }
            }
        }
        NotificationType::Zap(event) => {
            rsx! {
                ZapNotification {
                    event: event.clone()
                }
            }
        }
    }
}

#[component]
fn ReactionNotification(event: NostrEvent) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut reacted_post = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);

    let reactor_pubkey = event.pubkey.to_string();

    // Get the reaction emoji (NIP-25)
    // Custom emoji URL if present
    let custom_emoji_url = if event.content.starts_with(':') && event.content.ends_with(':') {
        let shortcode = event.content.trim_matches(':');
        event.tags.iter()
            .find_map(|tag| {
                let vec = (*tag).clone().to_vec();
                if vec.get(0).map(|k| k == "emoji").unwrap_or(false) &&
                   vec.get(1).map(|s| s == shortcode).unwrap_or(false) {
                    vec.get(2).cloned()
                } else {
                    None
                }
            })
    } else {
        None
    };

    let reaction_emoji = if event.content.is_empty() || event.content == "+" {
        "❤️".to_string() // Default to heart if empty or just "+"
    } else if event.content == "-" {
        "👎".to_string() // Thumbs down for downvote
    } else if custom_emoji_url.is_some() {
        event.content.clone() // Will show as custom image below
    } else {
        event.content.clone() // Regular emoji
    };

    // Get the event ID that was reacted to
    let reacted_event_id = event.tags.iter()
        .find(|tag| tag.kind() == nostr_sdk::TagKind::SingleLetter(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::E)
        ))
        .and_then(|tag| tag.content())
        .map(|s| s.to_string());

    let reactor_pubkey_for_effect = reactor_pubkey.clone();
    let reactor_pubkey_for_display = reactor_pubkey.clone();
    let reactor_pubkey_for_avatar = reactor_pubkey.clone();
    let reactor_pubkey_for_link = reactor_pubkey.clone();

    // Fetch profile and reacted post
    use_effect(move || {
        let pubkey = reactor_pubkey_for_effect.clone();
        let event_id = reacted_event_id.clone();

        spawn(async move {
            // Fetch reactor's profile
            if let Ok(p) = profiles::fetch_profile(pubkey).await {
                profile.set(Some(p));
            }

            // Fetch the original post that was reacted to
            if let Some(eid) = event_id {
                if let Ok(client) = nostr_client::NOSTR_CLIENT.read().as_ref()
                    .ok_or("Client not initialized")
                {
                    let filter = Filter::new()
                        .id(nostr_sdk::EventId::from_hex(&eid).unwrap())
                        .limit(1);

                    // Use gossip for automatic relay routing
                    if let Ok(events) = client.fetch_events(filter, Duration::from_secs(5)).await
                        .map(|e| e.into_iter().collect::<Vec<_>>()) {
                        if let Some(original_event) = events.into_iter().next() {
                            reacted_post.set(Some(original_event));
                        }
                    }
                }
            }

            loading.set(false);
        });
    });

    let display_name = profile.read().as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| format!("{}...", &reactor_pubkey_for_display[..16]));

    let avatar_url = profile.read().as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", reactor_pubkey_for_avatar));

    rsx! {
        div {
            class: "p-4 hover:bg-accent/50 transition",

            // Header with reaction info
            div {
                class: "flex items-center gap-3 mb-2",

                // Profile image
                Link {
                    to: Route::Profile { pubkey: reactor_pubkey_for_link.clone() },
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    img {
                        src: "{avatar_url}",
                        alt: "{display_name}",
                        class: "w-10 h-10 rounded-full object-cover flex-shrink-0",
                    }
                }

                // Reaction text
                div {
                    class: "flex items-center gap-2 text-sm",
                    // Show custom emoji image or regular emoji
                    if let Some(emoji_url) = custom_emoji_url {
                        img {
                            src: "{emoji_url}",
                            alt: "{reaction_emoji}",
                            class: "w-6 h-6 inline-block",
                        }
                    } else {
                        span {
                            class: "text-2xl",
                            "{reaction_emoji}"
                        }
                    }
                    Link {
                        to: Route::Profile { pubkey: reactor_pubkey_for_link.clone() },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        class: "font-semibold hover:underline",
                        "{display_name}"
                    }
                    span {
                        class: "text-muted-foreground",
                        "reacted to your post"
                    }
                }
            }

            // Show the original post that was reacted to
            if let Some(post) = reacted_post.read().as_ref() {
                div {
                    class: "ml-13 mt-2",
                    NoteCard {
                        event: post.clone(),
                        collapsible: true
                    }
                }
            } else if *loading.read() {
                div {
                    class: "ml-13 mt-2 text-sm text-muted-foreground",
                    "Loading post..."
                }
            }
        }
    }
}

#[component]
fn RepostNotification(event: NostrEvent) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut reposted_post = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);

    let reposter_pubkey = event.pubkey.to_string();

    // Get the event ID that was reposted from 'e' tag
    let reposted_event_id = event.tags.iter()
        .find(|tag| tag.kind() == nostr_sdk::TagKind::SingleLetter(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::E)
        ))
        .and_then(|tag| tag.content())
        .map(|s| s.to_string());

    let reposter_pubkey_for_effect = reposter_pubkey.clone();
    let reposter_pubkey_for_display = reposter_pubkey.clone();
    let reposter_pubkey_for_avatar = reposter_pubkey.clone();
    let reposter_pubkey_for_link = reposter_pubkey.clone();

    // Fetch profile and reposted post
    use_effect(move || {
        let pubkey = reposter_pubkey_for_effect.clone();
        let event_id = reposted_event_id.clone();

        spawn(async move {
            // Fetch reposter's profile
            if let Ok(p) = profiles::fetch_profile(pubkey).await {
                profile.set(Some(p));
            }

            // Fetch the original post that was reposted
            if let Some(eid) = event_id {
                if let Ok(client) = nostr_client::NOSTR_CLIENT.read().as_ref()
                    .ok_or("Client not initialized")
                {
                    let filter = Filter::new()
                        .id(nostr_sdk::EventId::from_hex(&eid).unwrap())
                        .limit(1);

                    // Use gossip for automatic relay routing
                    if let Ok(events) = client.fetch_events(filter, Duration::from_secs(5)).await
                        .map(|e| e.into_iter().collect::<Vec<_>>()) {
                        if let Some(original_event) = events.into_iter().next() {
                            reposted_post.set(Some(original_event));
                        }
                    }
                }
            }

            loading.set(false);
        });
    });

    let display_name = profile.read().as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| format!("{}...", &reposter_pubkey_for_display[..16]));

    let avatar_url = profile.read().as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", reposter_pubkey_for_avatar));

    rsx! {
        div {
            class: "p-4 hover:bg-accent/50 transition",

            // Header with repost info
            div {
                class: "flex items-center gap-3 mb-2",

                // Profile image
                Link {
                    to: Route::Profile { pubkey: reposter_pubkey_for_link.clone() },
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    img {
                        src: "{avatar_url}",
                        alt: "{display_name}",
                        class: "w-10 h-10 rounded-full object-cover flex-shrink-0",
                    }
                }

                // Repost text
                div {
                    class: "flex items-center gap-2 text-sm",
                    span {
                        class: "text-green-500 text-2xl",
                        "🔁"
                    }
                    Link {
                        to: Route::Profile { pubkey: reposter_pubkey_for_link.clone() },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        class: "font-semibold hover:underline",
                        "{display_name}"
                    }
                    span {
                        class: "text-muted-foreground",
                        "reposted your post"
                    }
                }
            }

            // Show the original post that was reposted
            if let Some(post) = reposted_post.read().as_ref() {
                div {
                    class: "ml-13 mt-2",
                    NoteCard {
                        event: post.clone(),
                        collapsible: true
                    }
                }
            } else if *loading.read() {
                div {
                    class: "ml-13 mt-2 text-sm text-muted-foreground",
                    "Loading post..."
                }
            }
        }
    }
}

#[component]
fn ZapNotification(event: NostrEvent) -> Element {
    let mut profile = use_signal(|| None::<profiles::Profile>);
    let mut zapped_post = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);

    // Extract the actual zapper's pubkey from the description tag (zap request)
    // The event.pubkey is the Lightning node's pubkey, not the actual zapper
    let zapper_pubkey = extract_zapper_pubkey(&event).unwrap_or_else(|| event.pubkey.to_string());

    // Get the zap amount from the bolt11 tag or description
    let zap_amount_sats = extract_zap_amount(&event);

    // Get the event ID that was zapped from 'e' tag
    let zapped_event_id = event.tags.iter()
        .find(|tag| tag.kind() == nostr_sdk::TagKind::SingleLetter(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::E)
        ))
        .and_then(|tag| tag.content())
        .map(|s| s.to_string());

    let zapper_pubkey_for_effect = zapper_pubkey.clone();
    let zapper_pubkey_for_display = zapper_pubkey.clone();
    let zapper_pubkey_for_avatar = zapper_pubkey.clone();
    let zapper_pubkey_for_link = zapper_pubkey.clone();

    // Fetch profile and zapped post
    use_effect(move || {
        let pubkey = zapper_pubkey_for_effect.clone();
        let event_id = zapped_event_id.clone();

        spawn(async move {
            // Fetch zapper's profile
            if let Ok(p) = profiles::fetch_profile(pubkey).await {
                profile.set(Some(p));
            }

            // Fetch the original post that was zapped
            if let Some(eid) = event_id {
                if let Ok(client) = nostr_client::NOSTR_CLIENT.read().as_ref()
                    .ok_or("Client not initialized")
                {
                    let filter = Filter::new()
                        .id(nostr_sdk::EventId::from_hex(&eid).unwrap())
                        .limit(1);

                    // Use gossip for automatic relay routing
                    if let Ok(events) = client.fetch_events(filter, Duration::from_secs(5)).await
                        .map(|e| e.into_iter().collect::<Vec<_>>()) {
                        if let Some(original_event) = events.into_iter().next() {
                            zapped_post.set(Some(original_event));
                        }
                    }
                }
            }

            loading.set(false);
        });
    });

    let display_name = profile.read().as_ref()
        .map(|p| p.get_display_name())
        .unwrap_or_else(|| format!("{}...", &zapper_pubkey_for_display[..16]));

    let avatar_url = profile.read().as_ref()
        .map(|p| p.get_avatar_url())
        .unwrap_or_else(|| format!("https://api.dicebear.com/7.x/identicon/svg?seed={}", zapper_pubkey_for_avatar));

    rsx! {
        div {
            class: "p-4 hover:bg-accent/50 transition",

            // Header with zap info
            div {
                class: "flex items-center gap-3 mb-2",

                // Profile image
                Link {
                    to: Route::Profile { pubkey: zapper_pubkey_for_link.clone() },
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    img {
                        src: "{avatar_url}",
                        alt: "{display_name}",
                        class: "w-10 h-10 rounded-full object-cover flex-shrink-0",
                    }
                }

                // Zap text
                div {
                    class: "flex items-center gap-2 text-sm",
                    span {
                        class: "text-yellow-500 text-2xl",
                        "⚡"
                    }
                    Link {
                        to: Route::Profile { pubkey: zapper_pubkey_for_link.clone() },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        class: "font-semibold hover:underline",
                        "{display_name}"
                    }
                    span {
                        class: "text-muted-foreground",
                        "zapped your post"
                    }
                    if let Some(amount) = zap_amount_sats {
                        span {
                            class: "text-yellow-600 dark:text-yellow-400 font-bold",
                            "{amount} sats"
                        }
                    }
                }
            }

            // Show the original post that was zapped
            if let Some(post) = zapped_post.read().as_ref() {
                div {
                    class: "ml-13 mt-2",
                    NoteCard {
                        event: post.clone()
                    }
                }
            } else if *loading.read() {
                div {
                    class: "ml-13 mt-2 text-sm text-muted-foreground",
                    "Loading post..."
                }
            }
        }
    }
}

/// Helper to extract the actual zapper's pubkey from a zap receipt event (kind 9735)
/// The event.pubkey is the Lightning node's pubkey, the actual zapper is in the description
fn extract_zapper_pubkey(event: &NostrEvent) -> Option<String> {
    // Find the description tag which contains the zap request
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        let vec = (*tag).clone().to_vec();
        vec.get(0).map(|k| k == "description").unwrap_or(false)
    }) {
        let vec = (*description_tag).clone().to_vec();
        if let Some(description) = vec.get(1) {
            // Parse the zap request event from the description
            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(description) {
                // Extract the pubkey from the zap request event
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
    // Try to find the bolt11 tag and parse the amount from it
    if let Some(bolt11_tag) = event.tags.iter().find(|tag| {
        let vec = (*tag).clone().to_vec();
        vec.get(0).map(|k| k == "bolt11").unwrap_or(false)
    }) {
        let vec = (*bolt11_tag).clone().to_vec();
        if let Some(bolt11) = vec.get(1) {
            return parse_bolt11_amount(bolt11);
        }
    }

    // Fallback: try to parse from description tag (zap request)
    if let Some(description_tag) = event.tags.iter().find(|tag| {
        let vec = (*tag).clone().to_vec();
        vec.get(0).map(|k| k == "description").unwrap_or(false)
    }) {
        let vec = (*description_tag).clone().to_vec();
        if let Some(description) = vec.get(1) {
            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(description) {
                if let Some(amount_msat) = zap_request.get("amount").and_then(|a| a.as_u64()) {
                    return Some(amount_msat / 1000); // Convert millisats to sats
                }
            }
        }
    }

    None
}

/// Parse amount from bolt11 invoice string
fn parse_bolt11_amount(bolt11: &str) -> Option<u64> {
    // bolt11 format: ln[prefix][amount][multiplier]...
    // Example: lnbc1000n... where 1000n = 1000 nanosats

    let lower = bolt11.to_lowercase();

    // Find where the amount starts (after "lnbc" or "lntb" etc)
    let prefix_end = if lower.starts_with("lnbc") {
        4
    } else if lower.starts_with("lntb") {
        4
    } else if lower.starts_with("lnbcrt") {
        6
    } else {
        return None;
    };

    let amount_part = &lower[prefix_end..];

    // Find the multiplier character (p, n, u, m or no multiplier)
    let mut amount_str = String::new();
    let mut multiplier_char = None;

    for c in amount_part.chars() {
        if c.is_ascii_digit() {
            amount_str.push(c);
        } else if c == 'p' || c == 'n' || c == 'u' || c == 'm' {
            multiplier_char = Some(c);
            break;
        } else {
            break;
        }
    }

    let amount: u64 = amount_str.parse().ok()?;

    // Convert to satoshis based on multiplier
    let sats = match multiplier_char {
        Some('m') => amount * 100_000,      // milli-bitcoin = 100,000 sats
        Some('u') => amount * 100,          // micro-bitcoin = 100 sats
        Some('n') => amount / 10,           // nano-bitcoin = 0.1 sats
        Some('p') => amount / 10_000,       // pico-bitcoin = 0.0001 sats
        None => amount * 100_000_000,       // whole bitcoin = 100,000,000 sats
        _ => return None,
    };

    Some(sats)
}

/// Helper to get timestamp from notification
fn get_timestamp(notification: &NotificationType) -> u64 {
    match notification {
        NotificationType::Mention(e) | NotificationType::Reply(e) |
        NotificationType::Reaction(e) | NotificationType::Repost(e) |
        NotificationType::Zap(e) => e.created_at.as_secs(),
    }
}

async fn load_notifications(until: Option<u64>) -> Result<Vec<NotificationType>, String> {
    let client = nostr_client::NOSTR_CLIENT.read().as_ref()
        .ok_or("Client not initialized")?.clone();

    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated")?;

    log::info!("Loading notifications for {} (until: {:?})", pubkey_str, until);

    let mut all_notifications = Vec::new();

    // Build unified filter for all notification types using #p tag
    // This is the correct way - fetch events that tag our pubkey
    // Use limit: 100 for historical/initial load
    let mut filter = Filter::new()
        .kinds(vec![
            Kind::TextNote,      // 1 - for mentions and replies
            Kind::Repost,        // 6
            Kind::Reaction,      // 7
            Kind::ZapReceipt,    // 9735
        ])
        .custom_tag(
            nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::P),
            pubkey_str.clone()
        )
        .limit(100);

    // Add until timestamp for pagination
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    // Use gossip for automatic relay routing
    match client.fetch_events(filter, Duration::from_secs(10)).await
        .map(|e| e.into_iter().collect::<Vec<_>>()) {
        Ok(events) => {
            for event in events {
                // Skip our own events
                if event.pubkey.to_hex() == pubkey_str {
                    continue;
                }

                match event.kind {
                    Kind::TextNote => {
                        // Check if it's a reply (has 'e' tag) or just a mention
                        let is_reply = event.tags.iter().any(|tag| {
                            tag.kind() == nostr_sdk::TagKind::SingleLetter(
                                nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::E)
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
        }
        Err(e) => {
            log::error!("Failed to fetch notifications: {}", e);
            return Err(format!("Failed to fetch notifications: {}", e));
        }
    }

    // Sort by timestamp (newest first)
    all_notifications.sort_by(|a, b| {
        get_timestamp(b).cmp(&get_timestamp(a))
    });

    log::info!("Loaded {} notifications", all_notifications.len());
    Ok(all_notifications)
}

/// Batch prefetch author metadata for notification authors
async fn prefetch_notification_authors(notifications: &[NotificationType]) {
    use crate::utils::profile_prefetch;

    if notifications.is_empty() {
        return;
    }

    // Extract pubkeys directly without string conversion
    let pubkeys = profile_prefetch::extract_pubkeys(notifications, |notif| {
        match notif {
            NotificationType::Mention(e) => e.pubkey,
            NotificationType::Reply(e) => e.pubkey,
            NotificationType::Reaction(e) => e.pubkey,
            NotificationType::Repost(e) => e.pubkey,
            NotificationType::Zap(e) => e.pubkey,
        }
    });

    // Use optimized prefetch utility - no string conversions, direct database queries
    profile_prefetch::prefetch_pubkeys(pubkeys).await;
}
