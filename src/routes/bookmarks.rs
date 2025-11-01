use dioxus::prelude::*;
use crate::stores::{auth_store, bookmarks, nostr_client};
use crate::components::{NoteCard, ClientInitializing};
use nostr_sdk::Event as NostrEvent;

#[component]
pub fn Bookmarks() -> Element {
    let auth = auth_store::AUTH_STATE.read();
    let mut bookmarked_events = use_signal(|| Vec::<NostrEvent>::new());
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    // Load bookmarks on mount
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !auth_store::is_authenticated() {
            return;
        }

        // Only load if client is initialized
        if !client_initialized {
            return;
        }

        loading.set(true);
        error.set(None);

        spawn(async move {
            // Initialize bookmarks from relays
            match bookmarks::init_bookmarks().await {
                Ok(_) => {
                    // Fetch actual events
                    match bookmarks::fetch_bookmarked_events().await {
                        Ok(events) => {
                            bookmarked_events.set(events);
                        }
                        Err(e) => {
                            error.set(Some(e));
                        }
                    }
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
            loading.set(false);
        });
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3",
                    h2 {
                        class: "text-xl font-bold",
                        "🔖 Bookmarks"
                    }
                }
            }

            // Not authenticated
            if !auth.is_authenticated {
                div {
                    class: "text-center py-12",
                    div {
                        class: "text-6xl mb-4",
                        "🔐"
                    }
                    h3 {
                        class: "text-xl font-semibold mb-2",
                        "Sign in to view bookmarks"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Connect your account to save and view bookmarked posts"
                    }
                }
            } else {
                // Error state
                if let Some(err) = error.read().as_ref() {
                    div {
                        class: "p-4",
                        div {
                            class: "p-4 bg-red-100 dark:bg-red-900 text-red-800 dark:text-red-200 rounded-lg",
                            "❌ {err}"
                        }
                    }
                }

                // Loading state
                if !*nostr_client::CLIENT_INITIALIZED.read() || (*loading.read() && bookmarked_events.read().is_empty()) {
                    // Show client initializing animation during:
                    // 1. Client initialization
                    // 2. Initial bookmarks load (loading + no bookmarks, regardless of error state)
                    ClientInitializing {}
                } else if bookmarked_events.read().is_empty() {
                    div {
                        class: "text-center py-12",
                        div {
                            class: "text-6xl mb-4",
                            "📭"
                        }
                        h3 {
                            class: "text-xl font-semibold mb-2",
                            "No bookmarks yet"
                        }
                        p {
                            class: "text-muted-foreground mb-4",
                            "Bookmark posts to save them for later"
                        }
                        p {
                            class: "text-sm text-muted-foreground",
                            "Tip: Click the bookmark button on any post to save it"
                        }
                    }
                } else {
                    div {
                        class: "space-y-4 p-4",
                        p {
                            class: "text-sm text-muted-foreground mb-4",
                            "You have {bookmarked_events.read().len()} bookmarked post(s)"
                        }
                        for event in bookmarked_events.read().iter() {
                            NoteCard {
                                event: event.clone()
                            }
                        }
                    }
                }
            }
        }
    }
}
