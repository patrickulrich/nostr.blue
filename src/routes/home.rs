use dioxus::prelude::*;
use crate::stores::{auth_store, nostr_client};
use crate::routes::Route;
use crate::components::{NoteCard, NoteComposer, ArticleCard, ClientInitializing};
use crate::hooks::use_infinite_scroll;
use nostr_sdk::{Event, Filter, Kind, Timestamp, PublicKey};
use std::time::Duration;

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
    let relays = nostr_client::RELAY_POOL.read();

    // State for feed events
    let mut events = use_signal(|| Vec::<Event>::new());
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);

    // Pagination state for infinite scroll
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Load feed on mount and when refresh is triggered or feed type changes
    use_effect(move || {
        // Watch refresh trigger and feed type
        let _ = refresh_trigger.read();
        let current_feed_type = *feed_type.read();

        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load feed if both authenticated AND client is initialized
        if is_authenticated && client_initialized {
            loading.set(true);
            error.set(None);
            oldest_timestamp.set(None);
            has_more.set(true);

            spawn(async move {
                match current_feed_type {
                    FeedType::Following => {
                        match load_following_feed(None).await {
                            Ok((feed_events, raw_count)) => {
                                // Track oldest timestamp for pagination
                                if let Some(last_event) = feed_events.last() {
                                    oldest_timestamp.set(Some(last_event.created_at.as_u64()));
                                }

                                // Determine if there are more events based on RAW count before filtering
                                has_more.set(raw_count >= 100);

                                events.set(feed_events);
                                loading.set(false);
                            }
                            Err(e) => {
                                error.set(Some(e));
                                loading.set(false);
                            }
                        }
                    }
                    FeedType::FollowingWithReplies => {
                        match load_following_with_replies(None).await {
                            Ok(feed_events) => {
                                // Track oldest timestamp for pagination
                                if let Some(last_event) = feed_events.last() {
                                    oldest_timestamp.set(Some(last_event.created_at.as_u64()));
                                }

                                // Determine if there are more events to load
                                has_more.set(feed_events.len() >= 150);

                                events.set(feed_events);
                                loading.set(false);
                            }
                            Err(e) => {
                                error.set(Some(e));
                                loading.set(false);
                            }
                        }
                    }
                }
            });
        }
    });

    // Real-time subscription for live feed updates
    use_effect(move || {
        let current_feed_type = *feed_type.read();
        let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only subscribe if both authenticated AND client is initialized
        if !is_authenticated || !client_initialized {
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

            // Subscribe to new posts from followed users
            let filter = Filter::new()
                .kind(Kind::TextNote)
                .authors(authors)
                .since(Timestamp::now())
                .limit(0); // limit=0 means only new events

            log::info!("Starting real-time subscription for {} followed users", contacts.len());

            match client.subscribe(filter, None).await {
                Ok(output) => {
                    let home_feed_sub_id = output.val.clone();
                    log::info!("Subscribed to home feed updates: {:?}", home_feed_sub_id);

                    // Handle incoming events
                    spawn(async move {
                        let mut notifications = client.notifications();

                        while let Ok(notification) = notifications.recv().await {
                            if let nostr_sdk::RelayPoolNotification::Event {
                                subscription_id,
                                event,
                                ..
                            } = notification
                            {
                                // Only process events from our home feed subscription
                                if subscription_id != home_feed_sub_id {
                                    continue;
                                }

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
                                    log::info!("New post received in real-time!");

                                    // Check if event already exists (avoid duplicates)
                                    let exists = {
                                        let current_events = events.read();
                                        current_events.iter().any(|e| e.id == event.id)
                                    };

                                    if !exists {
                                        // Prepend to feed
                                        let current_events = events.read().clone();
                                        let mut new_events = vec![(*event).clone()];
                                        new_events.extend(current_events);
                                        events.set(new_events);
                                    }
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    log::error!("Failed to subscribe to home feed: {}", e);
                }
            }
        });
    });

    // Load more function for infinite scroll
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }

        let until = *oldest_timestamp.read();
        let current_feed_type = *feed_type.read();

        loading.set(true);

        spawn(async move {
            match current_feed_type {
                FeedType::Following => {
                    match load_following_feed(until).await {
                        Ok((mut new_events, raw_count)) => {
                            // Track oldest timestamp from new events
                            if let Some(last_event) = new_events.last() {
                                oldest_timestamp.set(Some(last_event.created_at.as_u64()));
                            }

                            // Determine if there are more events based on RAW count before filtering
                            has_more.set(raw_count >= 100);

                            // Append new events to existing events
                            let mut current = events.read().clone();
                            current.append(&mut new_events);
                            events.set(current);
                            loading.set(false);
                        }
                        Err(e) => {
                            log::error!("Failed to load more events: {}", e);
                            loading.set(false);
                        }
                    }
                }
                FeedType::FollowingWithReplies => {
                    match load_following_with_replies(until).await {
                        Ok(mut new_events) => {
                            // Track oldest timestamp from new events
                            if let Some(last_event) = new_events.last() {
                                oldest_timestamp.set(Some(last_event.created_at.as_u64()));
                            }

                            // Determine if there are more events to load
                            has_more.set(new_events.len() >= 150);

                            // Append new events to existing events
                            let mut current = events.read().clone();
                            current.append(&mut new_events);
                            events.set(current);
                            loading.set(false);
                        }
                        Err(e) => {
                            log::error!("Failed to load more events: {}", e);
                            loading.set(false);
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
        loading
    );

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
                            disabled: *loading.read(),
                            onclick: move |_| {
                                let current = *refresh_trigger.read();
                                refresh_trigger.set(current + 1);
                            },
                            title: "Refresh feed",
                            if *loading.read() {
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
                } else if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && events.read().is_empty()) {
                    // Show client initializing animation during:
                    // 1. Client initialization
                    // 2. Initial feed load (loading + no events, regardless of error state)
                    // This prevents error flashing during the loading process
                    ClientInitializing {}
                } else {
                    // Show feed (only after loading is complete)
                    if let Some(err) = error.read().as_ref() {
                        // Only show error if we're not loading and have no events
                        if !*loading.read() && events.read().is_empty() {
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
                        }
                    } else if events.read().is_empty() {
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
                        // Show events (with conditional rendering for articles)
                        for event in events.read().iter() {
                            // Check if this is a long-form article (NIP-23)
                            if event.kind == Kind::LongFormTextNote {
                                ArticleCard {
                                    key: "{event.id}",
                                    event: event.clone()
                                }
                            } else {
                                NoteCard {
                                    event: event.clone()
                                }
                            }
                        }

                        // Infinite scroll sentinel / loading indicator
                        if *has_more.read() {
                            div {
                                id: "{sentinel_id}",
                                class: "p-8 flex justify-center",
                                if *loading.read() {
                                    span {
                                        class: "flex items-center gap-2 text-muted-foreground",
                                        span {
                                            class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin"
                                        }
                                        "Loading more..."
                                    }
                                }
                            }
                        } else if !events.read().is_empty() {
                            div {
                                class: "p-8 text-center text-muted-foreground",
                                "You've reached the end"
                            }
                        }
                    }
                }
            }

            // Relay Status (collapsed at bottom)
            details {
                class: "border-b border-gray-200 dark:border-gray-800",
                summary {
                    class: "px-4 py-3 cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-800/50 text-sm text-gray-600 dark:text-gray-400",
                    "📡 Relay Status ({relays.len()} connected)"
                }
                div {
                    class: "px-4 py-2 space-y-1",
                    if relays.is_empty() {
                        div {
                            class: "py-4 text-center text-gray-500 dark:text-gray-400 text-sm",
                            "Connecting to relays..."
                        }
                    } else {
                        for relay in relays.iter() {
                            div {
                                class: "flex justify-between items-center py-2 text-sm",
                                span {
                                    class: "font-mono text-gray-600 dark:text-gray-400 truncate flex-1",
                                    "{relay.url}"
                                }
                                span {
                                    class: match relay.status {
                                        nostr_client::RelayStatus::Connected => "text-green-600 dark:text-green-400 text-xs",
                                        nostr_client::RelayStatus::Connecting => "text-yellow-600 dark:text-yellow-400 text-xs",
                                        nostr_client::RelayStatus::Disconnected => "text-gray-600 dark:text-gray-400 text-xs",
                                        nostr_client::RelayStatus::Error(_) => "text-red-600 dark:text-red-400 text-xs",
                                    },
                                    "●"
                                }
                            }
                        }
                    }
                }
            }

        }
    }
}

#[component]
fn LoginSection() -> Element {
    use nostr::ToBech32;

    let mut nsec_input = use_signal(|| String::new());
    let mut npub_input = use_signal(|| String::new());
    let mut error = use_signal(|| None::<String>);
    let mut active_tab = use_signal(|| "nsec");

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
            class: "p-6 max-w-md mx-auto",
            h3 {
                class: "text-2xl font-bold mb-6 text-gray-900 dark:text-white text-center",
                "Connect Your Account"
            }

            // NIP-07 Extension Login (Recommended)
            if has_extension {
                div {
                    class: "mb-6 p-4 bg-gradient-to-r from-purple-50 to-blue-50 dark:from-purple-900/20 dark:to-blue-900/20 rounded-lg border-2 border-purple-200 dark:border-purple-700",
                    div {
                        class: "flex items-center gap-2 mb-2",
                        span {
                            class: "text-lg",
                            "🔌"
                        }
                        span {
                            class: "font-semibold text-gray-900 dark:text-white",
                            "Browser Extension (Recommended)"
                        }
                        span {
                            class: "px-2 py-0.5 text-xs bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200 rounded-full",
                            "Most Secure"
                        }
                    }
                    p {
                        class: "text-sm text-gray-600 dark:text-gray-400 mb-3",
                        "Sign in securely with your Nostr extension (Alby, nos2x, Flamingo, etc.)"
                    }
                    button {
                        class: "w-full px-4 py-3 bg-gradient-to-r from-purple-600 to-blue-600 hover:from-purple-700 hover:to-blue-700 text-white rounded-lg font-medium transition shadow-lg",
                        onclick: login_with_extension,
                        "🔐 Connect Extension"
                    }
                }
            } else {
                div {
                    class: "mb-6 p-4 bg-yellow-50 dark:bg-yellow-900/20 rounded-lg border border-yellow-200 dark:border-yellow-700",
                    div {
                        class: "flex items-center gap-2 mb-2",
                        span {
                            class: "text-lg",
                            "ℹ️"
                        }
                        span {
                            class: "font-semibold text-gray-900 dark:text-white",
                            "No Extension Detected"
                        }
                    }
                    p {
                        class: "text-sm text-gray-600 dark:text-gray-400 mb-2",
                        "For the best security, install a Nostr browser extension:"
                    }
                    ul {
                        class: "text-sm text-gray-600 dark:text-gray-400 list-disc list-inside space-y-1",
                        li {
                            a {
                                href: "https://getalby.com",
                                target: "_blank",
                                class: "text-blue-600 dark:text-blue-400 hover:underline",
                                "Alby"
                            }
                            " - Bitcoin Lightning + Nostr"
                        }
                        li {
                            a {
                                href: "https://github.com/fiatjaf/nos2x",
                                target: "_blank",
                                class: "text-blue-600 dark:text-blue-400 hover:underline",
                                "nos2x"
                            }
                            " - Simple Nostr extension"
                        }
                        li {
                            a {
                                href: "https://www.getflamingo.org",
                                target: "_blank",
                                class: "text-blue-600 dark:text-blue-400 hover:underline",
                                "Flamingo"
                            }
                            " - Feature-rich Nostr wallet"
                        }
                    }
                }
            }

            // Divider
            div {
                class: "flex items-center gap-3 my-4",
                div {
                    class: "flex-1 border-t border-gray-300 dark:border-gray-600"
                }
                span {
                    class: "text-sm text-gray-500 dark:text-gray-400",
                    "Or use private/public key"
                }
                div {
                    class: "flex-1 border-t border-gray-300 dark:border-gray-600"
                }
            }

            div {
                class: "flex gap-2 mb-4",
                button {
                    class: if *active_tab.read() == "nsec" {
                        "px-4 py-2 bg-blue-600 text-white rounded-lg font-medium"
                    } else {
                        "px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg"
                    },
                    onclick: move |_| active_tab.set("nsec"),
                    "Private Key (nsec)"
                }
                button {
                    class: if *active_tab.read() == "npub" {
                        "px-4 py-2 bg-blue-600 text-white rounded-lg font-medium"
                    } else {
                        "px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg"
                    },
                    onclick: move |_| active_tab.set("npub"),
                    "Public Key (npub)"
                }
            }

            if let Some(err) = error.read().as_ref() {
                div {
                    class: "mb-4 p-3 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                    "❌ {err}"
                }
            }

            if *active_tab.read() == "nsec" {
                div {
                    class: "space-y-3",
                    input {
                        class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        r#type: "password",
                        placeholder: "nsec1...",
                        value: "{nsec_input}",
                        oninput: move |evt| nsec_input.set(evt.value())
                    }
                    div {
                        class: "flex gap-2",
                        button {
                            class: "flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition",
                            onclick: login_with_nsec,
                            "Login"
                        }
                        button {
                            class: "px-4 py-2 bg-green-600 hover:bg-green-700 text-white rounded-lg font-medium transition",
                            onclick: generate_new,
                            "Generate New"
                        }
                    }
                    p {
                        class: "text-sm text-gray-600 dark:text-gray-400",
                        "⚠️ Stored in localStorage. Only use on trusted devices."
                    }
                }
            } else {
                div {
                    class: "space-y-3",
                    input {
                        class: "w-full px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent",
                        r#type: "text",
                        placeholder: "npub1...",
                        value: "{npub_input}",
                        oninput: move |evt| npub_input.set(evt.value())
                    }
                    button {
                        class: "w-full px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg font-medium transition",
                        onclick: login_with_npub,
                        "Load Profile (Read-Only)"
                    }
                    p {
                        class: "text-sm text-gray-600 dark:text-gray-400",
                        "ℹ️ Read-only: View content but cannot post."
                    }
                }
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
                            None => "Unknown",
                        }
                    }
                }
            }
        }
    }
}

