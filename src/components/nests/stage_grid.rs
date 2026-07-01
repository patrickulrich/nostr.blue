use crate::hooks::{use_long_press, DEFAULT_LONG_PRESS_MS};
use crate::routes::Route;
use crate::stores::profiles;
use crate::utils::nip19_urls::profile_route_id;
use crate::utils::nips::nip53::RoomPresence;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use std::collections::HashSet;

#[derive(Props, Clone, PartialEq)]
pub struct StageGridProps {
    pub participants: Vec<RoomPresence>,
    pub my_pubkey: String,
    pub is_publishing: bool,
    pub is_muted: bool,
    /// Phase 3.7: pubkeys whose decoded audio level is currently above the
    /// speaking threshold (0.06). Passed from NestViewer's snapshot read of
    /// `NEST_ROOM.speaking_now` to avoid StageGrid subscribing to the whole
    /// store (which would cause 10x/sec re-renders from the mic_level poll).
    pub speaking_now: HashSet<String>,
    /// Phase 3.2: callback fired when the user long-presses / right-clicks
    /// a stage tile. The viewer opens a `ParticipantActionSheet` with
    /// Promote/Demote/Kick actions (host-only).
    pub on_participant_action: EventHandler<String>,
}

#[component]
pub fn StageGrid(props: StageGridProps) -> Element {
    let speakers: Vec<&RoomPresence> = props
        .participants
        .iter()
        .filter(|p| p.publishing || p.onstage)
        .collect();

    rsx! {
        div { class: "px-4 py-3",
            if !speakers.is_empty() {
                h3 { class: "text-sm font-semibold text-muted-foreground mb-3",
                    "Speakers ({speakers.len()})"
                }
            }
            div { class: "flex gap-4 overflow-x-auto pb-2 scrollbar-hide",
                for speaker in &speakers {
                    StageTile {
                        key: "{speaker.pubkey}",
                        participant: (*speaker).clone(),
                        is_me: speaker.pubkey == props.my_pubkey,
                        is_speaking: props.speaking_now.contains(&speaker.pubkey),
                        on_action: props.on_participant_action,
                    }
                }
            }
        }
    }
}

#[component]
fn StageTile(
    participant: RoomPresence,
    is_me: bool,
    is_speaking: bool,
    on_action: EventHandler<String>,
) -> Element {
    let pubkey = participant.pubkey.clone();
    let pk_for_name = pubkey.clone();
    let metadata = use_memo(move || profiles::get_profile(&pubkey));
    let avatar_url = use_memo(move || {
        metadata
            .read()
            .as_ref()
            .and_then(|m| m.picture.clone())
    });
    let name = use_memo(move || {
        if let Some(ref meta) = *metadata.read() {
            meta.display_name
                .clone()
                .or_else(|| meta.name.clone())
                .unwrap_or_else(|| truncate_pubkey(&pk_for_name))
        } else {
            truncate_pubkey(&pk_for_name)
        }
    });

    // Phase 3.7: three-state ring — actively speaking (green pulse),
    // publishing but silent (dim), not publishing (muted).
    let ring_class = if is_speaking {
        "ring-2 ring-green-500 animate-pulse"
    } else if participant.publishing {
        "ring-2 ring-muted-foreground/30"
    } else {
        "ring-2 ring-muted"
    };

    let action_pubkey = participant.pubkey.clone();
    let (on_touch_start, on_touch_move, on_touch_end, on_touch_cancel) = use_long_press(
        Callback::new(move |_| on_action.call(action_pubkey.clone())),
        DEFAULT_LONG_PRESS_MS,
    );

    rsx! {
        div {
            class: "flex flex-col items-center gap-1.5 min-w-[5rem]",
            // Desktop: right-click opens the participant action sheet
            // (Phase 3.2). Mobile is a no-op here so native text selection is
            // preserved; mobile long-press is handled by the touch handlers.
            oncontextmenu: move |e: MouseEvent| {
                if cfg!(feature = "mobile_platform") { return; }
                e.prevent_default();
                e.stop_propagation();
                on_action.call(participant.pubkey.clone());
            },
            ontouchstart: on_touch_start,
            ontouchmove: on_touch_move,
            ontouchend: on_touch_end,
            ontouchcancel: on_touch_cancel,
            Link {
                to: Route::AddressViewer { address: profile_route_id(&participant.pubkey) },
                class: "contents",
            div { class: "relative",
                if let Some(ref url) = *avatar_url.read() {
                    img {
                        src: "{url}",
                        class: "w-16 h-16 rounded-full object-cover {ring_class}",
                        title: "{name.read()}",
                        loading: "lazy",
                    }
                } else {
                    div {
                        class: "w-16 h-16 rounded-full bg-blue-600 flex items-center justify-center text-white text-lg font-bold {ring_class}",
                        title: "{name.read()}",
                        {
                            let first = name.read().chars().next().unwrap_or('?').to_uppercase().to_string();
                            rsx! { "{first}" }
                        }
                    }
                }
                if participant.muted {
                    div { class: "absolute -bottom-1 -right-1 w-5 h-5 bg-muted rounded-full flex items-center justify-center border-2 border-background",
                        span { class: "text-[8px] text-muted-foreground", "🔇" }
                    }
                }
                if is_me {
                    div { class: "absolute -top-1 -right-1 w-4 h-4 bg-blue-500 rounded-full border-2 border-background" }
                }
            }
            span { class: "text-xs text-center truncate w-20",
                "{name.read()}"
            }
            }
        }
    }
}
