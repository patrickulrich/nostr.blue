use dioxus::prelude::*;
use crate::stores::{nostr_client, auth_store};
use crate::components::{NoteCard, NoteCardSkeleton};
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;
use nostr_sdk::prelude::*;
use nostr_sdk::{Event as NostrEvent, TagKind};
use std::time::Duration;

#[derive(Clone, PartialEq, Debug)]
enum ProfileTab {
    Posts,
    Replies,
    Articles,
    Media,
    Likes,
}

#[component]
pub fn Profile(pubkey: String) -> Element {
    // State management
    let mut profile_data = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Tab and events state
    let mut active_tab = use_signal(|| ProfileTab::Posts);
    let mut events = use_signal(|| Vec::<NostrEvent>::new());
    let mut loading_events = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);

    // Follow state
    let mut is_following = use_signal(|| false);
    let mut follow_loading = use_signal(|| false);
    let mut follows_you = use_signal(|| false);

    // Stats
    let mut following_count = use_signal(|| 0);
    let mut followers_count = use_signal(|| 0);
    let mut post_count = use_signal(|| 0);

    // Clone pubkey for various uses
    let pubkey_for_metadata = pubkey.clone();
    let pubkey_for_events = pubkey.clone();
    let pubkey_for_follow = pubkey.clone();
    let pubkey_for_stats = pubkey.clone();
    let pubkey_for_button = pubkey.clone();
    let pubkey_for_follows_you = pubkey.clone();
    let pubkey_for_display = pubkey.clone();
    let pubkey_for_load_more = pubkey.clone();

    // Parse pubkey once for comparisons
    let parsed_pubkey = PublicKey::from_bech32(&pubkey)
        .or_else(|_| PublicKey::from_hex(&pubkey))
        .ok();

    // Check if this is own profile
    let auth = auth_store::AUTH_STATE.read();
    let is_own_profile = auth.pubkey.as_ref()
        .and_then(|pk| PublicKey::parse(pk).ok())
        .and_then(|user_pk| parsed_pubkey.map(|profile_pk| user_pk == profile_pk))
        .unwrap_or(false);

    // Fetch profile metadata
    use_effect(move || {
        let pubkey_str = pubkey_for_metadata.clone();
        spawn(async move {
            loading.set(true);
            error.set(None);

            // Parse the public key
            let public_key = match PublicKey::from_bech32(&pubkey_str)
                .or_else(|_| PublicKey::from_hex(&pubkey_str)) {
                Ok(pk) => pk,
                Err(e) => {
                    error.set(Some(format!("Invalid public key: {}", e)));
                    loading.set(false);
                    return;
                }
            };

            // Get the client
            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => {
                    error.set(Some("Nostr client not initialized".to_string()));
                    loading.set(false);
                    return;
                }
            };

            // Create filter for kind 0 (metadata) events
            let filter = Filter::new()
                .author(public_key)
                .kind(Kind::Metadata)
                .limit(1);

            // Query relays
            match client.fetch_events(filter, Duration::from_secs(10)).await {
                Ok(events) => {
                    if let Some(event) = events.into_iter().next() {
                        match nostr_sdk::Metadata::from_json(&event.content) {
                            Ok(metadata) => {
                                profile_data.set(Some(metadata));
                            }
                            Err(e) => {
                                log::warn!("Failed to parse metadata: {}", e);
                                // Set empty metadata so we can still show the profile
                                profile_data.set(Some(nostr_sdk::Metadata::new()));
                            }
                        }
                    } else {
                        // No metadata event found, use empty metadata
                        profile_data.set(Some(nostr_sdk::Metadata::new()));
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch profile: {}", e);
                    // Still set empty metadata so profile can be viewed
                    profile_data.set(Some(nostr_sdk::Metadata::new()));
                }
            }

            loading.set(false);
        });
    });

    // Fetch events based on active tab
    use_effect(move || {
        let tab = active_tab.read().clone();
        let pubkey_str = pubkey_for_events.clone();

        loading_events.set(true);
        oldest_timestamp.set(None);

        spawn(async move {
            match load_tab_events(&pubkey_str, &tab, None).await {
                Ok(tab_events) => {
                    if let Some(last) = tab_events.last() {
                        oldest_timestamp.set(Some(last.created_at.as_u64()));
                    }
                    has_more.set(tab_events.len() >= 50);

                    // Count posts for header (only for Posts tab)
                    if matches!(tab, ProfileTab::Posts) {
                        post_count.set(tab_events.len());
                    }

                    events.set(tab_events);
                }
                Err(e) => {
                    log::error!("Failed to load events: {}", e);
                    events.set(Vec::new());
                }
            }
            loading_events.set(false);
        });
    });

    // Check if following this user
    use_effect(move || {
        if !auth_store::is_authenticated() {
            return;
        }

        let pubkey_str = pubkey_for_follow.clone();
        spawn(async move {
            // Convert pubkey to hex format for comparison
            let hex_pubkey = if let Ok(pk) = PublicKey::from_bech32(&pubkey_str) {
                pk.to_hex()
            } else if let Ok(pk) = PublicKey::from_hex(&pubkey_str) {
                pk.to_hex()
            } else {
                return;
            };

            match nostr_client::is_following(hex_pubkey).await {
                Ok(following) => {
                    is_following.set(following);
                }
                Err(e) => {
                    log::error!("Failed to check following status: {}", e);
                }
            }
        });
    });

    // Check if this user follows you
    use_effect(move || {
        if !auth_store::is_authenticated() {
            return;
        }

        let my_pubkey = match auth_store::get_pubkey() {
            Some(pk) => pk,
            None => return,
        };

        let profile_pubkey_str = pubkey_for_follows_you.clone();

        spawn(async move {
            // Convert profile pubkey to hex
            let profile_hex = if let Ok(pk) = PublicKey::from_bech32(&profile_pubkey_str) {
                pk.to_hex()
            } else if let Ok(pk) = PublicKey::from_hex(&profile_pubkey_str) {
                pk.to_hex()
            } else {
                return;
            };

            // Fetch their contact list
            match nostr_client::fetch_contacts(profile_hex).await {
                Ok(contacts) => {
                    // Check if our pubkey is in their contacts
                    let my_hex = if let Ok(pk) = PublicKey::parse(&my_pubkey) {
                        pk.to_hex()
                    } else {
                        return;
                    };

                    follows_you.set(contacts.contains(&my_hex));
                }
                Err(e) => {
                    log::debug!("Failed to check if user follows you: {}", e);
                }
            }
        });
    });

    // Fetch following/followers counts
    use_effect(move || {
        let pubkey_str = pubkey_for_stats.clone();

        spawn(async move {
            // Convert to hex
            let hex_pubkey = if let Ok(pk) = PublicKey::from_bech32(&pubkey_str) {
                pk.to_hex()
            } else if let Ok(pk) = PublicKey::from_hex(&pubkey_str) {
                pk.to_hex()
            } else {
                return;
            };

            // Fetch following count (from their contact list)
            match nostr_client::fetch_contacts(hex_pubkey.clone()).await {
                Ok(contacts) => {
                    following_count.set(contacts.len());
                }
                Err(e) => {
                    log::debug!("Failed to fetch following count: {}", e);
                }
            }

            // Fetch followers count (would require indexing service - placeholder for now)
            // TODO: Implement via relay or indexer
            followers_count.set(0);
        });
    });

    // Load more handler
    let load_more = move || {
        if *loading_events.read() || !*has_more.read() {
            return;
        }

        let tab = active_tab.read().clone();
        let pubkey_str = pubkey_for_load_more.clone();
        let until = *oldest_timestamp.read();

        loading_events.set(true);

        spawn(async move {
            match load_tab_events(&pubkey_str, &tab, until).await {
                Ok(mut new_events) => {
                    if let Some(last) = new_events.last() {
                        oldest_timestamp.set(Some(last.created_at.as_u64()));
                    }
                    has_more.set(new_events.len() >= 50);

                    // Append new events
                    let mut current = events.read().clone();
                    current.append(&mut new_events);
                    events.set(current);
                }
                Err(e) => {
                    log::error!("Failed to load more events: {}", e);
                }
            }
            loading_events.set(false);
        });
    };

    // Set up infinite scroll
    let sentinel_id = use_infinite_scroll(
        load_more,
        has_more,
        loading_events
    );


    rsx! {
        div {
            class: "min-h-screen",

            // Header with back button
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center gap-4",
                    button {
                        class: "p-2 hover:bg-accent rounded-full transition",
                        onclick: move |_| {
                            let nav = navigator();
                            nav.go_back();
                        },
                        "←"
                    }
                    div {
                        if let Some(metadata) = profile_data.read().as_ref() {
                            h2 {
                                class: "text-xl font-bold",
                                "{get_display_name(metadata, &pubkey_for_display)}"
                            }
                            if matches!(*active_tab.read(), ProfileTab::Posts) && *post_count.read() > 0 {
                                p {
                                    class: "text-sm text-muted-foreground",
                                    "{post_count.read()} posts"
                                }
                            }
                        }
                    }
                }
            }

            // Banner with overlapping avatar
            div {
                class: "relative",
                // Banner image
                if let Some(metadata) = profile_data.read().as_ref() {
                    if let Some(banner) = &metadata.banner {
                        img {
                            src: "{banner}",
                            class: "w-full h-48 object-cover",
                            alt: "Profile banner"
                        }
                    } else {
                        // Gradient fallback
                        div {
                            class: "w-full h-48 bg-gradient-to-r from-blue-500 via-purple-500 to-pink-500"
                        }
                    }
                } else {
                    div {
                        class: "w-full h-48 bg-gradient-to-r from-blue-500 via-purple-500 to-pink-500"
                    }
                }

                // Avatar positioned absolutely overlapping the banner (half over, half below)
                div {
                    class: "absolute bottom-0 left-4 transform translate-y-1/2",
                    if let Some(metadata) = profile_data.read().as_ref() {
                        if let Some(picture) = &metadata.picture {
                            img {
                                class: "w-32 h-32 rounded-full border-4 border-background bg-background",
                                src: "{picture}",
                                alt: "Avatar"
                            }
                        } else {
                            div {
                                class: "w-32 h-32 rounded-full border-4 border-background bg-blue-600 flex items-center justify-center text-white text-4xl font-bold",
                                "{get_avatar_initial(metadata)}"
                            }
                        }
                    }
                }
            }

            // Profile info section
            div {
                class: "px-4 pb-4",

                // Buttons aligned to the right
                div {
                    class: "flex justify-end pt-4 mb-16",
                    if is_own_profile {
                        Link {
                            to: Route::Settings {},
                            class: "px-6 py-2 border border-border rounded-full font-semibold hover:bg-accent transition inline-block",
                            "Edit Profile"
                        }
                    } else if auth.is_authenticated {
                        button {
                                class: if *is_following.read() {
                                    "px-6 py-2 border border-border rounded-full font-semibold hover:bg-accent transition"
                                } else {
                                    "px-6 py-2 bg-foreground text-background rounded-full font-semibold hover:opacity-90 transition"
                                },
                                disabled: *follow_loading.read(),
                                onclick: move |_| {
                                    let pubkey_clone = pubkey_for_button.clone();
                                    follow_loading.set(true);

                                    spawn(async move {
                                        // Convert to hex
                                        let hex_pubkey = if let Ok(pk) = PublicKey::from_bech32(&pubkey_clone) {
                                            pk.to_hex()
                                        } else if let Ok(pk) = PublicKey::from_hex(&pubkey_clone) {
                                            pk.to_hex()
                                        } else {
                                            follow_loading.set(false);
                                            return;
                                        };

                                        let result = if *is_following.read() {
                                            nostr_client::unfollow_user(hex_pubkey).await
                                        } else {
                                            nostr_client::follow_user(hex_pubkey).await
                                        };

                                        match result {
                                            Ok(_) => {
                                                let current = *is_following.read();
                                                is_following.set(!current);
                                            }
                                            Err(e) => {
                                                log::error!("Failed to follow/unfollow: {}", e);
                                            }
                                        }

                                        follow_loading.set(false);
                                    });
                                },
                                if *follow_loading.read() {
                                    "..."
                                } else if *is_following.read() {
                                    "Following"
                                } else {
                                    "Follow"
                                }
                            }
                        }
                    }

                // "Follows You" badge
                if *follows_you.read() && !is_own_profile && auth.is_authenticated {
                    span {
                        class: "inline-block px-2 py-1 bg-muted text-muted-foreground text-xs rounded mb-2",
                        "Follows you"
                    }
                }

                // Display name and username
                if let Some(metadata) = profile_data.read().as_ref() {
                    h1 {
                        class: "text-2xl font-bold",
                        "{get_display_name(metadata, &pubkey_for_display)}"
                    }
                    p {
                        class: "text-muted-foreground",
                        "@{get_username(metadata, &pubkey_for_display)}"
                    }

                    // Bio
                    if let Some(about) = &metadata.about {
                        if !about.is_empty() {
                            p {
                                class: "whitespace-pre-wrap mt-3",
                                "{about}"
                            }
                        }
                    }

                    // Website and joined date
                    div {
                        class: "flex flex-wrap gap-4 mt-3 text-sm text-muted-foreground",

                        if let Some(website) = &metadata.website {
                            if !website.is_empty() {
                                a {
                                    href: "{website}",
                                    target: "_blank",
                                    rel: "noopener noreferrer",
                                    class: "text-blue-500 hover:underline flex items-center gap-1",
                                    "🔗 {strip_https(website)}"
                                }
                            }
                        }

                        // Joined date placeholder
                        span {
                            class: "flex items-center gap-1",
                            "📅 Joined recently"
                        }
                    }

                    // Following/Followers counts
                    div {
                        class: "flex gap-4 mt-3",
                        div {
                            class: "hover:underline cursor-pointer",
                            span {
                                class: "font-bold",
                                "{following_count.read()}"
                            }
                            span {
                                class: "text-muted-foreground ml-1",
                                "Following"
                            }
                        }
                        div {
                            class: "hover:underline cursor-pointer",
                            span {
                                class: "font-bold",
                                "{followers_count.read()}"
                            }
                            span {
                                class: "text-muted-foreground ml-1",
                                "Followers"
                            }
                        }
                    }
                }
            }

            // Content tabs
            div {
                class: "border-b border-border sticky top-[57px] bg-background z-10",
                div {
                    class: "flex overflow-x-auto scrollbar-hide",

                    ProfileTabButton {
                        label: "Posts",
                        active: matches!(*active_tab.read(), ProfileTab::Posts),
                        onclick: move |_| active_tab.set(ProfileTab::Posts)
                    }
                    ProfileTabButton {
                        label: "Replies",
                        active: matches!(*active_tab.read(), ProfileTab::Replies),
                        onclick: move |_| active_tab.set(ProfileTab::Replies)
                    }
                    ProfileTabButton {
                        label: "Articles",
                        active: matches!(*active_tab.read(), ProfileTab::Articles),
                        onclick: move |_| active_tab.set(ProfileTab::Articles)
                    }
                    ProfileTabButton {
                        label: "Media",
                        active: matches!(*active_tab.read(), ProfileTab::Media),
                        onclick: move |_| active_tab.set(ProfileTab::Media)
                    }
                    ProfileTabButton {
                        label: "Likes",
                        active: matches!(*active_tab.read(), ProfileTab::Likes),
                        onclick: move |_| active_tab.set(ProfileTab::Likes)
                    }
                }
            }

            // Content area
            div {
                if *loading_events.read() && events.read().is_empty() {
                    div {
                        class: "divide-y divide-border",
                        for _ in 0..5 {
                            NoteCardSkeleton {}
                        }
                    }
                } else if !events.read().is_empty() {
                    div {
                        class: "divide-y divide-border",
                        for event in events.read().iter() {
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
                            if *loading_events.read() {
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
                } else {
                    // Empty state
                    div {
                        class: "text-center py-12",
                        div {
                            class: "text-6xl mb-4",
                            "{get_empty_state_icon(&active_tab.read())}"
                        }
                        p {
                            class: "text-muted-foreground",
                            "{get_empty_state_message(&active_tab.read())}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ProfileTabButton(label: &'static str, active: bool, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "flex-shrink-0 px-4 py-4 font-semibold hover:bg-accent transition relative",
            onclick: move |e| onclick.call(e),

            span {
                class: if active { "" } else { "text-muted-foreground" },
                "{label}"
            }

            if active {
                div {
                    class: "absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-t"
                }
            }
        }
    }
}

// Helper function to load events based on tab type
async fn load_tab_events(pubkey: &str, tab: &ProfileTab, until: Option<u64>) -> Result<Vec<NostrEvent>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    // Parse the public key
    let public_key = PublicKey::from_bech32(pubkey)
        .or_else(|_| PublicKey::from_hex(pubkey))
        .map_err(|e| format!("Invalid public key: {}", e))?;

    // Create filter based on tab type
    let mut filter = Filter::new()
        .author(public_key)
        .limit(50);

    // Add until timestamp for pagination
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    // Modify filter based on tab
    match tab {
        ProfileTab::Posts => {
            // Kind 1 (text notes) only - we'll filter out replies after fetching
            filter = filter.kind(Kind::TextNote);
        }
        ProfileTab::Replies => {
            // Kind 1 (text notes) only - we'll filter for replies after fetching
            filter = filter.kind(Kind::TextNote);
        }
        ProfileTab::Articles => {
            // Kind 30023 (long-form content)
            filter = filter.kind(Kind::LongFormTextNote);
        }
        ProfileTab::Media => {
            // Video kinds (not yet in nostr-sdk constants, use custom)
            // For now, return empty
            return Ok(Vec::new());
        }
        ProfileTab::Likes => {
            // Kind 7 (reactions)
            filter = filter.kind(Kind::Reaction);
        }
    }

    log::info!("Fetching events with filter: {:?}", filter);

    // Fetch events from relays
    match client.fetch_events(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            let mut event_vec: Vec<NostrEvent> = events.into_iter().collect();

            // Filter based on tab type
            match tab {
                ProfileTab::Posts => {
                    // Only events WITHOUT 'e' tags (not replies)
                    event_vec.retain(|e| {
                        !e.tags.iter().any(|t| t.kind() == TagKind::e())
                    });
                }
                ProfileTab::Replies => {
                    // Only events WITH 'e' tags (replies)
                    event_vec.retain(|e| {
                        e.tags.iter().any(|t| t.kind() == TagKind::e())
                    });
                }
                _ => {
                    // Other tabs don't need additional filtering
                }
            }

            // Sort by created_at (newest first)
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            log::info!("Loaded {} events for tab {:?}", event_vec.len(), tab);
            Ok(event_vec)
        }
        Err(e) => {
            log::error!("Failed to fetch events: {}", e);
            Err(format!("Failed to fetch events: {}", e))
        }
    }
}

// Helper functions
fn get_display_name(metadata: &nostr_sdk::Metadata, pubkey: &str) -> String {
    metadata.display_name
        .clone()
        .or_else(|| metadata.name.clone())
        .unwrap_or_else(|| {
            // Generate from pubkey
            if let Ok(pk) = PublicKey::from_hex(pubkey).or_else(|_| PublicKey::from_bech32(pubkey)) {
                let hex = pk.to_hex();
                format!("{}...{}", &hex[..8], &hex[hex.len()-4..])
            } else {
                "Unknown".to_string()
            }
        })
}

fn get_username(metadata: &nostr_sdk::Metadata, pubkey: &str) -> String {
    metadata.name.clone().unwrap_or_else(|| {
        if let Ok(pk) = PublicKey::from_hex(pubkey).or_else(|_| PublicKey::from_bech32(pubkey)) {
            let npub = pk.to_bech32().expect("to_bech32 is infallible");
            if npub.len() > 18 {
                format!("{}...{}", &npub[..12], &npub[npub.len()-6..])
            } else {
                npub
            }
        } else {
            "unknown".to_string()
        }
    })
}

fn get_avatar_initial(metadata: &nostr_sdk::Metadata) -> String {
    metadata.display_name
        .as_ref()
        .or(metadata.name.as_ref())
        .and_then(|n| n.chars().next())
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn strip_https(url: &str) -> String {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
        .to_string()
}

fn get_empty_state_message(tab: &ProfileTab) -> &'static str {
    match tab {
        ProfileTab::Posts => "No posts yet",
        ProfileTab::Replies => "No replies yet",
        ProfileTab::Articles => "No articles yet",
        ProfileTab::Media => "No media yet",
        ProfileTab::Likes => "No likes yet",
    }
}

fn get_empty_state_icon(tab: &ProfileTab) -> &'static str {
    match tab {
        ProfileTab::Posts => "📝",
        ProfileTab::Replies => "💬",
        ProfileTab::Articles => "📄",
        ProfileTab::Media => "🎬",
        ProfileTab::Likes => "❤️",
    }
}
