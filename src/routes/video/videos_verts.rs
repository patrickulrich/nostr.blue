use super::video_detail::{FeedType, ShortsPlayer};
use crate::stores::nostr_client::get_contacts_cache;
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;

#[component]
pub fn VideosVerts() -> Element {
    let is_authenticated = auth_store::AUTH_STATE.read().is_authenticated;
    let default_feed = if is_authenticated {
        let has_contacts = get_contacts_cache()
            .lock()
            .map(|guard| {
                guard
                    .as_ref()
                    .map(|c| !c.contacts.is_empty())
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if has_contacts {
            FeedType::Following
        } else {
            FeedType::Global
        }
    } else {
        FeedType::Global
    };
    let mut feed_type = use_signal(|| default_feed);
    let _feed_key = match *feed_type.read() {
        FeedType::Following => "following",
        FeedType::Global => "global",
    };

    rsx! {
        if !*nostr_client::CLIENT_INITIALIZED.read() {
            div { class: "fixed inset-0 bg-black flex items-center justify-center",
                div { class: "text-center",
                    div { class: "mb-4 flex justify-center",
                        span { class: "inline-block h-10 w-10 rounded-full border-4 border-white/30 border-t-white animate-spin" }
                    }
                    p { class: "text-sm text-white/70", "Loading..." }
                }
            }
        } else {
            div {
                ShortsPlayer {
                    key: "{_feed_key}",
                    initial_video_id: String::new(),
                    feed_type: *feed_type.read(),
                    initial_event: None,
                    fallback_to_global_on_empty: false,
                    title: "Verts",
                }
                div { class: "fixed top-16 left-1/2 z-[60] -translate-x-1/2",
                    div { class: "inline-flex items-center gap-2 rounded-full border border-white/15 bg-black/60 p-1 backdrop-blur-md",
                        button {
                            class: if *feed_type.read() == FeedType::Following {
                                "rounded-full bg-white px-4 py-2 text-sm font-semibold text-black transition"
                            } else if !is_authenticated {
                                "rounded-full px-4 py-2 text-sm font-semibold text-white/40 transition cursor-not-allowed"
                            } else {
                                "rounded-full px-4 py-2 text-sm font-semibold text-white/80 transition hover:bg-white/10 hover:text-white"
                            },
                            disabled: !is_authenticated,
                            onclick: move |_| {
                                if is_authenticated {
                                    feed_type.set(FeedType::Following);
                                }
                            },
                            "Following"
                        }
                        button {
                            class: if *feed_type.read() == FeedType::Global {
                                "rounded-full bg-white px-4 py-2 text-sm font-semibold text-black transition"
                            } else {
                                "rounded-full px-4 py-2 text-sm font-semibold text-white/80 transition hover:bg-white/10 hover:text-white"
                            },
                            onclick: move |_| feed_type.set(FeedType::Global),
                            "Global"
                        }
                    }
                }
                if !is_authenticated {
                    div { class: "fixed top-32 left-1/2 z-[60] -translate-x-1/2 rounded-full border border-white/10 bg-black/40 px-4 py-2 text-xs text-white/70 backdrop-blur-md",
                        "Sign in to unlock Following"
                    }
                }
            }
        }
    }
}
