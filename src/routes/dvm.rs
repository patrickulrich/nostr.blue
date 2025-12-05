//! DVM Content Discovery Page
//!
//! Displays a feed of notes recommended by a Data Vending Machine (DVM).
//! Users can select which DVM provider to use via a gear icon.

use dioxus::prelude::*;
use crate::stores::{nostr_client, dvm_store};
use crate::stores::dvm_store::{DVM_FEED_EVENTS, DVM_FEED_LOADING, DVM_FEED_ERROR, DVM_PROVIDERS, SELECTED_DVM_PROVIDER};
use crate::components::{NoteCard, ClientInitializing, DvmSelectorModal};
use crate::services::aggregation::{InteractionCounts, fetch_interaction_counts_batch};
use nostr_sdk::PublicKey;
use std::collections::HashMap;
use std::time::Duration;

/// Main DVM page component
#[component]
pub fn DVM() -> Element {
    let mut show_selector = use_signal(|| false);
    let mut refresh_trigger = use_signal(|| 0);

    // Interaction counts cache (event_id -> counts) for batch optimization
    let mut interaction_counts = use_signal(|| HashMap::<String, InteractionCounts>::new());
    let mut interactions_loaded = use_signal(|| false);
    let mut fetch_in_progress = use_signal(|| false);

    let feed_loading = *DVM_FEED_LOADING.read();
    let feed_error = DVM_FEED_ERROR.read().clone();
    let feed_events = DVM_FEED_EVENTS.read().clone();
    let selected_provider = SELECTED_DVM_PROVIDER.read().clone();

    // Load DVMs and feed on mount and when client initializes
    use_effect(move || {
        // Subscribe to both refresh_trigger AND client_initialized
        let _ = refresh_trigger.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();

        if !client_initialized {
            return;
        }

        // Reset interaction counts on refresh
        interactions_loaded.set(false);
        fetch_in_progress.set(false);

        // Discover DVMs and request content feed in parallel
        let provider = *SELECTED_DVM_PROVIDER.peek();
        spawn(async move {
            let (discover_result, feed_result) = futures::join!(
                dvm_store::discover_content_dvms(),
                dvm_store::request_content_feed(provider)
            );
            if let Err(e) = discover_result {
                log::error!("Failed to discover DVMs: {}", e);
            }
            if let Err(e) = feed_result {
                log::error!("Failed to request content feed: {}", e);
            }
        });
    });

    // Fetch interaction counts when feed events change
    use_effect(move || {
        let events = DVM_FEED_EVENTS.read().clone();

        if events.is_empty() {
            return;
        }

        // Only fetch if not already loaded for this batch and no fetch in progress
        // This guards against race conditions where effect runs multiple times
        if *interactions_loaded.peek() || *fetch_in_progress.peek() {
            return;
        }

        fetch_in_progress.set(true);

        spawn(async move {
            let event_ids: Vec<_> = events.iter().map(|e| e.id).collect();
            match fetch_interaction_counts_batch(event_ids, Duration::from_secs(5)).await {
                Ok(counts) => {
                    interaction_counts.set(counts);
                    interactions_loaded.set(true);
                }
                Err(e) => {
                    log::error!("Failed to fetch interaction counts: {}", e);
                }
            }
            fetch_in_progress.set(false);
        });
    });

    // Get current provider name for display
    let current_provider_name = {
        let providers = DVM_PROVIDERS.read();
        if let Some(pubkey) = selected_provider {
            providers.iter()
                .find(|p| p.pubkey == pubkey)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "Selected DVM".to_string())
        } else {
            "Default DVM".to_string()
        }
    };

    rsx! {
        div {
            class: "min-h-screen",

            // Header
            div {
                class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div {
                    class: "px-4 py-3 flex items-center justify-between",
                    div {
                        h2 {
                            class: "text-xl font-bold",
                            "Discover"
                        }
                        p {
                            class: "text-sm text-muted-foreground",
                            "Powered by {current_provider_name}"
                        }
                    }
                    div {
                        class: "flex items-center gap-2",
                        // Refresh button
                        button {
                            class: "p-2 hover:bg-accent rounded-full transition disabled:opacity-50",
                            disabled: feed_loading,
                            onclick: move |_| {
                                dvm_store::clear_feed();
                                let next = *refresh_trigger.peek() + 1;
                                refresh_trigger.set(next);
                            },
                            title: "Refresh feed",
                            if feed_loading {
                                span {
                                    class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin"
                                }
                            } else {
                                "🔄"
                            }
                        }
                        // Settings/DVM selector button
                        button {
                            class: "p-2 hover:bg-accent rounded-full transition",
                            onclick: move |_| show_selector.set(true),
                            title: "Select DVM provider",
                            "⚙️"
                        }
                    }
                }
            }

            // Content
            if !*nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else if feed_loading && feed_events.is_empty() {
                // Loading state
                div {
                    class: "flex flex-col items-center justify-center py-20 gap-4",
                    span {
                        class: "inline-block w-8 h-8 border-2 border-blue-500 border-t-transparent rounded-full animate-spin"
                    }
                    p {
                        class: "text-muted-foreground",
                        "Requesting content from DVM..."
                    }
                }
            } else if let Some(error) = feed_error {
                // Error state
                div {
                    class: "p-6 text-center",
                    div {
                        class: "max-w-md mx-auto",
                        div {
                            class: "text-4xl mb-4",
                            "⚠️"
                        }
                        h3 {
                            class: "text-lg font-semibold mb-2",
                            "Failed to load feed"
                        }
                        p {
                            class: "text-muted-foreground text-sm mb-4",
                            "{error}"
                        }
                        button {
                            class: "px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition",
                            onclick: move |_| {
                                dvm_store::clear_feed();
                                let next = *refresh_trigger.peek() + 1;
                                refresh_trigger.set(next);
                            },
                            "Try Again"
                        }
                    }
                }
            } else if feed_events.is_empty() {
                // Empty state
                div {
                    class: "p-6 text-center",
                    div {
                        class: "max-w-md mx-auto",
                        div {
                            class: "text-6xl mb-4",
                            "🔍"
                        }
                        h3 {
                            class: "text-lg font-semibold mb-2",
                            "No content yet"
                        }
                        p {
                            class: "text-muted-foreground text-sm",
                            "The DVM hasn't returned any content. Try selecting a different provider or refreshing."
                        }
                    }
                }
            } else {
                // Feed content
                div {
                    class: "divide-y divide-border",
                    for event in feed_events.iter() {
                        NoteCard {
                            key: "{event.id.to_hex()}",
                            event: event.clone(),
                            precomputed_counts: interaction_counts.read().get(&event.id.to_hex()).cloned(),
                            collapsible: true
                        }
                    }
                }
            }

            // DVM Selector Modal
            if *show_selector.read() {
                DvmSelectorModal {
                    on_close: move |_| show_selector.set(false),
                    on_select: move |pubkey: Option<PublicKey>| {
                        dvm_store::set_selected_provider(pubkey);
                        show_selector.set(false);
                        dvm_store::clear_feed();
                        let next = *refresh_trigger.peek() + 1;
                        refresh_trigger.set(next);
                    }
                }
            }
        }
    }
}

