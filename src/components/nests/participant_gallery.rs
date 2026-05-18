use crate::routes::Route;
use crate::stores::profiles;
use crate::utils::nip19_urls::profile_route_id;
use crate::utils::nips::nip53::RoomPresence;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ParticipantGalleryProps {
    pub participants: Vec<RoomPresence>,
    #[props(default = None)]
    pub max_display: Option<usize>,
}

#[component]
pub fn ParticipantGallery(props: ParticipantGalleryProps) -> Element {
    let max = props.max_display.unwrap_or(10);
    let displayed: Vec<&RoomPresence> = props.participants.iter().take(max).collect();
    let overflow_count = props.participants.len().saturating_sub(max);

    rsx! {
        div { class: "flex items-center flex-wrap gap-1",
            for participant in displayed {
                ParticipantAvatar {
                    key: "{participant.pubkey}",
                    participant: participant.clone(),
                }
            }
            if overflow_count > 0 {
                div { class: "w-8 h-8 rounded-full bg-muted flex items-center justify-center text-xs font-medium text-muted-foreground",
                    "+{overflow_count}"
                }
            }
        }
    }
}

#[component]
fn ParticipantAvatar(participant: RoomPresence) -> Element {
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

    let ring_class = if participant.publishing {
        "ring-2 ring-green-500"
    } else {
        ""
    };

    rsx! {
        Link {
            to: Route::Profile { pubkey: profile_route_id(&participant.pubkey) },
            class: "relative",
            if let Some(ref url) = *avatar_url.read() {
                img {
                    src: "{url}",
                    class: "w-8 h-8 rounded-full object-cover {ring_class}",
                    title: "{name.read()}",
                    loading: "lazy",
                }
            } else {
                div {
                    class: "w-8 h-8 rounded-full bg-blue-600 flex items-center justify-center text-white text-[10px] font-bold {ring_class}",
                    title: "{name.read()}",
                    {
                        let first = name.read().chars().next().unwrap_or('?').to_uppercase().to_string();
                        rsx! { "{first}" }
                    }
                }
            }
            if participant.hand_raised {
                span { class: "absolute -top-0.5 -right-0.5 w-3 h-3 bg-yellow-400 rounded-full border border-background",
                    title: "Hand raised",
                }
            }
        }
    }
}
