use dioxus::prelude::*;
use crate::components::icons::MoreHorizontalIcon;
use crate::stores::nostr_client;

#[derive(Props, Clone, PartialEq)]
pub struct NoteMenuProps {
    /// Public key of the note author
    pub author_pubkey: String,
    /// Event ID of the note
    pub event_id: String,
}

#[component]
pub fn NoteMenu(props: NoteMenuProps) -> Element {
    let mut is_open = use_signal(|| false);
    let mut is_following = use_signal(|| false);
    let mut is_loading_follow_state = use_signal(|| true);
    let mut is_updating_follow = use_signal(|| false);

    // Clone props for use in closures
    let author_pubkey = props.author_pubkey.clone();
    let author_pubkey_follow_check = author_pubkey.clone();
    let author_pubkey_follow_action = author_pubkey.clone();
    let author_pubkey_block = author_pubkey.clone();
    let event_id = props.event_id.clone();
    let event_id_list = event_id.clone();
    let event_id_mute = event_id.clone();
    let event_id_report = event_id.clone();

    // Check follow status on mount
    use_effect(use_reactive(&author_pubkey_follow_check, move |pubkey| {
        spawn(async move {
            match nostr_client::is_following(pubkey).await {
                Ok(following) => {
                    is_following.set(following);
                    is_loading_follow_state.set(false);
                }
                Err(e) => {
                    log::warn!("Failed to check follow status: {}", e);
                    is_loading_follow_state.set(false);
                }
            }
        });
    }));

    rsx! {
        div {
            class: "relative",

            // Menu button
            button {
                class: "p-2 rounded-full hover:bg-accent transition-colors text-muted-foreground hover:text-foreground",
                onclick: move |e: MouseEvent| {
                    e.stop_propagation();
                    is_open.set(!is_open());
                },
                MoreHorizontalIcon {
                    class: "h-5 w-5".to_string(),
                    filled: false
                }
            }

            // Dropdown menu
            if *is_open.read() {
                // Backdrop to close menu when clicking outside
                div {
                    class: "fixed inset-0 z-40",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        is_open.set(false);
                    }
                }

                // Menu content
                div {
                    class: "absolute right-0 mt-2 w-48 bg-background border border-border rounded-lg shadow-lg z-50 py-1",

                    // Follow/Unfollow user
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        disabled: *is_loading_follow_state.read() || *is_updating_follow.read(),
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();

                            let pubkey = author_pubkey_follow_action.clone();
                            let currently_following = *is_following.read();

                            is_updating_follow.set(true);
                            is_open.set(false);

                            spawn(async move {
                                let result = if currently_following {
                                    nostr_client::unfollow_user(pubkey.clone()).await
                                } else {
                                    nostr_client::follow_user(pubkey.clone()).await
                                };

                                match result {
                                    Ok(_) => {
                                        // Update local state optimistically
                                        is_following.set(!currently_following);
                                        log::info!("{} user: {}",
                                            if currently_following { "Unfollowed" } else { "Followed" },
                                            pubkey
                                        );
                                    }
                                    Err(e) => {
                                        log::error!("Failed to {} user: {}",
                                            if currently_following { "unfollow" } else { "follow" },
                                            e
                                        );
                                    }
                                }
                                is_updating_follow.set(false);
                            });
                        },
                        span {
                            class: "text-sm",
                            {
                                if *is_loading_follow_state.read() {
                                    "Loading...".to_string()
                                } else if *is_updating_follow.read() {
                                    if *is_following.read() {
                                        "Unfollowing...".to_string()
                                    } else {
                                        "Following...".to_string()
                                    }
                                } else if *is_following.read() {
                                    "Unfollow user".to_string()
                                } else {
                                    "Follow user".to_string()
                                }
                            }
                        }
                    }

                    // Add to list
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            // TODO: Implement add to list
                            log::info!("Add to list: {}", event_id_list);
                            is_open.set(false);
                        },
                        span {
                            class: "text-sm",
                            "Add to list"
                        }
                    }

                    // Divider
                    div {
                        class: "h-px bg-border my-1"
                    }

                    // Mute post
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-muted-foreground",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            // TODO: Implement mute post
                            log::info!("Mute post: {}", event_id_mute);
                            is_open.set(false);
                        },
                        span {
                            class: "text-sm",
                            "Mute post"
                        }
                    }

                    // Block user
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-muted-foreground",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            // TODO: Implement block user
                            log::info!("Block user: {}", author_pubkey_block);
                            is_open.set(false);
                        },
                        span {
                            class: "text-sm",
                            "Block user"
                        }
                    }

                    // Report post
                    button {
                        class: "w-full text-left px-4 py-2 hover:bg-accent transition-colors flex items-center gap-2 text-red-500 hover:text-red-600",
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            // TODO: Implement report post
                            log::info!("Report post: {}", event_id_report);
                            is_open.set(false);
                        },
                        span {
                            class: "text-sm",
                            "Report post"
                        }
                    }
                }
            }
        }
    }
}
