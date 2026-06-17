use crate::components::icons::{ArrowLeftIcon, PhoneCallIcon, RadioIcon};
use crate::components::nests::{
    ActionBar, NestChat, NestHeader, NestReactions, ParticipantActionSheet, ParticipantGallery,
    SpeakerQueue, StageGrid,
};
use crate::components::ConfirmModal;
use crate::hooks::use_relay_subscription_to;
use crate::routes::Route;
use crate::stores::auth_store::get_pubkey;
use crate::stores::nest_room_store::{self, NEST_ROOM};
use crate::stores::nostr_client::{self, CLIENT_INITIALIZED};
use crate::stores::profiles;
use crate::stores::relay::effective_room_relays;
use crate::utils::nip19::parse_naddr;

use crate::utils::nips::nip53::{
    parse_meeting_space, parse_room_presence, rebuild_meeting_space_tags, should_honor_admin_command,
    RoomPresence, RoomStatus,
};
use dioxus::prelude::*;

#[cfg(feature = "mobile_platform")]
use crate::platform::pip;

#[component]
pub fn NestViewer(naddr: String) -> Element {
    let nav = navigator();

    // Initialize the singleton store for this room (idempotent — preserves
    // audio session state across navigation away and back, like MUSIC_PLAYER).
    use_effect(use_reactive(&naddr, move |naddr: String| {
        nest_room_store::ensure_initialized_for(&naddr);
    }));

    let parsed_naddr = use_memo(move || parse_naddr(&naddr).ok());
    let my_pubkey = use_memo(move || get_pubkey().unwrap_or_default());

    let room_author = parsed_naddr
        .read()
        .as_ref()
        .map(|p| p.pubkey.clone())
        .unwrap_or_default();
    let room_d_tag = parsed_naddr
        .read()
        .as_ref()
        .map(|p| p.identifier.clone())
        .unwrap_or_default();

    // Effective relay set for this room (NIP-65 ∪ naddr hints ∪ room relays
    // tag). Reactive — recomputes when the user's relay pool changes or the
    // room event arrives (which populates `room_relays` on NEST_ROOM). All
    // room subscriptions (presence, room updates, admin, chat) target this
    // set so edits and stage promotions on a room-specific relay are
    // received without manual relay addition. Mirrors NestsUI-v2's
    // RoomRelaysProvider.
    let naddr_hints = parsed_naddr
        .read()
        .as_ref()
        .map(|p| p.relay_hints.clone())
        .unwrap_or_default();
    let effective_relays: Memo<Vec<String>> = use_memo(move || {
        let user_relays = crate::stores::relay::user_nip65_relays();
        let room_relays = NEST_ROOM.read().room_relays.clone();
        effective_room_relays(&user_relays, &naddr_hints, &room_relays)
    });

    // Load the room event from relays on mount / when client becomes ready.
    use_effect(use_reactive(
        (&*CLIENT_INITIALIZED.read(), &parsed_naddr),
        move |(client_ready, _)| {
            if !client_ready {
                return;
            }
            let parsed = (*parsed_naddr.read()).clone();
            let Some(parsed) = parsed else {
                return;
            };
            spawn(async move {
                nest_room_store::set_loading(true);
                nest_room_store::set_error(None);
                match nostr_client::fetch_event_by_coordinate_with_relays(
                    parsed.kind,
                    parsed.pubkey.clone(),
                    parsed.identifier,
                    parsed.relay_hints,
                )
                .await
                {
                    Ok(Some(event)) => match parse_meeting_space(&event) {
                        Ok(ms) => {
                            let host_pk = ms
                                .providers
                                .first()
                                .map(|p| p.pubkey.clone())
                                .unwrap_or_default();
                            // Populate room_relays from the parsed event's
                            // `relays` tag so the effective_relays memo (and
                            // therefore all room subscriptions) can include
                            // them on the next render.
                            let room_relays = ms.relays.clone();
                            nest_room_store::set_space(Some(ms));
                            if !room_relays.is_empty() {
                                nest_room_store::set_room_relays(room_relays);
                            }
                            nest_room_store::set_loading(false);
                            let _ = profiles::fetch_profile(host_pk).await;
                        }
                        Err(e) => {
                            nest_room_store::set_error(Some(format!("Failed to parse room: {}", e)));
                            nest_room_store::set_loading(false);
                        }
                    },
                    Ok(None) => {
                        nest_room_store::set_error(Some("Room not found".to_string()));
                        nest_room_store::set_loading(false);
                    }
                    Err(e) => {
                        nest_room_store::set_error(Some(format!("Failed to load room: {}", e)));
                        nest_room_store::set_loading(false);
                    }
                }
            });
        },
    ));

    // Presence (kind 10312) live subscription.
    {
        let coordinate = format!("30312:{}:{}", room_author, room_d_tag);
        let presence_filter = if !room_author.is_empty() {
            Some(
                nostr_sdk::Filter::new()
                    .kind(nostr_sdk::Kind::Custom(10312))
                    .custom_tag(
                        nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::A),
                        coordinate.as_str(),
                    )
                    .limit(100),
            )
        } else {
            None
        };
        let relays_for_presence = effective_relays;
        use_relay_subscription_to(
            presence_filter,
            None,
            relays_for_presence.read().clone(),
            move |event: &nostr::Event| {
                if event.kind.as_u16() == 10312 {
                    match parse_room_presence(event) {
                        Ok(presence) => nest_room_store::upsert_participant(presence),
                        Err(e) => log::warn!("Failed to parse presence: {}", e),
                    }
                }
            },
        );
    }

    // Room update (kind 30312) subscription — auto-leave when host closes the room.
    {
        let pid_for_close = NEST_ROOM.read().publisher_id.clone();
        let space_filter = if !room_author.is_empty() && !room_d_tag.is_empty() {
            let mut filter = nostr_sdk::Filter::new()
                .kind(nostr_sdk::Kind::Custom(30312))
                .custom_tag(
                    nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::D),
                    room_d_tag.as_str(),
                )
                .limit(1);
            if let Ok(pk) = nostr_sdk::PublicKey::parse(&room_author) {
                filter = filter.author(pk);
            }
            Some(filter)
        } else {
            None
        };
        let relays_for_space = effective_relays;
        use_relay_subscription_to(
            space_filter,
            None,
            relays_for_space.read().clone(),
            move |event: &nostr::Event| {
                if event.kind.as_u16() == 30312 {
                    match parse_meeting_space(event) {
                        Ok(ms) => {
                            let was_open = NEST_ROOM
                                .read()
                                .space
                                .as_ref()
                                .map(|old| old.status != RoomStatus::Closed)
                                .unwrap_or(false);
                            let now_closed = ms.status == RoomStatus::Closed;
                            let joined = NEST_ROOM.read().is_joined;
                            // Capture the room's `relays` tag before publishing
                            // the space update, so subsequent subscriptions can
                            // re-target. Single source of truth: parse_meeting_space
                            // already extracted `ms.relays` for us.
                            let new_room_relays = ms.relays.clone();
                            nest_room_store::set_space(Some(ms));
                            if !new_room_relays.is_empty() {
                                nest_room_store::set_room_relays(new_room_relays);
                            }
                            if was_open && now_closed && joined {
                                let pid = pid_for_close.clone();
                                spawn(async move {
                                    let _ = crate::hooks::use_nest_audio::leave_room(&pid).await;
                                    #[cfg(feature = "mobile_platform")]
                                    {
                                        let _ = pip::set_nest_active(false);
                                    }
                                });
                                nest_room_store::set_joined(false);
                                nest_room_store::set_muted(true);
                                nest_room_store::set_publishing(false);
                                nest_room_store::set_hand_raised(false);
                                log::info!("Room closed by host, auto-leaving");
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to parse space update: {}", e);
                        }
                    }
                }
            },
        );
    }

    let room_coordinate = format!("30312:{}:{}", room_author, room_d_tag);

    // Phase 3.3: Admin commands (kind 4312). Filtered to the local user via
    // `#p` tag. The callback verifies signer authority (host/admin role on
    // the active 30312), 60s freshness, and event-id dedup before honoring.
    // Only `kick` is implemented (reference client's sole action).
    {
        let my_pk_for_admin = (*my_pubkey.read()).clone();
        let admin_filter = if !room_coordinate.is_empty() && !my_pk_for_admin.is_empty() {
            Some(
                nostr_sdk::Filter::new()
                    .kind(nostr_sdk::Kind::Custom(4312))
                    .custom_tag(
                        nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::A),
                        room_coordinate.as_str(),
                    )
                    .custom_tag(
                        nostr_sdk::SingleLetterTag::lowercase(nostr_sdk::Alphabet::P),
                        my_pk_for_admin.as_str(),
                    )
                    .limit(10),
            )
        } else {
            None
        };
        let mut seen_admin_ids = use_signal(std::collections::HashSet::<nostr_sdk::EventId>::new);
        let pid_for_kick = NEST_ROOM.read().publisher_id.clone();
        let relays_for_admin = effective_relays;
        use_relay_subscription_to(
            admin_filter,
            None,
            relays_for_admin.read().clone(),
            move |event: &nostr::Event| {
                if event.kind.as_u16() != 4312 {
                    return;
                }
                let space = match NEST_ROOM.read().space.clone() {
                    Some(s) => s,
                    None => return,
                };
                let mut seen = seen_admin_ids.write();
                match should_honor_admin_command(event, &space, &mut seen) {
                    Some(nostr_sdk_admin_action) => {
                        // For now, the only honored action is Kick.
                        drop(space);
                        drop(seen);
                        log::warn!("Honoring admin command: {:?}", nostr_sdk_admin_action);
                        let pid = pid_for_kick.clone();
                        spawn(async move {
                            let _ = crate::hooks::use_nest_audio::leave_room(&pid).await;
                            nest_room_store::set_joined(false);
                            nest_room_store::set_publishing(false);
                            nest_room_store::set_hand_raised(false);
                            nest_room_store::set_onstage(false);
                            #[cfg(feature = "mobile_platform")]
                            {
                                let _ = pip::set_nest_active(false);
                            }
                        });
                    }
                    None => {
                        // Ignored: stale, duplicate, unauthorized, or unknown action.
                    }
                }
            },
        );
    }

    // Reconcile audio subscriptions whenever the participant list or joined
    // state changes.
    {
        let pid = NEST_ROOM.read().publisher_id.clone();
        use_effect(use_reactive(&*NEST_ROOM.read(), move |_| {
            let (joined, parts_vec, current_subscribed) = {
                let s = NEST_ROOM.read();
                (
                    s.is_joined,
                    s.participants.clone(),
                    s.subscribed_pubkeys.clone(),
                )
            };
            if !joined {
                return;
            }
            let pid = pid.clone();
            let mut to_subscribe = Vec::new();
            let mut to_unsubscribe = Vec::new();
            for p in &parts_vec {
                if p.publishing && !current_subscribed.contains(&p.pubkey) {
                    to_subscribe.push(p.pubkey.clone());
                }
            }
            for pk in &current_subscribed {
                if !parts_vec.iter().any(|p| p.pubkey == *pk && p.publishing) {
                    to_unsubscribe.push(pk.clone());
                }
            }
            spawn(async move {
                for pk in &to_subscribe {
                    let _ = crate::hooks::use_nest_audio::subscribe_to_participant(&pid, pk).await;
                }
                for pk in &to_unsubscribe {
                    let _ =
                        crate::hooks::use_nest_audio::unsubscribe_from_participant(&pid, pk).await;
                }
                for pk in to_subscribe {
                    nest_room_store::mark_subscribed(pk);
                }
                for pk in to_unsubscribe {
                    nest_room_store::mark_unsubscribed(&pk);
                }
            });
        }));
    }

    // Presence heartbeat (60s tick while joined).
    {
        let coord = room_coordinate.clone();
        use_hook(move || {
            let task = spawn(async move {
                loop {
                    crate::platform::timer::sleep_ms(60_000).await;
                    let s = NEST_ROOM.read();
                    if !s.is_joined {
                        continue;
                    }
                    let muted = s.is_muted;
                    let publishing = s.is_publishing;
                    let hand = s.hand_raised;
                    let onstage = s.onstage;
                    drop(s);
                    let _ = crate::hooks::use_nest_audio::publish_presence(
                        &coord, muted, publishing, hand, onstage,
                    )
                    .await;
                }
            });
            nest_room_store::set_heartbeat_task(task);
        });
    }

    // Phase 1.4: Auto-promote to speaker when the local user's role on the
    // 30312 transitions to host/admin/speaker. Mirrors Amethyst's
    // `AutoConnectAndTrackSpeakers` + the reference impl's `RoomPage.tsx:273-285`
    // auth-listener-fallback (if publish-scoped JWT minting fails, retry as
    // listener so the user isn't stranded).
    //
    // Triggers when:
    //   - We're already joined to audio (listener)
    //   - The local user's role on the parsed 30312 is speak-capable
    //   - We're not currently publishing
    //   - We haven't been demoted (declined_publish)
    {
        let my_pk_for_promote = (*my_pubkey.read()).clone();
        let coord_for_promote = room_coordinate.clone();
        use_effect(use_reactive(&*NEST_ROOM.read(), move |_| {
            let (space, is_joined, is_publishing, declined, pid, muted_now, hand_now) = {
                let s = NEST_ROOM.read();
                (
                    s.space.clone(),
                    s.is_joined,
                    s.is_publishing,
                    s.declined_publish,
                    s.publisher_id.clone(),
                    s.is_muted,
                    s.hand_raised,
                )
            };
            let Some(ms) = space else { return };
            if !is_joined || is_publishing || declined {
                return;
            }
            // Implicit host = event author; explicit role from p-tag otherwise.
            let my_role = ms
                .providers
                .iter()
                .find(|p| p.pubkey == my_pk_for_promote)
                .map(|p| p.role.as_deref().unwrap_or("participant"))
                .unwrap_or_else(|| {
                    if ms.pubkey == my_pk_for_promote {
                        "host"
                    } else {
                        ""
                    }
                });
            if !matches!(my_role, "host" | "admin" | "speaker") {
                return;
            }
            // Speak-capable role + joined + not yet publishing → promote.
            let auth_url = ms.service_url.clone();
            let relay_url = ms.endpoint_url.clone().unwrap_or_default();
            let namespace = format!("nests/{}", ms.coordinate);
            let my_pk = my_pk_for_promote.clone();
            let coord = coord_for_promote.clone();
            let promotion = SpeakerPromotion {
                pid,
                auth_url,
                relay_url,
                namespace,
                my_pk,
                coordinate: coord,
                muted_now,
                hand_now,
            };
            spawn(async move {
                match promotion.promote().await {
                    Ok(()) => {
                        nest_room_store::set_publishing(true);
                        nest_room_store::set_onstage(true);
                        log::info!("Promoted to speaker");
                    }
                    Err(e) => {
                        log::warn!(
                            "Speaker promotion failed ({e}); falling back to listener per reference RoomPage.tsx:273-285"
                        );
                        // Auth-listener-fallback: re-join as listener (publish=false)
                        // so the user keeps hearing audio even if their publish
                        // JWT mint failed.
                        let _ = crate::hooks::use_nest_audio::join_room_with_retry(
                            &promotion.pid, &promotion.auth_url, &promotion.relay_url,
                            &promotion.namespace, &promotion.my_pk,
                            /*publish=*/ false, 3,
                        )
                        .await;
                        nest_room_store::set_declined_publish(true);
                    }
                }
            });
        }));
    }

    // Phase 1.5 + 3.7: Energy-gated speaking detection. Polls the local mic
    // level AND all remote participant levels every 100ms (Amethyst's
    // `LEVEL_TICK_MS`). Lights speaker rings when peak amplitude ≥ 0.06
    // (`SPEAKING_LEVEL_THRESHOLD`, ~-24 dBFS). 250ms hysteresis
    // (`SPEAKING_TIMEOUT_MS`) prevents flicker when audio dips briefly.
    {
        let pid_for_level = NEST_ROOM.read().publisher_id.clone();
        let my_pk_for_speaking = (*my_pubkey.read()).clone();
        use_hook(move || {
            let task = spawn(async move {
                // Local speaking-state hysteresis.
                let mut speaking_until_secs: f64 = 0.0;
                // Phase 3.7: per-remote-speaker hysteresis deadlines.
                let mut remote_speaking_until: std::collections::HashMap<String, f64> =
                    std::collections::HashMap::new();
                const SPEAKING_THRESHOLD: f32 = 0.06;
                const HYSTERESIS_SECS: f64 = 0.25;

                loop {
                    crate::platform::timer::sleep_ms(100).await;
                    let joined = NEST_ROOM.read().is_joined;
                    if !joined {
                        nest_room_store::set_mic_level(0.0);
                        nest_room_store::set_local_speaking(false);
                        nest_room_store::mark_not_speaking(&my_pk_for_speaking);
                        // Clear all remote speakers too.
                        for pk in remote_speaking_until.keys() {
                            nest_room_store::mark_not_speaking(pk);
                        }
                        remote_speaking_until.clear();
                        continue;
                    }

                    let now_secs = crate::platform::timestamp::now_secs() as f64;

                    // --- Local mic level (Phase 1.5) ---
                    let publishing = NEST_ROOM.read().is_publishing;
                    if publishing {
                        let level =
                            crate::hooks::use_nest_audio::get_mic_level(&pid_for_level).await;
                        nest_room_store::set_mic_level(level);
                        if level >= SPEAKING_THRESHOLD {
                            speaking_until_secs = now_secs + HYSTERESIS_SECS;
                            nest_room_store::set_local_speaking(true);
                            nest_room_store::mark_speaking(my_pk_for_speaking.clone());
                        } else if now_secs >= speaking_until_secs {
                            nest_room_store::set_local_speaking(false);
                            nest_room_store::mark_not_speaking(&my_pk_for_speaking);
                        }
                    } else {
                        nest_room_store::set_mic_level(0.0);
                        nest_room_store::set_local_speaking(false);
                        nest_room_store::mark_not_speaking(&my_pk_for_speaking);
                    }

                    // --- Remote participant levels (Phase 3.7) ---
                    // Batch-poll all levels in one call (one JS eval per tick).
                    let levels = crate::hooks::use_nest_audio::get_all_participant_levels(
                        &pid_for_level,
                    )
                    .await;
                    for (pk, level) in &levels {
                        if *level >= SPEAKING_THRESHOLD {
                            remote_speaking_until.insert(pk.clone(), now_secs + HYSTERESIS_SECS);
                            nest_room_store::mark_speaking(pk.clone());
                        } else if let Some(&deadline) = remote_speaking_until.get(pk) {
                            if now_secs >= deadline {
                                nest_room_store::mark_not_speaking(pk);
                                remote_speaking_until.remove(pk);
                            }
                        } else {
                            nest_room_store::mark_not_speaking(pk);
                        }
                    }
                    // Clean up participants no longer in the levels map.
                    let active_pks: std::collections::HashSet<&str> =
                        levels.keys().map(|s| s.as_str()).collect();
                    let stale: Vec<String> = remote_speaking_until
                        .keys()
                        .filter(|k| !active_pks.contains(k.as_str()))
                        .cloned()
                        .collect();
                    for pk in &stale {
                        nest_room_store::mark_not_speaking(pk);
                        remote_speaking_until.remove(pk);
                    }
                }
            });
            nest_room_store::set_level_poll_task(task);
        });
    }

    // Phase 2.2: Cliff detector — detects when an announced speaker is
    // subscribed but no audio frames have arrived for >2.5s (relay-side
    // forward-queue starvation, a known moq-rs production issue). Triggers a
    // session recycle with escalating backoff (0 → 5s → 12s → 24s → 30s).
    // Mirrors Amethyst's cliff detector constants.
    //
    // The detector only activates once Phase 4.2 plumbs the `onFrame` callback
    // from the audio layer into `NEST_ROOM.last_frame_at`. Until then,
    // `last_frame_at` is empty and the detector is a no-op.
    {
        let pid_for_cliff = NEST_ROOM.read().publisher_id.clone();
        use_hook(move || {
            let task = spawn(async move {
                // Escalating backoff between consecutive recycles (seconds).
                // Matches Amethyst: 0 → 5 → 12 → 24 → 30 cap.
                const BACKOFF_SCHEDULE_SECS: &[u64] = &[0, 5, 12, 24, 30];
                let mut consecutive_recycles: u32 = 0;
                loop {
                    crate::platform::timer::sleep_ms(1_000).await;
                    let s = NEST_ROOM.read();
                    if !s.is_joined {
                        continue;
                    }
                    let now_secs = crate::platform::timestamp::now_secs() as f64;
    let participants = s.participants.clone();
                    let last_frames = s.last_frame_at.clone();
                    let pid = pid_for_cliff.clone();
                    drop(s);
                    // Find subscribed speakers whose latest frame is stale.
                    // Only consider speakers whose presence still says publishing=1
                    // (otherwise they intentionally stopped, not a cliff).
                    let stale_speakers: Vec<&str> = participants
                        .iter()
                        .filter(|p| p.publishing)
                        .filter_map(|p| {
                            last_frames.get(&p.pubkey).map(|t| (p.pubkey.as_str(), *t))
                        })
                        .filter(|(_, t)| now_secs - t > 2.5)
                        .map(|(pk, _)| pk)
                        .collect();
                    if stale_speakers.is_empty() {
                        consecutive_recycles = 0;
                        nest_room_store::set_cliff_backoff_step(0);
                        continue;
                    }
                    let step =
                        consecutive_recycles.min(BACKOFF_SCHEDULE_SECS.len() as u32 - 1) as usize;
                    let backoff_secs = BACKOFF_SCHEDULE_SECS[step];
                    nest_room_store::set_cliff_backoff_step(step as u32);
                    log::warn!(
                        "Cliff detected: {} stale speaker(s), recycling (step={}, backoff={}s)",
                        stale_speakers.len(),
                        step,
                        backoff_secs,
                    );
                    if backoff_secs > 0 {
                        crate::platform::timer::sleep_ms((backoff_secs * 1000) as u32).await;
                    }
                    match crate::services::nests_audio::reconnect::recycle(&pid).await {
                        Ok(()) => {
                            consecutive_recycles = consecutive_recycles.saturating_add(1);
                        }
                        Err(e) => {
                            log::warn!("Cliff recycle failed: {e}");
                            consecutive_recycles = consecutive_recycles.saturating_add(1);
                        }
                    }
                }
            });
            nest_room_store::set_cliff_task(task);
        });
    }

    // Phase 2.4: JWT proactive refresh. The moq-auth JWT expires after 600s
    // (`moq-auth/src/index.ts:10` TOKEN_TTL). The reference impl has a bug
    // where it does NOT proactively refresh (`RoomPage.tsx`'s authenticate
    // effect only runs on dep change), so long sessions silently degrade
    // after 10 min. We schedule a recycle at 540s (60s margin) whenever the
    // user is joined. The recycle re-mints a fresh JWT and re-establishes the
    // session via `reconnect::recycle`.
    {
        let pid_for_jwt = NEST_ROOM.read().publisher_id.clone();
        use_hook(move || {
            let task = spawn(async move {
                loop {
                    crate::platform::timer::sleep_ms(540_000).await;
                    let joined = NEST_ROOM.read().is_joined;
                    if !joined {
                        continue;
                    }
                    log::info!("JWT proactive refresh (540s elapsed)");
                    if let Err(e) =
                        crate::services::nests_audio::reconnect::recycle(&pid_for_jwt).await
                    {
                        log::warn!("JWT refresh recycle failed: {e}");
                    }
                }
            });
            nest_room_store::set_jwt_refresh_task(task);
        });
    }

    // Phase 2.5: Network-change session recycle. When the device's default
    // network changes (Wi-Fi ↔ cellular), QUIC's PTO would take ~30s to
    // detect the dead path. We watch the online-status global signal and
    // recycle immediately on transition. (Web's online/offline events are
    // plumbed by `stores::ui::online_status::setup_online_status`; mobile
    // uses the same signal via the platform layer.)
    {
        let pid_for_net = NEST_ROOM.read().publisher_id.clone();
        use_effect(use_reactive(
            (&*crate::stores::ui::online_status::ONLINE_STATUS.read(), &*NEST_ROOM.read()),
            move |(now_online, _)| {
                let joined = NEST_ROOM.read().is_joined;
                if !joined {
                    return;
                }
                let pid = if NEST_ROOM.read().publisher_id.is_empty() {
                    pid_for_net.clone()
                } else {
                    NEST_ROOM.read().publisher_id.clone()
                };
                if !now_online {
                    return;
                }
                spawn(async move {
                    log::info!("Network online, recycling QUIC session");
                    if let Err(e) =
                        crate::services::nests_audio::reconnect::recycle(&pid).await
                    {
                        log::warn!("Network-change recycle failed: {e}");
                    }
                });
            },
        ));
    }

    // Phase 4.1: MoQ ANNOUNCE hybrid participant discovery. Polls the
    // ANNOUNCE stream every 3s for real-time join/leave detection, then
    // reconciles with Nostr presence (which is durable but 60s-bound).
    // ANNOUNCE is faster (sub-second via MoQ session); Nostr presence is
    // the fallback for hand_raised/muted/onstage state.
    {
        let pid_for_announce = NEST_ROOM.read().publisher_id.clone();
        let my_pk_for_announce = (*my_pubkey.read()).clone();
        use_hook(move || {
            spawn(async move {
                loop {
                    crate::platform::timer::sleep_ms(3_000).await;
                    let joined = NEST_ROOM.read().is_joined;
                    if !joined {
                        continue;
                    }
                    let announced = crate::hooks::use_nest_audio::poll_announced_participants(
                        &pid_for_announce,
                    )
                    .await;
                    // Mark announced speakers as online (real-time signal).
                    // Nostr presence handles the detailed state.
                    let now_secs = crate::platform::timestamp::now_secs() as f64;
                    for pk in &announced {
                        if pk != &my_pk_for_announce {
                            nest_room_store::record_frame(pk.clone(), now_secs);
                        }
                    }
                }
            });
        });
    }

    // On unmount: leave audio + clear PiP. Do NOT call reset() — we want audio
    // to survive navigation. State is wiped on next init_for_room() call.
    // (For an explicit leave, the user uses the Leave button; we just disconnect
    // the audio session when the viewer itself unmounts.)
    {
        let pid = NEST_ROOM.read().publisher_id.clone();
        use_drop(move || {
            #[cfg(feature = "mobile_platform")]
            {
                let _ = pip::set_nest_active(false);
                // Phase 4.3: Stop the Android foreground notification.
                #[cfg(all(target_os = "android", feature = "mobile_platform"))]
                {
                    let _ = crate::services::nests_audio::android::stop_nest_notification();
                }
            }
            let pid = pid;
            spawn(async move {
                let _ = crate::hooks::use_nest_audio::leave_room(&pid).await;
            });
        });
    }

    // PiP integration (mobile).
    #[cfg(feature = "mobile_platform")]
    {
        let pip_pid = NEST_ROOM.read().publisher_id.clone();
        use_effect(move || {
            let joined = NEST_ROOM.read().is_joined;
            let muted = NEST_ROOM.read().is_muted;
            if pip::consume_pip_mute_toggle() && joined {
                let new_muted = !muted;
                nest_room_store::set_muted(new_muted);
                let pid = pip_pid.clone();
                spawn(async move {
                    let result = if new_muted {
                        crate::hooks::use_nest_audio::mute(&pid).await
                    } else {
                        crate::hooks::use_nest_audio::unmute(&pid).await
                    };
                    if result.is_err() {
                        nest_room_store::set_muted(!new_muted);
                    }
                });
            }
        });
    }

    let handle_join = {
        let space_val_publisher_id = NEST_ROOM.read().publisher_id.clone();
        let coord = room_coordinate.clone();
        let my_pk = (*my_pubkey.read()).clone();
        move |_: Event<MouseData>| {
            let ms = NEST_ROOM.read().space.clone();
            let Some(ms) = ms else {
                return;
            };
            let auth_url = ms.service_url.clone();
            let relay_url = ms.endpoint_url.clone().unwrap_or_default();
            let coordinate = coord.clone();
            let pid = space_val_publisher_id.clone();
            let my_pk = my_pk.clone();
            spawn(async move {
                nest_room_store::set_audio_error(None);
                let namespace = format!("nests/{}", coordinate);
                match crate::hooks::use_nest_audio::join_room_with_retry(
                    &pid,
                    &auth_url,
                    &relay_url,
                    &namespace,
                    &my_pk,
                    /*publish=*/ false,
                    3,
                )
                .await
                {
                    Ok(()) => {
                        nest_room_store::set_joined(true);
                        nest_room_store::set_muted(true);
                        #[cfg(feature = "mobile_platform")]
                        {
                            let _ = pip::set_nest_active(true);
                            // Phase 4.3: Android foreground notification for
                            // wake lock + persistent notification while in a
                            // nest (matches Amethyst's NestForegroundService).
                            #[cfg(all(target_os = "android", feature = "mobile_platform"))]
                            {
                                let _ = crate::services::nests_audio::android::start_nest_notification(
                                    &ms.room_name,
                                );
                            }
                        }
                        let _ =
                            crate::hooks::use_nest_audio::publish_presence(
                                &coordinate, true, false, false, false,
                            )
                            .await;
                    }
                    Err(e) => {
                        nest_room_store::set_audio_error(Some(e));
                    }
                }
            });
        }
    };

    let handle_toggle_mute = {
        let pid = NEST_ROOM.read().publisher_id.clone();
        move |_: ()| {
            let joined = NEST_ROOM.read().is_joined;
            if !joined {
                return;
            }
            let pid = pid.clone();
            let currently_muted = NEST_ROOM.read().is_muted;
            spawn(async move {
                let result = if currently_muted {
                    crate::hooks::use_nest_audio::unmute(&pid).await
                } else {
                    crate::hooks::use_nest_audio::mute(&pid).await
                };
                if result.is_ok() {
                    nest_room_store::set_muted(!currently_muted);
                }
            });
        }
    };

    let handle_raise_hand = {
        let coord = room_coordinate.clone();
        move |_: ()| {
            let new_hand = !NEST_ROOM.read().hand_raised;
            nest_room_store::set_hand_raised(new_hand);
            let coord = coord.clone();
            spawn(async move {
                let s = NEST_ROOM.read();
                let muted = s.is_muted;
                let publishing = s.is_publishing;
                drop(s);
                let _ = crate::hooks::use_nest_audio::publish_presence(
                    &coord, muted, publishing, new_hand, false,
                )
                .await;
            });
        }
    };

    let handle_close_and_leave = {
        let pid = NEST_ROOM.read().publisher_id.clone();
        move |_| {
            nest_room_store::set_show_host_leave_confirm(false);
            let ms = match NEST_ROOM.read().space.clone() {
                Some(ms) => ms,
                None => return,
            };
            let pid = pid.clone();
            spawn(async move {
                let tags = rebuild_meeting_space_tags(&ms, RoomStatus::Closed);
                let builder = nostr_sdk::EventBuilder::new(nostr_sdk::Kind::Custom(30312), "")
                    .tags(tags);
                let _ = crate::stores::publish_queue::signing::sign_event_builder(builder)
                    .await
                    .map(|event| {
                        crate::stores::publish_queue::enqueue(
                            event,
                            crate::stores::publish_queue::types::QueueEventType::Other(
                                "nest".to_string(),
                            ),
                            None,
                            std::collections::HashMap::new(),
                        )
                    });
                let _ = crate::hooks::use_nest_audio::leave_room(&pid).await;
                #[cfg(feature = "mobile_platform")]
                {
                    let _ = pip::set_nest_active(false);
                }
                nest_room_store::set_joined(false);
                nest_room_store::set_muted(true);
                nest_room_store::set_publishing(false);
                nest_room_store::set_hand_raised(false);
                nest_room_store::set_audio_error(None);
                nav.push(Route::NestsHome {});
            });
        }
    };

    let handle_just_leave = {
        let pid = NEST_ROOM.read().publisher_id.clone();
        move |_| {
            nest_room_store::set_show_host_leave_confirm(false);
            let pid = pid.clone();
            spawn(async move {
                let _ = crate::hooks::use_nest_audio::leave_room(&pid).await;
                #[cfg(feature = "mobile_platform")]
                {
                    let _ = pip::set_nest_active(false);
                }
                nest_room_store::set_joined(false);
                nest_room_store::set_muted(true);
                nest_room_store::set_publishing(false);
                nest_room_store::set_hand_raised(false);
                nest_room_store::set_audio_error(None);
            });
        }
    };

    // Note: hand-raise (handle_raise_hand above) IS the "request to speak"
    // affordance. There is no separate request-to-speak flow — matches
    // Amethyst's HandRaiseToggle. The kind 10312 `hand=1` presence puts the
    // user in the host's SpeakerQueue; the host promotes via the 30312 role
    // flip in `handle_approve_speaker` below.

    let handle_approve_speaker = move |pubkey: String| {
        spawn(async move {
            let ms = match NEST_ROOM.read().space.clone() {
                Some(ms) => ms,
                None => return,
            };
            // Monotonic timestamp guard — avoids the same-second silent no-op
            // when the host created the room and immediately promotes in the
            // same wall-clock second. Matches Amethyst's `RoomParticipantActions`.
            let now_secs = nostr_sdk::Timestamp::now().as_secs();
            let ts = nostr_sdk::Timestamp::from_secs(std::cmp::max(ms.created_at + 1, now_secs));
            let tags = match crate::utils::nips::nip53::rebuild_meeting_space_tags_with_role(
                &ms,
                &pubkey,
                "speaker",
            ) {
                Ok(t) => t,
                Err(e) => {
                    log::error!("role flip failed: {e}");
                    return;
                }
            };
            let builder = nostr_sdk::EventBuilder::new(nostr_sdk::Kind::Custom(30312), "")
                .tags(tags)
                .custom_created_at(ts);
            match crate::stores::publish_queue::signing::sign_event_builder(builder).await {
                Ok(event) => {
                    let _ = crate::stores::publish_queue::enqueue(
                        event,
                        crate::stores::publish_queue::types::QueueEventType::Other(
                            "nest-role-update".to_string(),
                        ),
                        None,
                        std::collections::HashMap::new(),
                    )
                    .await;
                }
                Err(e) => log::error!("approve_speaker sign failed: {e}"),
            }
        });
    };

    // Deny keeps the existing 4312 admin command wire format for now. Phase
    // 3.3 will revisit this — the reference impl has no "deny" wire event,
    // so this is really a host-side UX affordance pending that decision.
    let handle_deny_speaker = {
        let coord = room_coordinate.clone();
        move |pubkey: String| {
            let coord = coord.clone();
            spawn(async move {
                let _ = crate::hooks::use_nest_admin::publish_admin_command(
                    &coord,
                    &pubkey,
                    "remove_speaker",
                )
                .await;
            });
        }
    };

    // Phase 3.2: Target pubkey for the ParticipantActionSheet. None when
    // the sheet is closed. Set by the StageGrid / ParticipantGallery
    // `on_participant_action` callbacks (long-press / right-click).
    let mut participant_action_target: Signal<Option<String>> = use_signal(|| None);

    let on_leave_clicked = {
        let is_host = NEST_ROOM
            .read()
            .space
            .as_ref()
            .map(|ms| {
                ms.providers
                    .first()
                    .map(|p| p.pubkey == *my_pubkey.read())
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        let pid = NEST_ROOM.read().publisher_id.clone();
        move |_: ()| {
            let joined = NEST_ROOM.read().is_joined;
            if is_host && joined {
                nest_room_store::set_show_host_leave_confirm(true);
                return;
            }
            let pid = pid.clone();
            spawn(async move {
                let _ = crate::hooks::use_nest_audio::leave_room(&pid).await;
                #[cfg(feature = "mobile_platform")]
                {
                    let _ = pip::set_nest_active(false);
                }
                nest_room_store::set_joined(false);
                nest_room_store::set_muted(true);
                nest_room_store::set_publishing(false);
                nest_room_store::set_hand_raised(false);
                nest_room_store::set_audio_error(None);
            });
        }
    };

    // Snapshot reads for render. We re-read on each render (Dioxus reruns the
    // component body when any subscribed store field changes).
    let s = NEST_ROOM.read();
    let space_ref = s.space.clone();
    let loading = s.loading;
    let error = s.error.clone();
    let is_joined = s.is_joined;
    let is_muted = s.is_muted;
    let is_publishing = s.is_publishing;
    let hand_raised = s.hand_raised;
    let audio_error = s.audio_error.clone();
    let show_host_leave_confirm = s.show_host_leave_confirm;
    let participants = s.participants.clone();
    let speaking_now = s.speaking_now.clone();
    let is_host = space_ref
        .as_ref()
        .map(|ms| {
            ms.providers
                .first()
                .map(|p| p.pubkey == *my_pubkey.read())
                .unwrap_or(false)
        })
        .unwrap_or(false);
    drop(s);

    let hand_raised_participants: Vec<RoomPresence> = participants
        .iter()
        .filter(|p| p.hand_raised && !p.onstage && !p.publishing)
        .cloned()
        .collect();
    let speaker_request_count = hand_raised_participants.len() as u32;

    let content_class = "flex-1 flex flex-col min-h-0 overflow-hidden";

    #[cfg(feature = "mobile_platform")]
    let is_pip = pip::is_pip_mode();
    #[cfg(not(feature = "mobile_platform"))]
    let is_pip = false;

    rsx! {
        div { class: "flex flex-col h-[calc(100dvh-4.5rem)] lg:h-dvh overflow-hidden",
            if !is_pip {
                div { class: "shrink-0 bg-background/95 backdrop-blur-sm border-b border-border p-4",
                    div { class: "flex items-center gap-4",
                        Link {
                            to: Route::NestsHome {},
                            class: "p-2 hover:bg-muted rounded-lg transition",
                            ArrowLeftIcon { class: "w-5 h-5".to_string() }
                        }
                        h1 { class: "text-lg font-bold truncate", "Nest Room" }
                    }
                }
            }

            if loading {
                div { class: "flex-1 flex items-center justify-center",
                    div { class: "animate-pulse text-muted-foreground",
                        RadioIcon { class: "w-12 h-12".to_string() }
                    }
                }
            } else if let Some(err) = error.as_ref() {
                div { class: "flex-1 flex items-center justify-center p-4",
                    div { class: "text-center space-y-4",
                        p { class: "text-destructive", "{err}" }
                        Link {
                            to: Route::NestsHome {},
                            class: "inline-block px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                            "Back to Nests"
                        }
                    }
                }
            } else if let Some(ms) = space_ref.as_ref() {
                div { class: "{content_class}",
                    if is_pip {
                        div { class: "flex items-center justify-center gap-3 p-3",
                            h1 { class: "text-sm font-bold truncate", "{ms.room_name}" }
                            if !participants.is_empty() {
                                span { class: "text-xs text-muted-foreground",
                                    "{participants.len()}"
                                }
                            }
                        }
                    } else {
                        NestHeader {
                            space: ms.clone(),
                            listener_count: participants.len() as u32,
                            is_host: is_host,
                        }
                    }

                    div { class: if is_pip { "p-2" } else { "p-4 space-y-4" },
                        StageGrid {
                            participants: participants.clone(),
                            my_pubkey: (*my_pubkey.read()).clone(),
                            is_publishing: is_publishing,
                            is_muted: is_muted,
                            speaking_now: speaking_now.clone(),
                            on_participant_action: move |pk: String| {
                                participant_action_target.set(Some(pk));
                            },
                        }

                        if !is_pip {
                            NestReactions {
                                room_coordinate: room_coordinate.clone(),
                                is_joined: is_joined,
                            }

                            if !is_joined {
                                if let Some(ref err) = audio_error {
                                    p { class: "text-sm text-destructive text-center", "{err}" }
                                }
                                button {
                                    class: "w-full py-3 bg-blue-500 hover:bg-blue-600 text-white font-bold rounded-xl transition flex items-center justify-center gap-2",
                                    onclick: handle_join,
                                    PhoneCallIcon { class: "w-5 h-5".to_string() }
                                    "Join Audio"
                                }
                            }
                        }
                    }

                    // Tabbed content (Chat / Audience / Hands). Mirrors
                    // Amethyst's `NestFullScreen.NestTabRow`. Hands tab is
                    // host-only and only shown when there's at least one
                    // raised hand. Skipped entirely in PiP mode.
                    if !is_pip && !room_author.is_empty() && !room_d_tag.is_empty() {
                        {let active_tab = NEST_ROOM.read().active_room_tab;
                         let audience_count = participants.iter().filter(|p| !p.publishing && !p.onstage).count();
                         let show_hands_tab = is_host && !hand_raised_participants.is_empty();
                         let effective_tab = if active_tab == crate::stores::nest_room_store::RoomTab::Hands && !show_hands_tab {
                             crate::stores::nest_room_store::RoomTab::Chat
                         } else {
                             active_tab
                         };
                         rsx! {
                            div { class: "border-t border-border flex flex-col flex-1 min-h-0",
                                // Tab strip
                                div { class: "flex border-b border-border shrink-0",
                                    button {
                                        class: if effective_tab == crate::stores::nest_room_store::RoomTab::Chat {
                                            "flex-1 px-4 py-2.5 text-sm font-medium border-b-2 border-primary text-primary"
                                        } else {
                                            "flex-1 px-4 py-2.5 text-sm font-medium border-b-2 border-transparent text-muted-foreground hover:text-foreground transition"
                                        },
                                        onclick: move |_| nest_room_store::set_active_room_tab(crate::stores::nest_room_store::RoomTab::Chat),
                                        "Chat"
                                    }
                                    button {
                                        class: if effective_tab == crate::stores::nest_room_store::RoomTab::Audience {
                                            "flex-1 px-4 py-2.5 text-sm font-medium border-b-2 border-primary text-primary"
                                        } else {
                                            "flex-1 px-4 py-2.5 text-sm font-medium border-b-2 border-transparent text-muted-foreground hover:text-foreground transition"
                                        },
                                        onclick: move |_| nest_room_store::set_active_room_tab(crate::stores::nest_room_store::RoomTab::Audience),
                                        span { "Audience" }
                                        if audience_count > 0 {
                                            span { class: "ml-1.5 text-xs text-muted-foreground",
                                                "({audience_count})"
                                            }
                                        }
                                    }
                                    if show_hands_tab {
                                        button {
                                            class: if effective_tab == crate::stores::nest_room_store::RoomTab::Hands {
                                                "flex-1 px-4 py-2.5 text-sm font-medium border-b-2 border-primary text-primary"
                                            } else {
                                                "flex-1 px-4 py-2.5 text-sm font-medium border-b-2 border-transparent text-muted-foreground hover:text-foreground transition"
                                            },
                                            onclick: move |_| nest_room_store::set_active_room_tab(crate::stores::nest_room_store::RoomTab::Hands),
                                            span { "Hands" }
                                            span { class: "ml-1.5 text-xs text-muted-foreground",
                                                "({speaker_request_count})"
                                            }
                                        }
                                    }
                                }

                                // Tab content
                                div { class: "flex-1 overflow-y-auto min-h-0",
                                    match effective_tab {
                                        crate::stores::nest_room_store::RoomTab::Chat => rsx! {
                                            NestChat {
                                                room_coordinate: room_coordinate.clone(),
                                                room_author: room_author.clone(),
                                                room_d_tag: room_d_tag.clone(),
                                                room_relays: effective_relays.read().clone(),
                                            }
                                        },
                                        crate::stores::nest_room_store::RoomTab::Audience => rsx! {
                                            div { class: "p-4",
                                                if audience_count > 0 {
                                                    ParticipantGallery {
                                                        participants: participants.iter().filter(|p| !p.publishing && !p.onstage).cloned().collect(),
                                                        max_display: Some(50),
                                                    }
                                                } else {
                                                    p { class: "text-sm text-muted-foreground text-center py-8",
                                                        "No listeners yet"
                                                    }
                                                }
                                            }
                                        },
                                        crate::stores::nest_room_store::RoomTab::Hands => rsx! {
                                            div { class: "p-4",
                                                SpeakerQueue {
                                                    hand_raised_participants: hand_raised_participants,
                                                    is_host: is_host,
                                                    on_approve: handle_approve_speaker,
                                                    on_deny: handle_deny_speaker,
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                         }}
                    }
                }

                if is_joined && !is_pip {
                    ActionBar {
                        is_connected: is_joined,
                        is_muted: is_muted,
                        is_publishing: is_publishing,
                        is_host: is_host,
                        hand_raised: hand_raised,
                        speaker_request_count: speaker_request_count,
                        on_toggle_mute: handle_toggle_mute,
                        on_raise_hand: handle_raise_hand,
                        on_leave: on_leave_clicked,
                    }
                }
            }

            if show_host_leave_confirm {
                ConfirmModal {
                    title: "End this nest?".to_string(),
                    message: "You're the host. Closing the room will disconnect everyone. Choose \"Just Leave\" if you want to come back later.".to_string(),
                    confirm_text: Some("Close Room".to_string()),
                    cancel_text: Some("Just Leave".to_string()),
                    on_confirm: handle_close_and_leave,
                    on_cancel: handle_just_leave,
                }
            }

            // Phase 3.2: Per-participant host-action sheet. Opens when the
            // user long-presses / right-clicks a stage or audience tile.
            // Host sees Promote/Demote/Kick; non-hosts see profile nav.
            // Handlers are duplicated (not shared with the SpeakerQueue
            // above) because Dioxus closures move into the component.
            if let Some(ref target_pk) = *participant_action_target.read() {
                {let target_pk = target_pk.clone();
                 let target_on_stage = participants
                    .iter()
                    .any(|p| p.pubkey == target_pk && (p.publishing || p.onstage));
                 let coord_for_deny = room_coordinate.clone();
                 let coord_for_kick = room_coordinate.clone();
                 rsx! {
                    ParticipantActionSheet {
                        target_pubkey: target_pk,
                        is_host: is_host,
                        is_target_on_stage: target_on_stage,
                        on_promote: move |pubkey: String| {
                            spawn(async move {
                                let ms = match NEST_ROOM.read().space.clone() {
                                    Some(ms) => ms,
                                    None => return,
                                };
                                let now_secs = nostr_sdk::Timestamp::now().as_secs();
                                let ts = nostr_sdk::Timestamp::from_secs(std::cmp::max(ms.created_at + 1, now_secs));
                                let tags = match crate::utils::nips::nip53::rebuild_meeting_space_tags_with_role(
                                    &ms, &pubkey, "speaker",
                                ) {
                                    Ok(t) => t,
                                    Err(e) => { log::error!("role flip failed: {e}"); return; }
                                };
                                let builder = nostr_sdk::EventBuilder::new(nostr_sdk::Kind::Custom(30312), "")
                                    .tags(tags).custom_created_at(ts);
                                if let Ok(event) = crate::stores::publish_queue::signing::sign_event_builder(builder).await {
                                    let _ = crate::stores::publish_queue::enqueue(
                                        event,
                                        crate::stores::publish_queue::types::QueueEventType::Other("nest-role-update".to_string()),
                                        None, std::collections::HashMap::new(),
                                    ).await;
                                }
                            });
                        },
                        on_demote: move |pubkey: String| {
                            let coord = coord_for_deny.clone();
                            spawn(async move {
                                let _ = crate::hooks::use_nest_admin::publish_admin_command(
                                    &coord, &pubkey, "remove_speaker",
                                ).await;
                            });
                        },
                        on_kick: move |pubkey: String| {
                            let coord = coord_for_kick.clone();
                            spawn(async move {
                                let _ = crate::hooks::use_nest_admin::publish_admin_command(
                                    &coord, &pubkey, "kick",
                                ).await;
                            });
                        },
                        on_close: move |_| participant_action_target.set(None),
                    }
                 }}
            }
        }
    }
}

/// Bundles the parameters needed to promote a listener to speaker. Passed as
/// a single value to stay under clippy's `too many arguments` threshold.
struct SpeakerPromotion {
    pid: String,
    auth_url: String,
    relay_url: String,
    namespace: String,
    my_pk: String,
    coordinate: String,
    muted_now: bool,
    hand_now: bool,
}

impl SpeakerPromotion {
    /// Promote the local user from listener to speaker.
    ///
    /// 1. Tear down the existing listener session.
    /// 2. Re-join with `publish=true` so the JWT carries the `put` claim for our
    ///    pubkey (`moq-auth/src/index.ts:188-197`).
    /// 3. Start publishing microphone audio.
    /// 4. Publish a kind 10312 presence with `publishing=1, onstage=1`.
    async fn promote(&self) -> Result<(), String> {
        let _ = crate::hooks::use_nest_audio::leave_room(&self.pid).await;
        crate::hooks::use_nest_audio::join_room_with_retry(
            &self.pid, &self.auth_url, &self.relay_url, &self.namespace, &self.my_pk,
            /*publish=*/ true, 3,
        )
        .await?;
        crate::hooks::use_nest_audio::start_publishing(&self.pid).await?;
        let _ = crate::hooks::use_nest_audio::publish_presence(
            &self.coordinate, self.muted_now, /*publishing=*/ true, self.hand_now, /*onstage=*/ true,
        )
        .await;
        Ok(())
    }
}
