use crate::hooks::{use_nostr_resource, NostrResourceState};
use crate::routes::Route;
use crate::stores::nostr_client;
use dioxus::prelude::*;

#[component]
pub fn SettingsMuted() -> Element {
    let muted_posts = use_nostr_resource(move || {
        async move {
            nostr_client::get_muted_posts()
                .await
                .map_err(|e| format!("Failed to load muted posts: {}", e))
        }
    });
    let mut muted_list = use_signal(Vec::<String>::new);
    {
        let state = muted_posts.state();
        use_effect(move || {
            if let NostrResourceState::Loaded(data) = &*state.read() {
                muted_list.set(data.clone());
            }
        });
    }
    let handle_unmute = move |event_id: String| {
        let event_id_clone = event_id.clone();
        spawn(async move {
            match nostr_client::unmute_post(event_id).await {
                Ok(_) => {
                    log::info!("Post unmuted successfully");
                    muted_list.with_mut(|posts| {
                        posts.retain(|p| p != &event_id_clone);
                    });
                }
                Err(e) => {
                    log::error!("Failed to unmute post: {}", e);
                }
            }
        });
    };
    rsx! {
        div { class: "max-w-2xl mx-auto px-4 py-6",
            div { class: "mb-6",
                Link {
                    to: Route::Settings {},
                    class: "text-sm text-primary hover:underline mb-4 inline-block",
                    "← Back to Settings"
                }
                h1 { class: "text-2xl font-bold", "Muted Posts" }
                p { class: "text-muted-foreground mt-2", "Posts you've muted or reported" }
            }
            div { class: "bg-background border border-border rounded-lg shadow-xs",
                match &*muted_posts.state().read() {
                    NostrResourceState::Initializing | NostrResourceState::Loading => rsx! {
                        div { class: "p-8 text-center",
                            div { class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto mb-4" }
                            p { class: "text-muted-foreground", "Loading muted posts..." }
                        }
                    },
                    NostrResourceState::AuthRequired => rsx! {
                        div { class: "p-8 text-center",
                            p { class: "text-muted-foreground", "Sign in required" }
                        }
                    },
                    NostrResourceState::Error(e) => rsx! {
                        div { class: "p-8",
                            div { class: "bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-red-600",
                                "{e}"
                            }
                        }
                    },
                    NostrResourceState::Loaded(_) if muted_list.read().is_empty() => rsx! {
                        div { class: "p-8 text-center",
                            div { class: "text-4xl mb-4", "🔇" }
                            h3 { class: "text-lg font-semibold mb-2", "No muted posts" }
                            p { class: "text-muted-foreground",
                                "Posts you mute or report will appear here"
                            }
                        }
                    },
                    NostrResourceState::Loaded(_) => rsx! {
                        div { class: "divide-y divide-border",
                            for event_id in muted_list.read().iter() {
                                div {
                                    key: "{event_id}",
                                    class: "p-4 flex items-center justify-between hover:bg-accent/50 transition",
                                    div { class: "flex-1 min-w-0",
                                        Link {
                                            to: Route::Note {
                                                note_id: crate::utils::nip19_urls::note_route_id(event_id, None),
                                                from_voice: None,
                                            },
                                            class: "font-mono text-sm text-muted-foreground hover:text-foreground hover:underline truncate block",
                                            if event_id.len() > 40 {
                                                "{&event_id[..16]}...{&event_id[event_id.len()-16..]}"
                                            } else {
                                                "{event_id}"
                                            }
                                        }
                                    }
                                    button {
                                        class: "px-4 py-2 text-sm bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg transition",
                                        onclick: {
                                            let eid = event_id.clone();
                                            move |_| handle_unmute(eid.clone())
                                        },
                                        "Unmute"
                                    }
                                }
                            }
                        }
                        div { class: "p-4 bg-accent/30 text-sm text-muted-foreground text-center border-t border-border",
                            {
                                let count = muted_list.read().len();
                                let word = if count == 1 { "post" } else { "posts" };
                                format!("{} muted {}", count, word)
                            }
                        }
                    },
                }
            }
        }
    }
}
