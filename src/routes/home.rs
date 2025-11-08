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
                                    oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                                }

                                // Determine if there are more events based on RAW count before filtering
                                has_more.set(raw_count >= 100);

                                // Show feed immediately with database-cached metadata
                                events.set(feed_events.clone());
                                loading.set(false);

                                // Spawn non-blocking background prefetch for missing metadata
                                spawn(async move {
                                    prefetch_author_metadata(&feed_events).await;
                                });
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
                                    oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                                }

                                // Determine if there are more events to load
                                has_more.set(feed_events.len() >= 150);

                                // Show feed immediately with database-cached metadata
                                events.set(feed_events.clone());
                                loading.set(false);

                                // Spawn non-blocking background prefetch for missing metadata
                                spawn(async move {
                                    prefetch_author_metadata(&feed_events).await;
                                });
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

            log::info!("Starting real-time subscription for {} followed users using gossip", contacts.len());

            match client.subscribe(filter, None).await {
                Ok(output) => {
                    let home_feed_sub_id = output.val;
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
                                oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                            }

                            // Determine if there are more events based on RAW count before filtering
                            has_more.set(raw_count >= 100);

                            // Append and show new events immediately
                            let prefetch_events = new_events.clone();
                            let mut current = events.read().clone();
                            current.append(&mut new_events);
                            events.set(current);
                            loading.set(false);

                            // Spawn non-blocking background prefetch for missing metadata
                            spawn(async move {
                                prefetch_author_metadata(&prefetch_events).await;
                            });
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
                                oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                            }

                            // Determine if there are more events to load
                            has_more.set(new_events.len() >= 150);

                            // Append and show new events immediately
                            let prefetch_events = new_events.clone();
                            let mut current = events.read().clone();
                            current.append(&mut new_events);
                            events.set(current);
                            loading.set(false);

                            // Spawn non-blocking background prefetch for missing metadata
                            spawn(async move {
                                prefetch_author_metadata(&prefetch_events).await;
                            });
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
// Returns (events, raw_count_before_filtering) tuple
async fn load_following_feed(until: Option<u64>) -> Result<(Vec<Event>, usize), String> {
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

    // Fetch events using aggregated pattern (database-first)
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

    // Fetch events using aggregated pattern (database-first)
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

    // Fetch events using aggregated pattern (database-first)
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

/// Batch prefetch author metadata for all events
/// This checks the database first and only fetches missing metadata
async fn prefetch_author_metadata(events: &[Event]) {
    use std::collections::HashSet;
    use crate::stores::profiles;

    // Collect unique author pubkeys as hex strings
    let pubkeys: Vec<String> = events.iter()
        .map(|e| e.pubkey.to_hex())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    if pubkeys.is_empty() {
        return;
    }

    log::info!("Batch fetching profiles for {} unique authors", pubkeys.len());

    // Use the batch fetch function which populates PROFILE_CACHE
    match profiles::fetch_profiles_batch(pubkeys).await {
        Ok(profiles_map) => {
            log::info!("Successfully batch fetched {} profiles into cache", profiles_map.len());
            // Profiles are now cached in PROFILE_CACHE with LRU eviction
        }
        Err(e) => {
            log::warn!("Failed to batch fetch profiles: {}", e);
            // Non-fatal - individual components will fetch as needed
        }
    }
}
