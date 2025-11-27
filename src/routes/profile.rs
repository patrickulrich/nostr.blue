use dioxus::prelude::*;
use crate::stores::{nostr_client, auth_store, dms};
use crate::components::{NoteCard, ClientInitializing, ProfileEditorModal, PhotoCard, VideoCard, ArticleCard};
use crate::components::icons::{InfoIcon, MailIcon};
use crate::components::dialog::{DialogRoot, DialogTitle, DialogDescription};
use crate::hooks::use_infinite_scroll;
use crate::services::profile_stats;
use nostr_sdk::prelude::*;
use nostr_sdk::{Event as NostrEvent, TagKind};
use nostr_sdk::nips::nip19::ToBech32;
use std::time::Duration;
use std::collections::HashMap;
use wasm_bindgen::JsCast;

#[derive(Clone, PartialEq, Debug, Eq, Hash)]
enum MediaSubTab {
    Photos,
    Videos,
    Verts,
}

#[derive(Clone, PartialEq, Debug, Eq, Hash)]
enum ProfileTab {
    Posts,
    Replies,
    Articles,
    Media(MediaSubTab),
    Likes,
}

// Per-tab state to track events, pagination, and loading status
#[derive(Clone, Debug)]
struct TabData {
    events: Vec<NostrEvent>,
    oldest_timestamp: Option<u64>,
    has_more: bool,
    loaded: bool,
}

// Result type for load_tab_events containing events and the proper pagination cursor
#[derive(Clone, Debug)]
struct LoadOutcome {
    events: Vec<NostrEvent>,
    // The cursor for pagination - for Likes this is the oldest reaction timestamp,
    // for other tabs it's the oldest event.created_at
    oldest_cursor: Option<u64>,
}

impl Default for TabData {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            oldest_timestamp: None,
            has_more: true,
            loaded: false,
        }
    }
}

