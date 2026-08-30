use crate::components::icons::{ClockIcon, RadioIcon, UsersIcon};
use crate::components::nests::NestQuickActionSheet;
use crate::hooks::{use_long_press, DEFAULT_LONG_PRESS_MS};
use crate::routes::Route;
use crate::stores::profiles;
use crate::utils::nips::nip53::{LiveStatus, MeetingSpace};
use crate::utils::time::format_time_ago;
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
    let mut show_actions = use_signal(|| false);
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

    let (on_touch_start, on_touch_move, on_touch_end, on_touch_cancel) = use_long_press(
        Callback::new(move |_| show_actions.set(true)),
        DEFAULT_LONG_PRESS_MS,
    );

    rsx! {
        div {
            class: "block bg-card border border-border rounded-xl overflow-hidden hover:border-foreground/20 transition group",
            // Desktop: right-click opens the quick action sheet. Mobile is a
            // no-op here so the native Android text-selection ActionMode is
            // preserved — mobile uses the touch long-press handlers below.
            // Previously this unconditionally called `prevent_default()`,
            // which suppressed copy/paste popups on Android WebView.
            oncontextmenu: move |e: MouseEvent| {
                if cfg!(feature = "mobile_platform") { return; }
                e.prevent_default();
                e.stop_propagation();
                show_actions.set(true);
            },
            ontouchstart: on_touch_start,
            ontouchmove: on_touch_move,
            ontouchend: on_touch_end,
            ontouchcancel: on_touch_cancel,
            Link {
                to: Route::AddressViewer { address: props.space.naddr.clone() },
                class: "block",
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
        if *show_actions.read() {
            NestQuickActionSheet {
                space: props.space.clone(),
                on_close: move |_| show_actions.set(false),
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct NestEndedCompactCardProps {
    pub space: MeetingSpace,
}

/// Compact one-line card for the "Recently Ended" bucket. Skips the 16:9
/// hero, participants gallery, and listener count to keep the historic list
/// scannable while preserving the entry point (reference
/// `NestEndedCompactCard` — tapping still opens the room (via the
/// `/:naddr` universal dispatcher) so users can play back a recording if
/// one is attached.
#[component]
pub fn NestEndedCompactCard(props: NestEndedCompactCardProps) -> Element {
    let mut show_actions = use_signal(|| false);
    let host_pubkey = props
        .space
        .providers
        .first()
        .map(|p| p.pubkey.clone())
        .unwrap_or_else(|| props.space.pubkey.clone());
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
    let ended_ago = format_time_ago(props.space.created_at);
    let has_recording = props.space.recording.is_some();

    let (on_touch_start, on_touch_move, on_touch_end, on_touch_cancel) = use_long_press(
        Callback::new(move |_| show_actions.set(true)),
        DEFAULT_LONG_PRESS_MS,
    );

    rsx! {
        div {
            class: "flex items-center gap-3 bg-card border border-border rounded-lg p-3 hover:border-foreground/20 transition",
            // Desktop: right-click opens the quick action sheet. Mobile is a
            // no-op so native text-selection ActionMode works; mobile uses
            // the touch long-press handlers below.
            oncontextmenu: move |e: MouseEvent| {
                if cfg!(feature = "mobile_platform") { return; }
                e.prevent_default();
                e.stop_propagation();
                show_actions.set(true);
            },
            ontouchstart: on_touch_start,
            ontouchmove: on_touch_move,
            ontouchend: on_touch_end,
            ontouchcancel: on_touch_cancel,
            Link {
                to: Route::AddressViewer { address: props.space.naddr.clone() },
                class: "flex items-center gap-3 flex-1 min-w-0",
                // 44px thumbnail (or icon fallback). If a recording is
                // attached, overlay a play affordance so the audience knows
                // the room is listen-back-able.
                div { class: "relative w-11 h-11 rounded-md overflow-hidden shrink-0 bg-muted",
                    if let Some(ref image) = props.space.image {
                        img {
                            src: "{image}",
                            class: "w-full h-full object-cover",
                            loading: "lazy",
                        }
                    } else {
                        div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-blue-600/20 to-purple-600/20",
                            RadioIcon { class: "w-5 h-5 text-muted-foreground".to_string() }
                        }
                    }
                    if has_recording {
                        div { class: "absolute inset-0 bg-black/40 flex items-center justify-center",
                            span { class: "text-white text-lg", "▶" }
                        }
                    }
                }
                div { class: "flex-1 min-w-0",
                    h3 { class: "font-medium text-sm truncate",
                        "{props.space.room_name}"
                    }
                    p { class: "text-xs text-muted-foreground truncate mt-0.5",
                        "{host_name.read()}"
                    }
                    div { class: "flex items-center gap-1 mt-1 text-xs text-muted-foreground",
                        ClockIcon { class: "w-3 h-3".to_string() }
                        "Ended {ended_ago}"
                    }
                }
            }
        }
        if *show_actions.read() {
            NestQuickActionSheet {
                space: props.space.clone(),
                on_close: move |_| show_actions.set(false),
            }
        }
    }
}
