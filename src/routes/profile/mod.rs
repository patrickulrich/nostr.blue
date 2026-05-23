mod loader;
mod types;

pub use types::{MediaSubTab, ProfileTab, ZapSubTab};

use crate::components::dialog::{DialogDescription, DialogRoot, DialogTitle};
use crate::components::icons::{InfoIcon, ListIcon, MailIcon};
use crate::components::rich_content::mentions::{MentionRenderer, TextLinkMention};
use crate::components::{
    AddToPeopleListModal, ArticleCard, ArticleCardSkeleton, ClientInitializing, ExternalIdentitiesSection, Nip05Badge, NoteCard,
    PhotoCard, PinnedNotesCarousel, ProfileBadgesSection, ProfileEditorModal, VideoCard,
};
use crate::hooks::{use_infinite_scroll, use_mute_block_cache};
use crate::services::nip05;
use crate::services::profile_stats;
use crate::stores::{auth_store, dms, nostr_client, pinned_notes, profiles};
use crate::utils::article_meta::get_published_at;
use crate::utils::content_parser::{parse_content, ContentToken};
use crate::utils::repost::{expand_events_for_prefetch, extract_reposted_event};
use dioxus::prelude::*;
use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use std::time::Duration;

use types::{TabData, default_tab_data_map, dedupe_articles_by_address, get_display_name, get_username, get_avatar_initial, strip_https, get_empty_state_message, get_empty_state_icon, format_timestamp};
use loader::{load_tab_events_db, load_tab_events, prefetch_author_metadata, build_tab_filter, process_tab_events, load_likes_relays};

fn render_bio_content(about: &str) -> Element {
    let tokens = parse_content(about, &[]);
    let mut elements: Vec<Element> = Vec::new();
    for (idx, token) in tokens.into_iter().enumerate() {
        let el = match token {
            ContentToken::Text(text) => {
                rsx! { span { key: "{idx}", "{text}" } }
            }
            ContentToken::Link(url) => {
                let is_safe = url.starts_with("http://")
                    || url.starts_with("https://")
                    || url.starts_with("nostr:");
                if is_safe {
                    rsx! {
                        a {
                            key: "{idx}",
                            href: "{url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "text-foreground hover:text-muted-foreground underline break-all",
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            "{url}"
                        }
                    }
                } else {
                    rsx! { span { key: "{idx}", "{url}" } }
                }
            }
            ContentToken::Mention(mention) => {
                rsx! {
                    span { key: "{idx}",
                        MentionRenderer { mention: mention.clone() }
                    }
                }
            }
            ContentToken::EventMention(mention) => {
                rsx! {
                    span { key: "{idx}",
                        TextLinkMention { mention: mention.clone() }
                    }
                }
            }
            ContentToken::Hashtag(tag) => {
                rsx! {
                    Link {
                        key: "{idx}",
                        to: crate::routes::Route::Hashtag { tag: tag.clone() },
                        class: "text-foreground hover:text-muted-foreground font-medium hover:underline",
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        "#{tag}"
                    }
                }
            }
            _ => {
                rsx! { span { key: "{idx}" } }
            }
        };
        elements.push(el);
    }
    rsx! { {elements.into_iter()} }
}

