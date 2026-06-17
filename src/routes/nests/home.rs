use crate::components::icons;
use crate::components::nests::{NestCard, NestEndedCompactCard};
use crate::routes::Route;
use crate::stores::nostr_client::{self, CLIENT_INITIALIZED};
use crate::utils::nips::nip53::{
    is_joinable, is_within_ended_window, is_within_planned_window, nest_effective_status,
    parse_meeting_space, LiveStatus, MeetingSpace,
};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::collections::HashMap;

/// Per-room presence summary derived from kind 10312 events. Used to sort the
/// Live bucket by participant count (Phase 3.6, matching Amethyst's
/// `NestsFeedFilter.sort` and the reference impl's presence-based liveness).
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
struct PresenceSummary {
    /// Latest presence event timestamp (unix seconds) for this room.
    last_seen: u64,
    /// Count of unique pubkeys with a presence event within the freshness
    /// window (`PRESENCE_LIVE_THRESHOLD_SECS` = 300s).
    participant_count: u32,
}

#[component]
pub fn NestsHome() -> Element {
    let mut spaces = use_signal(Vec::<MeetingSpace>::new);
    let mut loading = use_signal(|| true);
    // Per-room participant tracking: `room_coordinate → (pubkey → last_seen_secs)`.
    // Used to derive `PresenceSummary` on each render.
    let mut presence_details = use_signal(HashMap::<String, HashMap<String, u64>>::new);
    // Local user's follow graph (kind 3 contacts) and block list, loaded
    // once on mount. Used for the follows-boost sort key, p-tag follow
    // expansion, and mute/block filter — mirrors Amethyst's
    // `NestsFeedFilter` feed-filter axes.
    let mut follows = use_signal(std::collections::HashSet::<String>::new);
    let mut blocked = use_signal(std::collections::HashSet::<String>::new);

    // Fetch follows + blocked lists on mount. Both are cached in
    // `nostr_client` (5min for contacts, similar for mute/block), so the
    // first NestsHome visit after login is the only network round-trip.
    use_effect(move || {
        let client_initialized = *CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            if let Some(pk) = crate::stores::auth_store::get_pubkey() {
                if let Ok(contacts) = nostr_client::fetch_contacts(pk).await {
                    follows.set(contacts.into_iter().collect());
                }
                if let Ok(blocked_list) = nostr_client::get_blocked_users().await {
                    blocked.set(blocked_list.into_iter().collect());
                }
            }
        });
    });

    use_effect(move || {
        let client_initialized = *CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            let filter = nostr_sdk::Filter::new()
                .kind(nostr_sdk::Kind::Custom(30312))
                .limit(100);
            match nostr_client::fetch_events_aggregated(
                filter,
                std::time::Duration::from_secs(15),
            )
            .await
            {
                Ok(events) => {
                    let now = crate::platform::timestamp::now_secs();
                    let mut parsed = Vec::new();
                    for event in events {
                        match parse_meeting_space(&event) {
                            Ok(space) => {
                                // Feed-filter gate (Amethyst's EGG-01): drop
                                // rooms missing required fields or with
                                // non-HTTPS service/endpoint URLs.
                                if !is_joinable(&space) {
                                    continue;
                                }
                                // Drop planned rooms outside the
                                // accepted window and ended rooms older
                                // than 7 days.
                                if !is_within_planned_window(&space, now) {
                                    continue;
                                }
                                if !is_within_ended_window(&space, now) {
                                    continue;
                                }
                                parsed.push(space);
                            }
                            Err(e) => {
                                log::warn!("Failed to parse meeting space: {}", e);
                            }
                        }
                    }
                    parsed.sort_by_key(|b| std::cmp::Reverse(b.created_at));
                    spaces.set(parsed);
                }
                Err(e) => {
                    log::error!("Failed to fetch meeting spaces: {}", e);
                }
            }
            loading.set(false);
        });
    });

    {
        let mut sub_handle: Signal<Option<crate::stores::notification_dispatcher::DispatcherHandle>> =
            use_signal(|| None);
        let mut sub_fallback_id: Signal<Option<nostr_sdk::SubscriptionId>> =
            use_signal(|| None);

        use_effect(move || {
            let current_spaces = spaces.read();
            let coordinates: Vec<String> = current_spaces
                .iter()
                .map(|s| s.coordinate.clone())
                .collect();
            drop(current_spaces);
            let filter = if coordinates.is_empty() {
                return;
            } else {
                nostr_sdk::Filter::new()
                    .kind(nostr_sdk::Kind::Custom(10312))
                    .custom_tags(
                        SingleLetterTag::lowercase(Alphabet::A),
                        coordinates,
                    )
                    .limit(0)
            };
            spawn(async move {
                if let Some(handle) = sub_handle.write().take() {
                    handle.unregister().await;
                }
                if let Some(sid) = sub_fallback_id.write().take() {
                    if let Some(client) = crate::stores::nostr_client::get_client() {
                        let _ = client.unsubscribe(&sid).await;
                    }
                }
                let client = match crate::stores::nostr_client::get_client() {
                    Some(c) => c,
                    None => return,
                };
                match client.subscribe(filter, None).await {
                    Ok(output) => {
                        let sub_id = output.val;
                        if let Some((handle, mut rx)) =
                            crate::stores::notification_dispatcher::DispatcherHandle::create(
                                sub_id.clone(),
                            )
                        {
                            sub_handle.set(Some(handle));
                            spawn(async move {
                                let mut buffer = Vec::new();
                                while let Some(event) = rx.recv().await {
                                    buffer.push(event);
                                    while let Ok(event) = rx.try_recv() {
                                        buffer.push(event);
                                    }
                                    for event in &buffer {
                                        if event.kind.as_u16() == 10312 {
                                            let coordinate = event
                                                .tags
                                                .iter()
                                                .find(|t| {
                                                    t.as_slice()
                                                        .first()
                                                        .map(|s| s.as_str())
                                                        == Some("a")
                                                })
                                                .and_then(|t| t.as_slice().get(1).cloned());
                                            if let Some(coord) = coordinate {
                                                let ts = event.created_at.as_secs();
                                                let pk = event.pubkey.to_hex();
                                                let mut details = presence_details.write();
                                                let room_map = details.entry(coord).or_default();
                                                let entry = room_map.entry(pk).or_insert(0);
                                                if ts > *entry {
                                                    *entry = ts;
                                                }
                                            }
                                        }
                                    }
                                    buffer.clear();
                                }
                            });
                        } else {
                            sub_fallback_id.set(Some(sub_id));
                        }
                    }
                    Err(e) => {
                        log::error!("presence subscription failed: {}", e);
                    }
                }
            });
        });
    }

    let mut live_rooms: Vec<(MeetingSpace, LiveStatus, Option<u64>, u32)> = Vec::new();
    let mut scheduled_rooms: Vec<(MeetingSpace, LiveStatus, Option<u64>, u32)> = Vec::new();
    let mut ended_rooms: Vec<(MeetingSpace, LiveStatus, Option<u64>, u32)> = Vec::new();
    {
        let current_spaces = spaces.read();
        let details = presence_details.read();
        let follows_set = follows.read();
        let blocked_set = blocked.read();
        let now = crate::platform::timestamp::now_secs();
        // Derive PresenceSummary per room from the per-pubkey tracking.
        for space in current_spaces.iter() {
            // Mute/block filter (Phase 2.8): drop rooms hosted by a blocked
            // user. Mirrors NestsUI-v2's `mutedPubkeys` filter.
            if blocked_set.contains(&space.pubkey) {
                continue;
            }
            let room_details = details.get(&space.coordinate);
            let last_presence = room_details
                .and_then(|m| m.values().max().copied());
            // Count unique pubkeys with presence within the freshness window.
            let participant_count = room_details
                .map(|m| {
                    m.values()
                        .filter(|&&t| now.saturating_sub(t) < 300)
                        .count() as u32
                })
                .unwrap_or(0);
            let status = nest_effective_status(
                space.status,
                last_presence,
                space.created_at,
            );
            // p-tag follow expansion (Phase 2.9): keep rooms whose host is
            // followed OR any p-tagged provider is followed, even when the
            // host isn't. Matches Amethyst's
            // `NestsFeedFilter.followsAuthorsForExpansion`.
            let host_followed = follows_set.contains(&space.pubkey);
            let any_provider_followed = space
                .providers
                .iter()
                .any(|p| follows_set.contains(&p.pubkey));
            if !host_followed && !any_provider_followed && !follows_set.is_empty() {
                // No relationship to the user and follows is populated — skip.
                // When follows is empty (logged-out or fresh account), show all.
                continue;
            }
            match status {
                LiveStatus::Live => live_rooms.push((space.clone(), status, last_presence, participant_count)),
                LiveStatus::Planned => {
                    scheduled_rooms.push((space.clone(), status, last_presence, participant_count))
                }
                LiveStatus::Ended => ended_rooms.push((space.clone(), status, last_presence, participant_count)),
            }
        }
    }

    // Sort Live bucket: follows-participating DESC, total participants DESC,
    // created_at DESC. Matches Amethyst's `NestsFeedFilter.sort` for the
    // LIVE bucket — rooms where the user's follows are participating rank
    // highest, then by total participant count, then recency.
    {
        let follows_set = follows.read();
        live_rooms.sort_by(|a, b| {
            let a_follows = follows_participating(&a.0, &follows_set);
            let b_follows = follows_participating(&b.0, &follows_set);
            b_follows.cmp(&a_follows) // follows-participating desc
                .then_with(|| b.3.cmp(&a.3)) // total participants desc
                .then_with(|| b.0.created_at.cmp(&a.0.created_at)) // created_at desc
        });
    }
    // Scheduled: starts ASC (soonest first); rooms without `starts` fall to
    // the end. Mirrors Amethyst's SCHEDULED bucket ordering.
    scheduled_rooms.sort_by(|a, b| {
        let a_starts = a.0.starts.unwrap_or(u64::MAX);
        let b_starts = b.0.starts.unwrap_or(u64::MAX);
        a_starts.cmp(&b_starts)
    });
    // Ended: created_at DESC (most recently ended first; the 30312's
    // created_at advances when the host republishes with status=closed).
    ended_rooms.sort_by_key(|b| std::cmp::Reverse(b.0.created_at));

    let has_live = !live_rooms.is_empty();
    let has_scheduled = !scheduled_rooms.is_empty();
    let has_ended = !ended_rooms.is_empty();
    let is_empty = !has_live && !has_scheduled && !has_ended;

    rsx! {
        div { class: "min-h-screen pb-20",
            div { class: "sticky top-0 z-30 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3",
                    h1 { class: "text-xl font-bold", "Nests" }
                    p { class: "text-sm text-muted-foreground", "Live audio rooms" }
                }
            }

            if *loading.read() {
                div { class: "p-4 space-y-4",
                    for _ in 0..6 {
                        div { class: "bg-card border border-border rounded-xl overflow-hidden animate-pulse",
                            div { class: "aspect-video bg-muted" }
                            div { class: "p-3 space-y-2",
                                div { class: "h-4 bg-muted rounded w-3/4" }
                                div { class: "h-3 bg-muted rounded w-1/2" }
                            }
                        }
                    }
                }
            } else {
                div { class: "divide-y divide-border",
                    {if has_live {
                        rsx! {
                            div {
                                div { class: "sticky top-[60px] z-20 bg-background/90 backdrop-blur-sm px-4 py-2 border-b border-border",
                                    h2 { class: "text-sm font-bold text-red-500 uppercase tracking-wider",
                                        "Live Now"
                                    }
                                }
                                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4 p-4",
                                    for (space, status, _last_presence, _count) in &live_rooms {
                                        NestCard {
                                            key: "{space.coordinate}",
                                            space: space.clone(),
                                            display_status: *status,
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }}

                    {if has_scheduled {
                        rsx! {
                            div {
                                div { class: "sticky top-[60px] z-20 bg-background/90 backdrop-blur-sm px-4 py-2 border-b border-border",
                                    h2 { class: "text-sm font-bold text-blue-500 uppercase tracking-wider",
                                        "Scheduled"
                                    }
                                }
                                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4 p-4",
                                    for (space, status, _, _) in &scheduled_rooms {
                                        NestCard {
                                            key: "{space.coordinate}",
                                            space: space.clone(),
                                            display_status: *status,
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }}

                    {if has_ended {
                        rsx! {
                            div {
                                div { class: "sticky top-[60px] z-20 bg-background/90 backdrop-blur-sm px-4 py-2 border-b border-border",
                                    h2 { class: "text-sm font-bold text-muted-foreground uppercase tracking-wider",
                                        "Recently Ended"
                                    }
                                }
                                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-3 p-4",
                                    for (space, _status, _, _) in &ended_rooms {
                                        NestEndedCompactCard {
                                            key: "{space.coordinate}",
                                            space: space.clone(),
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }}

                    if is_empty {
                        div { class: "flex flex-col items-center justify-center py-20 text-muted-foreground",
                            icons::RadioIcon {
                                class: "w-16 h-16 mb-4 opacity-30".to_string(),
                            }
                            h3 { class: "text-lg font-medium", "No nests found" }
                            p { class: "text-sm mt-1", "Create one to get started!" }
                        }
                    }
                }
            }

            div { class: "fixed bottom-20 right-4 lg:bottom-6 lg:right-6 z-40 flex flex-col gap-3",
                Link {
                    to: Route::NestServers {},
                    class: "w-12 h-12 bg-muted hover:bg-accent text-muted-foreground rounded-full shadow-lg flex items-center justify-center transition",
                    span {
                        dangerous_inner_html: icons::SETTINGS,
                    }
                }
                Link {
                    to: Route::NestCreate { naddr: None },
                    class: "w-14 h-14 bg-blue-500 hover:bg-blue-600 text-white rounded-full shadow-lg flex items-center justify-center transition",
                    span {
                        dangerous_inner_html: icons::PLUS,
                    }
                }
            }
        }
    }
}

/// Count how many of a room's host + p-tagged providers are in the user's
/// follow set. Used as the primary sort key for the Live bucket — rooms
/// where the user's follows are participating rank highest. Mirrors
/// Amethyst's `ParticipantListBuilder.countFollowsThatParticipateOn`.
fn follows_participating(
    space: &MeetingSpace,
    follows: &std::collections::HashSet<String>,
) -> u32 {
    let mut count = 0u32;
    if follows.contains(&space.pubkey) {
        count += 1;
    }
    for provider in &space.providers {
        if follows.contains(&provider.pubkey) {
            count += 1;
        }
    }
    count
}
