use crate::components::icons::{ArrowLeftIcon, PhoneCallIcon, RadioIcon};
use crate::components::nests::{
    ActionBar, NestChat, NestHeader, NestReactions, ParticipantGallery, SpeakerQueue, StageGrid,
};
use crate::hooks::use_relay_subscription;
use crate::routes::Route;
use crate::stores::auth_store::get_pubkey;
use crate::stores::nostr_client::{self, CLIENT_INITIALIZED};
use crate::stores::profiles;
use crate::utils::nip19::parse_naddr;
use crate::utils::nips::nip53::{
    parse_meeting_space, parse_room_presence, MeetingSpace, RoomPresence,
};
use dioxus::prelude::*;

#[cfg(feature = "mobile_platform")]
use crate::platform::pip;

#[component]
pub fn NestDetail(naddr: String) -> Element {
    let publisher_id: String = {
        let pk = get_pubkey().unwrap_or_default();
        format!("nest-{}-{pk}", naddr)
    };
    let parsed_naddr = use_memo(move || parse_naddr(&naddr).ok());
    let mut space = use_signal(|| None::<MeetingSpace>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut participants = use_signal(Vec::<RoomPresence>::new);

    let my_pubkey = use_memo(move || get_pubkey().unwrap_or_default());
    let is_joined = use_signal(|| false);
    let is_muted = use_signal(|| true);
    let is_publishing = use_signal(|| false);
    let hand_raised = use_signal(|| false);
    let audio_error = use_signal(|| None::<String>);

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
                loading.set(true);
                error.set(None);
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
                            space.set(Some(ms));
                            loading.set(false);
                            let _ = profiles::fetch_profile(host_pk).await;
                        }
                        Err(e) => {
                            error.set(Some(format!("Failed to parse room: {}", e)));
                            loading.set(false);
                        }
                    },
                    Ok(None) => {
                        error.set(Some("Room not found".to_string()));
                        loading.set(false);
                    }
                    Err(e) => {
                        error.set(Some(format!("Failed to load room: {}", e)));
                        loading.set(false);
                    }
                }
            });
        },
    ));

    {
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
        use_relay_subscription(presence_filter, move |event: &nostr::Event| {
            if event.kind.as_u16() == 10312 {
                match parse_room_presence(event) {
                    Ok(presence) => {
                        let mut parts = participants.write();
                        if let Some(existing) = parts
                            .iter()
                            .position(|p| p.pubkey == presence.pubkey)
                        {
                            parts[existing] = presence;
                        } else {
                            parts.push(presence);
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to parse presence: {}", e);
                    }
                }
            }
        });
    }

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
    let room_coordinate = format!("30312:{}:{}", room_author, room_d_tag);

    {
        let pid = publisher_id.clone();
        let is_joined_cb = is_joined;
        use_effect(use_reactive(&participants, move |parts: Signal<Vec<RoomPresence>>| {
            if !*is_joined_cb.read() {
                return;
            }
            let parts_vec = parts.read().clone();
            let pid = pid.clone();
            spawn(async move {
                for p in &parts_vec {
                    if p.publishing {
                        let _ =
                            crate::hooks::use_nest_audio::subscribe_to_participant(&pid, &p.pubkey)
                                .await;
                    }
                }
            });
        }));
    }

    {
        let coord = room_coordinate.clone();
        let is_joined_hb = is_joined;
        let is_muted_hb = is_muted;
        let is_publishing_hb = is_publishing;
        let hand_raised_hb = hand_raised;
        spawn(async move {
            loop {
                crate::platform::timer::sleep_ms(60_000).await;
                if !*is_joined_hb.read() {
                    continue;
                }
                let _ = crate::hooks::use_nest_audio::publish_presence(
                    &coord,
                    *is_muted_hb.read(),
                    *is_publishing_hb.read(),
                    *hand_raised_hb.read(),
                    false,
                )
                .await;
            }
        });
    }

    {
        let pid = publisher_id.clone();
        use_drop(move || {
            #[cfg(feature = "mobile_platform")]
            { let _ = pip::set_nest_active(false); }
            spawn(async move {
                let _ = crate::hooks::use_nest_audio::leave_room(&pid).await;
            });
        });
    }

    #[cfg(feature = "mobile_platform")]
    {
        let mut is_muted_pip = is_muted;
        let _is_publishing_pip = is_publishing;
        let _hand_raised_pip = hand_raised;
        let pip_pid = publisher_id.clone();
        use_effect(move || {
            if pip::consume_pip_mute_toggle() && *is_joined.read() {
                let new_muted = !*is_muted_pip.read();
                is_muted_pip.set(new_muted);
                let pid = pip_pid.clone();
                spawn(async move {
                    let result = if new_muted {
                        crate::hooks::use_nest_audio::mute(&pid).await
                    } else {
                        crate::hooks::use_nest_audio::unmute(&pid).await
                    };
                    if result.is_err() {
                        is_muted_pip.set(!new_muted);
                    }
                });
            }
        });
    }

    let handle_join = {
        let space_val = space;
        let pid = publisher_id.clone();
        let coord = room_coordinate.clone();
        let mut audio_error_cb = audio_error;
        let mut is_joined_cb = is_joined;
        let mut is_muted_cb = is_muted;
        move |_: Event<MouseData>| {
            let ms = space_val.read().clone();
            let Some(ms) = ms else {
                return;
            };
            let auth_url = ms.service_url.clone();
            let relay_url = ms.endpoint_url.clone().unwrap_or_default();
            let coordinate = coord.clone();
            let pid = pid.clone();
            spawn(async move {
                audio_error_cb.set(None);
                let namespace = format!("nests/{}", coordinate);
                match crate::hooks::use_nest_audio::join_room_with_retry(
                    &pid, &auth_url, &relay_url, &namespace, 3,
                )
                .await
                {
                    Ok(()) => {
                        is_joined_cb.set(true);
                        is_muted_cb.set(true);
                        #[cfg(feature = "mobile_platform")]
                        { let _ = pip::set_nest_active(true); }
                        let _ = crate::hooks::use_nest_audio::publish_presence(
                            &coordinate, true, false, false, false,
                        )
                        .await;
                    }
                    Err(e) => {
                        audio_error_cb.set(Some(e));
                    }
                }
            });
        }
    };

    let handle_toggle_mute = {
        let pid = publisher_id.clone();
        let mut is_muted_cb = is_muted;
        let is_joined_cb = is_joined;
        move |_: ()| {
            if !*is_joined_cb.read() {
                return;
            }
            let pid = pid.clone();
            let currently_muted = *is_muted_cb.read();
            spawn(async move {
                let result = if currently_muted {
                    crate::hooks::use_nest_audio::unmute(&pid).await
                } else {
                    crate::hooks::use_nest_audio::mute(&pid).await
                };
                if result.is_ok() {
                    is_muted_cb.set(!currently_muted);
                }
            });
        }
    };

    let handle_raise_hand = {
        let coord = room_coordinate.clone();
        let mut hand_raised_cb = hand_raised;
        let is_muted_hr = is_muted;
        let is_publishing_hr = is_publishing;
        move |_: ()| {
            let new_hand = !*hand_raised_cb.read();
            hand_raised_cb.set(new_hand);
            let coord = coord.clone();
            spawn(async move {
                let _ = crate::hooks::use_nest_audio::publish_presence(
                    &coord,
                    *is_muted_hr.read(),
                    *is_publishing_hr.read(),
                    new_hand,
                    false,
                )
                .await;
            });
        }
    };

    let handle_leave = {
        let pid = publisher_id.clone();
        let mut is_joined_cb = is_joined;
        let mut is_muted_cb = is_muted;
        let mut is_publishing_cb = is_publishing;
        let mut hand_raised_cb = hand_raised;
        let mut audio_error_cb = audio_error;
        move |_: ()| {
            let pid = pid.clone();
            spawn(async move {
                let _ = crate::hooks::use_nest_audio::leave_room(&pid).await;
                #[cfg(feature = "mobile_platform")]
                { let _ = pip::set_nest_active(false); }
                is_joined_cb.set(false);
                is_muted_cb.set(true);
                is_publishing_cb.set(false);
                hand_raised_cb.set(false);
                audio_error_cb.set(None);
            });
        }
    };

    let handle_request_speak = move |_: ()| {};

    let handle_approve_speaker = {
        let coord = room_coordinate.clone();
        move |pubkey: String| {
            let coord = coord.clone();
            spawn(async move {
                let _ = crate::hooks::use_nest_admin::publish_admin_command(
                    &coord,
                    &pubkey,
                    "approve_speaker",
                )
                .await;
            });
        }
    };

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

    let space_ref = space.read();
    let is_host = space_ref
        .as_ref()
        .map(|ms| {
            ms.providers
                .first()
                .map(|p| p.pubkey == *my_pubkey.read())
                .unwrap_or(false)
        })
        .unwrap_or(false);

    let hand_raised_participants: Vec<RoomPresence> = participants
        .read()
        .iter()
        .filter(|p| p.hand_raised && !p.onstage && !p.publishing)
        .cloned()
        .collect();

    let speaker_request_count = hand_raised_participants.len() as u32;

    let content_class = if *is_joined.read() {
        "flex-1 overflow-y-auto pb-28"
    } else {
        "flex-1 overflow-y-auto"
    };

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

            if *loading.read() {
                div { class: "flex-1 flex items-center justify-center",
                    div { class: "animate-pulse text-muted-foreground",
                        RadioIcon { class: "w-12 h-12".to_string() }
                    }
                }
            } else if let Some(err) = error.read().as_ref() {
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
                            if !participants.read().is_empty() {
                                span { class: "text-xs text-muted-foreground",
                                    "{participants.read().len()}"
                                }
                            }
                        }
                    } else {
                        NestHeader {
                            space: ms.clone(),
                            listener_count: participants.read().len() as u32,
                            is_host: is_host,
                        }
                    }

                    div { class: if is_pip { "p-2" } else { "p-4 space-y-4" },
                        StageGrid {
                            participants: participants.read().clone(),
                            my_pubkey: (*my_pubkey.read()).clone(),
                            is_publishing: *is_publishing.read(),
                            is_muted: *is_muted.read(),
                        }

                        if !is_pip {
                            SpeakerQueue {
                                hand_raised_participants: hand_raised_participants,
                                is_host: is_host,
                                on_approve: handle_approve_speaker,
                                on_deny: handle_deny_speaker,
                            }

                            if !participants.read().is_empty() {
                                div { class: "space-y-2",
                                    h3 { class: "text-sm font-semibold text-muted-foreground",
                                        "Listeners ({participants.read().len()})"
                                    }
                                    ParticipantGallery {
                                        participants: participants.read().clone(),
                                        max_display: Some(15),
                                    }
                                }
                            }

                            NestReactions {
                                room_coordinate: room_coordinate.clone(),
                                is_joined: *is_joined.read(),
                            }
                        }

                        if !*is_joined.read() && !is_pip {
                            if let Some(ref err) = *audio_error.read() {
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

                    if !is_pip && !room_author.is_empty() && !room_d_tag.is_empty() {
                        div { class: "border-t border-border",
                            NestChat {
                                room_coordinate: room_coordinate,
                                room_author: room_author,
                                room_d_tag: room_d_tag,
                            }
                        }
                    }
                }

                if *is_joined.read() && !is_pip {
                    ActionBar {
                        is_connected: *is_joined.read(),
                        is_muted: *is_muted.read(),
                        is_publishing: *is_publishing.read(),
                        is_host: is_host,
                        hand_raised: *hand_raised.read(),
                        speaker_request_count: speaker_request_count,
                        on_toggle_mute: handle_toggle_mute,
                        on_raise_hand: handle_raise_hand,
                        on_leave: handle_leave,
                        on_request_speak: handle_request_speak,
                    }
                }
            }
        }
    }
}
