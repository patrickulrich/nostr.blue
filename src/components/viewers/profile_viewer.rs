use crate::components::dialog::{DialogDescription, DialogRoot, DialogTitle};
use chrono::Utc;
use crate::components::icons::{CopyIcon, InfoIcon, Link2Icon, ListIcon, MailIcon};
use crate::components::rich_content::mentions::{MentionRenderer, TextLinkMention};
use crate::components::{
    AddToPeopleListModal, ArticleCard, ArticleCardSkeleton, ClientInitializing, ExternalIdentitiesSection, FollowersModal, FollowersTab, Nip05Badge, NoteCard,
    PhotoCard, PinnedNotesCarousel, ProfileBadgesSection, ProfileEditorModal, VideoCard,
};
use crate::hooks::{use_infinite_scroll_with_generation, use_mute_block_cache};
use crate::routes::profile::{MediaSubTab, ProfileTab, ZapSubTab};
use crate::services::nip05;
use crate::services::profile_stats;
use crate::stores::ui::settings_store::get_canonical_external_origin;
use crate::stores::{auth_store, dms, nostr_client, pinned_notes, profiles};
use crate::utils::article_meta::get_published_at;
use crate::utils::clipboard::copy_to_clipboard;
use crate::utils::content_parser::{parse_content, ContentToken};
use crate::utils::pagination::{is_likely_future, safe_cursor_from_timestamps};
use crate::utils::repost::{expand_events_for_prefetch, extract_reposted_event};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions, Toasts};
use nostr_sdk::nips::nip19::ToBech32;
use nostr_sdk::prelude::*;
use nostr_sdk::Event as NostrEvent;
use qrcode::render::svg;
use qrcode::QrCode;
use std::time::Duration;

