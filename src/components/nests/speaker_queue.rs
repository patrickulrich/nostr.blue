use crate::components::icons::{CheckIcon, XIcon};
use crate::stores::profiles;
use crate::utils::nip19_urls::profile_route_id;
use crate::utils::nips::nip53::RoomPresence;
use crate::utils::truncate_pubkey;
use crate::routes::Route;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct SpeakerQueueProps {
    pub hand_raised_participants: Vec<RoomPresence>,
    pub is_host: bool,
    pub on_approve: EventHandler<String>,
    pub on_deny: EventHandler<String>,
}

#[component]
pub fn SpeakerQueue(props: SpeakerQueueProps) -> Element {
    let hand_raised: Vec<&RoomPresence> = props
        .hand_raised_participants
        .iter()
        .filter(|p| p.hand_raised && !p.onstage && !p.publishing)
        .collect();

    if hand_raised.is_empty() {
        return rsx! {};
    }

    let count = hand_raised.len();

    rsx! {
        div { class: "border border-border rounded-lg overflow-hidden",
            div { class: "px-4 py-3 bg-muted/50 flex items-center justify-between",
                div { class: "flex items-center gap-2",
                    crate::components::icons::HandIcon { class: "w-4 h-4 text-yellow-500".to_string() }
                    span { class: "text-sm font-semibold",
                        if props.is_host {
                            "Speaker Requests ({count})"
                        } else if count == 1 {
                            "1 speaker requested"
                        } else {
                            "{count} speakers requested"
                        }
                    }
                }
            }
            if props.is_host {
                div { class: "divide-y divide-border",
                    for participant in &hand_raised {
                        SpeakerRequestItem {
                            key: "{participant.pubkey}",
                            participant: (*participant).clone(),
                            on_approve: props.on_approve,
                            on_deny: props.on_deny,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SpeakerRequestItem(
    participant: RoomPresence,
    on_approve: EventHandler<String>,
    on_deny: EventHandler<String>,
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

    let pk_for_approve = participant.pubkey.clone();
    let pk_for_deny = participant.pubkey.clone();

    rsx! {
        div { class: "flex items-center justify-between px-4 py-2.5",
            Link {
                to: Route::AddressViewer { address: profile_route_id(&participant.pubkey) },
                class: "flex items-center gap-2 min-w-0 flex-1",
                if let Some(ref url) = *avatar_url.read() {
                    img {
                        src: "{url}",
                        class: "w-8 h-8 rounded-full object-cover",
                        loading: "lazy",
                    }
                } else {
                    div { class: "w-8 h-8 rounded-full bg-blue-600 flex items-center justify-center text-white text-xs font-bold" }
                }
                span { class: "text-sm truncate",
                    "{name.read()}"
                }
            }
            div { class: "flex items-center gap-2 shrink-0 ml-2",
                button {
                    class: "p-1.5 rounded-lg bg-green-500/20 text-green-500 hover:bg-green-500/30 transition",
                    onclick: move |_: Event<MouseData>| {
                        on_approve.call(pk_for_approve.clone());
                    },
                    CheckIcon { class: "w-4 h-4".to_string() }
                }
                button {
                    class: "p-1.5 rounded-lg bg-red-500/20 text-red-500 hover:bg-red-500/30 transition",
                    onclick: move |_: Event<MouseData>| {
                        on_deny.call(pk_for_deny.clone());
                    },
                    XIcon { class: "w-4 h-4".to_string() }
                }
            }
        }
    }
}
