use crate::components::icons::{RadioIcon, UsersIcon};
use crate::routes::Route;
use crate::stores::profiles;
use crate::utils::nips::nip53::{LiveStatus, MeetingSpace};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct NestCardProps {
    pub space: MeetingSpace,
    #[props(default = None)]
    pub presence_count: Option<u32>,
    pub display_status: LiveStatus,
}

#[component]
pub fn NestCard(props: NestCardProps) -> Element {
    let host_pubkey = props
        .space
        .providers
        .first()
        .map(|p| p.pubkey.clone())
        .unwrap_or_default();
    let pk_for_name = host_pubkey.clone();
    let host_metadata = use_memo(move || profiles::get_profile(&host_pubkey));
    let host_name = use_memo(move || {
        if let Some(ref meta) = *host_metadata.read() {
            meta.display_name
                .clone()
                .or_else(|| meta.name.clone())
                .unwrap_or_else(|| truncate_pubkey(&pk_for_name))
        } else {
            truncate_pubkey(&pk_for_name)
        }
    });
    let host_avatar = use_memo(move || {
        host_metadata
            .read()
            .as_ref()
            .and_then(|m| m.picture.clone())
    });

    let status_badge_class = match props.display_status {
        LiveStatus::Live => "bg-red-500/20 text-red-500",
        LiveStatus::Planned => "bg-blue-500/20 text-blue-500",
        LiveStatus::Ended => "bg-muted text-muted-foreground",
    };
    let status_label = match props.display_status {
        LiveStatus::Live => "LIVE",
        LiveStatus::Planned => "SCHEDULED",
        LiveStatus::Ended => "ENDED",
    };

    let listener_count = props.presence_count.unwrap_or(0);

    rsx! {
        Link {
            to: Route::NestDetail { naddr: props.space.naddr.clone() },
            class: "block bg-card border border-border rounded-xl overflow-hidden hover:border-foreground/20 transition group",
            div { class: "relative aspect-video bg-muted",
                if let Some(ref image) = props.space.image {
                    img {
                        src: "{image}",
                        class: "w-full h-full object-cover",
                        loading: "lazy",
                    }
                } else {
                    div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-blue-600/20 to-purple-600/20",
                        RadioIcon { class: "w-12 h-12 text-muted-foreground".to_string() }
                    }
                }
                div { class: "absolute top-2 left-2 flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-bold {status_badge_class}",
                    if props.display_status == LiveStatus::Live {
                        span { class: "w-2 h-2 rounded-full bg-red-500 animate-pulse" }
                    }
                    "{status_label}"
                }
                if props.space.recording.is_some() {
                    div { class: "absolute top-2 right-2 flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-bold bg-orange-500/20 text-orange-500",
                        span { class: "w-2 h-2 rounded-full bg-orange-500" }
                        "REC"
                    }
                }
            }
            div { class: "p-3",
                h3 { class: "font-semibold text-sm truncate",
                    "{props.space.room_name}"
                }
                div { class: "flex items-center gap-2 mt-1.5",
                    if let Some(ref avatar_url) = *host_avatar.read() {
                        img {
                            src: "{avatar_url}",
                            class: "w-5 h-5 rounded-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-5 h-5 rounded-full bg-blue-600 flex items-center justify-center text-white text-[10px] font-bold" }
                    }
                    span { class: "text-xs text-muted-foreground truncate",
                        "{host_name.read()}"
                    }
                }
                if listener_count > 0 {
                    div { class: "flex items-center gap-1 mt-1.5 text-xs text-muted-foreground",
                        UsersIcon { class: "w-3.5 h-3.5".to_string() }
                        "{listener_count} listening"
                    }
                }
            }
        }
    }
}