use crate::routes::profile::loader::{load_tab_events_db, load_tab_events, prefetch_author_metadata, build_tab_filter, process_tab_events, load_likes_relays};
use crate::routes::profile::types::{TabData, default_tab_data_map, dedupe_articles_by_address, get_display_name, get_username, get_avatar_initial, strip_https, get_empty_state_message, get_empty_state_icon, format_timestamp};

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
pub fn ProfileViewer(pubkey: String) -> Element {
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
    let mut show_followers_modal = use_signal(|| false);
    let mut followers_modal_tab = use_signal(|| FollowersTab::Following);
    let mut pinned_events = use_signal(Vec::<NostrEvent>::new);
    let mut pinned_loading = use_signal(|| true);
    let mut user_write_relays = use_signal(Vec::<String>::new);
    let mut request_id = use_signal(|| 0u32);
    let mut current_pubkey = use_signal(|| pubkey.clone());
    let mut feed_reset_generation = use_signal(|| 0u64);
    // The events sentinel unmounts when the pubkey resets (tab_data cleared,
    // current_tab_has_more forced true) and when switching to a not-yet-loaded
    // tab empties the current list. Bump the generation so the observer
    // re-attaches to the sentinel that mounts with the new data.
    use_effect(move || {
        let _ = *active_tab.read();
        feed_reset_generation += 1;
    });
    let (cached_muted_posts, cached_blocked_users, cached_muted_words) = use_mute_block_cache();
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
        feed_reset_generation += 1;
        is_following.set(false);
        follows_you.set(false);
        following_count.set(0);
        followers_count.set(0);
        post_count.set(0);
        pinned_events.set(Vec::new());
        pinned_loading.set(true);
        user_write_relays.set(Vec::new());
        // Clean up ephemeral relays from the previous profile view.
        // The metadata + tab-events fetches share these connections and
        // don't clean up per-call (to avoid races). They idle-timeout after
        // 5 min, but we remove them promptly on profile change.
        if let Some(client) = nostr_client::get_client() {
            spawn(async move {
                let all = client.pool().all_relays().await;
                let to_remove: Vec<String> = all
                    .iter()
                    .filter(|(_, r)| {
                        let flags = r.flags();
                        flags.has_read() && flags.has_write() && !flags.has_discovery()
                    })
                    .filter_map(|(url, _)| {
                        // Only remove ephemeral relays (tracked in EPHEMERAL_IN_USE)
                        if crate::stores::relay::coverage::is_ephemeral_relay(url.as_str()) {
                            Some(url.to_string())
                        } else {
                            None
                        }
                    })
                    .collect();
                if !to_remove.is_empty() {
                    crate::stores::relay::coverage::cleanup_ephemeral_relays(&client, &to_remove)
                        .await;
                }
            });
        }
    }));
    use_effect(use_reactive(
        (
            &pubkey_for_pinned,
            &*nostr_client::CLIENT_INITIALIZED.read(),
            &*nostr_client::HAS_SIGNER.read(),
        ),
        move |(pubkey_str, client_initialized, has_signer)| {
            if !client_initialized {
                return;
            }
            // For authenticated users whose signer hasn't attached yet, defer.
            // The effect re-runs when HAS_SIGNER flips true.
            if auth_store::is_authenticated() && !has_signer {
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
            &*nostr_client::HAS_SIGNER.read(),
        ),
        move |(pubkey_str, client_initialized, _retry, has_signer)| {
            if !client_initialized {
                return;
            }
            // For authenticated users whose signer hasn't attached yet, defer.
            // The effect re-runs when HAS_SIGNER flips true.
            if auth_store::is_authenticated() && !has_signer {
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

                // Tier 1: LRU cache — serve immediately (stale-while-
                // revalidate). When the underlying kind-0 event is older
                // than 24h, race indexers/outbox in the background and
                // replace only on a strictly-newer result.
                if let Some(cached) = profiles::get_cached_profile(&hex_pubkey) {
                    if *rid.peek() != current_id {
                        return;
                    }
                    log::debug!("Loaded profile metadata from LRU cache");
                    // Revalidation floor: the source event's `created_at`
                    // when known, else the cache insertion time — entries
                    // cached without `event_created_at` (pre-freshness-work
                    // cache, or `cache_profile(..., None)` callers) would
                    // otherwise have a `0` floor and accept ANY race winner,
                    // even one older than what's displayed.
                    let displayed_created_at = cached
                        .event_created_at
                        .unwrap_or_else(|| cached.fetched_at.timestamp().max(0) as u64);
                    let stale = cached.needs_revalidation();
                    if stale {
                        log::info!(
                            "Profile {} cache stale (kind-0 created_at {}); revalidating in background",
                            hex_pubkey,
                            displayed_created_at
                        );
                    }
                    profile_data.set(Some(profiles::profile_to_metadata(&cached)));
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
                    if stale {
                        let hex_rv = hex_pubkey.clone();
                        let rid_rv = rid;
                        let cid_rv = current_id;
                        let mut profile_data_rv = profile_data;
                        spawn(async move {
                            match race_profile_metadata(&hex_rv).await {
                                Ok(Some((metadata, created_at)))
                                    if created_at > displayed_created_at
                                        && *rid_rv.peek() == cid_rv =>
                                {
                                    log::info!(
                                        "Profile {} revalidated: newer kind 0 (created_at {} > {})",
                                        hex_rv,
                                        created_at,
                                        displayed_created_at
                                    );
                                    profiles::cache_profile(&hex_rv, &metadata, Some(created_at));
                                    profile_data_rv.set(Some(metadata));
                                }
                                Ok(_) => {
                                    // Completed check, nothing newer: stamp so
                                    // `needs_revalidation` throttles to once
                                    // per TTL instead of refetching per view.
                                    profiles::mark_profile_revalidated(&hex_rv);
                                }
                                Err(_) => {}
                            }
                        });
                    }
                    return;
                }

                // Tier 2: SDK database. Query the raw kind-0 event (not
                // `database().metadata()`) so the event's `created_at` is
                // available for freshness decisions. The DB passively
                // ingests kind 0s from feeds and may hold an older snapshot
                // than the relays — serve it immediately, but revalidate in
                // the background when it is stale.
                let db_filter = Filter::new()
                    .author(public_key)
                    .kind(Kind::Metadata)
                    .limit(1);
                if let Ok(db_events) = client.database().query(db_filter).await {
                    if *rid.peek() != current_id {
                        return;
                    }
                    // `Events` iterates newest-first.
                    if let Some(event) = db_events.into_iter().next() {
                        log::info!(
                            "Profile {} tier-2 DB hit: kind-0 event {} created_at {}",
                            hex_pubkey,
                            event.id,
                            event.created_at
                        );
                        if let Ok(metadata) = nostr_client::parse_metadata_content(&event) {
                            let event_created_at = event.created_at.as_secs();
                            let stale = (Utc::now().timestamp() - event_created_at as i64)
                                .max(0)
                                >= crate::stores::profiles::CACHE_TTL_SECONDS;
                            // Share the DB hit with the PROFILE_CACHE so
                            // NoteCards and repeat visits don't re-fetch.
                            profiles::cache_profile(
                                &hex_pubkey,
                                &metadata,
                                Some(event_created_at),
                            );
                            profile_data.set(Some(metadata));
                            loading.set(false);
                            let hex_bg = hex_pubkey.clone();
                            let rid_bg = rid;
                            let cid_bg = current_id;
                            spawn(async move {
                                let write_relays =
                                    crate::stores::relay::coverage::resolve_user_relays(
                                        &hex_bg,
                                        crate::stores::relay::coverage::RelayPurpose::Write,
                                    )
                                    .await;
                                if *rid_bg.peek() == cid_bg && !write_relays.is_empty() {
                                    user_write_relays.set(write_relays);
                                }
                            });
                            if stale {
                                log::info!(
                                    "Profile {} DB kind 0 stale (created_at {}); revalidating in background",
                                    hex_pubkey,
                                    event_created_at
                                );
                                let hex_rv = hex_pubkey.clone();
                                let rid_rv = rid;
                                let cid_rv = current_id;
                                let mut profile_data_rv = profile_data;
                                spawn(async move {
                                    match race_profile_metadata(&hex_rv).await {
                                        Ok(Some((metadata, created_at)))
                                            if created_at > event_created_at
                                                && *rid_rv.peek() == cid_rv =>
                                        {
                                            log::info!(
                                                "Profile {} revalidated: newer kind 0 (created_at {} > {})",
                                                hex_rv,
                                                created_at,
                                                event_created_at
                                            );
                                            profiles::cache_profile(
                                                &hex_rv,
                                                &metadata,
                                                Some(created_at),
                                            );
                                            profile_data_rv.set(Some(metadata));
                                        }
                                        Ok(_) => {
                                            // Completed check, nothing newer:
                                            // stamp the throttle (see tier-1).
                                            profiles::mark_profile_revalidated(&hex_rv);
                                        }
                                        Err(_) => {}
                                    }
                                });
                            }
                            return;
                        }
                        // Unparseable DB content — fall through to the race.
                    }
                }

                if *rid.peek() != current_id {
                    return;
                }

                // Resolve write relays in the background (non-blocking) so the
                // metadata race isn't gated on a 5s kind-10002 network fetch.
                // `fetch_metadata_targeted` resolves these internally too, but
                // resolving here ensures `user_write_relays` is populated
                // promptly for the tab-events effect (Effect 4).
                {
                    let hex_bg = hex_pubkey.clone();
                    let mut uwr = user_write_relays;
                    let rid_bg = rid;
                    let cid_bg = current_id;
                    spawn(async move {
                        let write_relays = crate::stores::relay::coverage::resolve_user_relays(
                            &hex_bg,
                            crate::stores::relay::coverage::RelayPurpose::Write,
                        )
                        .await;
                        if *rid_bg.peek() == cid_bg && !write_relays.is_empty() {
                            uwr.set(write_relays);
                        }
                    });
                }

                // Tier 3: blocking race — indexers (fast path) vs outbox.
                // Indexers are connected at boot and purpose-built for kind 0
                // discovery — typically <500ms. The outbox path resolves the
                // user's write relays + ephemeral-connects + fetches. First
                // Ok(Some) wins; if the winner returns None/Err we fall back
                // to the remaining future.
                let metadata_result = race_profile_metadata(&hex_pubkey).await;

                if *rid.peek() != current_id {
                    return;
                }
                match metadata_result {
                    Ok(Some((metadata, created_at))) => {
                        log::debug!("Fetched profile metadata from race winner");
                        // Share the race result (indexers or outbox) with the
                        // PROFILE_CACHE so feed NoteCards for this author
                        // resolve from cache instead of a separate fetch.
                        profiles::cache_profile(&hex_pubkey, &metadata, Some(created_at));
                        profile_data.set(Some(metadata));
                    }
                    Ok(None) => {
                        log::debug!("No metadata found from any source");
                        metadata_error.set(Some("No profile data found".to_string()));
                        profile_data.set(Some(nostr_sdk::Metadata::new()));
                    }
                    Err(e) => {
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
            &*nostr_client::HAS_SIGNER.read(),
        ),
        move |(pubkey_str, tab, client_initialized, has_signer)| {
            if !client_initialized {
                return;
            }
            // For authenticated users whose signer hasn't attached yet, defer.
            // The effect re-runs when HAS_SIGNER flips true.
            if auth_store::is_authenticated() && !has_signer {
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
                        let oldest_ts = safe_cursor_from_timestamps(
                            &db_outcome.events.iter().map(|e| e.created_at.as_secs()).collect::<Vec<u64>>()
                        );
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
                    // Wait for user relay lists to be applied (no-op if
                    // logged out or already applied). Ensures the user's
                    // NIP-65 read relays are in the pool before streaming.
                    crate::stores::relay::wait_for_user_relays(
                        Duration::from_millis(500),
                        "profile_tab_events",
                    )
                    .await;
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
                    // Guard: ensure the client is initialized before proceeding.
                    // `stream_profile_events_from_relays` obtains the client
                    // internally, but we bail early here to avoid building a
                    // filter + collector for a client that doesn't exist.
                    if nostr_client::get_client().is_none() {
                        loading_events.set(false);
                        return;
                    }
                    // Media Videos/Verts tabs: connect the divine specialty
                    // relay before streaming so Source 1's connected-pool
                    // snapshot (taken inside stream_profile_events_from_relays)
                    // includes it. Divine-hosted video content is invisible to
                    // the outbox path — divine platform users don't list
                    // relay.divine.video in their kind 10002 (#362). Bounded
                    // wait: an unreachable relay must not stall the stream
                    // phase for the full 30s internal connection timeout.
                    if matches!(
                        tab_for_relay,
                        ProfileTab::Media(MediaSubTab::Videos)
                            | ProfileTab::Media(MediaSubTab::Verts)
                    ) {
                        if let Some(client) = nostr_client::get_client() {
                            crate::stores::relay::ensure_video_relay_connected_bounded(
                                &client,
                                Duration::from_secs(5),
                            )
                            .await;
                            if *rid.peek() != current_id {
                                loading_events.set(false);
                                return;
                            }
                        }
                    }
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
                                    let zaps_cursor = safe_cursor_from_timestamps(&merged.iter().map(|e| e.created_at.as_secs()).collect::<Vec<u64>>());
                                    data_map.insert(
                                        tab_for_relay.clone(),
                                        TabData {
                                            events: merged,
                                            oldest_timestamp: zaps_cursor,
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

                    // Stream events from the author's outbox relays for
                    // progressive UI updates — posts paint as they arrive
                    // instead of blocking for the full EOSE window. Mirrors
                    // the home Following feed's `load_following_feed_streaming`
                    // pattern: per-event callback + `DebouncedCollector` for
                    // coalesced signal writes.
                    let collector =
                        crate::utils::debounced_collector::DebouncedCollector::<NostrEvent>::new(
                            50,
                        );
                    let all_streamed: std::rc::Rc<std::cell::RefCell<Vec<NostrEvent>>> =
                        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                    let td_stream = tab_data;
                    let tfr_stream = tab_for_relay.clone();
                    let pc_stream = post_count;
                    let rid_stream = rid;
                    let cid_stream = current_id;

                    let stream_result = nostr_client::stream_profile_events_from_relays(
                        filter,
                        &known_relays,
                        Duration::from_secs(8),
                        {
                            let collector = collector.clone();
                            let all_streamed = all_streamed.clone();
                            move |event| {
                                if *rid_stream.peek() != cid_stream {
                                    return;
                                }
                                if is_likely_future(event.created_at) {
                                    return;
                                }
                                all_streamed.borrow_mut().push(event.clone());
                                collector.extend(std::iter::once(event), {
                                    let mut td = td_stream;
                                    let tfr = tfr_stream.clone();
                                    let mut pc = pc_stream;
                                    move |batch| {
                                        if *rid_stream.peek() != cid_stream {
                                            return;
                                        }
                                        let processed = process_tab_events(batch, &tfr);
                                        if processed.is_empty() {
                                            return;
                                        }
                                        let mut data_map = td.read().clone();
                                        let existing =
                                            data_map.get(&tfr).cloned().unwrap_or_default();
                                        let existing_ids: std::collections::HashSet<_> =
                                            existing.events.iter().map(|e| e.id).collect();
                                        let new: Vec<_> = processed
                                            .into_iter()
                                            .filter(|e| !existing_ids.contains(&e.id))
                                            .collect();
                                        if new.is_empty() {
                                            return;
                                        }
                                        let mut merged = existing.events;
                                        merged.extend(new);
                                        if matches!(tfr, ProfileTab::Articles) {
                                            merged = dedupe_articles_by_address(merged);
                                            merged.sort_by_key(|e| {
                                                std::cmp::Reverse(get_published_at(e))
                                            });
                                        } else {
                                            merged
                                                .sort_by_key(|e| std::cmp::Reverse(e.created_at));
                                        }
                                        let oldest_ts = if matches!(tfr, ProfileTab::Articles) {
                                            safe_cursor_from_timestamps(
                                                &merged.iter().map(get_published_at).collect::<Vec<u64>>(),
                                            )
                                        } else {
                                            safe_cursor_from_timestamps(
                                                &merged.iter().map(|e| e.created_at.as_secs()).collect::<Vec<u64>>(),
                                            )
                                        };
                                        let merged_len = merged.len();
                                        data_map.insert(
                                            tfr.clone(),
                                            TabData {
                                                events: merged,
                                                oldest_timestamp: oldest_ts,
                                                has_more: true,
                                                loaded: true,
                                            },
                                        );
                                        td.set(data_map);
                                        if matches!(tfr, ProfileTab::Posts) {
                                            pc.set(merged_len);
                                        }
                                    }
                                });
                            }
                        },
                    )
                    .await;

                    if let Err(e) = &stream_result {
                        log::warn!("Profile tab stream error for {:?}: {}", tab_for_relay, e);
                    }

                    if *rid.peek() != current_id {
                        loading_events.set(false);
                        return;
                    }

                    // Flush any events buffered after the last debounce window.
                    let tail = collector.drain();
                    if !tail.is_empty() {
                        let processed = process_tab_events(tail, &tab_for_relay);
                        let mut data_map = tab_data.read().clone();
                        let existing = data_map.get(&tab_for_relay).cloned().unwrap_or_default();
                        let existing_ids: std::collections::HashSet<_> =
                            existing.events.iter().map(|e| e.id).collect();
                        let new: Vec<_> = processed
                            .into_iter()
                            .filter(|e| !existing_ids.contains(&e.id))
                            .collect();
                        if !new.is_empty() {
                            let mut merged = existing.events;
                            merged.extend(new);
                            if matches!(tab_for_relay, ProfileTab::Articles) {
                                merged = dedupe_articles_by_address(merged);
                                merged.sort_by_key(|e| std::cmp::Reverse(get_published_at(e)));
                            } else {
                                merged.sort_by_key(|e| std::cmp::Reverse(e.created_at));
                            }
                            let oldest_ts = if matches!(tab_for_relay, ProfileTab::Articles) {
                                safe_cursor_from_timestamps(
                                    &merged.iter().map(get_published_at).collect::<Vec<u64>>(),
                                )
                            } else {
                                safe_cursor_from_timestamps(
                                    &merged.iter().map(|e| e.created_at.as_secs()).collect::<Vec<u64>>(),
                                )
                            };
                            let merged_len = merged.len();
                            data_map.insert(
                                tab_for_relay.clone(),
                                TabData {
                                    events: merged,
                                    oldest_timestamp: oldest_ts,
                                    has_more: true,
                                    loaded: true,
                                },
                            );
                            tab_data.set(data_map);
                            if matches!(tab_for_relay, ProfileTab::Posts) {
                                post_count.set(merged_len);
                            }
                        }
                    }

                    // Finalize has_more based on total events received.
                    let total = all_streamed.borrow().len();
                    let has_more = total >= 100;
                    {
                        let mut data_map = tab_data.read().clone();
                        if let Some(data) = data_map.get_mut(&tab_for_relay) {
                            data.has_more = has_more;
                            data.loaded = true;
                        }
                        tab_data.set(data_map);
                    }
                    current_tab_has_more.set(has_more);
                    log::info!(
                        "Phase 2 stream: received {} events for {:?} (has_more: {})",
                        total,
                        tab_for_relay,
                        has_more
                    );

                    // Prefetch author metadata for all streamed events.
                    let events_for_prefetch =
                        expand_events_for_prefetch(&all_streamed.borrow());
                    if !events_for_prefetch.is_empty() {
                        spawn(async move {
                            prefetch_author_metadata(&events_for_prefetch).await;
                        });
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
    // Live-tail subscription: after the initial page streams in, subscribe to
    // new events for realtime updates (amethyst/wisp pattern). Uses
    // `use_relay_subscription_to` which is reactive to relay_urls changes —
    // it auto-re-subscribes when user_write_relays are discovered
    // mid-session (the wisp "re-subscribe on discovery" pattern).
    //
    // `since` is derived from the newest loaded event so only genuinely-new
    // events arrive. As new events merge, the filter advances and the hook
    // re-subscribes with the updated `since` (duplicates are deduped).
    // Disabled for Likes/Zaps tabs (multi-stage fetch logic).
    {
        let live_tab = active_tab.read().clone();
        let live_td = tab_data.read();
        let live_loaded = live_td.get(&live_tab).map(|d| d.loaded).unwrap_or(false);
        let live_since = live_td
            .get(&live_tab)
            .and_then(|d| d.events.first())
            .map(|e| e.created_at);
        drop(live_td);
        let is_simple_tab = !matches!(live_tab, ProfileTab::Likes | ProfileTab::Zaps(_));
        let live_filter: Option<Filter> =
            if live_loaded && *nostr_client::CLIENT_INITIALIZED.read() && is_simple_tab {
                live_since.and_then(|since| {
                    crate::utils::nip19_urls::parse_profile_id(&pubkey)
                        .map(|pk| build_tab_filter(pk, &live_tab, None, 10).since(since))
                })
            } else {
                None
            };
        let live_relays = if live_filter.is_some() {
            user_write_relays.read().clone()
        } else {
            Vec::new()
        };
        let mut td_live = tab_data;
        let tab_live = live_tab;
        let mut pc_live = post_count;
        crate::hooks::use_relay_subscription_to(
            live_filter,
            None,
            live_relays,
            move |event: &nostr::Event| {
                if is_likely_future(event.created_at) {
                    return;
                }
                let processed = process_tab_events(vec![event.clone()], &tab_live);
                if processed.is_empty() {
                    return;
                }
                let mut data_map = td_live.read().clone();
                let existing = data_map.get(&tab_live).cloned().unwrap_or_default();
                let existing_ids: std::collections::HashSet<_> =
                    existing.events.iter().map(|e| e.id).collect();
                let new: Vec<_> = processed
                    .into_iter()
                    .filter(|e| !existing_ids.contains(&e.id))
                    .collect();
                if new.is_empty() {
                    return;
                }
                let mut merged = existing.events;
                merged.extend(new);
                if matches!(tab_live, ProfileTab::Articles) {
                    merged = dedupe_articles_by_address(merged);
                    merged.sort_by_key(|e| std::cmp::Reverse(get_published_at(e)));
                } else {
                    merged.sort_by_key(|e| std::cmp::Reverse(e.created_at));
                }
                let merged_len = merged.len();
                data_map.insert(
                    tab_live.clone(),
                    TabData {
                        events: merged,
                        oldest_timestamp: existing.oldest_timestamp,
                        has_more: existing.has_more,
                        loaded: true,
                    },
                );
                td_live.set(data_map);
                if matches!(tab_live, ProfileTab::Posts) {
                    pc_live.set(merged_len);
                }
            },
        );
    }
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
                    let oldest_ts = safe_cursor_from_timestamps(
                        &outcome.events.iter().map(|e| e.created_at.as_secs()).collect::<Vec<u64>>()
                    );
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
    let sentinel_id = use_infinite_scroll_with_generation(
        load_more,
        current_tab_has_more,
        loading_events,
        feed_reset_generation,
    );

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
                        div {
                            class: "hover:underline cursor-pointer",
                            onclick: move |_| {
                                followers_modal_tab.set(FollowersTab::Following);
                                show_followers_modal.set(true);
                            },
                            span { class: "font-bold", "{following_count.read()}" }
                            span { class: "text-muted-foreground ml-1", "Following" }
                        }
                        div {
                            class: "hover:underline cursor-pointer",
                            onclick: move |_| {
                                followers_modal_tab.set(FollowersTab::Followers);
                                show_followers_modal.set(true);
                            },
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
                        div {
                            class: "hover:underline cursor-pointer",
                            onclick: move |_| {
                                followers_modal_tab.set(FollowersTab::Following);
                                show_followers_modal.set(true);
                            },
                            span { class: "font-bold", "{following_count.read()}" }
                            span { class: "text-muted-foreground ml-1", "Following" }
                        }
                        div {
                            class: "hover:underline cursor-pointer",
                            onclick: move |_| {
                                followers_modal_tab.set(FollowersTab::Followers);
                                show_followers_modal.set(true);
                            },
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
                                                        cached_muted_words: cached_muted_words.read().clone(),
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
                                                                cached_muted_words: cached_muted_words.read().clone(),
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
                                                        cached_muted_words: cached_muted_words.read().clone(),
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
        FollowersModal {
            pubkey: pubkey.clone(),
            initial_tab: *followers_modal_tab.read(),
            open: show_followers_modal,
        }
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
        ProfileInfoDialog {
            open: show_info_dialog,
            pubkey: pubkey_for_info.clone(),
            profile_data,
            is_own_profile,
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
fn ProfileInfoDialog(
    mut open: Signal<bool>,
    pubkey: String,
    profile_data: Signal<Option<nostr_sdk::Metadata>>,
    is_own_profile: bool,
) -> Element {
    let toast = consume_toast();

    let npub = crate::utils::nip19_urls::parse_profile_id(&pubkey)
        .and_then(|pk| pk.to_bech32().ok())
        .unwrap_or_else(|| pubkey.clone());

    let route_id = crate::utils::nip19_urls::profile_route_id(&pubkey);
    let profile_link = format!("{}/{}", get_canonical_external_origin(), route_id);

    // Encode a NIP-21 URI (nostr:nprofile1...) so phone cameras route
    // straight into Nostr apps; the relay hints embedded in the nprofile
    // improve fetch reliability when scanned. Fall back to the raw npub if
    // bech32 encoding failed (route_id doesn't start with 'n').
    let qr_svg = if *open.read() {
        let qr_payload = if route_id.starts_with('n') {
            format!("nostr:{route_id}")
        } else {
            npub.clone()
        };
        QrCode::new(qr_payload.as_bytes()).ok().map(|code| {
            code.render::<svg::Color>()
                .min_dimensions(200, 200)
                .dark_color(svg::Color("#000000"))
                .light_color(svg::Color("#ffffff"))
                .build()
        })
    } else {
        None
    };

    rsx! {
        DialogRoot { open: *open.read(),
            div {
                class: "fixed inset-0 bg-black/50 backdrop-blur-sm z-50 flex items-center justify-center p-4",
                onclick: move |_| open.set(false),
                div {
                    class: "bg-card border border-border rounded-lg shadow-xl p-6 max-w-md w-full",
                    onclick: move |e| e.stop_propagation(),
                    DialogTitle { class: "text-xl font-semibold mb-2", "Profile Information" }
                    DialogDescription { class: "text-sm text-muted-foreground mb-4",
                        "Scan the QR code or copy the public key, profile link, or lightning address"
                    }
                    div { class: "space-y-4",
                        div { class: "flex justify-center",
                            if let Some(ref svg_str) = qr_svg {
                                div {
                                    class: "p-4 bg-black rounded-lg",
                                    dangerous_inner_html: "{svg_str}",
                                }
                            } else {
                                div { class: "w-[200px] h-[200px] bg-muted rounded-lg flex items-center justify-center",
                                    p { class: "text-sm text-muted-foreground", "QR generation failed" }
                                }
                            }
                        }
                        div {
                            label { class: "block text-sm font-medium mb-1", "Public Key (npub)" }
                            div { class: "flex items-center gap-2",
                                div { class: "flex-1 p-2 bg-muted rounded border border-border text-sm font-mono break-all",
                                    "{npub}"
                                }
                                button {
                                    class: "shrink-0 p-2 hover:bg-accent rounded-lg transition text-muted-foreground hover:text-foreground",
                                    title: "Copy npub",
                                    aria_label: "Copy npub to clipboard",
                                    r#type: "button",
                                    onclick: move |_| copy_with_toast(toast, npub.clone(), "npub"),
                                    CopyIcon { class: "w-4 h-4" }
                                }
                                button {
                                    class: "shrink-0 p-2 hover:bg-accent rounded-lg transition text-muted-foreground hover:text-foreground",
                                    title: "Copy profile link",
                                    aria_label: "Copy profile link to clipboard",
                                    r#type: "button",
                                    onclick: move |_| copy_with_toast(toast, profile_link.clone(), "profile link"),
                                    Link2Icon { class: "w-4 h-4" }
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
                                            class: "shrink-0 p-2 hover:bg-accent rounded-lg transition text-muted-foreground hover:text-foreground",
                                            title: "Copy lightning address",
                                            aria_label: "Copy lightning address to clipboard",
                                            r#type: "button",
                                            onclick: {
                                                let lud16 = lud16.clone();
                                                move |_| copy_with_toast(toast, lud16.clone(), "lightning address")
                                            },
                                            CopyIcon { class: "w-4 h-4" }
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
                            onclick: move |_| open.set(false),
                            "Close"
                        }
                    }
                }
            }
        }
    }
}

fn copy_with_toast(toast: Toasts, text: String, label: &'static str) {
    spawn(async move {
        if copy_to_clipboard(&text).await.is_ok() {
            toast.success(
                format!("Copied {label} to clipboard"),
                ToastOptions::new(),
            );
        } else {
            toast.error("Failed to copy".to_string(), ToastOptions::new());
        }
    });
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
    use crate::utils::nips::dip03;
    // DIP-03: classify the embedded zap request (kind 9734) from `description`.
    let zap_request_event = dip03::parse_description_event(&event);
    let anon_kind = zap_request_event
        .as_ref()
        .map(dip03::classify_anon)
        .unwrap_or(dip03::AnonKind::None);
    let is_private_zap = matches!(anon_kind, dip03::AnonKind::Private(_));
    let is_anonymous_zap = matches!(anon_kind, dip03::AnonKind::Anonymous);

    let private_zap_resolved = use_signal(|| None::<dip03::DecryptedPrivateZap>);
    let private_zap_failed = use_signal(|| false);
    {
        let zap_request_for_decrypt = zap_request_event.clone();
        let mut resolved_sig = private_zap_resolved;
        let mut failed_sig = private_zap_failed;
        use_effect(move || {
            // Only the Received tab resolves senders; the Sent tab identity is
            // the public `p` tag recipient.
            if show_recipient {
                return;
            }
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
                    Ok(decrypted) => resolved_sig.set(Some(decrypted)),
                    Err(e) => {
                        log::warn!("Failed to decrypt private zap: {}", e);
                        failed_sig.set(true);
                    }
                }
            });
        });
    }

    let linked_pubkey = if show_recipient {
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
    // For received anon/private zaps the `P` tag / description pubkey belong
    // to an ephemeral key — only a successful DIP-03 decrypt reveals the
    // sender identity.
    let identity_pubkey = if show_recipient || (!is_private_zap && !is_anonymous_zap) {
        linked_pubkey
    } else {
        private_zap_resolved.read().as_ref().map(|d| d.sender_pubkey)
    };
    let private_pending = !show_recipient
        && is_private_zap
        && private_zap_resolved.read().is_none()
        && !*private_zap_failed.read();

    // Public zap message: the `content` of the embedded zap request (not the
    // raw description JSON blob).
    let public_message: Option<String> = event.tags.iter().find_map(|tag| {
        let slice = tag.as_slice();
        if slice.first().map(|s| s.as_str()) == Some("description") && slice.len() > 1 {
            let description = slice[1].as_str();
            let content = serde_json::from_str::<serde_json::Value>(description)
                .ok()
                .and_then(|value| {
                    value.get("content").and_then(|c| c.as_str()).map(String::from)
                });
            // Malformed JSON: only surface non-JSON strings as-is.
            content.or_else(|| {
                (!description.trim_start_matches('{').starts_with('{'))
                    .then(|| description.trim().to_string())
                    .filter(|s| !s.is_empty())
            })
        } else {
            None
        }
    });
    let zap_message: Option<String> = if !show_recipient {
        if let Some(decrypted) = private_zap_resolved.read().as_ref() {
            decrypted.message.clone()
        } else if is_private_zap {
            None // pending/failed private zap: nothing to show
        } else if is_anonymous_zap {
            public_message.or_else(|| {
                let content = event.content.trim();
                (!content.is_empty()).then(|| content.to_string())
            })
        } else {
            public_message
        }
    } else {
        public_message
    };
    let zap_amount = crate::services::aggregation::extract_zap_amount(&event);
    let profile_sig = use_signal(|| None::<nostr_sdk::Metadata>);
    let profile_hex = identity_pubkey.as_ref().map(|pk| pk.to_hex());
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
            if private_pending {
                "Private zap".to_string()
            } else if !show_recipient && (is_anonymous_zap || is_private_zap) {
                "Anonymous".to_string()
            } else {
                profile_hex.as_deref().map(|h| {
                    if h.len() > 12 {
                        format!("{}...{}", &h[..8], &h[h.len()-4..])
                    } else { h.to_string() }
                }).unwrap_or_else(|| "Anonymous".to_string())
            }
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
                    if !show_recipient && is_private_zap {
                        span {
                            class: "inline-flex items-center gap-1 text-xs px-1.5 py-0.5 rounded bg-accent text-muted-foreground shrink-0",
                            crate::components::icons::LockIcon { class: "w-3 h-3".to_string() }
                            if private_pending { "Decrypting..." } else { "Private" }
                        }
                    }
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

/// Race the indexer fast path against the outbox (author write relays) for a
/// profile's kind 0. First `Ok(Some)` wins; if the winner returns `None`/`Err`
/// the remaining future is awaited as a fallback. Returns the metadata
/// together with the source event's `created_at` for strictly-newer
/// replacement decisions (kind 0 is replaceable; arrival order is not
/// freshness).
async fn race_profile_metadata(
    hex_pubkey: &str,
) -> std::result::Result<Option<(nostr_sdk::Metadata, u64)>, String> {
    use futures::future::{select, Either};
    use futures::pin_mut;
    let indexer_fut = async {
        // Cold start: the indexers may still be handshaking. Wait (bounded)
        // so the fast path isn't forfeited to the "no indexer connected yet"
        // error — the outbox leg keeps racing in parallel, so this gate
        // never delays a profile the outbox can find first (issue #374).
        if let Some(client) = nostr_client::get_client() {
            let _ = crate::stores::relay::nip65::wait_for_indexer_connected(
                &client,
                Duration::from_secs(3),
            )
            .await;
        }
        nostr_client::fetch_metadata_from_indexers(hex_pubkey, Duration::from_secs(5)).await
    };
    let outbox_fut = nostr_client::fetch_metadata_targeted(hex_pubkey, Duration::from_secs(8));
    pin_mut!(indexer_fut, outbox_fut);
    match select(indexer_fut, outbox_fut).await {
        Either::Left((indexer_res, remaining_outbox)) => match indexer_res {
            Ok(found @ Some(_)) => Ok(found),
            _ => remaining_outbox.await,
        },
        Either::Right((outbox_res, remaining_indexer)) => match outbox_res {
            Ok(found @ Some(_)) => Ok(found),
            _ => remaining_indexer.await,
        },
    }
}