// Helper function to load following feed
// Returns (events, raw_count_before_filtering) tuple
async fn load_following_feed(until: Option<u64>) -> Result<(Vec<Event>, usize), String> {
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

    // Create filter for posts from followed users
    let mut filter = Filter::new()
        .kind(Kind::TextNote)
        .authors(authors)
        .limit(100);

    // Add until for pagination, no since filter to allow going back indefinitely
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    log::info!("Fetching events from {} followed accounts", filter.authors.as_ref().map(|a| a.len()).unwrap_or(0));

    // Fetch events using aggregated pattern (database first, then relays)
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let raw_count = events.len();
            log::info!("Loaded {} events from following feed", raw_count);

            // Convert to Vec, filter out replies (posts with e tags), and sort by created_at (newest first)
            let mut event_vec: Vec<Event> = events.into_iter()
                .filter(|event| {
                    // Filter out replies - posts with e tags are replies
                    use nostr_sdk::TagKind;
                    !event.tags.iter().any(|tag| tag.kind() == TagKind::e())
                })
                .collect();

            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            log::info!("After filtering out replies: {} top-level posts (raw: {})", event_vec.len(), raw_count);

            // If no events found, fall back to global feed
            if event_vec.is_empty() {
                log::info!("No top-level posts from followed users, showing global feed");
                let global = load_global_feed(until).await?;
                let count = global.len();
                return Ok((global, count));
            }

            Ok((event_vec, raw_count))
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
async fn load_following_with_replies(until: Option<u64>) -> Result<Vec<Event>, String> {
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

    // Create filter for all kind 1 posts from followed users (including replies)
    // Unlike load_following_feed, we don't filter out posts with e-tags
    let mut filter = Filter::new()
        .kind(Kind::TextNote)
        .authors(authors)
        .limit(150); // Increased limit since we're getting more content

    // Add until for pagination, or since for initial load
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    } else {
        let since = Timestamp::now() - Duration::from_secs(86400); // 24 hours ago
        filter = filter.since(since);
    }

    log::info!("Fetching all events (including replies) from {} followed accounts", filter.authors.as_ref().map(|a| a.len()).unwrap_or(0));

    // Fetch events using aggregated pattern
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} events (including replies) from following feed", events.len());

            // Convert to Vec and sort by created_at (newest first)
            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            // If no events found, fall back to global feed
            if event_vec.is_empty() {
                log::info!("No events from followed users, showing global feed");
                return load_global_feed(until).await;
            }

            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch following feed with replies: {}, falling back to global", e);
            load_global_feed(until).await
        }
    }
}

// Helper function to load global feed
async fn load_global_feed(until: Option<u64>) -> Result<Vec<Event>, String> {
    log::info!("Loading global feed (until: {:?})...", until);

    // Create filter for recent text notes (kind 1)
    let mut filter = Filter::new()
        .kind(Kind::TextNote)
        .limit(50);

    // Add until for pagination, or since for initial load
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    } else {
        let since = Timestamp::now() - Duration::from_secs(86400); // 24 hours ago
        filter = filter.since(since);
    }

    log::info!("Fetching events with filter: {:?}", filter);

    // Fetch events using aggregated pattern
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            log::info!("Loaded {} events", events.len());

            // Convert Events to Vec<Event> and sort by created_at (newest first)
            let mut event_vec: Vec<Event> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch events: {}", e);
            Err(format!("Failed to load feed: {}", e))
        }
    }
}