#[component]
pub fn Profile(pubkey: String) -> Element {
    // State management
    let mut profile_data = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Tab and events state
    let mut active_tab = use_signal(|| ProfileTab::Posts);
    let mut tab_data = use_signal(|| {
        let mut map = HashMap::new();
        map.insert(ProfileTab::Posts, TabData::default());
        map.insert(ProfileTab::Replies, TabData::default());
        map.insert(ProfileTab::Articles, TabData::default());
        map.insert(ProfileTab::Media(MediaSubTab::Photos), TabData::default());
        map.insert(ProfileTab::Media(MediaSubTab::Videos), TabData::default());
        map.insert(ProfileTab::Media(MediaSubTab::Verts), TabData::default());
        map.insert(ProfileTab::Likes, TabData::default());
        map
    });
    let mut loading_events = use_signal(|| false);
    let mut current_tab_has_more = use_signal(|| true);

    // Follow state
    let mut is_following = use_signal(|| false);
    let mut follow_loading = use_signal(|| false);
    let mut follows_you = use_signal(|| false);

    // Stats
    let mut following_count = use_signal(|| 0);
    let mut followers_count = use_signal(|| 0);
    let mut post_count = use_signal(|| 0);

    // Profile editor modal
    let mut show_profile_modal = use_signal(|| false);

    // DM dialog state
    let mut show_dm_dialog = use_signal(|| false);
    let mut dm_message = use_signal(|| String::new());
    let mut dm_sending = use_signal(|| false);
    let mut dm_error = use_signal(|| None::<String>);

    // Info dialog state (npub/lightning)
    let mut show_info_dialog = use_signal(|| false);

    // Clone pubkey for various uses
    let pubkey_for_metadata = pubkey.clone();
    let pubkey_for_events = pubkey.clone();
    let pubkey_for_follow = pubkey.clone();
    let pubkey_for_stats = pubkey.clone();
    let pubkey_for_button = pubkey.clone();
    let pubkey_for_follows_you = pubkey.clone();
    let pubkey_for_display = pubkey.clone();
    let pubkey_for_load_more = pubkey.clone();
    let pubkey_for_dm = pubkey.clone();
    let pubkey_for_info = pubkey.clone();

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
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

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

            // Get client for metadata fetching
            let client = match nostr_client::get_client() {
                Some(c) => c,
                None => {
                    error.set(Some("Client not initialized".to_string()));
                    loading.set(false);
                    return;
                }
            };

            // 2-tier fetch: Check database first (instant), then fetch from relays if needed
            // This matches the SDK-recommended pattern used in note_card.rs

            // Tier 1: Check database cache first (instant, no network)
            if let Ok(Some(metadata)) = client.database().metadata(public_key).await {
                log::debug!("Loaded profile metadata from database cache");
                profile_data.set(Some(metadata));
                loading.set(false);
                return;
            }

            // Tier 2: Not in cache, fetch from relays (with gossip routing)
            match client.fetch_metadata(public_key, Duration::from_secs(5)).await {
                Ok(Some(metadata)) => {
                    log::debug!("Fetched profile metadata from relays");
                    profile_data.set(Some(metadata));
                }
                Ok(None) => {
                    log::debug!("No metadata found, using empty profile");
                    // No metadata event found, use empty metadata
                    profile_data.set(Some(nostr_sdk::Metadata::new()));
                }
                Err(e) => {
                    log::error!("Failed to fetch profile metadata: {}", e);
                    // Still set empty metadata so profile can be viewed
                    profile_data.set(Some(nostr_sdk::Metadata::new()));
                }
            }

            loading.set(false);
        });
    });

    // Fetch events based on active tab - TWO-PHASE LOADING for instant display
    // Phase 1: Load from DB instantly (cached data)
    // Phase 2: Fetch from relays in background (fresh data)
    use_effect(move || {
        let tab = active_tab.read().clone();
        let pubkey_str = pubkey_for_events.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

        // Check if this tab is already loaded
        let already_loaded = tab_data.read().get(&tab).map(|d| d.loaded).unwrap_or(false);
        if already_loaded {
            // Tab already loaded, just update has_more signal for infinite scroll
            let has_more = tab_data.read().get(&tab).map(|d| d.has_more).unwrap_or(true);
            current_tab_has_more.set(has_more);
            return;
        }

        loading_events.set(true);

        // Clone for the spawned tasks
        let pubkey_for_relay = pubkey_str.clone();
        let tab_for_relay = tab.clone();

        spawn(async move {
            // ===== PHASE 1: Load from DB instantly =====
            match load_tab_events_db(&pubkey_str, &tab, None).await {
                Ok(db_outcome) => {
                    let oldest_ts = db_outcome.oldest_cursor.map(|ts| ts.saturating_sub(1));
                    // Always assume there might be more from relays
                    let has_more = true;

                    // Count posts for header (only for Posts tab)
                    if matches!(tab, ProfileTab::Posts) {
                        post_count.set(db_outcome.events.len());
                    }

                    // Show DB results immediately
                    let mut data_map = tab_data.read().clone();
                    data_map.insert(tab.clone(), TabData {
                        events: db_outcome.events.clone(),
                        oldest_timestamp: oldest_ts,
                        has_more,
                        loaded: true, // Mark as loaded so UI shows content
                    });
                    tab_data.set(data_map);
                    current_tab_has_more.set(has_more);

                    // Stop showing loading spinner - DB results are displayed
                    loading_events.set(false);

                    log::info!("Phase 1 complete: showing {} events from DB instantly", db_outcome.events.len());

                    // Prefetch metadata for DB results
                    let db_events_for_metadata = db_outcome.events.clone();
                    spawn(async move {
                        prefetch_author_metadata(&db_events_for_metadata).await;
                    });
                }
                Err(e) => {
                    log::warn!("DB phase failed: {}, will try relays", e);
                    // Don't set loading to false yet - relay fetch will provide data
                }
            }

            // ===== PHASE 2: Fetch from relays in background =====
            spawn(async move {
                match load_tab_events_relays(&pubkey_for_relay, &tab_for_relay, None).await {
                    Ok(relay_outcome) => {
                        // Merge relay results with existing DB results
                        let mut data_map = tab_data.read().clone();
                        let existing_data = data_map.get(&tab_for_relay).cloned().unwrap_or_default();

                        // Build set of existing event IDs
                        let existing_ids: std::collections::HashSet<_> = existing_data.events.iter()
                            .map(|e| e.id)
                            .collect();

                        // Find new events from relay
                        let new_events: Vec<_> = relay_outcome.events.into_iter()
                            .filter(|e| !existing_ids.contains(&e.id))
                            .collect();

                        if !new_events.is_empty() {
                            log::info!("Phase 2: found {} new events from relays", new_events.len());

                            // Merge and sort
                            let mut merged = existing_data.events;
                            merged.extend(new_events.clone());
                            merged.sort_by(|a, b| b.created_at.cmp(&a.created_at));

                            // Update oldest cursor
                            let oldest_ts = merged.last().map(|e| e.created_at.as_secs().saturating_sub(1));

                            // Update tab data
                            data_map.insert(tab_for_relay.clone(), TabData {
                                events: merged.clone(),
                                oldest_timestamp: oldest_ts,
                                has_more: true,
                                loaded: true,
                            });
                            tab_data.set(data_map);

                            // Update post count if Posts tab
                            if matches!(tab_for_relay, ProfileTab::Posts) {
                                post_count.set(merged.len());
                            }

                            // Prefetch metadata for new events
                            spawn(async move {
                                prefetch_author_metadata(&new_events).await;
                            });
                        } else {
                            log::info!("Phase 2: no new events from relays (all already in DB)");
                        }
                    }
                    Err(e) => {
                        log::warn!("Relay phase failed: {}, using DB results only", e);
                    }
                }

                // Ensure loading is false after both phases
                loading_events.set(false);
            });
        });
    });

    // Check if following this user
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized || !auth_store::is_authenticated() {
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

    // OPTIMIZATION: Combined "follows you" check + stats fetch
    // This eliminates a duplicate fetch_contacts() call and runs both in parallel
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        let pubkey_str = pubkey_for_stats.clone();
        let _ = pubkey_for_follows_you.clone(); // Used to track dependency, actual value comes from pubkey_str
        let is_authenticated = auth_store::is_authenticated();
        let my_pubkey = auth_store::get_pubkey();

        spawn(async move {
            // Convert profile pubkey to hex
            let hex_pubkey = if let Ok(pk) = PublicKey::from_bech32(&pubkey_str) {
                pk.to_hex()
            } else if let Ok(pk) = PublicKey::from_hex(&pubkey_str) {
                pk.to_hex()
            } else {
                return;
            };

            // Fetch contacts and nostr.band stats in parallel
            let contacts_future = nostr_client::fetch_contacts(hex_pubkey.clone());
            let stats_future = profile_stats::fetch_profile_stats(&hex_pubkey);

            let (contacts_result, stats_result) = futures::join!(contacts_future, stats_future);

            // Process contacts result - used for both "following count" and "follows you" check
            if let Ok(contacts) = contacts_result {
                // Set following count
                following_count.set(contacts.len());

                // Check if this user follows you (only if authenticated)
                if is_authenticated {
                    if let Some(ref my_pk) = my_pubkey {
                        if let Ok(pk) = PublicKey::parse(my_pk) {
                            let my_hex = pk.to_hex();
                            follows_you.set(contacts.contains(&my_hex));
                        }
                    }
                }
            }

            // Process stats result for followers count
            if let Ok(stats) = stats_result {
                if let Some(count) = stats.followers_pubkey_count {
                    followers_count.set(count as usize);
                }
            }
        });
    });

    // Load more handler
    let load_more = move || {
        let tab = active_tab.read().clone();

        log::info!("load_more called for tab {:?}", tab);

        // Get current tab's state
        let (has_more, until) = {
            let data = tab_data.read();
            let tab_state = data.get(&tab).unwrap();
            (tab_state.has_more, tab_state.oldest_timestamp)
        };

        log::info!("load_more: has_more={}, loading={}, until={:?}", has_more, *loading_events.read(), until);

        if *loading_events.read() || !has_more {
            log::info!("load_more: bailing early");
            return;
        }

        let pubkey_str = pubkey_for_load_more.clone();
        let mut post_count_clone = post_count.clone();

        loading_events.set(true);

        spawn(async move {
            match load_tab_events(&pubkey_str, &tab, until).await {
                Ok(outcome) => {
                    // Subtract 1 from the oldest cursor to avoid re-fetching the same last event
                    let oldest_ts = outcome.oldest_cursor.map(|ts| ts.saturating_sub(1));
                    // If we got 0 events, we've hit the end
                    let has_more_val = !outcome.events.is_empty();

                    log::info!("load_more: got {} new events, has_more={}", outcome.events.len(), has_more_val);

                    // Append new events to the current tab's data - clone, modify, set to trigger reactivity
                    let mut data_map = tab_data.read().clone();
                    if let Some(data) = data_map.get_mut(&tab) {
                        data.events.extend(outcome.events.clone());
                        data.oldest_timestamp = oldest_ts;
                        data.has_more = has_more_val;

                        // Update post count if we're on the Posts tab
                        if tab == ProfileTab::Posts {
                            post_count_clone.set(data.events.len());
                        }
                    }
                    tab_data.set(data_map);

                    // Update has_more signal for infinite scroll to continue working
                    current_tab_has_more.set(has_more_val);

                    // Spawn non-blocking background prefetch for missing metadata
                    spawn(async move {
                        prefetch_author_metadata(&outcome.events).await;
                    });
                }
                Err(e) => {
                    log::error!("Failed to load more events: {}", e);
                    // On error, disable further loading in both reactive signal and persisted state
                    current_tab_has_more.set(false);

                    // Update the persisted TabData.has_more as well
                    let mut data_map = tab_data.read().clone();
                    if let Some(data) = data_map.get_mut(&tab) {
                        data.has_more = false;
                    }
                    tab_data.set(data_map);
                }
            }
            loading_events.set(false);
        });
    };

    // Set up infinite scroll
    let sentinel_id = use_infinite_scroll(
        load_more,
        current_tab_has_more,
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
                        } else {
                            // Placeholder header while metadata loads
                            h2 {
                                class: "text-xl font-bold",
                                {
                                    if let Ok(pk) = PublicKey::from_bech32(&pubkey_for_display)
                                        .or_else(|_| PublicKey::from_hex(&pubkey_for_display)) {
                                        let npub = pk.to_bech32().unwrap_or_else(|_| pubkey_for_display.clone());
                                        if npub.len() > 16 {
                                            format!("{}...{}", &npub[..12], &npub[npub.len()-4..])
                                        } else {
                                            npub
                                        }
                                    } else {
                                        pubkey_for_display.clone()
                                    }
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
                    } else {
                        // Placeholder avatar while metadata loads
                        div {
                            class: "w-32 h-32 rounded-full border-4 border-background bg-gray-600 flex items-center justify-center text-white text-4xl font-bold",
                            "?"
                        }
                    }
                }
            }

            // Profile info section
            div {
                class: "px-4 pb-4",

                // Buttons aligned to the right
                div {
                    class: "flex justify-end gap-2 pt-4 mb-16",

                    // Info button (all profiles)
                    button {
                        class: "p-2 border border-border rounded-full hover:bg-accent transition",
                        onclick: move |_| show_info_dialog.set(true),
                        "aria-label": "Info",
                        title: "Info",
                        InfoIcon { class: "w-5 h-5".to_string(), filled: false }
                    }

                    // Message button (other users' profiles only)
                    if !is_own_profile && auth.is_authenticated {
                        button {
                            class: "p-2 border border-border rounded-full hover:bg-accent transition",
                            onclick: move |_| show_dm_dialog.set(true),
                            "aria-label": "Message",
                            title: "Message",
                            MailIcon { class: "w-5 h-5".to_string(), filled: false }
                        }
                    }

                    if is_own_profile {
                        button {
                            class: "px-6 py-2 border border-border rounded-full font-semibold hover:bg-accent transition",
                            onclick: move |_| show_profile_modal.set(true),
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
                } else {
                    // Placeholder profile info while metadata loads
                    h1 {
                        class: "text-2xl font-bold",
                        {
                            // Show truncated npub as placeholder
                            if let Ok(pk) = PublicKey::from_bech32(&pubkey_for_display)
                                .or_else(|_| PublicKey::from_hex(&pubkey_for_display)) {
                                let npub = pk.to_bech32().unwrap_or_else(|_| pubkey_for_display.clone());
                                if npub.len() > 16 {
                                    format!("{}...{}", &npub[..12], &npub[npub.len()-4..])
                                } else {
                                    npub
                                }
                            } else {
                                pubkey_for_display.clone()
                            }
                        }
                    }
                    p {
                        class: "text-muted-foreground",
                        {
                            // Show npub as username placeholder
                            if let Ok(pk) = PublicKey::from_bech32(&pubkey_for_display)
                                .or_else(|_| PublicKey::from_hex(&pubkey_for_display)) {
                                let npub = pk.to_bech32().unwrap_or_else(|_| pubkey_for_display.clone());
                                if npub.len() > 18 {
                                    format!("@{}...{}", &npub[..12], &npub[npub.len()-6..])
                                } else {
                                    format!("@{}", npub)
                                }
                            } else {
                                format!("@{}", pubkey_for_display)
                            }
                        }
                    }

                    // Following/Followers counts (still show even without metadata)
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
                        active: matches!(*active_tab.read(), ProfileTab::Media(_)),
                        onclick: move |_| active_tab.set(ProfileTab::Media(MediaSubTab::Photos))
                    }
                    ProfileTabButton {
                        label: "Likes",
                        active: matches!(*active_tab.read(), ProfileTab::Likes),
                        onclick: move |_| active_tab.set(ProfileTab::Likes)
                    }
                }

                // Media sub-tabs (only show when Media tab is active)
                if matches!(*active_tab.read(), ProfileTab::Media(_)) {
                    div {
                        class: "flex gap-2 px-4 py-2 bg-accent/10",
                        button {
                            class: if matches!(*active_tab.read(), ProfileTab::Media(MediaSubTab::Photos)) {
                                "px-4 py-2 rounded-full bg-primary text-primary-foreground font-medium"
                            } else {
                                "px-4 py-2 rounded-full hover:bg-accent font-medium"
                            },
                            onclick: move |_| active_tab.set(ProfileTab::Media(MediaSubTab::Photos)),
                            "Photos"
                        }
                        button {
                            class: if matches!(*active_tab.read(), ProfileTab::Media(MediaSubTab::Videos)) {
                                "px-4 py-2 rounded-full bg-primary text-primary-foreground font-medium"
                            } else {
                                "px-4 py-2 rounded-full hover:bg-accent font-medium"
                            },
                            onclick: move |_| active_tab.set(ProfileTab::Media(MediaSubTab::Videos)),
                            "Videos"
                        }
                        button {
                            class: if matches!(*active_tab.read(), ProfileTab::Media(MediaSubTab::Verts)) {
                                "px-4 py-2 rounded-full bg-primary text-primary-foreground font-medium"
                            } else {
                                "px-4 py-2 rounded-full hover:bg-accent font-medium"
                            },
                            onclick: move |_| active_tab.set(ProfileTab::Media(MediaSubTab::Verts)),
                            "Verts"
                        }
                    }
                }
            }

            // Content area
            div {
                {
                    // Get current tab's events
                    let tab = active_tab.read().clone();
                    let current_events = tab_data.read().get(&tab).map(|d| d.events.clone()).unwrap_or_default();
                    let current_has_more = tab_data.read().get(&tab).map(|d| d.has_more).unwrap_or(false);

                    log::debug!("Rendering tab {:?}: {} events, has_more={}, sentinel_signal={}",
                        tab, current_events.len(), current_has_more, *current_tab_has_more.read());

                    rsx! {
                        if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading_events.read() && current_events.is_empty()) {
                            // Show client initializing animation during:
                            // 1. Client initialization
                            // 2. Initial events load (loading + no events, regardless of error state)
                            ClientInitializing {}
                        } else if !current_events.is_empty() {
                            // Use grid layout for Articles and Verts, list layout for others
                            div {
                                class: match &tab {
                                    ProfileTab::Articles => "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 p-4",
                                    ProfileTab::Media(MediaSubTab::Verts) => "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3 p-4",
                                    _ => "divide-y divide-border"
                                },
                                for event in current_events.iter() {
                                    // Render the appropriate card based on tab type and event kind
                                    match &tab {
                                        ProfileTab::Articles => rsx! {
                                            ArticleCard {
                                                event: event.clone()
                                            }
                                        },
                                        ProfileTab::Media(MediaSubTab::Photos) => rsx! {
                                            PhotoCard {
                                                event: event.clone()
                                            }
                                        },
                                        ProfileTab::Media(MediaSubTab::Videos) => rsx! {
                                            VideoCard {
                                                event: event.clone()
                                            }
                                        },
                                        ProfileTab::Media(MediaSubTab::Verts) => rsx! {
                                            VertsVideoCard {
                                                event: event.clone()
                                            }
                                        },
                                        ProfileTab::Likes => {
                                            // Render based on the kind of event that was liked
                                            match event.kind.as_u16() {
                                                20 => rsx! {
                                                    PhotoCard {
                                                        event: event.clone()
                                                    }
                                                },
                                                21 | 22 => rsx! {
                                                    VideoCard {
                                                        event: event.clone()
                                                    }
                                                },
                                                30023 => rsx! {
                                                    ArticleCard {
                                                        event: event.clone()
                                                    }
                                                },
                                                _ => rsx! {
                                                    NoteCard {
                                                        event: event.clone(),
                                                        collapsible: true
                                                    }
                                                }
                                            }
                                        },
                                        _ => rsx! {
                                            NoteCard {
                                                event: event.clone(),
                                                collapsible: true
                                            }
                                        }
                                    }
                                }
                            }

                            // Infinite scroll sentinel / loading indicator
                            if current_has_more {
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
                            } else if !current_events.is_empty() {
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

        // Profile Editor Modal
        ProfileEditorModal { show: show_profile_modal }

        // DM Dialog
        DialogRoot {
            open: *show_dm_dialog.read(),
            div {
                class: "fixed inset-0 bg-black/50 backdrop-blur-sm z-50 flex items-center justify-center p-4",
                onclick: move |_| {
                    if !*dm_sending.read() {
                        show_dm_dialog.set(false);
                        dm_message.set(String::new());
                        dm_error.set(None);
                    }
                },
                div {
                    class: "bg-card border border-border rounded-lg shadow-xl p-6 max-w-md w-full",
                    onclick: move |e| e.stop_propagation(),

                    DialogTitle {
                        class: "text-xl font-semibold mb-2",
                        if let Some(metadata) = profile_data.read().as_ref() {
                            "Send message to {metadata.name.as_deref().unwrap_or(\"user\")}"
                        } else {
                            "Send message"
                        }
                    }

                    DialogDescription {
                        class: "text-sm text-muted-foreground mb-4",
                        "This message will be encrypted and sent privately."
                    }

                    textarea {
                        class: "w-full p-3 border border-border rounded-lg bg-background text-foreground resize-none focus:outline-none focus:ring-2 focus:ring-blue-500",
                        rows: "4",
                        placeholder: "Type your message...",
                        value: "{dm_message.read()}",
                        oninput: move |e| dm_message.set(e.value()),
                        disabled: *dm_sending.read()
                    }

                    if let Some(err) = dm_error.read().as_ref() {
                        div {
                            class: "mt-2 text-sm text-red-500",
                            "{err}"
                        }
                    }

                    div {
                        class: "flex justify-end gap-2 mt-4",
                        button {
                            class: "px-4 py-2 border border-border rounded-lg hover:bg-accent transition",
                            onclick: move |_| {
                                show_dm_dialog.set(false);
                                dm_message.set(String::new());
                                dm_error.set(None);
                            },
                            disabled: *dm_sending.read(),
                            "Cancel"
                        }
                        button {
                            class: "px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition disabled:opacity-50",
                            disabled: dm_message.read().trim().is_empty() || *dm_sending.read(),
                            onclick: move |_| {
                                let message = dm_message.read().clone();
                                let recipient = pubkey_for_dm.clone();
                                dm_sending.set(true);
                                dm_error.set(None);

                                spawn(async move {
                                    // Convert pubkey to proper format
                                    let hex_pubkey = if let Ok(pk) = PublicKey::from_bech32(&recipient) {
                                        pk.to_hex()
                                    } else if let Ok(pk) = PublicKey::from_hex(&recipient) {
                                        pk.to_hex()
                                    } else {
                                        dm_error.set(Some("Invalid public key".to_string()));
                                        dm_sending.set(false);
                                        return;
                                    };

                                    match dms::send_dm(hex_pubkey, message).await {
                                        Ok(_) => {
                                            show_dm_dialog.set(false);
                                            dm_message.set(String::new());
                                            dm_error.set(None);
                                            // Stay on profile page
                                        }
                                        Err(e) => {
                                            dm_error.set(Some(format!("Failed to send message: {}", e)));
                                        }
                                    }
                                    dm_sending.set(false);
                                });
                            },
                            if *dm_sending.read() {
                                "Sending..."
                            } else {
                                "Send"
                            }
                        }
                    }
                }
            }
        }

        // Info Dialog (npub/lightning)
        DialogRoot {
            open: *show_info_dialog.read(),
            div {
                class: "fixed inset-0 bg-black/50 backdrop-blur-sm z-50 flex items-center justify-center p-4",
                onclick: move |_| show_info_dialog.set(false),
                div {
                    class: "bg-card border border-border rounded-lg shadow-xl p-6 max-w-md w-full",
                    onclick: move |e| e.stop_propagation(),

                    DialogTitle {
                        class: "text-xl font-semibold mb-2",
                        "Profile Information"
                    }

                    DialogDescription {
                        class: "text-sm text-muted-foreground mb-4",
                        "Copy the public key or lightning address"
                    }

                    div {
                        class: "space-y-4",

                        // npub
                        div {
                            label {
                                class: "block text-sm font-medium mb-1",
                                "Public Key (npub)"
                            }
                            div {
                                class: "flex items-center gap-2",
                                div {
                                    class: "flex-1 p-2 bg-muted rounded border border-border text-sm font-mono break-all",
                                    {
                                        if let Ok(pk) = PublicKey::from_bech32(&pubkey_for_info)
                                            .or_else(|_| PublicKey::from_hex(&pubkey_for_info)) {
                                            pk.to_bech32().unwrap_or_else(|_| pubkey_for_info.clone())
                                        } else {
                                            pubkey_for_info.clone()
                                        }
                                    }
                                }
                                button {
                                    class: "px-3 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 transition",
                                    onclick: move |_| {
                                        if let Ok(pk) = PublicKey::from_bech32(&pubkey_for_info)
                                            .or_else(|_| PublicKey::from_hex(&pubkey_for_info)) {
                                            let npub = pk.to_bech32().unwrap();
                                            if let Some(window) = web_sys::window() {
                                                let _ = window.navigator().clipboard().write_text(&npub);
                                            }
                                        }
                                    },
                                    "Copy"
                                }
                            }
                        }

                        // Lightning address
                        if let Some(metadata) = profile_data.read().as_ref() {
                            if let Some(lud16) = &metadata.lud16 {
                                div {
                                    label {
                                        class: "block text-sm font-medium mb-1",
                                        "Lightning Address"
                                    }
                                    div {
                                        class: "flex items-center gap-2",
                                        div {
                                            class: "flex-1 p-2 bg-muted rounded border border-border text-sm break-all",
                                            "{lud16}"
                                        }
                                        button {
                                            class: "px-3 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 transition",
                                            onclick: move |_| {
                                                if let Some(metadata) = profile_data.read().as_ref() {
                                                    if let Some(lud16) = &metadata.lud16 {
                                                        if let Some(window) = web_sys::window() {
                                                            let _ = window.navigator().clipboard().write_text(lud16);
                                                        }
                                                    }
                                                }
                                            },
                                            "Copy"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div {
                        class: "flex justify-end mt-6",
                        button {
                            class: "px-4 py-2 bg-accent rounded-lg hover:bg-accent/80 transition",
                            onclick: move |_| show_info_dialog.set(false),
                            "Close"
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

// Video metadata structure for verts
#[derive(Clone, Debug, PartialEq)]
struct VideoMeta {
    url: Option<String>,
    thumbnail: Option<String>,
    title: Option<String>,
}

// Parse NIP-71 video metadata from event tags
fn parse_video_meta(event: &NostrEvent) -> VideoMeta {
    let mut meta = VideoMeta {
        url: None,
        thumbnail: None,
        title: None,
    };

    // Parse title tag
    for tag in event.tags.iter() {
        let tag_vec = (*tag).clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("title") && tag_vec.len() > 1 {
            meta.title = Some(tag_vec[1].clone());
            break;
        }
    }

    // Parse imeta tags
    for tag in event.tags.iter() {
        let tag_vec = (*tag).clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("imeta") {
            for field in tag_vec.iter().skip(1) {
                if let Some((key, value)) = field.split_once(' ') {
                    match key {
                        "url" => meta.url = Some(value.to_string()),
                        "image" => meta.thumbnail = Some(value.to_string()),
                        _ => {}
                    }
                }
            }
        }
    }

    meta
}

// Vertical video card component for verts
#[component]
fn VertsVideoCard(event: NostrEvent) -> Element {
    let video_meta = parse_video_meta(&event);
    let mut is_hovering = use_signal(|| false);
    let video_element_id = format!("preview-vert-{}", event.id.to_hex()[..12].to_string());
    let video_element_id_for_effect = video_element_id.clone();

    // Play/pause video on hover (only if no thumbnail)
    use_effect(use_reactive(&*is_hovering.read(), move |hovering| {
        let id = video_element_id_for_effect.clone();
        spawn(async move {
            if let Some(window) = web_sys::window() {
                if let Some(document) = window.document() {
                    if let Some(element) = document.get_element_by_id(&id) {
                        if let Ok(video) = element.dyn_into::<web_sys::HtmlVideoElement>() {
                            if hovering {
                                let _ = video.play();
                            } else {
                                let _ = video.pause();
                                video.set_current_time(0.0);
                            }
                        }
                    }
                }
            }
        });
    }));

    let video_id = event.id.to_hex();

    rsx! {
        div {
            class: "group cursor-pointer",
            onmouseenter: move |_| is_hovering.set(true),
            onmouseleave: move |_| is_hovering.set(false),

            Link {
                to: crate::routes::Route::VideoDetail { video_id: video_id.clone() },

                div {
                    class: "relative aspect-[9/16] bg-muted rounded-lg overflow-hidden mb-2",

                    // Show thumbnail if available, otherwise show video (first frame until hover)
                    if let Some(thumbnail) = &video_meta.thumbnail {
                        img {
                            src: "{thumbnail}",
                            alt: "{video_meta.title.as_deref().unwrap_or(\"Vert\")}",
                            class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200"
                        }
                    } else if let Some(url) = &video_meta.url {
                        video {
                            id: "{video_element_id}",
                            class: "w-full h-full object-cover",
                            src: "{url}",
                            muted: true,
                            loop: true,
                            playsinline: true,
                            preload: "metadata",
                        }
                    } else {
                        div {
                            class: "w-full h-full flex items-center justify-center bg-muted",
                            crate::components::icons::VideoIcon { class: "w-8 h-8 text-muted-foreground" }
                        }
                    }

                    // Verts indicator
                    div {
                        class: "absolute bottom-2 left-2 bg-black/80 text-white text-xs px-2 py-1 rounded flex items-center gap-1",
                        crate::components::icons::VideoIcon { class: "w-3 h-3" }
                        "Vert"
                    }
                }

                // Title
                if let Some(title) = &video_meta.title {
                    p {
                        class: "text-sm font-medium line-clamp-2 group-hover:text-primary transition",
                        "{title}"
                    }
                }
            }
        }
    }
}

/// Build a filter for the given tab type
fn build_tab_filter(public_key: PublicKey, tab: &ProfileTab, until: Option<u64>, limit: usize) -> Filter {
    let mut filter = match tab {
        ProfileTab::Posts | ProfileTab::Replies => {
            Filter::new()
                .author(public_key)
                .kind(Kind::TextNote)
                .limit(limit)
        }
        ProfileTab::Articles => {
            Filter::new()
                .author(public_key)
                .kind(Kind::LongFormTextNote)
                .limit(limit)
        }
        ProfileTab::Media(MediaSubTab::Photos) => {
            Filter::new()
                .author(public_key)
                .kind(Kind::Custom(20))
                .limit(limit)
        }
        ProfileTab::Media(MediaSubTab::Videos) => {
            Filter::new()
                .author(public_key)
                .kind(Kind::Custom(21))
                .limit(limit)
        }
        ProfileTab::Media(MediaSubTab::Verts) => {
            Filter::new()
                .author(public_key)
                .kind(Kind::Custom(22))
                .limit(limit)
        }
        ProfileTab::Likes => {
            Filter::new()
                .author(public_key)
                .kind(Kind::Reaction)
                .limit(limit)
        }
    };

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    filter
}

/// Filter and process events based on tab type
fn process_tab_events(events: Vec<NostrEvent>, tab: &ProfileTab) -> Vec<NostrEvent> {
    match tab {
        ProfileTab::Posts => {
            // Filter for posts only (no e-tags = not replies)
            events.into_iter()
                .filter(|e| !e.tags.iter().any(|t| t.kind() == TagKind::e()))
                .collect()
        }
        ProfileTab::Replies => {
            // Filter for replies only (with e-tags)
            events.into_iter()
                .filter(|e| e.tags.iter().any(|t| t.kind() == TagKind::e()))
                .collect()
        }
        _ => events, // No filtering needed for other tabs
    }
}

// Helper function to load events from DB only (instant, Phase 1)
async fn load_tab_events_db(pubkey: &str, tab: &ProfileTab, until: Option<u64>) -> std::result::Result<LoadOutcome, String> {
    let public_key = PublicKey::from_bech32(pubkey)
        .or_else(|_| PublicKey::from_hex(pubkey))
        .map_err(|e| format!("Invalid public key: {}", e))?;

    // For Likes tab, we need special handling
    if matches!(tab, ProfileTab::Likes) {
        return load_likes_db(public_key, until).await;
    }

    let filter = build_tab_filter(public_key, tab, until, 100);

    let events = nostr_client::fetch_profile_events_db(filter).await?;
    let mut processed = process_tab_events(events, tab);

    // Sort and deduplicate
    processed.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let mut seen_ids = std::collections::HashSet::new();
    processed.retain(|e| seen_ids.insert(e.id));

    let oldest_cursor = processed.last().map(|e| e.created_at.as_secs());

    log::info!("DB Phase: loaded {} {} events", processed.len(), format!("{:?}", tab));

    Ok(LoadOutcome {
        events: processed,
        oldest_cursor,
    })
}

// Helper function to load events from relays (Phase 2, background)
async fn load_tab_events_relays(pubkey: &str, tab: &ProfileTab, until: Option<u64>) -> std::result::Result<LoadOutcome, String> {
    let public_key = PublicKey::from_bech32(pubkey)
        .or_else(|_| PublicKey::from_hex(pubkey))
        .map_err(|e| format!("Invalid public key: {}", e))?;

    // For Likes tab, we need special handling
    if matches!(tab, ProfileTab::Likes) {
        return load_likes_relays(public_key, until).await;
    }

    let filter = build_tab_filter(public_key, tab, until, 100);

    let events = nostr_client::fetch_profile_events_from_relays(filter, Duration::from_secs(10)).await?;
    let mut processed = process_tab_events(events, tab);

    // Sort and deduplicate
    processed.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let mut seen_ids = std::collections::HashSet::new();
    processed.retain(|e| seen_ids.insert(e.id));

    let oldest_cursor = processed.last().map(|e| e.created_at.as_secs());

    log::info!("Relay Phase: fetched {} {} events", processed.len(), format!("{:?}", tab));

    Ok(LoadOutcome {
        events: processed,
        oldest_cursor,
    })
}

// Special handling for Likes tab - DB phase
async fn load_likes_db(public_key: PublicKey, until: Option<u64>) -> std::result::Result<LoadOutcome, String> {
    let mut filter = Filter::new()
        .author(public_key)
        .kind(Kind::Reaction)
        .limit(50);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    let reactions = nostr_client::fetch_profile_events_db(filter).await?;

    if reactions.is_empty() {
        return Ok(LoadOutcome { events: Vec::new(), oldest_cursor: None });
    }

    // Extract event IDs from reactions
    let mut liked_event_ids = Vec::new();
    let mut reaction_times: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for reaction in reactions.iter() {
        for tag in reaction.tags.iter() {
            if tag.kind() == TagKind::e() {
                if let Some(event_id_str) = tag.content() {
                    if let Ok(event_id) = nostr_sdk::EventId::from_hex(event_id_str) {
                        liked_event_ids.push(event_id);
                        reaction_times.insert(event_id_str.to_string(), reaction.created_at.as_secs());
                    }
                }
            }
        }
    }

    if liked_event_ids.is_empty() {
        return Ok(LoadOutcome { events: Vec::new(), oldest_cursor: None });
    }

    // Fetch liked events from DB
    let liked_filter = Filter::new().ids(liked_event_ids).limit(500);
    let liked_events = nostr_client::fetch_profile_events_db(liked_filter).await?;

    let mut event_vec: Vec<NostrEvent> = liked_events;
    event_vec.sort_by(|a, b| {
        let time_a = reaction_times.get(&a.id.to_hex()).copied().unwrap_or(0);
        let time_b = reaction_times.get(&b.id.to_hex()).copied().unwrap_or(0);
        time_b.cmp(&time_a)
    });

    let oldest_cursor = event_vec.last()
        .and_then(|e| reaction_times.get(&e.id.to_hex()).copied());

    Ok(LoadOutcome { events: event_vec, oldest_cursor })
}

// Special handling for Likes tab - Relay phase
async fn load_likes_relays(public_key: PublicKey, until: Option<u64>) -> std::result::Result<LoadOutcome, String> {
    let mut filter = Filter::new()
        .author(public_key)
        .kind(Kind::Reaction)
        .limit(50);

    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }

    let reactions = nostr_client::fetch_profile_events_from_relays(filter, Duration::from_secs(10)).await?;

    if reactions.is_empty() {
        return Ok(LoadOutcome { events: Vec::new(), oldest_cursor: None });
    }

    // Extract event IDs from reactions
    let mut liked_event_ids = Vec::new();
    let mut reaction_times: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for reaction in reactions.iter() {
        for tag in reaction.tags.iter() {
            if tag.kind() == TagKind::e() {
                if let Some(event_id_str) = tag.content() {
                    if let Ok(event_id) = nostr_sdk::EventId::from_hex(event_id_str) {
                        liked_event_ids.push(event_id);
                        reaction_times.insert(event_id_str.to_string(), reaction.created_at.as_secs());
                    }
                }
            }
        }
    }

    if liked_event_ids.is_empty() {
        return Ok(LoadOutcome { events: Vec::new(), oldest_cursor: None });
    }

    // Fetch liked events from relays
    let liked_filter = Filter::new().ids(liked_event_ids).limit(500);
    let liked_events = nostr_client::fetch_profile_events_from_relays(liked_filter, Duration::from_secs(10)).await?;

    let mut event_vec: Vec<NostrEvent> = liked_events;
    event_vec.sort_by(|a, b| {
        let time_a = reaction_times.get(&a.id.to_hex()).copied().unwrap_or(0);
        let time_b = reaction_times.get(&b.id.to_hex()).copied().unwrap_or(0);
        time_b.cmp(&time_a)
    });

    let oldest_cursor = event_vec.last()
        .and_then(|e| reaction_times.get(&e.id.to_hex()).copied());

    Ok(LoadOutcome { events: event_vec, oldest_cursor })
}

// Helper function to load events based on tab type (legacy - for pagination/load_more)
// Fetches enough events to return approximately 50 items for the specific tab
async fn load_tab_events(pubkey: &str, tab: &ProfileTab, until: Option<u64>) -> std::result::Result<LoadOutcome, String> {
    // Parse the public key
    let public_key = PublicKey::from_bech32(pubkey)
        .or_else(|_| PublicKey::from_hex(pubkey))
        .map_err(|e| format!("Invalid public key: {}", e))?;

    const TARGET_COUNT: usize = 50;
    const MAX_FETCH_LIMIT: usize = 500; // Safety limit to prevent infinite fetching

    match tab {
        ProfileTab::Posts => {
            // Fetch kind 1 events until we have 50 posts (without e-tags)
            let mut all_posts = Vec::new();
            let mut current_until = until;
            let mut total_fetched = 0;
            let mut hit_end = false;

            while all_posts.len() < TARGET_COUNT && total_fetched < MAX_FETCH_LIMIT {
                let mut filter = Filter::new()
                    .author(public_key.clone())
                    .kind(Kind::TextNote)
                    .limit(100); // Fetch more at once to reduce round trips

                if let Some(until_ts) = current_until {
                    filter = filter.until(Timestamp::from(until_ts));
                }

                // Use relay fetch for pagination (DB already shown)
                let events = nostr_client::fetch_profile_events_from_relays(filter, Duration::from_secs(10)).await
                    .map_err(|e| format!("Failed to fetch events: {}", e))?;

                let events_len = events.len();
                if events_len == 0 {
                    hit_end = true;
                    break; // No more events available
                }

                total_fetched += events_len;

                // Get the oldest event timestamp BEFORE filtering
                let oldest_event_ts = events.last().map(|e| e.created_at.as_secs());

                // Filter for posts only (no e-tags)
                let posts: Vec<NostrEvent> = events.into_iter()
                    .filter(|e| !e.tags.iter().any(|t| t.kind() == TagKind::e()))
                    .collect();

                all_posts.extend(posts);

                // Update until timestamp to the oldest event we saw (not just the oldest post)
                // This ensures we continue pagination even if we filtered out all results
                if let Some(ts) = oldest_event_ts {
                    current_until = Some(ts - 1); // Subtract 1 to avoid fetching the same event
                }

                // Only mark as end if we got fewer events than requested
                if events_len < 100 {
                    hit_end = true;
                    break;
                }
            }

            all_posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            // Deduplicate by event ID (in case of overlap between iterations)
            let mut seen_ids = std::collections::HashSet::new();
            all_posts.retain(|e| seen_ids.insert(e.id));

            log::info!("Loaded {} posts (fetched {} total events, hit_end={})", all_posts.len(), total_fetched, hit_end);

            let oldest_cursor = all_posts.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: all_posts,
                oldest_cursor,
            })
        }
        ProfileTab::Replies => {
            // Fetch kind 1 events until we have 50 replies (with e-tags)
            let mut all_replies = Vec::new();
            let mut current_until = until;
            let mut total_fetched = 0;
            let mut hit_end = false;

            while all_replies.len() < TARGET_COUNT && total_fetched < MAX_FETCH_LIMIT {
                let mut filter = Filter::new()
                    .author(public_key.clone())
                    .kind(Kind::TextNote)
                    .limit(100);

                if let Some(until_ts) = current_until {
                    filter = filter.until(Timestamp::from(until_ts));
                }

                // Use relay fetch for pagination (DB already shown in phase 1)
                let events = nostr_client::fetch_profile_events_from_relays(filter, Duration::from_secs(10)).await
                    .map_err(|e| format!("Failed to fetch events: {}", e))?;

                let events_len = events.len();
                if events_len == 0 {
                    hit_end = true;
                    break;
                }

                total_fetched += events_len;

                // Get the oldest event timestamp BEFORE filtering
                let oldest_event_ts = events.last().map(|e| e.created_at.as_secs());

                // Filter for replies only (with e-tags)
                let replies: Vec<NostrEvent> = events.into_iter()
                    .filter(|e| e.tags.iter().any(|t| t.kind() == TagKind::e()))
                    .collect();

                all_replies.extend(replies);

                // Update until timestamp to the oldest event we saw
                if let Some(ts) = oldest_event_ts {
                    current_until = Some(ts - 1);
                }

                if events_len < 100 {
                    hit_end = true;
                    break;
                }
            }

            all_replies.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            // Deduplicate by event ID
            let mut seen_ids = std::collections::HashSet::new();
            all_replies.retain(|e| seen_ids.insert(e.id));

            log::info!("Loaded {} replies (fetched {} total events, hit_end={})", all_replies.len(), total_fetched, hit_end);

            let oldest_cursor = all_replies.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: all_replies,
                oldest_cursor,
            })
        }
        ProfileTab::Articles => {
            // Kind 30023 (long-form content) - direct query
            let mut filter = Filter::new()
                .author(public_key)
                .kind(Kind::LongFormTextNote)
                .limit(TARGET_COUNT);

            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }

            // Use relay fetch for pagination
            let events = nostr_client::fetch_profile_events_from_relays(filter, Duration::from_secs(10)).await
                .map_err(|e| format!("Failed to fetch events: {}", e))?;

            let mut event_vec: Vec<NostrEvent> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            log::info!("Loaded {} articles", event_vec.len());

            let oldest_cursor = event_vec.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: event_vec,
                oldest_cursor,
            })
        }
        ProfileTab::Media(MediaSubTab::Photos) => {
            // Kind 20 (Picture Events - NIP-68) - direct query
            let mut filter = Filter::new()
                .author(public_key)
                .kind(Kind::Custom(20))
                .limit(TARGET_COUNT);

            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }

            // Use relay fetch for pagination
            let events = nostr_client::fetch_profile_events_from_relays(filter, Duration::from_secs(10)).await
                .map_err(|e| format!("Failed to fetch events: {}", e))?;

            let mut event_vec: Vec<NostrEvent> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            log::info!("Loaded {} photos", event_vec.len());

            let oldest_cursor = event_vec.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: event_vec,
                oldest_cursor,
            })
        }
        ProfileTab::Media(MediaSubTab::Videos) => {
            // Kind 21 (Landscape Video Events - NIP-71)
            let mut filter = Filter::new()
                .author(public_key)
                .kind(Kind::Custom(21))
                .limit(TARGET_COUNT);

            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }

            // Use relay fetch for pagination
            let events = nostr_client::fetch_profile_events_from_relays(filter, Duration::from_secs(10)).await
                .map_err(|e| format!("Failed to fetch events: {}", e))?;

            let mut event_vec: Vec<NostrEvent> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            log::info!("Loaded {} videos", event_vec.len());

            let oldest_cursor = event_vec.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: event_vec,
                oldest_cursor,
            })
        }
        ProfileTab::Media(MediaSubTab::Verts) => {
            // Kind 22 (Vertical/Short Video Events - NIP-71)
            let mut filter = Filter::new()
                .author(public_key)
                .kind(Kind::Custom(22))
                .limit(TARGET_COUNT);

            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }

            // Use relay fetch for pagination
            let events = nostr_client::fetch_profile_events_from_relays(filter, Duration::from_secs(10)).await
                .map_err(|e| format!("Failed to fetch events: {}", e))?;

            let mut event_vec: Vec<NostrEvent> = events.into_iter().collect();
            event_vec.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            log::info!("Loaded {} verts", event_vec.len());

            let oldest_cursor = event_vec.last().map(|e| e.created_at.as_secs());
            Ok(LoadOutcome {
                events: event_vec,
                oldest_cursor,
            })
        }
        ProfileTab::Likes => {
            // Fetch Kind 7 (reactions) to get what was liked
            let mut filter = Filter::new()
                .author(public_key)
                .kind(Kind::Reaction)
                .limit(TARGET_COUNT);

            if let Some(until_ts) = until {
                filter = filter.until(Timestamp::from(until_ts));
            }

            // Use relay fetch for pagination
            let reactions = nostr_client::fetch_profile_events_from_relays(filter, Duration::from_secs(10)).await
                .map_err(|e| format!("Failed to fetch reactions: {}", e))?;

            if reactions.is_empty() {
                return Ok(LoadOutcome {
                    events: Vec::new(),
                    oldest_cursor: None,
                });
            }

            // Extract event IDs from reactions' e tags
            let mut liked_event_ids = Vec::new();
            for reaction in reactions.iter() {
                for tag in reaction.tags.iter() {
                    if tag.kind() == TagKind::e() {
                        if let Some(event_id_str) = tag.content() {
                            if let Ok(event_id) = nostr_sdk::EventId::from_hex(event_id_str) {
                                liked_event_ids.push(event_id);
                            }
                        }
                    }
                }
            }

            if liked_event_ids.is_empty() {
                log::info!("No event IDs found in reactions");
                return Ok(LoadOutcome {
                    events: Vec::new(),
                    oldest_cursor: None,
                });
            }

            // Fetch the actual liked events
            let liked_filter = Filter::new()
                .ids(liked_event_ids)
                .limit(500);

            // Use relay fetch for pagination
            let liked_events = nostr_client::fetch_profile_events_from_relays(liked_filter, Duration::from_secs(10)).await
                .map_err(|e| format!("Failed to fetch liked events: {}", e))?;

            // Sort by the reaction timestamp (when the user liked it), not the original event timestamp
            // Create a map of event_id -> reaction_timestamp for sorting
            let mut reaction_times: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
            for reaction in reactions.iter() {
                for tag in reaction.tags.iter() {
                    if tag.kind() == TagKind::e() {
                        if let Some(event_id_str) = tag.content() {
                            reaction_times.insert(event_id_str.to_string(), reaction.created_at.as_secs());
                        }
                    }
                }
            }

            let mut event_vec: Vec<NostrEvent> = liked_events.into_iter().collect();
            // Sort by when they were liked (reaction timestamp), most recent first
            event_vec.sort_by(|a, b| {
                let time_a = reaction_times.get(&a.id.to_hex()).copied().unwrap_or(0);
                let time_b = reaction_times.get(&b.id.to_hex()).copied().unwrap_or(0);
                time_b.cmp(&time_a)
            });

            log::info!("Loaded {} liked events", event_vec.len());

            // Get the oldest reaction timestamp (not event.created_at) for pagination
            let oldest_cursor = event_vec.last()
                .and_then(|e| reaction_times.get(&e.id.to_hex()).copied());

            Ok(LoadOutcome {
                events: event_vec,
                oldest_cursor,
            })
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
        ProfileTab::Media(MediaSubTab::Photos) => "No photos yet",
        ProfileTab::Media(MediaSubTab::Videos) => "No videos yet",
        ProfileTab::Media(MediaSubTab::Verts) => "No verts yet",
        ProfileTab::Likes => "No likes yet",
    }
}

fn get_empty_state_icon(tab: &ProfileTab) -> &'static str {
    match tab {
        ProfileTab::Posts => "📝",
        ProfileTab::Replies => "💬",
        ProfileTab::Articles => "📄",
        ProfileTab::Media(MediaSubTab::Photos) => "🖼️",
        ProfileTab::Media(MediaSubTab::Videos) => "🎬",
        ProfileTab::Media(MediaSubTab::Verts) => "📱",
        ProfileTab::Likes => "❤️",
    }
}

/// Batch prefetch author metadata for all events
async fn prefetch_author_metadata(events: &[NostrEvent]) {
    use crate::utils::profile_prefetch;

    // Use optimized prefetch utility - no string conversions, direct database queries
    profile_prefetch::prefetch_event_authors(events).await;
}