#[component]
pub fn Profile(pubkey: String) -> Element {
    let mut profile_data = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut metadata_error = use_signal(|| None::<String>);
    let mut retry_count = use_signal(|| 0u32);
    let mut active_tab = use_signal(|| ProfileTab::Posts);
    let mut tab_data = use_signal(default_tab_data_map);
    let mut loading_events = use_signal(|| false);
    let mut current_tab_has_more = use_signal(|| true);
    let mut is_following = use_signal(|| false);
    let mut follow_loading = use_signal(|| false);
    let mut follows_you = use_signal(|| false);
    let mut following_count = use_signal(|| 0);
    let mut followers_count = use_signal(|| 0);
    let mut post_count = use_signal(|| 0);
    let mut show_profile_modal = use_signal(|| false);
    let mut show_dm_dialog = use_signal(|| false);
    let mut dm_message = use_signal(String::new);
    let mut dm_sending = use_signal(|| false);
    let mut dm_error = use_signal(|| None::<String>);
    let mut show_info_dialog = use_signal(|| false);
    let mut show_add_to_list_modal = use_signal(|| false);
    let mut pinned_events = use_signal(Vec::<NostrEvent>::new);
    let mut pinned_loading = use_signal(|| true);
    let mut user_write_relays = use_signal(Vec::<String>::new);
    let mut request_id = use_signal(|| 0u32);
    let mut current_pubkey = use_signal(|| pubkey.clone());
    let (cached_muted_posts, cached_blocked_users) = use_mute_block_cache();
    let pubkey_for_button = pubkey.clone();
    let pubkey_for_display = pubkey.clone();
    let pubkey_for_dm = pubkey.clone();
    let pubkey_for_info = pubkey.clone();
    let pubkey_for_pinned = pubkey.clone();
    let pubkey_for_list = pubkey.clone();
    let parsed_pubkey = crate::utils::nip19_urls::parse_profile_id(&pubkey);
    let auth = auth_store::AUTH_STATE.read();
    let is_own_profile = auth
        .pubkey
        .as_ref()
        .and_then(|pk| PublicKey::parse(pk).ok())
        .and_then(|user_pk| parsed_pubkey.map(|profile_pk| user_pk == profile_pk))
        .unwrap_or(false);
    use_effect(use_reactive(&pubkey, move |_new_pubkey| {
        let current_id = request_id.peek().wrapping_add(1);
        request_id.set(current_id);
        current_pubkey.set(_new_pubkey.clone());
        profile_data.set(None);
        loading.set(true);
        error.set(None);
        metadata_error.set(None);
        retry_count.set(0);
        active_tab.set(ProfileTab::Posts);
        tab_data.set(default_tab_data_map());
        loading_events.set(false);
        current_tab_has_more.set(true);
        is_following.set(false);
        follows_you.set(false);
        following_count.set(0);
        followers_count.set(0);
        post_count.set(0);
        pinned_events.set(Vec::new());
        pinned_loading.set(true);
        user_write_relays.set(Vec::new());
    }));
    use_effect(use_reactive(
        (
            &pubkey_for_pinned,
            &*nostr_client::CLIENT_INITIALIZED.read(),
        ),
        move |(pubkey_str, client_initialized)| {
            if !client_initialized {
                return;
            }
            pinned_loading.set(true);
            let current_id = *request_id.peek();
            let rid = request_id;
            spawn(async move {
                match pinned_notes::fetch_pinned_notes_for_user(&pubkey_str).await {
                    Ok((pin_ids, events)) => {
                        if *rid.peek() != current_id {
                            return;
                        }
                        let mut sorted_events = Vec::new();
                        for pin_id in pin_ids {
                            if let Some(event) = events.iter().find(|e| e.id.to_hex() == pin_id) {
                                sorted_events.push(event.clone());
                            }
                        }
                        pinned_events.set(sorted_events);
                    }
                    Err(e) => {
                        if *rid.peek() != current_id {
                            return;
                        }
                        log::warn!("Failed to fetch pinned notes: {}", e);
                        pinned_events.set(Vec::new());
                    }
                }
                if *rid.peek() == current_id {
                    pinned_loading.set(false);
                }
            });
        },
    ));
    use_effect(use_reactive(
        (
            &pubkey,
            &*nostr_client::CLIENT_INITIALIZED.read(),
            &*retry_count.read(),
        ),
        move |(pubkey_str, client_initialized, _retry)| {
            if !client_initialized {
                return;
            }
            let current_id = *request_id.peek();
            let rid = request_id;
            spawn(async move {
                if *rid.peek() != current_id {
                    return;
                }
                loading.set(true);
                metadata_error.set(None);
                let public_key = match crate::utils::nip19_urls::parse_profile_id(&pubkey_str) {
                    Some(pk) => pk,
                    None => {
                        error.set(Some("Invalid public key".to_string()));
                        loading.set(false);
                        return;
                    }
                };
                let client = match nostr_client::get_client() {
                    Some(c) => c,
                    None => {
                        error.set(Some("Client not initialized".to_string()));
                        loading.set(false);
                        return;
                    }
                };
                if *rid.peek() != current_id {
                    return;
                }
                let hex_pubkey = public_key.to_hex();

                if let Some(metadata) = profiles::get_profile(&hex_pubkey) {
                    if *rid.peek() != current_id {
                        return;
                    }
                    log::debug!("Loaded profile metadata from LRU cache");
                    profile_data.set(Some(metadata));
                    loading.set(false);
                    let hex_bg = hex_pubkey.clone();
                    let rid_bg = rid;
                    let cid_bg = current_id;
                    spawn(async move {
                        let write_relays = crate::stores::relay::coverage::resolve_user_relays(
                            &hex_bg,
                            crate::stores::relay::coverage::RelayPurpose::Write,
                        )
                        .await;
                        if *rid_bg.peek() == cid_bg && !write_relays.is_empty() {
                            user_write_relays.set(write_relays);
                        }
                    });
                    return;
                }

                if let Ok(Some(metadata)) = client.database().metadata(public_key).await {
                    if *rid.peek() != current_id {
                        return;
                    }
                    log::debug!("Loaded profile metadata from database cache");
                    profile_data.set(Some(metadata));
                    loading.set(false);
                    let hex_bg = hex_pubkey.clone();
                    let rid_bg = rid;
                    let cid_bg = current_id;
                    spawn(async move {
                        let write_relays = crate::stores::relay::coverage::resolve_user_relays(
                            &hex_bg,
                            crate::stores::relay::coverage::RelayPurpose::Write,
                        )
                        .await;
                        if *rid_bg.peek() == cid_bg && !write_relays.is_empty() {
                            user_write_relays.set(write_relays);
                        }
                    });
                    return;
                }

                if *rid.peek() != current_id {
                    return;
                }

                let write_relays = crate::stores::relay::coverage::resolve_user_relays(
                    &hex_pubkey,
                    crate::stores::relay::coverage::RelayPurpose::Write,
                )
                .await;
                if *rid.peek() != current_id {
                    return;
                }
                if !write_relays.is_empty() {
                    user_write_relays.set(write_relays);
                }

                match nostr_client::fetch_metadata_targeted(&hex_pubkey, Duration::from_secs(5))
                    .await
                {
                    Ok(Some(metadata)) => {
                        if *rid.peek() != current_id {
                            return;
                        }
                        log::debug!("Fetched profile metadata from relays");
                        profile_data.set(Some(metadata));
                    }
                    Ok(None) => {
                        if *rid.peek() != current_id {
                            return;
                        }
                        log::debug!("No metadata found, using empty profile");
                        metadata_error.set(Some("No profile data found".to_string()));
                        profile_data.set(Some(nostr_sdk::Metadata::new()));
                    }
                    Err(e) => {
                        if *rid.peek() != current_id {
                            return;
                        }
                        log::error!("Failed to fetch profile metadata: {}", e);
                        metadata_error.set(Some(format!("Failed to load: {}", e)));
                        profile_data.set(Some(nostr_sdk::Metadata::new()));
                    }
                }
                if *rid.peek() == current_id {
                    loading.set(false);
                }
            });
        },
    ));
    use_effect(use_reactive(
        (
            &pubkey,
            &*active_tab.read(),
            &*nostr_client::CLIENT_INITIALIZED.read(),
        ),
        move |(pubkey_str, tab, client_initialized)| {
            if !client_initialized {
                return;
            }
            let already_loaded = tab_data.read().get(&tab).map(|d| d.loaded).unwrap_or(false);
            if already_loaded {
                let has_more = tab_data
                    .read()
                    .get(&tab)
                    .map(|d| d.has_more)
                    .unwrap_or(true);
                current_tab_has_more.set(has_more);
                return;
            }
            loading_events.set(true);
            let pubkey_for_relay = pubkey_str.clone();
            let tab_for_relay = tab.clone();
            let current_id = *request_id.peek();
            let rid = request_id;
            spawn(async move {
                if *rid.peek() != current_id {
                    return;
                }
                match load_tab_events_db(&pubkey_str, &tab, None).await {
                    Ok(db_outcome) => {
                        if *rid.peek() != current_id {
                            return;
                        }
                        let oldest_ts = db_outcome.oldest_cursor.map(|ts| ts.saturating_sub(1));
                        let has_more = true;
                        if matches!(tab, ProfileTab::Posts) {
                            post_count.set(db_outcome.events.len());
                        }
                        if !db_outcome.events.is_empty() {
                            let mut data_map = tab_data.read().clone();
                            data_map.insert(
                                tab.clone(),
                                TabData {
                                    events: db_outcome.events.clone(),
                                    oldest_timestamp: oldest_ts,
                                    has_more,
                                    loaded: true,
                                },
                            );
                            tab_data.set(data_map);
                            current_tab_has_more.set(has_more);
                            loading_events.set(false);
                            log::info!(
                                "Phase 1 complete: showing {} events from DB instantly",
                                db_outcome.events.len()
                            );
                            let db_events_for_metadata = expand_events_for_prefetch(&db_outcome.events);
                            spawn(async move {
                                crate::utils::profile_prefetch::prefetch_event_authors_with_relays(&db_events_for_metadata).await;
                            });
                        } else {
                            log::info!("Phase 1: DB returned 0 events, waiting for relay phase");
                        }
                    }
                    Err(e) => {
                        log::warn!("DB phase failed: {}, will try relays", e);
                    }
                }
                if *rid.peek() != current_id {
                    return;
                }
                spawn(async move {
                    if *rid.peek() != current_id {
                        loading_events.set(false);
                        return;
                    }
                    let public_key_for_relay = match crate::utils::nip19_urls::parse_profile_id(&pubkey_for_relay) {
                        Some(pk) => pk,
                        None => {
                            loading_events.set(false);
                            return;
                        }
                    };
                    let known_relays = user_write_relays.read().clone();
                    let client = match nostr_client::get_client() {
                        Some(c) => c,
                        None => {
                            loading_events.set(false);
                            return;
                        }
                    };
                    let filter = build_tab_filter(public_key_for_relay, &tab_for_relay, None, 100);

                    if matches!(tab_for_relay, ProfileTab::Likes) {
                        match load_likes_relays(public_key_for_relay, None).await {
                            Ok(relay_outcome) => {
                                if *rid.peek() != current_id {
                                    loading_events.set(false);
                                    return;
                                }
                                let mut data_map = tab_data.read().clone();
                                let existing_data = data_map.get(&tab_for_relay).cloned().unwrap_or_default();
                                let existing_ids: std::collections::HashSet<_> =
                                    existing_data.events.iter().map(|e| e.id).collect();
                                let new_events: Vec<_> = relay_outcome
                                    .events
                                    .into_iter()
                                    .filter(|e| !existing_ids.contains(&e.id))
                                    .collect();
                                if !new_events.is_empty() {
                                    let mut merged = existing_data.events;
                                    merged.extend(new_events.clone());
                                    merged.sort_by_key(|e| std::cmp::Reverse(e.created_at));
                                    data_map.insert(
                                        tab_for_relay.clone(),
                                        TabData {
                                            events: merged,
                                            oldest_timestamp: None,
                                            has_more: false,
                                            loaded: true,
                                        },
                                    );
                                    tab_data.set(data_map);
                                }
                            }
                            Err(e) => log::warn!("Likes relay phase failed: {}", e),
                        }
                        if *rid.peek() == current_id {
                            loading_events.set(false);
                        }
                        return;
                    }
                    if matches!(tab_for_relay, ProfileTab::Zaps(_)) {
                        match load_tab_events(&pubkey_for_relay, &tab_for_relay, None).await {
                            Ok(outcome) => {
                                if *rid.peek() != current_id {
                                    loading_events.set(false);
                                    return;
                                }
                                let mut data_map = tab_data.read().clone();
                                let existing = data_map.get(&tab_for_relay).cloned().unwrap_or_default();
                                let existing_ids: std::collections::HashSet<_> =
                                    existing.events.iter().map(|e| e.id).collect();
                                let new_events: Vec<_> = outcome
                                    .events
                                    .into_iter()
                                    .filter(|e| !existing_ids.contains(&e.id))
                                    .collect();
                                if !new_events.is_empty() {
                                    let mut merged = existing.events;
                                    merged.extend(new_events);
                                    merged.sort_by_key(|e| std::cmp::Reverse(e.created_at));
                                    data_map.insert(
                                        tab_for_relay.clone(),
                                        TabData {
                                            events: merged,
                                            oldest_timestamp: outcome.oldest_cursor,
                                            has_more: true,
                                            loaded: true,
                                        },
                                    );
                                    tab_data.set(data_map);
                                    current_tab_has_more.set(true);
                                } else {
                                    data_map.insert(
                                        tab_for_relay.clone(),
                                        TabData {
                                            events: existing.events,
                                            oldest_timestamp: existing.oldest_timestamp,
                                            has_more: false,
                                            loaded: true,
                                        },
                                    );
                                    tab_data.set(data_map);
                                    current_tab_has_more.set(false);
                                }
                            }
                            Err(e) => log::warn!("Zaps relay phase failed: {}", e),
                        }
                        if *rid.peek() == current_id {
                            loading_events.set(false);
                        }
                        return;
                    }

                    let targeted_future = nostr_client::fetch_profile_events_from_relays_direct(
                        &client, filter.clone(), &known_relays, Duration::from_secs(5),
                    );
                    let safety_future = nostr_client::fetch_events_from_connected_relays(
                        filter, Duration::from_secs(5),
                    );

                    let (targeted_result, safety_result) = futures::join!(targeted_future, safety_future);

                    if *rid.peek() != current_id {
                        loading_events.set(false);
                        return;
                    }

                    let mut all_events = Vec::new();
                    let mut seen_ids = std::collections::HashSet::new();
                    for events in [&targeted_result, &safety_result]
                        .into_iter()
                        .filter_map(|r| r.as_ref().ok())
                    {
                        for event in events {
                            if seen_ids.insert(event.id) {
                                all_events.push(event.clone());
                            }
                        }
                    }

                    let mut processed = process_tab_events(all_events, &tab_for_relay);
                    if matches!(tab_for_relay, ProfileTab::Articles) {
                        processed.sort_by_key(|e| std::cmp::Reverse(get_published_at(e)));
                    } else {
                        processed.sort_by_key(|e| std::cmp::Reverse(e.created_at));
                    }
                    let mut seen_ids = std::collections::HashSet::new();
                    processed.retain(|e| seen_ids.insert(e.id));
                    let relay_count = processed.len();

                    let mut data_map = tab_data.read().clone();
                    let existing_data = data_map.get(&tab_for_relay).cloned().unwrap_or_default();
                    let existing_ids: std::collections::HashSet<_> =
                        existing_data.events.iter().map(|e| e.id).collect();
                    let new_events: Vec<_> = processed
                        .into_iter()
                        .filter(|e| !existing_ids.contains(&e.id))
                        .collect();
                    let has_more = relay_count >= 100;
                    if !new_events.is_empty() {
                        log::info!(
                            "Phase 2: found {} new events from relays (has_more: {})",
                            new_events.len(),
                            has_more
                        );
                        let mut merged = existing_data.events;
                        merged.extend(new_events.clone());
                        if matches!(tab_for_relay, ProfileTab::Articles) {
                            merged = dedupe_articles_by_address(merged);
                        }
                        if matches!(tab_for_relay, ProfileTab::Articles) {
                            merged.sort_by_key(|e| std::cmp::Reverse(get_published_at(e)));
                        } else {
                            merged.sort_by_key(|e| std::cmp::Reverse(e.created_at));
                        }
                        let oldest_ts = if matches!(tab_for_relay, ProfileTab::Articles) {
                            merged.last().map(|e| get_published_at(e).saturating_sub(1))
                        } else {
                            merged
                                .last()
                                .map(|e| e.created_at.as_secs().saturating_sub(1))
                        };
                        if *rid.peek() != current_id {
                            loading_events.set(false);
                            return;
                        }
                        data_map.insert(
                            tab_for_relay.clone(),
                            TabData {
                                events: merged.clone(),
                                oldest_timestamp: oldest_ts,
                                has_more,
                                loaded: true,
                            },
                        );
                        tab_data.set(data_map);
                        current_tab_has_more.set(has_more);
                        if matches!(tab_for_relay, ProfileTab::Posts) {
                            post_count.set(merged.len());
                        }
                        let events_for_prefetch = expand_events_for_prefetch(&new_events);
                        spawn(async move {
                            prefetch_author_metadata(&events_for_prefetch).await;
                        });
                    } else {
                        log::info!(
                            "Phase 2: no new events from relays (all already in DB, has_more: {})",
                            has_more
                        );
                        if *rid.peek() != current_id {
                            loading_events.set(false);
                            return;
                        }
                        let mut data_map = tab_data.read().clone();
                        data_map.insert(
                            tab_for_relay.clone(),
                            TabData {
                                events: existing_data.events,
                                oldest_timestamp: existing_data.oldest_timestamp,
                                has_more,
                                loaded: true,
                            },
                        );
                        tab_data.set(data_map);
                        current_tab_has_more.set(has_more);
                    }
                    if *rid.peek() == current_id {
                        loading_events.set(false);
                    }
                });
            });
        },
    ));
    use_effect(use_reactive(&pubkey, move |pubkey_str| {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let is_authenticated = auth_store::is_authenticated();
        let my_pubkey = auth_store::get_pubkey();
        let current_id = *request_id.peek();
        let rid = request_id;
        let hex_pubkey = match crate::utils::nip19_urls::parse_profile_id(&pubkey_str) {
            Some(pk) => pk.to_hex(),
            None => return,
        };
        let hex_for_following = hex_pubkey.clone();
        let hex_for_stats = hex_pubkey.clone();

        spawn(async move {
            if is_authenticated {
                if let Ok(following) = nostr_client::is_following(hex_for_following).await {
                    if *rid.peek() != current_id { return; }
                    is_following.set(following);
                }
            }
        });
        spawn(async move {
            if let Ok(contacts) = nostr_client::fetch_contacts(hex_pubkey).await {
                if *rid.peek() != current_id { return; }
                following_count.set(contacts.len());
                if is_authenticated {
                    if let Some(ref my_pk) = my_pubkey {
                        if let Ok(pk) = PublicKey::parse(my_pk) {
                            follows_you.set(contacts.contains(&pk.to_hex()));
                        }
                    }
                }
            }
        });
        spawn(async move {
            if let Ok(stats) = profile_stats::fetch_profile_stats(&hex_for_stats).await {
                if *rid.peek() != current_id { return; }
                if let Some(count) = stats.followers_pubkey_count {
                    followers_count.set(count as usize);
                }
            }
        });
    }));
    let load_more = move || {
        let tab = active_tab.read().clone();
        log::info!("load_more called for tab {:?}", tab);
        let (has_more, until) = {
            let data = tab_data.read();
            let tab_state = data.get(&tab).unwrap();
            (tab_state.has_more, tab_state.oldest_timestamp)
        };
        log::info!(
            "load_more: has_more={}, loading={}, until={:?}",
            has_more,
            *loading_events.read(),
            until
        );
        if *loading_events.read() || !has_more {
            log::info!("load_more: bailing early");
            return;
        }
        let pubkey_str = current_pubkey.read().clone();
        let mut post_count_clone = post_count;
        let current_id = *request_id.peek();
        let rid = request_id;
        loading_events.set(true);
        spawn(async move {
            match load_tab_events(&pubkey_str, &tab, until).await {
                Ok(outcome) => {
                    if *rid.peek() != current_id {
                        return;
                    }
                    let oldest_ts = outcome.oldest_cursor.map(|ts| ts.saturating_sub(1));
                    let has_more_val = !outcome.events.is_empty();
                    log::info!(
                        "load_more: got {} new events, has_more={}",
                        outcome.events.len(),
                        has_more_val
                    );
                    let mut data_map = tab_data.read().clone();
                    if let Some(data) = data_map.get_mut(&tab) {
                        data.events.extend(outcome.events.clone());
                        if matches!(tab, ProfileTab::Articles) {
                            data.events = dedupe_articles_by_address(data.events.clone());
                            data.events
                                .sort_by_key(|e| std::cmp::Reverse(get_published_at(e)));
                        }
                        data.oldest_timestamp = oldest_ts;
                        data.has_more = has_more_val;
                        if tab == ProfileTab::Posts {
                            post_count_clone.set(data.events.len());
                        }
                    }
                    tab_data.set(data_map);
                    current_tab_has_more.set(has_more_val);
                    let events_for_prefetch = expand_events_for_prefetch(&outcome.events);
                    spawn(async move {
                        prefetch_author_metadata(&events_for_prefetch).await;
                    });
                }
                Err(e) => {
                    if *rid.peek() != current_id {
                        return;
                    }
                    log::error!("Failed to load more events: {}", e);
                    current_tab_has_more.set(false);
                    let mut data_map = tab_data.read().clone();
                    if let Some(data) = data_map.get_mut(&tab) {
                        data.has_more = false;
                    }
                    tab_data.set(data_map);
                }
            }
            if *rid.peek() == current_id {
                loading_events.set(false);
            }
        });
    };
    let sentinel_id = use_infinite_scroll(load_more, current_tab_has_more, loading_events);

    {
        let pubkey_for_nip05 = pubkey.clone();
        use_effect(move || {
            if let Some(metadata) = profile_data.read().as_ref() {
                if let Some(nip05_str) = &metadata.nip05 {
                    if !nip05_str.is_empty() {
                        let pk_hex = crate::utils::nip19_urls::parse_profile_id(&pubkey_for_nip05)
                            .map(|p| p.to_hex())
                            .unwrap_or_else(|| pubkey_for_nip05.clone());
                        nip05::verify_nip05(&pk_hex, nip05_str);
                    }
                }
            }
        });
    }
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center gap-4",
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
                            h2 { class: "text-xl font-bold",
                                "{get_display_name(metadata, &pubkey_for_display)}"
                            }
                            if matches!(*active_tab.read(), ProfileTab::Posts) && *post_count.read() > 0 {
                                p { class: "text-sm text-muted-foreground",
                                    "{post_count.read()} posts"
                                }
                            }
                        } else {
                            h2 { class: "text-xl font-bold",
                                {
                                    if let Some(pk) = crate::utils::nip19_urls::parse_profile_id(&pubkey_for_display) {
                                        let npub = pk.to_bech32().unwrap_or_else(|_| pubkey_for_display.clone());
                                        if npub.len() > 16 {
                                            format!("{}...{}", &npub[..12], &npub[npub.len() - 4..])
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
            div { class: "relative",
                if let Some(metadata) = profile_data.read().as_ref() {
                    if let Some(banner) = &metadata.banner {
                        img {
                            src: "{banner}",
                            class: "w-full h-48 object-cover",
                            alt: "Profile banner",
                        }
                    } else {
                        div { class: "w-full h-48 bg-gradient-to-r from-blue-500 via-purple-500 to-pink-500" }
                    }
                } else {
                    div { class: "w-full h-48 bg-gradient-to-r from-blue-500/20 via-purple-500/20 to-pink-500/20 animate-pulse" }
                }
                div { class: "absolute bottom-0 left-4 transform translate-y-1/2",
                    if let Some(metadata) = profile_data.read().as_ref() {
                        if let Some(picture) = &metadata.picture {
                            img {
                                class: "w-32 h-32 rounded-full border-4 border-background bg-background",
                                src: "{picture}",
                                alt: "Avatar",
                            }
                        } else {
                            div { class: "w-32 h-32 rounded-full border-4 border-background bg-blue-600 flex items-center justify-center text-white text-4xl font-bold",
                                "{get_avatar_initial(metadata)}"
                            }
                        }
                    } else {
                        div { class: "w-32 h-32 rounded-full border-4 border-background bg-muted animate-pulse" }
                    }
                }
            }
            div { class: "px-4 pb-4",
                div { class: "flex justify-end gap-2 pt-4 mb-16",
                    if metadata_error.read().is_some() {
                        {
                            let mut retry_count_sig = retry_count;
                            rsx! {
                                button {
                                    class: "p-2 border border-orange-300 rounded-full hover:bg-accent transition text-orange-500",
                                    onclick: move |_| {
                                        retry_count_sig += 1;
                                    },
                                    "aria-label": "Retry",
                                    title: "Retry loading profile",
                                    svg {
                                        class: "w-5 h-5",
                                        xmlns: "http://www.w3.org/2000/svg",
                                        view_box: "0 0 24 24",
                                        fill: "none",
                                        stroke: "currentColor",
                                        stroke_width: "2",
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        polyline { points: "23 4 23 10 17 10" }
                                        path { d: "20.49 15a9 9 0 1 1-2.12-9.36L23 10" }
                                    }
                                }
                            }
                        }
                    }
                    button {
                        class: "p-2 border border-border rounded-full hover:bg-accent transition",
                        onclick: move |_| show_info_dialog.set(true),
                        "aria-label": "Info",
                        title: "Info",
                        InfoIcon { class: "w-5 h-5".to_string(), filled: false }
                    }
                    if !is_own_profile && auth.is_authenticated {
                        button {
                            class: "p-2 border border-border rounded-full hover:bg-accent transition",
                            onclick: move |_| show_dm_dialog.set(true),
                            "aria-label": "Message",
                            title: "Message",
                            MailIcon { class: "w-5 h-5".to_string(), filled: false }
                        }
                    }
                    if !is_own_profile && auth.is_authenticated {
                        button {
                            class: "p-2 border border-border rounded-full hover:bg-accent transition",
                            onclick: move |_| show_add_to_list_modal.set(true),
                            "aria-label": "Add to List",
                            title: "Add to List",
                            ListIcon { class: "w-5 h-5".to_string(), filled: false }
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
                            class: if *is_following.read() { "px-6 py-2 border border-border rounded-full font-semibold hover:bg-accent transition" } else { "px-6 py-2 bg-foreground text-background rounded-full font-semibold hover:opacity-90 transition" },
                            disabled: *follow_loading.read(),
                            onclick: move |_| {
                                let pubkey_clone = pubkey_for_button.clone();
                                follow_loading.set(true);
                                spawn(async move {
                                    let hex_pubkey = match crate::utils::nip19_urls::parse_profile_id(&pubkey_clone) {
                                        Some(pk) => pk.to_hex(),
                                        None => { follow_loading.set(false); return; }
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
                if *follows_you.read() && !is_own_profile && auth.is_authenticated {
                    span { class: "inline-block px-2 py-1 bg-muted text-muted-foreground text-xs rounded mb-2",
                        "Follows you"
                    }
                }
                if let Some(metadata) = profile_data.read().as_ref() {
                    {
                        let is_bot_account = metadata
                            .custom
                            .get("bot")
                            .and_then(|v| {
                                if let Some(b) = v.as_bool() {
                                    Some(b)
                                } else if let Some(s) = v.as_str() {
                                    match s.to_lowercase().as_str() {
                                        "true" | "1" | "yes" => Some(true),
                                        _ => Some(false),
                                    }
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(false);
                        rsx! {
                            h1 { class: "text-2xl font-bold flex items-center gap-2",
                                "{get_display_name(metadata, &pubkey_for_display)}"
                                if is_bot_account {
                                    span {
                                        class: "inline-flex items-center gap-1 px-2 py-0.5 text-xs font-medium bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400 rounded-full",
                                        title: "This account is a bot",
                                        svg {
                                            class: "w-3 h-3",
                                            xmlns: "http://www.w3.org/2000/svg",
                                            fill: "none",
                                            view_box: "0 0 24 24",
                                            stroke: "currentColor",
                                            stroke_width: "2",
                                            path {
                                                stroke_linecap: "round",
                                                stroke_linejoin: "round",
                                                d: "M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z",
                                            }
                                        }
                                        "Bot"
                                    }
                                }
                            }
                        }
                    }
                    p { class: "text-muted-foreground flex items-center gap-1",
                        "@{get_username(metadata, &pubkey_for_display)}"
                        if let Some(nip05_str) = &metadata.nip05 {
                            if !nip05_str.is_empty() {
                                Nip05Badge {
                                    pubkey: crate::utils::nip19_urls::parse_profile_id(&pubkey_for_display)
                                        .map(|pk| pk.to_hex())
                                        .unwrap_or_else(|| pubkey_for_display.clone()),
                                    nip05: nip05_str.clone(),
                                }
                            }
                        }
                    }
                    if let Some(about) = &metadata.about {
                        if !about.is_empty() {
                            p { class: "whitespace-pre-wrap break-words mt-3",
                                {render_bio_content(about)}
                            }
                        }
                    }
                    div { class: "flex flex-wrap gap-4 mt-3 text-sm text-muted-foreground",
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
                        {
                            let birthday_str = metadata
                                .custom
                                .get("birthday")
                                .and_then(|v| {
                                    if v.is_object() {
                                        let months = [
                                            "Jan",
                                            "Feb",
                                            "Mar",
                                            "Apr",
                                            "May",
                                            "Jun",
                                            "Jul",
                                            "Aug",
                                            "Sep",
                                            "Oct",
                                            "Nov",
                                            "Dec",
                                        ];
                                        let month = v
                                            .get("month")
                                            .and_then(|m| m.as_u64())
                                            .map(|m| m as usize);
                                        let day = v.get("day").and_then(|d| d.as_u64());
                                        let year = v.get("year").and_then(|y| y.as_u64());
                                        match (month, day, year) {
                                            (Some(m), Some(d), Some(y)) if (1..=12).contains(&m) => {
                                                Some(format!("{} {}, {}", months[m - 1], d, y))
                                            }
                                            (Some(m), Some(d), None) if (1..=12).contains(&m) => {
                                                Some(format!("{} {}", months[m - 1], d))
                                            }
                                            (Some(m), None, None) if (1..=12).contains(&m) => {
                                                Some(months[m - 1].to_string())
                                            }
                                            _ => None,
                                        }
                                    } else {
                                        None
                                    }
                                });
                            if let Some(bday) = birthday_str {
                                rsx! {
                                    span { class: "flex items-center gap-1", "🎂 {bday}" }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                    }
                    div { class: "flex gap-4 mt-3",
                        div { class: "hover:underline cursor-pointer",
                            span { class: "font-bold", "{following_count.read()}" }
                            span { class: "text-muted-foreground ml-1", "Following" }
                        }
                        div { class: "hover:underline cursor-pointer",
                            span { class: "font-bold", "{followers_count.read()}" }
                            span { class: "text-muted-foreground ml-1", "Followers" }
                        }
                    }
                } else {
                    h1 { class: "text-2xl font-bold",
                        {
                            if let Some(pk) = crate::utils::nip19_urls::parse_profile_id(&pubkey_for_display) {
                                let npub = pk.to_bech32().unwrap_or_else(|_| pubkey_for_display.clone());
                                if npub.len() > 16 {
                                    format!("{}...{}", &npub[..12], &npub[npub.len() - 4..])
                                } else {
                                    npub
                                }
                            } else {
                                pubkey_for_display.clone()
                            }
                        }
                    }
                    p { class: "text-muted-foreground",
                        {
                            if let Some(pk) = crate::utils::nip19_urls::parse_profile_id(&pubkey_for_display) {
                                let npub = pk.to_bech32().unwrap_or_else(|_| pubkey_for_display.clone());
                                if npub.len() > 18 {
                                    format!("@{}...{}", &npub[..12], &npub[npub.len() - 6..])
                                } else {
                                    format!("@{}", npub)
                                }
                            } else {
                                format!("@{}", pubkey_for_display)
                            }
                        }
                    }
                    div { class: "flex gap-4 mt-3",
                        div { class: "hover:underline cursor-pointer",
                            span { class: "font-bold", "{following_count.read()}" }
                            span { class: "text-muted-foreground ml-1", "Following" }
                        }
                        div { class: "hover:underline cursor-pointer",
                            span { class: "font-bold", "{followers_count.read()}" }
                            span { class: "text-muted-foreground ml-1", "Followers" }
                        }
                    }
                }
            }
            ProfileBadgesSection { pubkey: pubkey.clone() }
            ExternalIdentitiesSection { pubkey: pubkey.clone() }
            div { class: "border-b border-border sticky top-[57px] bg-background z-10",
                div { class: "flex overflow-x-auto scrollbar-hide",
                    ProfileTabButton {
                        label: "Posts",
                        active: matches!(*active_tab.read(), ProfileTab::Posts),
                        onclick: move |_| active_tab.set(ProfileTab::Posts),
                    }
                    ProfileTabButton {
                        label: "Replies",
                        active: matches!(*active_tab.read(), ProfileTab::Replies),
                        onclick: move |_| active_tab.set(ProfileTab::Replies),
                    }
                    ProfileTabButton {
                        label: "Articles",
                        active: matches!(*active_tab.read(), ProfileTab::Articles),
                        onclick: move |_| active_tab.set(ProfileTab::Articles),
                    }
                    ProfileTabButton {
                        label: "Media",
                        active: matches!(*active_tab.read(), ProfileTab::Media(_)),
                        onclick: move |_| active_tab.set(ProfileTab::Media(MediaSubTab::Photos)),
                    }
                    ProfileTabButton {
                        label: "Likes",
                        active: matches!(*active_tab.read(), ProfileTab::Likes),
                        onclick: move |_| active_tab.set(ProfileTab::Likes),
                    }
                    ProfileTabButton {
                        label: "Zaps",
                        active: matches!(*active_tab.read(), ProfileTab::Zaps(_)),
                        onclick: move |_| active_tab.set(ProfileTab::Zaps(ZapSubTab::Received)),
                    }
                }
                if matches!(*active_tab.read(), ProfileTab::Media(_)) {
                    div { class: "flex gap-2 px-4 py-2 bg-accent/10",
                        button {
                            class: if matches!(*active_tab.read(), ProfileTab::Media(MediaSubTab::Photos)) { "px-4 py-2 rounded-full bg-primary text-primary-foreground font-medium" } else { "px-4 py-2 rounded-full hover:bg-accent font-medium" },
                            onclick: move |_| active_tab.set(ProfileTab::Media(MediaSubTab::Photos)),
                            "Photos"
                        }
                        button {
                            class: if matches!(*active_tab.read(), ProfileTab::Media(MediaSubTab::Videos)) { "px-4 py-2 rounded-full bg-primary text-primary-foreground font-medium" } else { "px-4 py-2 rounded-full hover:bg-accent font-medium" },
                            onclick: move |_| active_tab.set(ProfileTab::Media(MediaSubTab::Videos)),
                            "Videos"
                        }
                        button {
                            class: if matches!(*active_tab.read(), ProfileTab::Media(MediaSubTab::Verts)) { "px-4 py-2 rounded-full bg-primary text-primary-foreground font-medium" } else { "px-4 py-2 rounded-full hover:bg-accent font-medium" },
                            onclick: move |_| active_tab.set(ProfileTab::Media(MediaSubTab::Verts)),
                            "Verts"
                        }
                    }
                }
                if matches!(*active_tab.read(), ProfileTab::Zaps(_)) {
                    div { class: "flex gap-2 px-4 py-2 bg-accent/10",
                        button {
                            class: if matches!(*active_tab.read(), ProfileTab::Zaps(ZapSubTab::Sent)) { "px-4 py-2 rounded-full bg-primary text-primary-foreground font-medium" } else { "px-4 py-2 rounded-full hover:bg-accent font-medium" },
                            onclick: move |_| active_tab.set(ProfileTab::Zaps(ZapSubTab::Sent)),
                            "Sent"
                        }
                        button {
                            class: if matches!(*active_tab.read(), ProfileTab::Zaps(ZapSubTab::Received)) { "px-4 py-2 rounded-full bg-primary text-primary-foreground font-medium" } else { "px-4 py-2 rounded-full hover:bg-accent font-medium" },
                            onclick: move |_| active_tab.set(ProfileTab::Zaps(ZapSubTab::Received)),
                            "Received"
                        }
                    }
                }
            }
            div {
                {
                    let tab = active_tab.read().clone();
                    let current_events = tab_data
                        .read()
                        .get(&tab)
                        .map(|d| d.events.clone())
                        .unwrap_or_default();
                    let current_has_more = tab_data
                        .read()
                        .get(&tab)
                        .map(|d| d.has_more)
                        .unwrap_or(false);
                    let tab_loaded = tab_data
                        .read()
                        .get(&tab)
                        .map(|d| d.loaded)
                        .unwrap_or(false);
                    log::debug!(
                        "Rendering tab {:?}: {} events, has_more={}, sentinel_signal={}", tab,
                        current_events.len(), current_has_more, * current_tab_has_more.read()
                    );
                    rsx! {
                        if matches!(tab, ProfileTab::Posts) && !pinned_events.read().is_empty() {
                            div { class: "border-b border-border",
                                div { class: "py-3",
                                    h3 { class: "px-4 text-sm font-semibold text-muted-foreground mb-2", "Pinned" }
                                    PinnedNotesCarousel { events: pinned_events.read().clone() }
                                }
                            }
                        }
                        if !*nostr_client::CLIENT_INITIALIZED.read()
                            || (!tab_loaded && current_events.is_empty())
                            || (*loading_events.read() && current_events.is_empty())
                        {
                            match &tab {
                                ProfileTab::Articles => rsx! {
                                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 p-4",
                                        for _ in 0..6 {
                                            ArticleCardSkeleton {}
                                        }
                                    }
                                },
                                _ => rsx! {
                                    ClientInitializing {}
                                },
                            }
                        } else if !current_events.is_empty() {
                            div {
                                class: match &tab {
                                    ProfileTab::Articles => {
                                        "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 p-4"
                                    }
                                    ProfileTab::Media(MediaSubTab::Verts) => {
                                        "grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-3 p-4"
                                    }
                                    _ => "divide-y divide-border",
                                },
                                for event in current_events.iter() {
                                    match &tab {
                                        ProfileTab::Articles => rsx! {
                                            ArticleCard { key: "{event.id}", event: event.clone() }
                                        },
                                        ProfileTab::Media(MediaSubTab::Photos) => rsx! {
                                            PhotoCard { key: "{event.id}", event: event.clone() }
                                        },
                                        ProfileTab::Media(MediaSubTab::Videos) => rsx! {
                                            VideoCard { key: "{event.id}", event: event.clone() }
                                        },
                                        ProfileTab::Media(MediaSubTab::Verts) => rsx! {
                                            VertsVideoCard { key: "{event.id}", event: event.clone() }
                                        },
                                        ProfileTab::Likes => {
                                            match event.kind.as_u16() {
                                                20 => rsx! {
                                                    PhotoCard { key: "{event.id}", event: event.clone() }
                                                },
                                                21 | 22 => rsx! {
                                                    VideoCard { key: "{event.id}", event: event.clone() }
                                                },
                                                30023 => rsx! {
                                                    ArticleCard { key: "{event.id}", event: event.clone() }
                                                },
                                                _ => rsx! {
                                                    NoteCard {
                                                        key: "{event.id}",
                                                        event: event.clone(),
                                                        collapsible: true,
                                                        cached_muted_posts: cached_muted_posts.read().clone(),
                                                        cached_blocked_users: cached_blocked_users.read().clone(),
                                                    }
                                                },
                                            }
                                        }
                                        ProfileTab::Zaps(sub) => rsx! {
                                            ZapEntryCard {
                                                key: "{event.id}",
                                                event: event.clone(),
                                                show_recipient: matches!(sub, ZapSubTab::Sent),
                                            }
                                        },
                                        _ => {
                                            if event.kind == Kind::Repost {
                                                match extract_reposted_event(event) {
                                                    Ok(original_event) => {
                                                        let repost_info = Some((event.pubkey, event.created_at));
                                                        rsx! {
                                                            NoteCard {
                                                                key: "{event.id}",
                                                                event: original_event,
                                                                repost_info,
                                                                collapsible: true,
                                                                cached_muted_posts: cached_muted_posts.read().clone(),
                                                                cached_blocked_users: cached_blocked_users.read().clone(),
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        log::warn!(
                                                            "Failed to extract reposted event {}: {}", event.id, e
                                                        );
                                                        rsx! {}
                                                    }
                                                }
                                            } else {
                                                rsx! {
                                                    NoteCard {
                                                        key: "{event.id}",
                                                        event: event.clone(),
                                                        collapsible: true,
                                                        cached_muted_posts: cached_muted_posts.read().clone(),
                                                        cached_blocked_users: cached_blocked_users.read().clone(),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if current_has_more {
                                div { id: "{sentinel_id}", class: "p-8 flex justify-center",
                                    if *loading_events.read() {
                                        span { class: "flex items-center gap-2 text-muted-foreground",
                                            span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                            "Loading more..."
                                        }
                                    }
                                }
                            } else if !current_events.is_empty() {
                                div { class: "p-8 text-center text-muted-foreground", "You've reached the end" }
                            }
                        } else {
                            div { class: "text-center py-12",
                                div { class: "text-6xl mb-4", "{get_empty_state_icon(&active_tab.read())}" }
                                p { class: "text-muted-foreground", "{get_empty_state_message(&active_tab.read())}" }
                            }
                        }
                    }
                }
            }
        }
        ProfileEditorModal { show: show_profile_modal }
        DialogRoot { open: *show_dm_dialog.read(),
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
                    DialogTitle { class: "text-xl font-semibold mb-2",
                        if let Some(metadata) = profile_data.read().as_ref() {
                            "Send message to {metadata.name.as_deref().unwrap_or(\"user\")}"
                        } else {
                            "Send message"
                        }
                    }
                    DialogDescription { class: "text-sm text-muted-foreground mb-4",
                        "This message will be encrypted and sent privately."
                    }
                    textarea {
                        class: "w-full p-3 border border-border rounded-lg bg-background text-foreground resize-none focus:outline-hidden focus:ring-2 focus:ring-blue-500",
                        rows: "4",
                        placeholder: "Type your message...",
                        value: "{dm_message.read()}",
                        oninput: move |e| dm_message.set(e.value()),
                        disabled: *dm_sending.read(),
                    }
                    if let Some(err) = dm_error.read().as_ref() {
                        div { class: "mt-2 text-sm text-red-500", "{err}" }
                    }
                    div { class: "flex justify-end gap-2 mt-4",
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
                                    let hex_pubkey = match crate::utils::nip19_urls::parse_profile_id(&recipient) {
                                        Some(pk) => pk.to_hex(),
                                        None => {
                                            dm_error.set(Some("Invalid public key".to_string()));
                                            dm_sending.set(false);
                                            return;
                                        }
                                    };
                                    match dms::send_dm(hex_pubkey, message).await {
                                        Ok(_) => {
                                            show_dm_dialog.set(false);
                                            dm_message.set(String::new());
                                            dm_error.set(None);
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
        DialogRoot { open: *show_info_dialog.read(),
            div {
                class: "fixed inset-0 bg-black/50 backdrop-blur-sm z-50 flex items-center justify-center p-4",
                onclick: move |_| show_info_dialog.set(false),
                div {
                    class: "bg-card border border-border rounded-lg shadow-xl p-6 max-w-md w-full",
                    onclick: move |e| e.stop_propagation(),
                    DialogTitle { class: "text-xl font-semibold mb-2", "Profile Information" }
                    DialogDescription { class: "text-sm text-muted-foreground mb-4",
                        "Copy the public key or lightning address"
                    }
                    div { class: "space-y-4",
                        div {
                            label { class: "block text-sm font-medium mb-1", "Public Key (npub)" }
                            div { class: "flex items-center gap-2",
                                div { class: "flex-1 p-2 bg-muted rounded border border-border text-sm font-mono break-all",
                                    {
                                        crate::utils::nip19_urls::parse_profile_id(&pubkey_for_info)
                                            .map(|pk| pk.to_bech32().unwrap_or_else(|_| pubkey_for_info.clone()))
                                            .unwrap_or_else(|| pubkey_for_info.clone())
                                    }
                                }
                                button {
                                    class: "px-3 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 transition",
                                    onclick: move |_| {
                                        #[cfg(feature = "web")]
                                        if let Some(pk) = crate::utils::nip19_urls::parse_profile_id(&pubkey_for_info) {
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
                        if let Some(metadata) = profile_data.read().as_ref() {
                            if let Some(lud16) = &metadata.lud16 {
                                div {
                                    label { class: "block text-sm font-medium mb-1", "Lightning Address" }
                                    div { class: "flex items-center gap-2",
                                        div { class: "flex-1 p-2 bg-muted rounded border border-border text-sm break-all",
                                            "{lud16}"
                                        }
                                        button {
                                            class: "px-3 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 transition",
                                            onclick: move |_| {
                                                #[cfg(feature = "web")]
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
                        {
                            #[cfg(feature = "cashu")]
                            { render_cashu_p2pk_section(is_own_profile) }
                            #[cfg(not(feature = "cashu"))]
                            { rsx! {} }
                        }
                    }
                    div { class: "flex justify-end mt-6",
                        button {
                            class: "px-4 py-2 bg-accent rounded-lg hover:bg-accent/80 transition",
                            onclick: move |_| show_info_dialog.set(false),
                            "Close"
                        }
                    }
                }
            }
        }
        if *show_add_to_list_modal.read() {
            AddToPeopleListModal {
                person_pubkey: pubkey_for_list.clone(),
                on_close: move |_| show_add_to_list_modal.set(false),
                on_added: move |_| show_add_to_list_modal.set(false),
            }
        }
    }
}
#[component]
fn ProfileTabButton(
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: "shrink-0 px-4 py-4 font-semibold hover:bg-accent transition relative",
            onclick: move |e| onclick.call(e),
            span { class: if active { "" } else { "text-muted-foreground" }, "{label}" }
            if active {
                div { class: "absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-t" }
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
struct VideoMeta {
    url: Option<String>,
    thumbnail: Option<String>,
    title: Option<String>,
}
fn parse_video_meta(event: &NostrEvent) -> VideoMeta {
    let mut meta = VideoMeta {
        url: None,
        thumbnail: None,
        title: None,
    };
    for tag in event.tags.iter() {
        let tag_vec = (*tag).clone().to_vec();
        if tag_vec.first().map(|s| s.as_str()) == Some("title") && tag_vec.len() > 1 {
            meta.title = Some(tag_vec[1].clone());
            break;
        }
    }
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
#[cfg(feature = "cashu")]
fn render_cashu_p2pk_section(is_own_profile: bool) -> Element {
    if !is_own_profile {
        return rsx! {};
    }
    let Ok(p2pk_pubkey) = crate::stores::cashu::get_wallet_pubkey() else {
        return rsx! {};
    };
    #[allow(unused_variables)]
    let pubkey_for_copy = p2pk_pubkey.clone();
    rsx! {
        div {
            label { class: "block text-sm font-medium mb-1", "Cashu P2PK Pubkey" }
            p { class: "text-xs text-muted-foreground mb-2",
                "Others can send you locked ecash tokens that only you can redeem"
            }
            div { class: "flex items-center gap-2",
                div { class: "flex-1 p-2 bg-muted rounded border border-border text-xs font-mono break-all",
                    "{p2pk_pubkey}"
                }
                button {
                    class: "px-3 py-2 bg-purple-500 text-white rounded hover:bg-purple-600 transition",
                    onclick: move |_| {
                        #[cfg(feature = "web")]
                        if let Some(window) = web_sys::window() {
                            let _ = window.navigator().clipboard().write_text(&pubkey_for_copy);
                        }
                    },
                    "Copy"
                }
            }
        }
    }
}

#[component]
fn VertsVideoCard(event: NostrEvent) -> Element {
    let video_meta = parse_video_meta(&event);
    let mut is_hovering = use_signal(|| false);
    let video_element_id = format!("preview-vert-{}", &event.id.to_hex()[..12]);
    let video_element_id_for_effect = video_element_id.clone();
    use_effect(use_reactive(&*is_hovering.read(), move |hovering| {
        let id = video_element_id_for_effect.clone();
        spawn(async move {
            let action = if hovering { "play" } else { "pause" };
            let js = format!(
                r#"(function() {{ var v = document.getElementById("{id}"); if (v) {{ if ("{action}" === "play") {{ v.play().catch(function(){{}}); }} else {{ v.pause(); v.currentTime = 0; }} }} }})()"#
            );
            let _ = document::eval(&js).await;
        });
    }));
    let video_id = event.id.to_hex();
    let video_src = video_meta.url.as_ref().map(|u| {
        if video_meta.thumbnail.is_none() {
            format!("{}#t=0.1", u)
        } else {
            u.clone()
        }
    });
    rsx! {
        div {
            class: "group cursor-pointer",
            onmouseenter: move |_| is_hovering.set(true),
            onmouseleave: move |_| is_hovering.set(false),
            Link {
                to: crate::routes::Route::VideoDetail {
                    video_id: video_id.clone(),
                },
                div { class: "relative aspect-[9/16] bg-muted rounded-lg overflow-hidden mb-2",
                    if let Some(thumbnail) = &video_meta.thumbnail {
                        img {
                            src: "{thumbnail}",
                            alt: "{video_meta.title.as_deref().unwrap_or(\"Vert\")}",
                            class: "w-full h-full object-cover group-hover:scale-105 transition-transform duration-200",
                        }
                    } else if let Some(url) = &video_src {
                        video {
                            id: "{video_element_id}",
                            class: "w-full h-full object-cover",
                            src: "{url}",
                            muted: true,
                            r#loop: true,
                            playsinline: true,
                            preload: "metadata",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-muted",
                            crate::components::icons::VideoIcon { class: "w-8 h-8 text-muted-foreground" }
                        }
                    }
                    div { class: "absolute bottom-2 left-2 bg-black/80 text-white text-xs px-2 py-1 rounded flex items-center gap-1",
                        crate::components::icons::VideoIcon { class: "w-3 h-3" }
                        "Vert"
                    }
                }
                if let Some(title) = &video_meta.title {
                    p { class: "text-sm font-medium line-clamp-2 group-hover:text-primary transition",
                        "{title}"
                    }
                }
            }
        }
    }
}

#[component]
fn ZapEntryCard(event: NostrEvent, show_recipient: bool) -> Element {
    let profile_pubkey = if show_recipient {
        event.tags.iter().find_map(|tag| {
            let slice = tag.as_slice();
            if slice.first().map(|s| s.as_str()) == Some("p") && slice.len() > 1 {
                PublicKey::from_hex(slice[1].as_str()).ok()
            } else {
                None
            }
        })
    } else {
        event.tags.iter().find_map(|tag| {
            let slice = tag.as_slice();
            if slice.first().map(|s| s.as_str()) == Some("P") && slice.len() > 1 {
                PublicKey::from_hex(slice[1].as_str()).ok()
            } else {
                None
            }
        })
    };
    let zap_amount = crate::services::aggregation::extract_zap_amount(&event);
    let zap_message: Option<String> = event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) == Some("description") && slice.len() > 1 {
            Some(slice[1].clone())
        } else {
            None
        }
    }).or_else(|| {
        let content = event.content.trim();
        if content.is_empty() { None } else { Some(content.to_string()) }
    });
    let profile_sig = use_signal(|| None::<nostr_sdk::Metadata>);
    let profile_hex = profile_pubkey.as_ref().map(|pk| pk.to_hex());
    {
        let mut ps = profile_sig;
        let _pk_hex = profile_hex.clone();
        use_effect(use_reactive((&profile_hex,), move |(pk_hex,)| {
            if let Some(hex) = pk_hex {
                spawn(async move {
                    if let Some(metadata) = profiles::get_profile(&hex) {
                        ps.set(Some(metadata));
                    } else {
                        let _ = profiles::fetch_profile(hex.clone()).await;
                        if let Some(metadata) = profiles::get_profile(&hex) {
                            ps.set(Some(metadata));
                        }
                    }
                });
            }
        }));
    }
    let display_name = profile_sig.read().as_ref()
        .map(|m| get_display_name(m, profile_hex.as_deref().unwrap_or("")))
        .unwrap_or_else(|| {
            profile_hex.as_deref().map(|h| {
                if h.len() > 12 {
                    format!("{}...{}", &h[..8], &h[h.len()-4..])
                } else { h.to_string() }
            }).unwrap_or_else(|| "Anonymous".to_string())
        });
    let profile_picture = profile_sig.read().as_ref()
        .and_then(|m| m.picture.clone());
    let amount_str = zap_amount.map(|a| format!("{} sats", a)).unwrap_or_default();
    rsx! {
        div { class: "flex items-start gap-3 p-4 border-b border-border",
            if let Some(pic) = profile_picture {
                img {
                    class: "w-10 h-10 rounded-full",
                    src: "{pic}",
                    alt: if show_recipient { "Zap recipient" } else { "Zap sender" },
                }
            } else {
                div { class: "w-10 h-10 rounded-full bg-muted flex items-center justify-center text-sm font-bold text-muted-foreground",
                    "{display_name.chars().next().unwrap_or('?').to_uppercase()}"
                }
            }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-center gap-2",
                    span { class: "font-semibold text-sm truncate", "{display_name}" }
                    if !amount_str.is_empty() {
                        span { class: "text-orange-500 font-bold text-sm", "⚡ {amount_str}" }
                    }
                }
                if let Some(msg) = zap_message {
                    if !msg.is_empty() {
                        p { class: "text-sm text-muted-foreground mt-1 line-clamp-2", "{msg}" }
                    }
                }
                p { class: "text-xs text-muted-foreground mt-1",
                    "{format_timestamp(event.created_at.as_secs())}"
                }
            }
        }
    }
}
