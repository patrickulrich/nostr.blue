use dioxus::prelude::*;
use nostr_sdk::{Event as NostrEvent, EventId, Filter, Kind};
use crate::stores::nostr_client;
use crate::components::{PollCard, ClientInitializing};
use std::time::Duration;

#[component]
pub fn PollView(noteid: String) -> Element {
    // State for the poll
    let mut poll_event = use_signal(|| None::<NostrEvent>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Decode noteid and fetch poll - wait for client to be initialized
    use_effect(move || {
        let noteid_str = noteid.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            log::info!("Waiting for client initialization before loading poll...");
            return;
        }

        spawn(async move {
            loading.set(true);
            error.set(None);

            // Clear profile cache to prevent stale author metadata
            crate::stores::profiles::PROFILE_CACHE.write().clear();

            // Decode the note ID (support both bech32 and hex)
            match decode_event_id(&noteid_str) {
                Ok(event_id) => {
                    // Fetch the poll event
                    match fetch_poll_by_id(event_id).await {
                        Ok(Some(event)) => {
                            poll_event.set(Some(event));
                            loading.set(false);
                        }
                        Ok(None) => {
                            error.set(Some("Poll not found".to_string()));
                            loading.set(false);
                        }
                        Err(e) => {
                            error.set(Some(e));
                            loading.set(false);
                        }
                    }
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center gap-4",

                    // Back button
                    Link {
                        to: crate::routes::Route::Polls {},
                        class: "flex items-center gap-2 text-muted-foreground hover:text-foreground transition",
                        svg {
                            class: "w-5 h-5",
                            xmlns: "http://www.w3.org/2000/svg",
                            fill: "none",
                            view_box: "0 0 24 24",
                            stroke: "currentColor",
                            path {
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                stroke_width: "2",
                                d: "M15 19l-7-7 7-7"
                            }
                        }
                        "Back to Polls"
                    }

                    h1 {
                        class: "text-xl font-bold",
                        "📊 Poll"
                    }
                }
            }

            // Main content
            div {
                class: "max-w-2xl mx-auto",

                // Show client initializing
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if *loading.read() {
                    // Loading state
                    div {
                        class: "flex items-center justify-center py-12",
                        div {
                            class: "flex flex-col items-center gap-3 text-muted-foreground",
                            span {
                                class: "inline-block w-8 h-8 border-4 border-current border-t-transparent rounded-full animate-spin"
                            }
                            "Loading poll..."
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    // Error state
                    div {
                        class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "⚠️" }
                        h3 { class: "text-xl font-semibold mb-2", "Error" }
                        p { class: "text-muted-foreground mb-4", "{err}" }
                        Link {
                            to: crate::routes::Route::Polls {},
                            class: "inline-block px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Polls"
                        }
                    }
                } else if let Some(event) = poll_event.read().as_ref() {
                    // Poll display
                    div {
                        class: "border-b border-border",
                        PollCard {
                            event: event.clone()
                        }
                    }

                    // Additional info section
                    div {
                        class: "p-4 text-sm text-muted-foreground",
                        p {
                            "Poll ID: "
                            code {
                                class: "text-xs bg-muted px-2 py-1 rounded",
                                "{event.id.to_hex()}"
                            }
                        }
                    }
                } else {
                    // Empty state (shouldn't happen)
                    div {
                        class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "📊" }
                        h3 { class: "text-xl font-semibold mb-2", "Poll not found" }
                        Link {
                            to: crate::routes::Route::Polls {},
                            class: "inline-block mt-4 px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            "Back to Polls"
                        }
                    }
                }
            }
        }
    }
}

/// Decode event ID from bech32 (note1...) or hex format
fn decode_event_id(noteid: &str) -> Result<EventId, String> {
    // Try bech32 first (note1...)
    if noteid.starts_with("note1") {
        EventId::parse(noteid)
            .map_err(|e| format!("Invalid note ID (bech32): {}", e))
    } else {
        // Try hex
        EventId::from_hex(noteid)
            .map_err(|e| format!("Invalid note ID (hex): {}", e))
    }
}

/// Fetch a poll event by ID
async fn fetch_poll_by_id(event_id: EventId) -> Result<Option<NostrEvent>, String> {
    let filter = Filter::new()
        .id(event_id)
        .kind(Kind::Poll)
        .limit(1);

    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await
        .map_err(|e| format!("Failed to fetch poll: {}", e))?;

    Ok(events.into_iter().next())
}
