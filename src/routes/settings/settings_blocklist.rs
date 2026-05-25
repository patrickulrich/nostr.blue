use crate::hooks::{use_nostr_resource, NostrResourceState};
use crate::routes::Route;
use crate::stores::{nostr_client, profiles};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Clone, PartialEq)]
struct BlocklistData {
    users: Vec<String>,
    profiles: HashMap<String, profiles::Profile>,
}

#[component]
pub fn SettingsBlocklist() -> Element {
    let data = use_nostr_resource(move || async move {
        let users = nostr_client::get_blocked_users()
            .await
            .map_err(|e| format!("Failed to load blocked users: {}", e))?;
        let mut profs = HashMap::new();
        if !users.is_empty() {
            let _ = profiles::fetch_profiles_batch(users.clone())
                .await
                .map(|p| profs = p);
        }
        Ok(BlocklistData {
            users,
            profiles: profs,
        })
    });
    let mut blocked_users = use_signal(Vec::<String>::new);
    let mut user_profiles = use_signal(HashMap::<String, profiles::Profile>::new);
    {
        let state = data.state();
        use_effect(move || {
            if let NostrResourceState::Loaded(d) = &*state.read() {
                blocked_users.set(d.users.clone());
                user_profiles.set(d.profiles.clone());
            }
        });
    }
    let handle_unblock = move |pubkey: String| {
        let pubkey_clone = pubkey.clone();
        spawn(async move {
            match nostr_client::unblock_user(pubkey).await {
                Ok(_) => {
                    log::info!("User unblocked successfully");
                    blocked_users.with_mut(|users| {
                        users.retain(|u| u != &pubkey_clone);
                    });
                    user_profiles.with_mut(|profiles_map| {
                        profiles_map.remove(&pubkey_clone);
                    });
                }
                Err(e) => {
                    log::error!("Failed to unblock user: {}", e);
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
                h1 { class: "text-2xl font-bold", "Blocked Users" }
                p { class: "text-muted-foreground mt-2",
                    "Users you've blocked won't appear in your feeds"
                }
            }
            div { class: "bg-background border border-border rounded-lg shadow-xs",
                match &*data.state().read() {
                    NostrResourceState::Initializing | NostrResourceState::Loading => rsx! {
                        div { class: "p-8 text-center",
                            div { class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto mb-4" }
                            p { class: "text-muted-foreground", "Loading blocked users..." }
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
                    NostrResourceState::Loaded(_) if blocked_users.read().is_empty() => rsx! {
                        div { class: "p-8 text-center",
                            div { class: "text-4xl mb-4", "🚫" }
                            h3 { class: "text-lg font-semibold mb-2", "No blocked users" }
                            p { class: "text-muted-foreground", "Users you block will appear here" }
                        }
                    },
                    NostrResourceState::Loaded(_) => rsx! {
                        div { class: "divide-y divide-border",
                            for pubkey in blocked_users.read().iter() {
                                div {
                                    key: "{pubkey}",
                                    class: "p-4 flex items-center justify-between hover:bg-accent/50 transition",
                                    div { class: "flex-1 min-w-0",
                                        Link {
                                            to: Route::AddressViewer {
                                                address: crate::utils::nip19_urls::profile_route_id(pubkey),
                                            },
                                            class: "hover:text-foreground hover:underline truncate block",
                                            div { class: "font-semibold text-sm",
                                                {
                                                    user_profiles
                                                        .read()
                                                        .get(pubkey)
                                                        .map(|p| p.get_display_name())
                                                        .unwrap_or_else(|| truncate_pubkey(pubkey))
                                                }
                                            }
                                            div { class: "font-mono text-xs text-muted-foreground",
                                                if pubkey.len() > 40 {
                                                    "{&pubkey[..16]}...{&pubkey[pubkey.len()-16..]}"
                                                } else {
                                                    "{pubkey}"
                                                }
                                            }
                                        }
                                    }
                                    button {
                                        class: "px-4 py-2 text-sm bg-primary hover:bg-primary/90 text-primary-foreground rounded-lg transition",
                                        onclick: {
                                            let pk = pubkey.clone();
                                            move |_| handle_unblock(pk.clone())
                                        },
                                        "Unblock"
                                    }
                                }
                            }
                        }
                        div { class: "p-4 bg-accent/30 text-sm text-muted-foreground text-center border-t border-border",
                            {
                                let count = blocked_users.read().len();
                                let word = if count == 1 { "user" } else { "users" };
                                format!("{} blocked {}", count, word)
                            }
                        }
                    },
                }
            }
        }
    }
}
