use crate::components::{ClientInitializing, VoiceMessageCard};
use crate::hooks::use_infinite_scroll;
use crate::stores::{auth_store, nostr_client};
use dioxus::prelude::*;
use nostr_sdk::{Event, Filter, Kind, PublicKey, Timestamp};
use std::time::Duration;
#[derive(Clone, Copy, PartialEq, Debug)]
enum FeedType {
    Following,
    Global,
}
impl FeedType {
    fn label(&self) -> &'static str {
        match self {
            FeedType::Following => "Following",
            FeedType::Global => "Global",
        }
    }
}
#[component]
pub fn VoiceMessages() -> Element {
    let mut events = use_signal(Vec::<Event>::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut refresh_trigger = use_signal(|| 0);
    let mut feed_type = use_signal(|| FeedType::Following);
    let mut show_dropdown = use_signal(|| false);
    let mut has_more = use_signal(|| true);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut request_generation = use_signal(|| 0u64);
    let mut last_loaded_trigger = use_signal(|| (0i32, FeedType::Following));
    use_effect(move || {
        let refresh = *refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        let (last_refresh, last_feed) = *last_loaded_trigger.peek();
        let has_data = !events.peek().is_empty();
        let feed_type_changed = current_feed_type != last_feed;
        let refresh_changed = refresh != last_refresh;
        if has_data && !feed_type_changed && !refresh_changed {
            log::debug!(
                "Skipping voice messages re-load: data already present, no intentional change"
            );
            return;
        }
        last_loaded_trigger.set((refresh, current_feed_type));
        if !has_data {
            loading.set(true);
        }
        error.set(None);
        oldest_timestamp.set(None);
        has_more.set(true);
        let next_gen = request_generation.with_mut(|gen| {
            *gen += 1;
            *gen
        });
        let captured_gen = next_gen;
        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_voice_messages(None).await,
                FeedType::Global => load_global_voice_messages(None).await,
            };
            if *request_generation.read() != captured_gen {
                log::debug!("Discarding stale voice messages request {}", captured_gen);
                return;
            }
            match result {
                Ok(voice_events) => {
                    if let Some(last_event) = voice_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }
                    has_more.set(voice_events.len() >= 50);
                    events.set(voice_events);
                    loading.set(false);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });
    let load_more = move || {
        if *loading.read() || !*has_more.read() {
            return;
        }
        let until = {
            let timestamp_opt = *oldest_timestamp.read();
            timestamp_opt.map(|t| t.saturating_sub(1))
        };
        let current_feed_type = *feed_type.read();
        let captured_generation = *request_generation.read();
        loading.set(true);
        spawn(async move {
            let result = match current_feed_type {
                FeedType::Following => load_following_voice_messages(until).await,
                FeedType::Global => load_global_voice_messages(until).await,
            };
            if captured_generation != *request_generation.read() {
                log::info!("Dropping stale load_more response (generation mismatch)");
                loading.set(false);
                return;
            }
            match result {
                Ok(mut new_events) => {
                    if let Some(last_event) = new_events.last() {
                        oldest_timestamp.set(Some(last_event.created_at.as_secs()));
                    }
                    has_more.set(new_events.len() >= 50);
                    let mut current = events.read().clone();
                    current.append(&mut new_events);
                    events.set(current);
                    loading.set(false);
                }
                Err(e) => {
                    log::error!("Failed to load more voice messages: {}", e);
                    loading.set(false);
                }
            }
        });
    };
    let sentinel_id = use_infinite_scroll(load_more, has_more, loading);
    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "px-4 py-3 flex items-center justify-between",
                    div { class: "relative",
                        button {
                            class: "text-xl font-bold flex items-center gap-2 hover:bg-accent px-3 py-1 rounded-lg transition",
                            onclick: move |_| {
                                let current = *show_dropdown.read();
                                show_dropdown.set(!current);
                            },
                            "🎤 {feed_type.read().label()}"
                            span { class: "text-sm", "▼" }
                        }
                        if *show_dropdown.read() {
                            div { class: "absolute top-full left-0 mt-1 bg-card border border-border rounded-lg shadow-lg overflow-hidden z-30 min-w-[150px]",
                                button {
                                    class: "w-full px-4 py-2 text-left hover:bg-accent transition",
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Following);
                                        show_dropdown.set(false);
                                        refresh_trigger.with_mut(|v| *v += 1);
                                    },
                                    "Following"
                                }
                                button {
                                    class: "w-full px-4 py-2 text-left hover:bg-accent transition",
                                    onclick: move |_| {
                                        feed_type.set(FeedType::Global);
                                        show_dropdown.set(false);
                                        refresh_trigger.with_mut(|v| *v += 1);
                                    },
                                    "Global"
                                }
                            }
                        }
                    }
                    button {
                        class: "px-4 py-2 text-sm rounded-lg hover:bg-accent transition",
                        onclick: move |_| {
                            refresh_trigger.with_mut(|v| *v += 1);
                        },
                        "↻ Refresh"
                    }
                }
            }
            div { class: "max-w-2xl mx-auto",
                if !*nostr_client::CLIENT_INITIALIZED.read() {
                    ClientInitializing {}
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "⚠️" }
                        h3 { class: "text-xl font-semibold mb-2", "Error" }
                        p { class: "text-muted-foreground", "{err}" }
                        button {
                            class: "mt-4 px-6 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition",
                            onclick: move |_| {
                                refresh_trigger.with_mut(|v| *v += 1);
                            },
                            "Try Again"
                        }
                    }
                } else if events.read().is_empty() && !*loading.read() {
                    div { class: "text-center py-12 px-4",
                        div { class: "text-6xl mb-4", "🎤" }
                        h3 { class: "text-xl font-semibold mb-2", "No voice messages yet" }
                        p { class: "text-muted-foreground",
                            if *feed_type.read() == FeedType::Following {
                                "Voice messages from people you follow will appear here"
                            } else {
                                "Voice messages from everyone will appear here"
                            }
                        }
                    }
                } else {
                    div { class: "divide-y divide-border",
                        for event in events.read().iter() {
                            VoiceMessageCard { key: "{event.id}", event: event.clone() }
                        }
                    }
                    div { id: "{sentinel_id}", class: "p-8 flex justify-center",
                        if *loading.read() {
                            div { class: "flex items-center gap-3 text-muted-foreground",
                                span { class: "inline-block w-5 h-5 border-2 border-current border-t-transparent rounded-full animate-spin" }
                                "Loading more..."
                            }
                        } else if !*has_more.read() {
                            p { class: "text-muted-foreground text-sm",
                                "No more voice messages to load"
                            }
                        }
                    }
                }
            }
        }
    }
}
/// Load voice messages from followed users
async fn load_following_voice_messages(until: Option<u64>) -> Result<Vec<Event>, String> {
    let pubkey_str = auth_store::get_pubkey()
        .ok_or("Not authenticated. Please sign in to view your following feed.")?;
    let contacts = nostr_client::fetch_contacts(pubkey_str).await?;
    let authors: Vec<PublicKey> = contacts
        .iter()
        .filter_map(|c| PublicKey::parse(c).ok())
        .collect();
    if authors.is_empty() {
        return Ok(Vec::new());
    }
    let mut filter = Filter::new()
        .kinds(vec![Kind::VoiceMessage, Kind::VoiceMessageReply])
        .authors(authors)
        .limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch voice messages: {}", e))?;
    let mut event_vec: Vec<Event> = events.into_iter().collect();
    event_vec.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(event_vec)
}
/// Load voice messages from everyone (global feed)
async fn load_global_voice_messages(until: Option<u64>) -> Result<Vec<Event>, String> {
    let mut filter = Filter::new()
        .kinds(vec![Kind::VoiceMessage, Kind::VoiceMessageReply])
        .limit(50);
    if let Some(until_ts) = until {
        filter = filter.until(Timestamp::from(until_ts));
    }
    let events = nostr_client::fetch_events_aggregated(filter, Duration::from_secs(10))
        .await
        .map_err(|e| format!("Failed to fetch voice messages: {}", e))?;
    let mut event_vec: Vec<Event> = events.into_iter().collect();
    event_vec.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(event_vec)
}
