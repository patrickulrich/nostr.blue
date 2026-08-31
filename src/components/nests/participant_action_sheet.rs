//! Per-participant host-action sheet. Triggered by tap (host) or
//! long-press/right-click (anyone) on a stage or audience tile. Host sees
//! Promote/Demote/Kick actions; non-hosts see profile nav + zap. Mirrors
//! reference host-action layout.

use crate::components::Sheet;
use crate::routes::Route;
use crate::stores::profiles;
use crate::utils::nip19_urls::profile_route_id;
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ParticipantActionSheetProps {
    pub target_pubkey: String,
    pub is_host: bool,
    pub is_target_on_stage: bool,
    pub on_promote: EventHandler<String>,
    pub on_demote: EventHandler<String>,
    pub on_kick: EventHandler<String>,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn ParticipantActionSheet(props: ParticipantActionSheetProps) -> Element {
    let target_pk = props.target_pubkey.clone();
    let pk_for_profile_link = target_pk.clone();
    let pk_for_display = target_pk.clone();
    let pk_for_promote = target_pk.clone();
    let pk_for_demote = target_pk.clone();
    let pk_for_kick = target_pk.clone();
    let pk_for_memo = target_pk.clone();
    let metadata = use_memo(move || profiles::get_profile(&target_pk));
    let name = use_memo(move || {
        let pk_for_name = pk_for_memo.clone();
        if let Some(ref meta) = *metadata.read() {
            meta.display_name
                .clone()
                .or_else(|| meta.name.clone())
                .unwrap_or_else(|| truncate_pubkey(&pk_for_name))
        } else {
            truncate_pubkey(&pk_for_name)
        }
    });

    rsx! {
        Sheet {
            open: true,
            on_open_change: move |open: bool| {
                if !open {
                    props.on_close.call(());
                }
            },
            crate::components::SheetContent {
                side: crate::components::SheetSide::Bottom,
                class: "p-0",
                div { class: "p-4 space-y-1",
                    div { class: "pb-3 mb-2 border-b border-border",
                        p { class: "text-sm font-semibold truncate",
                            "{name.read()}"
                        }
                        p { class: "text-xs text-muted-foreground truncate mt-0.5",
                            "{pk_for_display}"
                        }
                    }
                    Link {
                        to: Route::AddressViewer {
                            address: profile_route_id(&pk_for_profile_link),
                        },
                        class: "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-accent transition text-left",
                        span { class: "w-4 h-4 shrink-0 flex items-center justify-center text-base", "👤" }
                        span { class: "text-sm", "View profile" }
                    }
                    // Host-only actions
                    if props.is_host {
                        if !props.is_target_on_stage {
                            button {
                                class: "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-accent transition text-left text-green-600 dark:text-green-400",
                                onclick: move |_| {
                                    props.on_promote.call(pk_for_promote.clone());
                                    props.on_close.call(());
                                },
                                span { class: "w-4 h-4 shrink-0 flex items-center justify-center text-base", "🎙" }
                                span { class: "text-sm", "Promote to speaker" }
                            }
                        } else {
                            button {
                                class: "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-accent transition text-left",
                                onclick: move |_| {
                                    props.on_demote.call(pk_for_demote.clone());
                                    props.on_close.call(());
                                },
                                span { class: "w-4 h-4 shrink-0 flex items-center justify-center text-base", "⬇" }
                                span { class: "text-sm", "Move to audience" }
                            }
                        }
                        button {
                            class: "w-full flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-accent transition text-left text-destructive",
                            onclick: move |_| {
                                props.on_kick.call(pk_for_kick.clone());
                                props.on_close.call(());
                            },
                            span { class: "w-4 h-4 shrink-0 flex items-center justify-center text-base", "🚫" }
                            span { class: "text-sm", "Kick from room" }
                        }
                    }
                }
            }
        }
    }
}
