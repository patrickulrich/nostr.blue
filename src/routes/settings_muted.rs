use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::routes::Route;

#[component]
pub fn SettingsMuted() -> Element {
    let mut muted_posts = use_signal(|| Vec::<String>::new());
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);

    // Fetch muted posts on mount
    use_effect(move || {
        spawn(async move {
            match nostr_client::get_muted_posts().await {
                Ok(posts) => {
                    muted_posts.set(posts);
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to fetch muted posts: {}", e);
                    error_msg.set(Some(format!("Failed to load muted posts: {}", e)));
                    loading.set(false);
                }
            }
        });
    });

    let handle_unmute = move |event_id: String| {
        let event_id_clone = event_id.clone();
        spawn(async move {
            match nostr_client::unmute_post(event_id).await {
                Ok(_) => {
                    log::info!("Post unmuted successfully");
                    // Remove from local list
                    muted_posts.with_mut(|posts| {
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
                    "Muted Posts"
                }
                p {
                    class: "text-muted-foreground mt-2",
                    "Posts you've muted or reported"
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
                            "Loading muted posts..."
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
                if !*loading.read() && error_msg.read().is_none() && muted_posts.read().is_empty() {
                    div {
                        class: "p-8 text-center",
                        div {
                            class: "text-4xl mb-4",
                            "🔇"
                        }
                        h3 {
                            class: "text-lg font-semibold mb-2",
                            "No muted posts"
                        }
                        p {
                            class: "text-muted-foreground",
                            "Posts you mute or report will appear here"
                        }
                    }
                }

                // Muted posts list
                if !*loading.read() && error_msg.read().is_none() && !muted_posts.read().is_empty() {
                    div {
                        class: "divide-y divide-border",

                        for event_id in muted_posts.read().iter() {
                            div {
                                key: "{event_id}",
                                class: "p-4 flex items-center justify-between hover:bg-accent/50 transition",

                                div {
                                    class: "flex-1 min-w-0",
                                    Link {
                                        to: Route::Note { note_id: event_id.clone(), from_voice: None },
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

                    // Footer with count
                    div {
                        class: "p-4 bg-accent/30 text-sm text-muted-foreground text-center border-t border-border",
                        {
                            let count = muted_posts.read().len();
                            let word = if count == 1 { "post" } else { "posts" };
                            format!("{} muted {}", count, word)
                        }
                    }
                }
            }
        }
    }
}
