use crate::components::icons;
use crate::components::nests::{NestCard, NestEndedCompactCard};
use crate::hooks::use_relay_subscription;
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
/// Live bucket by participant count (Phase 3.6, matching the reference
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
    // once on mount. `follows` is used as the Live-bucket sort boost; `blocked`
    // drives the mute/block filter (reference filter axes).
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
        // Signer/relay-readiness gate (matches routes/home/mod.rs:850,856 and
        // the established pattern): for authenticated users, wait until the
        // user's NIP-65 relay list has been applied to the pool so the backfill
        // queries the full relay set (user + defaults). Logged-out users
        // proceed immediately on DEFAULT_RELAYS. `get_pubkey()` is NOT a
        // readiness signal (it's set synchronously from localStorage).
        let has_signer = *nostr_client::HAS_SIGNER.read();
        let relays_applied = *crate::stores::relay::USER_RELAYS_APPLIED.read();
        if has_signer && !relays_applied {
            return;
        }
        spawn(async move {
            // Belt-and-suspenders: also wait inside the spawn in case the
            // signal flipped between the gate check and the fetch.
            crate::stores::relay::wait_for_user_relays(
                std::time::Duration::from_secs(5),
                "nests fetch",
            )
            .await;
            loading.set(true);
            let now_secs = crate::platform::timestamp::now_secs();
            let filter = nostr_sdk::Filter::new()
                .kind(nostr_sdk::Kind::Custom(30312))
                .limit(300)
                .since(Timestamp::from(now_secs.saturating_sub(7 * 24 * 60 * 60)));
            match nostr_client::fetch_nest_events(
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
                                // Feed-filter gate (rule 1): drop
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
                    let deduped = dedup_by_coordinate(parsed);
                    spaces.set(deduped);
                }
                Err(e) => {
                    log::error!("Failed to fetch meeting spaces: {}", e);
                }
            }
            loading.set(false);
        });
    });

    // Live-tail subscription for kind 30312 Meeting Spaces. `subscribe(None)`
    // stays open forever (subscribe_long_lived in nostr-sdk): relays deliver
    // stored events matching `since`/`limit` up to EOSE, then keep streaming
    // new and updated rooms. This is what makes the list grow over time,
    // via a never-closing global REQ. The overlap
    // with the initial backfill is handled by the SDK's EventId dedup plus the
    // coordinate dedup in the callback below.
    {
        let rooms_filter = use_memo(|| {
            let now = crate::platform::timestamp::now_secs();
            Some(
                Filter::new()
                    .kind(Kind::Custom(30312))
                    .limit(300)
                    .since(Timestamp::from(now.saturating_sub(7 * 24 * 60 * 60))),
            )
        });
        use_relay_subscription(rooms_filter(), move |event: &nostr::Event| {
            if event.kind.as_u16() != 30312 {
                return;
            }
            let Ok(space) = parse_meeting_space(event) else {
                return;
            };
            if !is_joinable(&space) {
                return;
            }
            let now = crate::platform::timestamp::now_secs();
            if !is_within_planned_window(&space, now) {
                return;
            }
            if !is_within_ended_window(&space, now) {
                return;
            }
            // Addressable coordinate dedup: skip if we already hold this
            // version or a newer one. Avoids a write (and re-render) for the
            // historical re-delivery that overlaps the initial backfill.
            {
                let current = spaces.read();
                if let Some(existing) = current.iter().find(|s| s.coordinate == space.coordinate) {
                    if existing.created_at >= space.created_at {
                        return;
                    }
                }
            }
            let mut current = spaces.write();
            if let Some(existing) =
                current.iter_mut().find(|s| s.coordinate == space.coordinate)
            {
                if space.created_at > existing.created_at {
                    *existing = space;
                }
            } else {
                current.push(space);
            }
        });
    }

    // Presence (kind 10312) receiving via 30s polling (a `kinds:[10312]`,
    // `since: now-10min`, `limit: 500` one-shot query). A single static
    // long-lived subscription proved unreliable here: the `limit=500`
    // historical response truncates a low-volume room's heartbeats out on busy
    // relays, without 30s refetching or forward-moving
    // `since` assembler) we have no recovery mechanism. Polling makes each tick
    // an independent EOSE-closing `fetch_events`, so a missed/truncated
    // heartbeat self-corrects on the next tick and `presence_details`
    // reliably populates — which is what lets `nest_effective_status` flip the
    // room to Live.
    //
    // `wait_for_user_relays` at the top of each tick self-gates: no-op when
    // `!HAS_SIGNER` (logged-out proceeds on DEFAULT_RELAYS), blocks only
    // authenticated users until their NIP-65 list is applied.
    {
        let mut presence_poll_task: Signal<Option<dioxus_core::Task>> =
            use_signal(|| None);
        let mut task_slot = presence_poll_task;
        // Cancel the polling loop when NestsHome unmounts.
        use_drop(move || {
            if let Some(t) = task_slot.write().take() {
                t.cancel();
            }
        });
        use_hook(move || {
            let task = spawn(async move {
                loop {
                    crate::stores::relay::wait_for_user_relays(
                        std::time::Duration::from_secs(5),
                        "nests presence poll",
                    )
                    .await;
                    let now = crate::platform::timestamp::now_secs();
                    let filter = nostr_sdk::Filter::new()
                        .kind(nostr_sdk::Kind::Custom(10312))
                        .limit(500)
                        .since(Timestamp::from(now.saturating_sub(10 * 60)));
                    // Fetch first, collect updates, THEN write — never hold the
                    // `presence_details` write lock across the network await.
                    let updates: Vec<(String, String, u64)> =
                        match crate::stores::nostr_client::get_client() {
                            Some(client) => {
                                match client.fetch_events(filter, std::time::Duration::from_secs(15)).await {
                                    Ok(events) => events
                                        .into_iter()
                                        .filter_map(|event| {
                                            if event.kind.as_u16() != 10312 {
                                                return None;
                                            }
                                            let coord = event
                                                .tags
                                                .iter()
                                                .find(|t| {
                                                    t.as_slice().first().map(|s| s.as_str()) == Some("a")
                                                })
                                                .and_then(|t| t.as_slice().get(1).cloned())?;
                                            Some((coord, event.pubkey.to_hex(), event.created_at.as_secs()))
                                        })
                                        .collect(),
                                    Err(e) => {
                                        log::warn!("Nest presence poll failed: {}", e);
                                        Vec::new()
                                    }
                                }
                            }
                            None => Vec::new(),
                        };
                    if !updates.is_empty() {
                        let mut details = presence_details.write();
                        for (coord, pk, ts) in updates {
                            let room_map = details.entry(coord).or_default();
                            let entry = room_map.entry(pk).or_insert(0);
                            if ts > *entry {
                                *entry = ts;
                            }
                        }
                    }
                    crate::platform::timer::sleep_ms(30_000).await;
                }
            });
            presence_poll_task.set(Some(task));
        });
    }

    let mut live_rooms: Vec<(MeetingSpace, LiveStatus, Option<u64>, u32)> = Vec::new();
    let mut scheduled_rooms: Vec<(MeetingSpace, LiveStatus, Option<u64>, u32)> = Vec::new();
    let mut ended_rooms: Vec<(MeetingSpace, LiveStatus, Option<u64>, u32)> = Vec::new();
    {
        let current_spaces = spaces.read();
        let details = presence_details.read();
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
            // Global feed behavior (reference Global-tab semantics):
            // no follow-expansion filter — all joinable rooms are shown. The
            // follow graph is still used as a sort boost below. Rooms hosted by
            // blocked users are dropped above.
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
    // created_at DESC (reference sort) for the
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
    // the end (reference scheduled-bucket ordering).
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
/// Follows-participating count for the room header.
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

/// Dedup addressable kind 30312 rooms by coordinate, keeping the newest
/// version (highest `created_at`). 30312 is addressable, so a single room may
/// be republished over time (e.g. status open→closed) producing multiple event
/// ids. The SDK's shared database dedups across relays but is ordering-
/// dependent, so collapse final duplicates here. Returns rooms sorted by
/// `created_at` descending.
fn dedup_by_coordinate(rooms: Vec<MeetingSpace>) -> Vec<MeetingSpace> {
    let mut latest: HashMap<String, MeetingSpace> = HashMap::new();
    for room in rooms {
        let keep = match latest.get(&room.coordinate) {
            Some(existing) => existing.created_at < room.created_at,
            None => true,
        };
        if keep {
            latest.insert(room.coordinate.clone(), room);
        }
    }
    let mut out: Vec<MeetingSpace> = latest.into_values().collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    out
}
