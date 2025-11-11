use dioxus::prelude::*;
use crate::stores::{nostr_client, profiles};
use crate::routes::Route;
use std::collections::HashMap;

#[component]
pub fn SettingsBlocklist() -> Element {
    let mut blocked_users = use_signal(|| Vec::<String>::new());
    let mut user_profiles = use_signal(|| HashMap::<String, profiles::Profile>::new());
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let refresh_trigger = use_signal(|| 0);

    // Fetch blocked users on mount and when refresh_trigger changes
    use_effect(use_reactive(&*refresh_trigger.read(), move |_| {
        loading.set(true);
        spawn(async move {
            match nostr_client::get_blocked_users().await {
                Ok(users) => {
                    blocked_users.set(users.clone());
                    loading.set(false);

                    // Fetch profiles for all blocked users
                    if !users.is_empty() {
                        match profiles::fetch_profiles_batch(users).await {
                            Ok(profiles_map) => {
                                user_profiles.set(profiles_map);
                            }
                            Err(e) => {
                                log::warn!("Failed to fetch profiles: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch blocked users: {}", e);
                    error_msg.set(Some(format!("Failed to load blocked users: {}", e)));
                    loading.set(false);
                }
            }
        });
    }));

    let handle_unblock = move |pubkey: String| {
        let pubkey_clone = pubkey.clone();
        spawn(async move {
            match nostr_client::unblock_user(pubkey).await {
                Ok(_) => {
                    log::info!("User unblocked successfully");
                    // Remove from local list
                    blocked_users.with_mut(|users| {
                        users.retain(|u| u != &pubkey_clone);
                    });
                    // Also remove from profiles map
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
        div {
            class: "max-w-2xl mx-auto px-4 py-6",

            // Header with back button
            div {
                class: "mb-6",
                Link {
                    to: Route::Settings {},
                    class: "text-sm text-primary hover:underline mb-4 inline-block",
                    "← Back to Settings"
                }
                h1 {
                    class: "text-2xl font-bold",
                    "Blocked Users"
                }
                p {
                    class: "text-muted-foreground mt-2",
                    "Users you've blocked won't appear in your feeds"
                }
            }

            // Content
            div {
                class: "bg-background border border-border rounded-lg shadow-sm",

                // Loading state
                if *loading.read() {
                    div {
                        class: "p-8 text-center",
                        div {
                            class: "animate-spin rounded-full h-8 w-8 border-b-2 border-primary mx-auto mb-4"
                        }
                        p {
                            class: "text-muted-foreground",
                            "Loading blocked users..."
                        }
                    }
                }

                // Error state
                if let Some(err) = error_msg.read().as_ref() {
                    div {
                        class: "p-8",
                        div {
                            class: "bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-red-600",
                            "{err}"
                        }
                    }
                }

                // Empty state
                if !*loading.read() && error_msg.read().is_none() && blocked_users.read().is_empty() {
                    div {
                        class: "p-8 text-center",
                        div {
                            class: "text-4xl mb-4",
                            "🚫"
                        }
                        h3 {
                            class: "text-lg font-semibold mb-2",
                            "No blocked users"
                        }
                        p {
                            class: "text-muted-foreground",
                            "Users you block will appear here"
                        }
                    }
                }

                // Blocked users list
                if !*loading.read() && error_msg.read().is_none() && !blocked_users.read().is_empty() {
                    div {
                        class: "divide-y divide-border",

                        for pubkey in blocked_users.read().iter() {
                            div {
                                key: "{pubkey}",
                                class: "p-4 flex items-center justify-between hover:bg-accent/50 transition",

                                div {
                                    class: "flex-1 min-w-0",
                                    a {
                                        href: "/profile/{pubkey}",
                                        class: "hover:text-foreground hover:underline truncate block",
                                        // Display name or fallback to truncated hex
                                        div {
                                            class: "font-semibold text-sm",
                                            {
                                                user_profiles.read().get(pubkey)
                                                    .map(|p| p.get_display_name())
                                                    .unwrap_or_else(|| format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..]))
                                            }
                                        }
                                        // Show hex as subtitle
                                        div {
                                            class: "font-mono text-xs text-muted-foreground",
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

                    // Footer with count
                    div {
                        class: "p-4 bg-accent/30 text-sm text-muted-foreground text-center border-t border-border",
                        {
                            let count = blocked_users.read().len();
                            let word = if count == 1 { "user" } else { "users" };
                            format!("{} blocked {}", count, word)
                        }
                    }
                }
            }
        }
    }
}
