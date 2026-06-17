use crate::components::icons::{ExternalLinkIcon, ShareIcon, UsersIcon};
use crate::platform::clipboard::copy_to_clipboard;
use crate::routes::Route;
use crate::stores::profiles;
use crate::utils::nip19_urls::profile_route_id;
use crate::utils::nips::nip53::{MeetingSpace, RoomStatus};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct NestHeaderProps {
    pub space: MeetingSpace,
    #[props(default = 0)]
    pub listener_count: u32,
    #[props(default = false)]
    pub is_host: bool,
}

#[component]
pub fn NestHeader(props: NestHeaderProps) -> Element {
    // Phase 3.5: Title-tap toggles summary between clamped (2 lines) and
    // fully expanded. Matches Amethyst's `summaryExpanded` state in
    // `NestFullScreen.kt:129`.
    let mut summary_expanded = use_signal(|| false);
    let host_pubkey = props
        .space
        .providers
        .first()
        .map(|p| p.pubkey.clone())
        .unwrap_or_default();
    let pk_for_name = host_pubkey.clone();
    let pk_for_profile = host_pubkey.clone();
    let pk_for_fetch = host_pubkey.clone();
    let host_metadata = use_memo(move || profiles::get_profile(&pk_for_fetch));
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

    let status_badge_class = match props.space.status {
        RoomStatus::Open => "bg-red-500/20 text-red-500",
        RoomStatus::Private => "bg-blue-500/20 text-blue-500",
        RoomStatus::Closed => "bg-muted text-muted-foreground",
    };
    let status_label = match props.space.status {
        RoomStatus::Open => "LIVE",
        RoomStatus::Private => "PRIVATE",
        RoomStatus::Closed => "ENDED",
    };

    rsx! {
        div { class: "relative",
            if let Some(ref image) = props.space.image {
                div { class: "aspect-[3/1] w-full overflow-hidden",
                    img {
                        src: "{image}",
                        class: "w-full h-full object-cover",
                        loading: "lazy",
                    }
                    div { class: "absolute inset-0 bg-gradient-to-t from-background via-background/60 to-transparent" }
                }
            } else {
                div { class: "aspect-[3/1] w-full bg-gradient-to-br from-blue-600/30 to-purple-600/30" }
                div { class: "absolute inset-0 bg-gradient-to-t from-background via-background/60 to-transparent" }
            }
            div { class: "absolute bottom-0 left-0 right-0 p-4",
                div { class: "flex items-center gap-2 mb-2 flex-wrap",
                    span { class: "inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-bold {status_badge_class}",
                        if props.space.status == RoomStatus::Open {
                            span { class: "w-2 h-2 rounded-full bg-red-500 animate-pulse" }
                        }
                        "{status_label}"
                    }
                    if props.listener_count > 0 {
                        span { class: "inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs bg-muted text-muted-foreground",
                            UsersIcon { class: "w-3.5 h-3.5".to_string() }
                            "{props.listener_count}"
                        }
                    }
                    if let Some(ref _recording_url) = props.space.recording {
                        span { class: "inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs bg-orange-500/20 text-orange-500",
                            span { class: "w-2 h-2 rounded-full bg-orange-500" }
                            "REC"
                        }
                    }
                }
                h1 {
                    class: "text-2xl font-bold cursor-pointer select-none",
                    title: if props.space.summary.is_some() { "Tap to {if *summary_expanded.read() { \"collapse\" } else { \"expand\" }} description" } else { "" },
                    onclick: move |_| {
                        if props.space.summary.is_some() {
                            let current = *summary_expanded.read();
                            summary_expanded.set(!current);
                        }
                    },
                    "{props.space.room_name}"
                }
                Link {
                    to: Route::AddressViewer { address: profile_route_id(&pk_for_profile) },
                    class: "flex items-center gap-2 mt-2 hover:opacity-80 transition",
                    if let Some(ref avatar_url) = *host_avatar.read() {
                        img {
                            src: "{avatar_url}",
                            class: "w-8 h-8 rounded-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-8 h-8 rounded-full bg-blue-600 flex items-center justify-center text-white text-xs font-bold" }
                    }
                    span { class: "text-sm font-medium",
                        "{host_name.read()}"
                    }
                }
                if let Some(ref summary) = props.space.summary {
                    p {
                        class: if *summary_expanded.read() {
                            "text-sm text-muted-foreground mt-2"
                        } else {
                            "text-sm text-muted-foreground mt-2 line-clamp-2"
                        },
                        "{summary}"
                    }
                }
                if props.space.status == RoomStatus::Closed {
                    if let Some(ref recording_url) = props.space.recording {
                        a {
                            href: "{recording_url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            class: "inline-flex items-center gap-1.5 mt-3 px-3 py-1.5 bg-muted hover:bg-accent rounded-lg text-sm transition",
                            ExternalLinkIcon { class: "w-3.5 h-3.5".to_string() }
                            "Watch Recording"
                        }
                    }
                }
                // Phase 3.6: Share button — available to everyone. Copies
                // the room's nostr:naddr URI to the clipboard.
                {
                    let naddr_for_share = format!("nostr:{}", props.space.naddr);
                    let room_name_for_share = props.space.room_name.clone();
                    rsx! {
                        button {
                            class: "inline-flex items-center gap-1.5 mt-3 ml-2 px-3 py-1.5 bg-muted hover:bg-accent rounded-lg text-sm transition",
                            onclick: move |_| {
                                let uri = naddr_for_share.clone();
                                let name = room_name_for_share.clone();
                                spawn(async move {
                                    let text = if name.is_empty() { uri } else { format!("{name} — {uri}") };
                                    if let Err(e) = copy_to_clipboard(&text).await {
                                        log::warn!("Failed to copy room link: {e}");
                                    }
                                });
                            },
                            ShareIcon { class: "w-3.5 h-3.5".to_string() }
                            "Share"
                        }
                    }
                }
                if props.is_host && props.space.status != RoomStatus::Closed {
                    Link {
                        to: Route::NestCreate { naddr: Some(props.space.naddr.clone()) },
                        class: "inline-flex items-center gap-1.5 mt-3 ml-2 px-3 py-1.5 bg-muted hover:bg-accent rounded-lg text-sm transition",
                        "Edit Room"
                    }
                }
            }
        }
    }
}
