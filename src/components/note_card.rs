use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, PublicKey, Filter, Kind, ToBech32, Timestamp};
use crate::routes::Route;
use crate::stores::nostr_client::{self, HAS_SIGNER, get_client, publish_repost};
use crate::hooks::use_reaction;
use crate::stores::bookmarks;
use crate::stores::signer::SIGNER_INFO;
use crate::services::aggregation::InteractionCounts;
use crate::components::{RichContent, ReplyComposer, ZapModal, NoteMenu, ReactionButton};
use crate::components::icons::{MessageCircleIcon, Repeat2Icon, BookmarkIcon, ZapIcon, ShareIcon};
use crate::utils::format_sats_compact;
use std::time::Duration;

#[component]
pub fn NoteCard(
    event: NostrEvent,
    #[props(default = None)] repost_info: Option<(PublicKey, Timestamp)>,
    #[props(default = None)] precomputed_counts: Option<InteractionCounts>,
    #[props(default = true)] collapsible: bool,
) -> Element {
    // Clone values that will be used in multiple closures
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_repost = author_pubkey.clone();
    let author_pubkey_like = author_pubkey.clone();
    let author_pubkey_for_fetch = author_pubkey.clone();
    let content = event.content.clone();
    let created_at = event.created_at;
    let event_id = event.id.to_string();
    let event_id_repost = event_id.clone();
    let event_id_like = event_id.clone();
    let event_id_bookmark = event_id.clone();
    let event_id_memo = event_id.clone();
    let event_id_counts = event_id.clone();

    // State for interactions
    let mut is_reposting = use_signal(|| false);
    let mut is_reposted = use_signal(|| false);
    let mut is_zapped = use_signal(|| false);
    let mut show_reply_modal = use_signal(|| false);
    let mut show_zap_modal = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    // Read bookmark state reactively - will update when store changes
    let is_bookmarked = bookmarks::is_bookmarked(&event_id_memo);
    let has_signer = *HAS_SIGNER.read();

    // State for muted/blocked content
    let mut is_muted = use_signal(|| false);
    let mut is_author_blocked = use_signal(|| false);
    let mut show_hidden_anyway = use_signal(|| false);

    // State for counts (likes handled by use_reaction hook)
    let mut reply_count = use_signal(|| 0usize);
    let mut repost_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);

    // Reaction hook - handles like state with optimistic updates and toggle support
    let reaction = use_reaction(
        event_id_like.clone(),
        author_pubkey_like.clone(),
        precomputed_counts.as_ref().map(|c| c.likes),
        None, // Will fetch is_liked state
    );

    // State for author profile
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // State for reposter profile (if this is a repost)
    let mut reposter_metadata = use_signal(|| None::<nostr_sdk::Metadata>);

    // Initialize counts from precomputed data if available (batch optimization)
    // Note: likes are handled by use_reaction hook
    use_effect(use_reactive(&precomputed_counts, move |counts_opt| {
        if let Some(counts) = counts_opt {
            // CRITICAL FIX: Don't overwrite non-zero counts with zeros from batch fetch
            // This prevents batch fetch from clearing reactions that individual fetch already found
            // Only update if new counts are non-zero OR if current counts are still at initial zero state
            let current_has_data = {
                let reply = *reply_count.peek();
                let repost = *repost_count.peek();
                let zap = *zap_amount_sats.peek();
                reply > 0 || repost > 0 || zap > 0
            };

            let new_has_data = counts.replies > 0 || counts.reposts > 0 || counts.zap_amount_sats > 0;

            // Only update if: new data exists, OR no current data exists
            if new_has_data || !current_has_data {
                reply_count.set(counts.replies.min(500));
                repost_count.set(counts.reposts.min(500));
                zap_amount_sats.set(counts.zap_amount_sats);
            }
        }
    }));

    // Fetch counts individually if not precomputed (fallback for single-note views)
    // Always fetch to get per-user interaction state, but only update counts if !has_precomputed
    // Note: Reactions/likes are handled by use_reaction hook
    // Note: Only depend on event_id, NOT precomputed_counts - to avoid re-running when batch data arrives
    let precomputed_for_fetch = precomputed_counts.clone();
    use_effect(use_reactive(&event_id_counts, move |event_id_for_counts| {
        let precomputed = precomputed_for_fetch.clone();
        spawn(async move {
            // Only consider precomputed if it actually has data (not just zeros from batch init)
            let has_precomputed = precomputed.as_ref().map_or(false, |c|
                c.replies > 0 || c.reposts > 0 || c.zap_amount_sats > 0
            );
            let client = match get_client() {
                Some(c) => c,
                None => return,
            };

            let event_id_parsed = match nostr_sdk::EventId::from_hex(&event_id_for_counts) {
                Ok(id) => id,
                Err(_) => return,
            };

            // Create a combined filter for interaction kinds (replies, reposts, zaps)
            // Note: Reactions are handled by use_reaction hook
            let combined_filter = Filter::new()
                .kinds(vec![
                    Kind::TextNote,      // kind 1 - replies
                    Kind::Repost,        // kind 6 - reposts
                    Kind::from(9735),    // kind 9735 - zaps
                ])
                .event(event_id_parsed)
                .limit(2000);

            // Single fetch for interaction types (excluding reactions - handled by hook)
            if let Ok(events) = client.fetch_events(combined_filter, Duration::from_secs(5)).await {
                // Get current user's pubkey to check if they've already reacted
                let current_user_pubkey = SIGNER_INFO.read().as_ref().map(|info| info.public_key.clone());

                // Partition events by kind
                let mut replies = 0;
                let mut reposts = 0;
                let mut total_sats = 0u64;
                let mut user_has_reposted = false;
                let mut user_has_zapped = false;

                for event in events {
                    match event.kind {
                        Kind::TextNote => replies += 1,
                        Kind::Repost => {
                            reposts += 1;
                            // Check if this repost is from the current user
                            if let Some(ref user_pk) = current_user_pubkey {
                                if event.pubkey.to_string() == *user_pk {
                                    user_has_reposted = true;
                                }
                            }
                        },
                        kind if kind == Kind::from(9735) => {
                            // Check if this zap is from the current user
                            // Per NIP-57: The uppercase P tag contains the pubkey of the zap sender
                            if let Some(ref user_pk) = current_user_pubkey {
                                // Method 1: Try to get sender from uppercase "P" tag (most common)
                                // Use as_slice for zero-copy access
                                let mut zap_sender_pubkey = event.tags.iter().find_map(|tag| {
                                    let slice = tag.as_slice();
                                    if slice.len() >= 2 && slice.first()?.as_str() == "P" {
                                        Some(slice.get(1)?.as_str().to_string())
                                    } else {
                                        None
                                    }
                                });

                                // Method 2: Fallback - parse description tag (contains zap request JSON)
                                // Use as_slice for zero-copy access
                                if zap_sender_pubkey.is_none() {
                                    zap_sender_pubkey = event.tags.iter().find_map(|tag| {
                                        let slice = tag.as_slice();
                                        if slice.first()?.as_str() == "description" {
                                            let zap_request_json = slice.get(1)?.as_str();
                                            if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(zap_request_json) {
                                                // The pubkey field in the zap request is the sender
                                                return zap_request.get("pubkey")
                                                    .and_then(|p| p.as_str())
                                                    .map(|s| s.to_string());
                                            }
                                        }
                                        None
                                    });
                                }

                                if let Some(zap_sender) = zap_sender_pubkey {
                                    if zap_sender == *user_pk {
                                        user_has_zapped = true;
                                    }
                                }
                            }

                            // Calculate zap amount (use as_slice for zero-copy access)
                            if let Some(amount) = event.tags.iter().find_map(|tag| {
                                let slice = tag.as_slice();
                                if slice.first()?.as_str() == "description" {
                                    // Parse the JSON zap request
                                    let zap_request_json = slice.get(1)?.as_str();
                                    if let Ok(zap_request) = serde_json::from_str::<serde_json::Value>(zap_request_json) {
                                        // Find the amount tag in the zap request
                                        if let Some(tags) = zap_request.get("tags").and_then(|t| t.as_array()) {
                                            for tag_array in tags {
                                                if let Some(tag_vals) = tag_array.as_array() {
                                                    if tag_vals.first().and_then(|v| v.as_str()) == Some("amount") {
                                                        if let Some(amount_str) = tag_vals.get(1).and_then(|v| v.as_str()) {
                                                            // Amount is in millisats, convert to sats
                                                            if let Ok(millisats) = amount_str.parse::<u64>() {
                                                                return Some(millisats / 1000);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                None
                            }) {
                                total_sats += amount;
                            }
                        }
                        _ => {}
                    }
                }

                // Update counts only if we don't have precomputed data
                // Note: likes are handled by use_reaction hook
                if !has_precomputed {
                    reply_count.set(replies.min(500));
                    repost_count.set(reposts.min(500));
                    zap_amount_sats.set(total_sats);
                }

                // Always update user interaction flags (except is_liked - handled by hook)
                is_reposted.set(user_has_reposted);
                is_zapped.set(user_has_zapped);
            }
        });
    }));

    // Fetch author's profile metadata - only run once per pubkey
    use_effect(use_reactive(&author_pubkey_for_fetch, move |pubkey_str| {
        // Clear old metadata immediately when author changes to prevent stale data
        author_metadata.set(None);

        spawn(async move {
            // Check PROFILE_CACHE first (instant, respects cache clears)
            if let Some(cached_profile) = crate::stores::profiles::get_cached_profile(&pubkey_str) {
                // Convert Profile to nostr_sdk::Metadata
                let mut metadata = nostr_sdk::Metadata::new();
                if let Some(name) = &cached_profile.name {
                    metadata = metadata.name(name);
                }
                if let Some(display_name) = &cached_profile.display_name {
                    metadata = metadata.display_name(display_name);
                }
                if let Some(about) = &cached_profile.about {
                    metadata = metadata.about(about);
                }
                if let Some(picture) = &cached_profile.picture {
                    if let Ok(url) = nostr_sdk::Url::parse(picture) {
                        metadata = metadata.picture(url);
                    }
                }
                if let Some(banner) = &cached_profile.banner {
                    if let Ok(url) = nostr_sdk::Url::parse(banner) {
                        metadata = metadata.banner(url);
                    }
                }
                if let Some(website) = &cached_profile.website {
                    if let Ok(url) = nostr_sdk::Url::parse(website) {
                        metadata = metadata.website(url);
                    }
                }
                if let Some(nip05) = &cached_profile.nip05 {
                    metadata = metadata.nip05(nip05);
                }
                if let Some(lud16) = &cached_profile.lud16 {
                    metadata = metadata.lud16(lud16);
                }

                author_metadata.set(Some(metadata));
                return;
            }

            // Not in cache - fetch using profile system (will populate cache)
            match crate::stores::profiles::fetch_profile(pubkey_str.clone()).await {
                Ok(profile) => {
                    // Convert Profile to Metadata
                    let mut metadata = nostr_sdk::Metadata::new();
                    if let Some(name) = &profile.name {
                        metadata = metadata.name(name);
                    }
                    if let Some(display_name) = &profile.display_name {
                        metadata = metadata.display_name(display_name);
                    }
                    if let Some(about) = &profile.about {
                        metadata = metadata.about(about);
                    }
                    if let Some(picture) = &profile.picture {
                        if let Ok(url) = nostr_sdk::Url::parse(picture) {
                            metadata = metadata.picture(url);
                        }
                    }
                    if let Some(banner) = &profile.banner {
                        if let Ok(url) = nostr_sdk::Url::parse(banner) {
                            metadata = metadata.banner(url);
                        }
                    }
                    if let Some(website) = &profile.website {
                        if let Ok(url) = nostr_sdk::Url::parse(website) {
                            metadata = metadata.website(url);
                        }
                    }
                    if let Some(nip05) = &profile.nip05 {
                        metadata = metadata.nip05(nip05);
                    }
                    if let Some(lud16) = &profile.lud16 {
                        metadata = metadata.lud16(lud16);
                    }

                    author_metadata.set(Some(metadata));
                }
                Err(e) => {
                    log::debug!("Failed to fetch profile for {}: {}", pubkey_str, e);
                }
            }
        });
    }));

    // Fetch reposter's profile metadata if this is a repost
    use_effect(use_reactive(&repost_info, move |info_opt| {
        // Clear old metadata immediately
        reposter_metadata.set(None);

        if let Some((reposter_pubkey, _timestamp)) = info_opt {
            let reposter_pubkey_str = reposter_pubkey.to_string();
            spawn(async move {
                // Check PROFILE_CACHE first
                if let Some(cached_profile) = crate::stores::profiles::get_cached_profile(&reposter_pubkey_str) {
                    let mut metadata = nostr_sdk::Metadata::new();
                    if let Some(name) = &cached_profile.name {
                        metadata = metadata.name(name);
                    }
                    if let Some(display_name) = &cached_profile.display_name {
                        metadata = metadata.display_name(display_name);
                    }
                    if let Some(picture) = &cached_profile.picture {
                        if let Ok(url) = nostr_sdk::Url::parse(picture) {
                            metadata = metadata.picture(url);
                        }
                    }
                    reposter_metadata.set(Some(metadata));
                    return;
                }

                // Not in cache - fetch using profile system
                match crate::stores::profiles::fetch_profile(reposter_pubkey_str.clone()).await {
                    Ok(profile) => {
                        let mut metadata = nostr_sdk::Metadata::new();
                        if let Some(name) = &profile.name {
                            metadata = metadata.name(name);
                        }
                        if let Some(display_name) = &profile.display_name {
                            metadata = metadata.display_name(display_name);
                        }
                        if let Some(picture) = &profile.picture {
                            if let Ok(url) = nostr_sdk::Url::parse(picture) {
                                metadata = metadata.picture(url);
                            }
                        }
                        reposter_metadata.set(Some(metadata));
                    }
                    Err(e) => {
                        log::debug!("Failed to fetch reposter profile: {}", e);
                    }
                }
            });
        }
    }));

    // Check if post is muted or author is blocked
    let event_id_mute_check = event_id.clone();
    let author_pubkey_block_check = author_pubkey.clone();
    use_effect(move || {
        let event_id = event_id_mute_check.clone();
        let author_pubkey = author_pubkey_block_check.clone();
        spawn(async move {
            // Check if post is muted
            if let Ok(muted) = nostr_client::is_post_muted(event_id).await {
                is_muted.set(muted);
            }

            // Check if author is blocked
            if let Ok(blocked) = nostr_client::is_user_blocked(author_pubkey).await {
                is_author_blocked.set(blocked);
            }
        });
    });

    // Format timestamp
    let timestamp = format_timestamp(created_at.as_secs());

    // Get display name and picture from metadata or fallback
    let display_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or(m.name.clone()))
        .unwrap_or_else(|| {
            // Fallback to truncated pubkey
            if author_pubkey.len() > 16 {
                format!("{}...{}", &author_pubkey[..8], &author_pubkey[author_pubkey.len()-8..])
            } else {
                author_pubkey.clone()
            }
        });

    let username = author_metadata.read().as_ref()
        .and_then(|m| m.name.clone())
        .unwrap_or_else(|| {
            // Truncated npub
            if let Ok(pk) = PublicKey::from_hex(&author_pubkey) {
                match pk.to_bech32() {
                    Ok(npub) => {
                        if npub.len() > 18 {
                            format!("{}...{}", &npub[..12], &npub[npub.len()-6..])
                        } else {
                            npub
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to encode pubkey to bech32: {}, using hex fallback", e);
                        // Fallback to hex truncation
                        format!("{}...{}", &author_pubkey[..8], &author_pubkey[author_pubkey.len()-8..])
                    }
                }
            } else {
                "unknown".to_string()
            }
        });

    let profile_picture = author_metadata.read().as_ref()
        .and_then(|m| m.picture.clone());

    // Get reposter info if this is a repost
    let reposter_display_info = repost_info.map(|(reposter_pubkey, repost_timestamp)| {
        let reposter_pubkey_str = reposter_pubkey.to_string();
        let reposter_display = reposter_metadata.read().as_ref()
            .and_then(|m| m.display_name.clone().or_else(|| m.name.clone()))
            .unwrap_or_else(|| format!("{}...{}",
                &reposter_pubkey_str[..8],
                &reposter_pubkey_str[reposter_pubkey_str.len()-8..]
            ));
        let repost_time = format_timestamp(repost_timestamp.as_secs());
        (reposter_pubkey_str, reposter_display, repost_time)
    });

    // Compute dynamic class strings
    let repost_button_class = if *is_reposted.read() {
        "flex items-center text-green-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-green-500 transition"
    };

    let zap_button_class = if *is_zapped.read() {
        "flex items-center gap-1 text-yellow-500 transition px-2 py-1.5 rounded"
    } else {
        "flex items-center gap-1 text-muted-foreground hover:text-yellow-500 hover:bg-yellow-500/10 transition px-2 py-1.5 rounded"
    };

    let bookmark_button_class = if is_bookmarked {
        "flex items-center text-blue-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-blue-500 transition"
    };

    let nav = use_navigator();
    let event_id_nav = event_id.clone();

    // Check if content should be hidden
    let is_hidden = (*is_muted.read() || *is_author_blocked.read()) && !*show_hidden_anyway.read();

    rsx! {
        article {
            class: "border-b border-border p-4 hover:bg-accent/50 transition-colors cursor-pointer",
            onclick: move |_| {
                if !is_hidden {
                    nav.push(Route::Note { note_id: event_id_nav.clone() });
                }
            },

            // Show hidden state if muted or blocked
            if is_hidden {
                div {
                    class: "flex items-center gap-3 py-4",
                    div {
                        class: "flex-1 text-muted-foreground text-sm",
                        if *is_author_blocked.read() {
                            "Post from blocked user"
                        } else if *is_muted.read() {
                            "Muted post"
                        }
                    }
                    button {
                        class: "px-3 py-1 text-sm text-primary hover:underline",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            show_hidden_anyway.set(true);
                        },
                        "Show anyway"
                    }
                }
            } else {
                // Repost indicator (if this is a repost)
                if let Some((reposter_pubkey_str, reposter_display, repost_time)) = &reposter_display_info {
                    div {
                        class: "flex items-center gap-2 text-sm text-muted-foreground mb-2",
                        Repeat2Icon { class: "w-4 h-4" }
                        Link {
                            to: Route::Profile { pubkey: reposter_pubkey_str.clone() },
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            class: "hover:underline font-medium text-muted-foreground",
                            "{reposter_display} reposted"
                        }
                        span {
                            "·"
                        }
                        span {
                            "{repost_time}"
                        }
                    }
                }

                div {
                    class: "flex gap-3",

                    // Avatar
                    div {
                        class: "flex-shrink-0",
                    Link {
                        to: Route::Profile { pubkey: author_pubkey.clone() },
                        onclick: move |e: MouseEvent| e.stop_propagation(),
                        if let Some(picture_url) = &profile_picture {
                            img {
                                class: "w-12 h-12 rounded-full object-cover",
                                src: "{picture_url}",
                                alt: "Profile picture",
                                loading: "lazy"
                            }
                        } else {
                            div {
                                class: "w-12 h-12 rounded-full bg-gradient-to-br from-blue-400 to-purple-500 flex items-center justify-center text-white font-bold text-lg",
                                "{display_name.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_else(|| \"?\".to_string())}"
                            }
                        }
                    }
                }

                // Content
                div {
                    class: "flex-1 min-w-0",

                    // Header
                    div {
                        class: "flex items-start justify-between gap-2 mb-1",
                        div {
                            class: "flex items-center gap-2 flex-wrap",
                            Link {
                                to: Route::Profile { pubkey: author_pubkey.clone() },
                                onclick: move |e: MouseEvent| e.stop_propagation(),
                                class: "font-bold hover:underline",
                                "{display_name}"
                            }
                            span {
                                class: "text-muted-foreground text-sm",
                                "@{username}"
                            }
                            span {
                                class: "text-muted-foreground text-sm",
                                "·"
                            }
                            span {
                                class: "text-muted-foreground text-sm",
                                "{timestamp}"
                            }
                        }
                        // Menu button
                        NoteMenu {
                            author_pubkey: author_pubkey.clone(),
                            event_id: event_id.clone()
                        }
                    }

                    // Post content
                    div {
                        class: "mb-3",
                        RichContent {
                            content: content.clone(),
                            tags: event.tags.iter().cloned().collect(),
                            collapsible: collapsible
                        }
                    }

                    // Action buttons
                    div {
                        class: "flex items-center justify-between max-w-md mt-2 -ml-2",

                        // Reply button
                        button {
                            class: "flex items-center gap-1 hover:text-blue-500 hover:bg-blue-500/10 transition px-2 py-1.5 rounded",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_reply_modal.set(true);
                            },
                            MessageCircleIcon {
                                class: "h-4 w-4".to_string(),
                                filled: false
                            }
                            span {
                                class: "text-xs",
                                {
                                    let count = *reply_count.read();
                                    if count > 500 {
                                        "500+".to_string()
                                    } else if count > 0 {
                                        count.to_string()
                                    } else {
                                        "".to_string()
                                    }
                                }
                            }
                        }

                        // Repost button
                        button {
                            class: "{repost_button_class} hover:bg-green-500/10 gap-1 px-2 py-1.5 rounded",
                            disabled: !has_signer || *is_reposting.read(),
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();

                                if !has_signer || *is_reposting.read() {
                                    return;
                                }

                                let event_id_clone = event_id_repost.clone();
                                let author_pubkey_clone = author_pubkey_repost.clone();

                                is_reposting.set(true);

                                spawn(async move {
                                    match publish_repost(event_id_clone, author_pubkey_clone, None).await {
                                        Ok(repost_id) => {
                                            log::info!("Reposted event, repost ID: {}", repost_id);
                                            is_reposted.set(true);
                                            is_reposting.set(false);
                                        }
                                        Err(e) => {
                                            log::error!("Failed to repost event: {}", e);
                                            is_reposting.set(false);
                                        }
                                    }
                                });
                            },
                            Repeat2Icon {
                                class: "h-4 w-4".to_string(),
                                filled: false
                            }
                            span {
                                class: "text-xs",
                                {
                                    let count = *repost_count.read();
                                    if count > 500 {
                                        "500+".to_string()
                                    } else if count > 0 {
                                        count.to_string()
                                    } else {
                                        "".to_string()
                                    }
                                }
                            }
                        }

                        // Like button with reaction picker
                        ReactionButton {
                            reaction: reaction.clone(),
                            has_signer: has_signer,
                        }

                        // Zap button (only show if author has lightning address)
                        {
                            let has_lightning = author_metadata.read().as_ref()
                                .and_then(|m| m.lud16.as_ref().or(m.lud06.as_ref()))
                                .is_some();

                            if has_lightning {
                                rsx! {
                                    button {
                                        class: "{zap_button_class}",
                                        onclick: move |e: MouseEvent| {
                                            e.stop_propagation();
                                            show_zap_modal.set(true);
                                        },
                                        ZapIcon {
                                            class: "h-4 w-4".to_string(),
                                            filled: *is_zapped.read()
                                        }
                                        span {
                                            class: "text-xs",
                                            {
                                                let amount = *zap_amount_sats.read();
                                                if amount > 0 {
                                                    format_sats_compact(amount)
                                                } else {
                                                    "".to_string()
                                                }
                                            }
                                        }
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }

                        // Bookmark button
                        button {
                            class: "{bookmark_button_class} hover:bg-blue-500/10 px-2 py-1.5 rounded",
                            disabled: !has_signer || *is_bookmarking.read(),
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();

                                if !has_signer || *is_bookmarking.read() {
                                    return;
                                }

                                let event_id_clone = event_id_bookmark.clone();
                                let currently_bookmarked = bookmarks::is_bookmarked(&event_id_clone);

                                is_bookmarking.set(true);

                                spawn(async move {
                                    let result = if currently_bookmarked {
                                        bookmarks::unbookmark_event(event_id_clone).await
                                    } else {
                                        bookmarks::bookmark_event(event_id_clone).await
                                    };

                                    match result {
                                        Ok(_) => {
                                            log::info!("Bookmark toggled successfully");
                                        }
                                        Err(e) => {
                                            log::error!("Failed to toggle bookmark: {}", e);
                                        }
                                    }
                                    is_bookmarking.set(false);
                                });
                            },
                            BookmarkIcon {
                                class: "h-4 w-4".to_string(),
                                filled: is_bookmarked
                            }
                        }

                        // Share button
                        button {
                            class: "flex items-center text-muted-foreground hover:text-blue-500 hover:bg-blue-500/10 px-2 py-1.5 rounded transition",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                // TODO: Implement share
                            },
                            ShareIcon {
                                class: "h-4 w-4".to_string(),
                                filled: false
                            }
                        }
                    }
                }
            }
            }
        }

        // Reply modal
        if *show_reply_modal.read() {
            ReplyComposer {
                reply_to: event.clone(),
                on_close: move |_| {
                    show_reply_modal.set(false);
                },
                on_success: move |_| {
                    show_reply_modal.set(false);
                    // Optionally refresh feed here
                }
            }
        }

        // Zap modal
        if *show_zap_modal.read() {
            ZapModal {
                recipient_pubkey: author_pubkey.clone(),
                recipient_name: display_name.clone(),
                lud16: author_metadata.read().as_ref().and_then(|m| m.lud16.clone()),
                lud06: author_metadata.read().as_ref().and_then(|m| m.lud06.clone()),
                event_id: Some(event_id.clone()),
                on_close: move |_| {
                    show_zap_modal.set(false);
                }
            }
        }
    }
}

// Helper function to format timestamp
fn format_timestamp(unix_timestamp: u64) -> String {
    // Use JavaScript's Date.now() for WASM compatibility
    let now = (js_sys::Date::now() / 1000.0) as u64;

    let diff = now.saturating_sub(unix_timestamp);

    match diff {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{}m", diff / 60),
        3600..=86399 => format!("{}h", diff / 3600),
        86400..=604799 => format!("{}d", diff / 86400),
        _ => format!("{}w", diff / 604800),
    }
}

// Skeleton loader for NoteCard
#[component]
pub fn NoteCardSkeleton() -> Element {
    rsx! {
        div {
            class: "border-b border-gray-200 dark:border-gray-800 p-4 animate-pulse",
            div {
                class: "flex gap-3",

                // Avatar skeleton
                div {
                    class: "w-12 h-12 rounded-full bg-gray-300 dark:bg-gray-700"
                }

                // Content skeleton
                div {
                    class: "flex-1 space-y-2",
                    div {
                        class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-1/4"
                    }
                    div {
                        class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-3/4"
                    }
                    div {
                        class: "h-4 bg-gray-300 dark:bg-gray-700 rounded w-1/2"
                    }
                }
            }
        }
    }
}
