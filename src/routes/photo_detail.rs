use dioxus::prelude::*;
use crate::stores::nostr_client;
use crate::components::PhotoCard;
use nostr_sdk::{Event, Filter, Kind, EventId};
use std::time::Duration;

#[component]
pub fn PhotoDetail(photo_id: String) -> Element {
    let mut photo_event = use_signal(|| None::<Event>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);

    // Load photo on mount - wait for client to be initialized
    use_effect(move || {
        let id = photo_id.clone();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        // Only load if client is initialized
        if !client_initialized {
            log::info!("Waiting for client initialization before loading photo...");
            return;
        }

        loading.set(true);
        error.set(None);

        spawn(async move {
            match load_photo_by_id(&id).await {
                Ok(event) => {
                    photo_event.set(Some(event));
                    loading.set(false);
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
                    class: "px-4 py-3 flex items-center gap-3",
                    Link {
                        to: crate::routes::Route::Photos {},
                        class: "hover:bg-accent p-2 rounded-full transition",
                        "← Back"
                    }
                    h2 {
                        class: "text-xl font-bold",
                        "Photo"
                    }
                }
            }

            // Content
            div {
                class: "max-w-[600px] mx-auto",

                if *loading.read() {
                    // Loading skeleton
                    div {
                        class: "border-b border-border bg-background pb-4",
                        div {
                            class: "p-3 flex items-center gap-3 mb-2",
                            div {
                                class: "w-8 h-8 rounded-full bg-gray-200 dark:bg-gray-800 animate-pulse"
                            }
                            div {
                                class: "flex-1",
                                div {
                                    class: "h-4 w-24 bg-gray-200 dark:bg-gray-800 rounded animate-pulse"
                                }
                            }
                        }
                        div {
                            class: "relative w-full pb-[100%] bg-gray-200 dark:bg-gray-800 animate-pulse"
                        }
                    }
                } else if let Some(err) = error.read().as_ref() {
                    // Error state
                    div {
                        class: "p-6 text-center",
                        div {
                            class: "max-w-md mx-auto",
                            div {
                                class: "text-4xl mb-2",
                                "⚠️"
                            }
                            p {
                                class: "text-red-600 dark:text-red-400 mb-4",
                                "Error loading photo: {err}"
                            }
                            Link {
                                to: crate::routes::Route::Photos {},
                                class: "text-blue-500 hover:underline",
                                "← Back to Photos"
                            }
                        }
                    }
                } else if let Some(event) = photo_event.read().as_ref() {
                    // Show photo card
                    PhotoCard {
                        event: event.clone()
                    }
                } else {
                    // Empty state
                    div {
                        class: "p-6 text-center",
                        div {
                            class: "max-w-md mx-auto",
                            div {
                                class: "text-4xl mb-2",
                                "📷"
                            }
                            p {
                                class: "text-muted-foreground mb-4",
                                "Photo not found"
                            }
                            Link {
                                to: crate::routes::Route::Photos {},
                                class: "text-blue-500 hover:underline",
                                "← Back to Photos"
                            }
                        }
                    }
                }
            }
        }
    }
}

// Helper function to load a single photo by ID
async fn load_photo_by_id(photo_id: &str) -> Result<Event, String> {
    log::info!("Loading photo by ID: {}", photo_id);

    // Parse the event ID (could be hex or note1...)
    let event_id = EventId::parse(photo_id)
        .map_err(|e| format!("Invalid photo ID: {}", e))?;

    // Create filter for this specific event
    let filter = Filter::new()
        .id(event_id)
        .kind(Kind::Custom(20))
        .limit(1);

    log::info!("Fetching photo event with filter: {:?}", filter);

    // Fetch event from relays
    match nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10)).await {
        Ok(events) => {
            if let Some(event) = events.into_iter().next() {
                log::info!("Loaded photo event: {}", event.id);
                Ok(event)
            } else {
                Err("Photo not found".to_string())
            }
        }
        Err(e) => {
            log::error!("Failed to fetch photo: {}", e);
            Err(format!("Failed to load photo: {}", e))
        }
    }
}
