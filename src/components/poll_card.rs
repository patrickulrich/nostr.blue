use dioxus::prelude::*;
use nostr_sdk::{
    Event as NostrEvent, EventId, Filter, Kind, Timestamp, PublicKey,
    nips::nip88::{Poll, PollResponse, PollType},
    TagStandard,
};
use crate::routes::Route;
use crate::stores::nostr_client;
use crate::components::PollTimer;
use std::collections::HashMap;
use std::time::Duration;

#[component]
pub fn PollCard(event: NostrEvent) -> Element {
    // Clone values for closures
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_metadata = author_pubkey.clone();
    let author_pubkey_for_display = author_pubkey.clone();
    let author_pubkey_for_link = author_pubkey.clone();
    let event_id = event.id;
    let event_id_str = event_id.to_string();
    let event_clone = event.clone();
    let created_at = event.created_at;

    // State
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut poll_data = use_signal(|| None::<Poll>);
    let mut votes = use_signal(|| Vec::<NostrEvent>::new());
    let mut loading_votes = use_signal(|| true);
    let mut user_vote = use_signal(|| None::<NostrEvent>);
    let mut selected_options = use_signal(|| Vec::<String>::new());
    let mut show_results = use_signal(|| false);
    let mut is_voting = use_signal(|| false);

    // Fetch author metadata using use_future for automatic cancellation
    let _metadata_task = use_future(move || {
        let pubkey_str = author_pubkey_for_metadata.clone();
        async move {
            match PublicKey::parse(&pubkey_str) {
                Ok(pk) => {
                    if let Some(client) = nostr_client::get_client() {
                        if let Ok(Some(metadata)) = client.fetch_metadata(pk, Duration::from_secs(5)).await {
                            author_metadata.set(Some(metadata));
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to parse pubkey: {}", e);
                }
            }
        }
    });

    // Parse poll data using use_future for automatic cancellation
    let _poll_parse_task = use_future(move || {
        let evt = event_clone.clone();
        async move {
            match Poll::from_event(&evt) {
                Ok(poll) => poll_data.set(Some(poll)),
                Err(e) => log::error!("Failed to parse poll: {}", e),
            }
        }
    });

    // Fetch votes for this poll using use_future for automatic cancellation
    let _votes_task = use_future(move || {
        let poll_id = event_id;
        let poll = poll_data.read().clone();
        async move {
            loading_votes.set(true);
            let (ends_at, poll_relays) = poll.map(|p| (p.ends_at, p.relays)).unwrap_or((None, Vec::new()));
            match fetch_poll_votes(poll_id, ends_at, poll_relays).await {
                Ok(vote_events) => {
                    votes.set(vote_events.clone());

                    // Check if current user has voted
                    if let Ok(user_pubkey) = nostr_client::get_user_pubkey().await {
                        let user_pubkey_str = user_pubkey.to_string();
                        if let Some(user_vote_event) = vote_events.iter()
                            .find(|v| v.pubkey.to_string() == user_pubkey_str) {
                            user_vote.set(Some(user_vote_event.clone()));
                            show_results.set(true);
                        }
                    }
                }
                Err(e) => log::error!("Failed to fetch votes: {}", e),
            }
            loading_votes.set(false);
        }
    });

    // Calculate poll results
    let results = use_memo(move || {
        let poll = match poll_data.read().clone() {
            Some(p) => p,
            None => return HashMap::new(),
        };

        let vote_events = votes.read().clone();
        calculate_poll_results(&poll, vote_events)
    });

    // Submit vote with optimistic UI update
    let submit_vote = move |_| {
        let options = selected_options.read().clone();
        if options.is_empty() {
            return;
        }

        let poll_id = event_id;
        let poll = match poll_data.read().as_ref() {
            Some(p) => p.clone(),
            None => return,
        };

        // Capture previous state for rollback on error
        let previous_selected = selected_options.read().clone();
        let previous_show_results = *show_results.read();

        // Optimistically update UI immediately
        show_results.set(true);
        is_voting.set(true);

        spawn(async move {
            let response = match poll.r#type {
                PollType::SingleChoice => PollResponse::SingleChoice {
                    poll_id,
                    response: options.first().unwrap().clone(),
                },
                PollType::MultipleChoice => PollResponse::MultipleChoice {
                    poll_id,
                    responses: options,
                },
            };

            match nostr_client::publish_poll_vote(poll_id, response, poll.relays.clone()).await {
                Ok(_event_id) => {
                    log::info!("Vote published successfully");

                    // Refresh votes to get updated totals including the new vote
                    match fetch_poll_votes(poll_id, poll.ends_at, poll.relays.clone()).await {
                        Ok(vote_events) => {
                            votes.set(vote_events.clone());

                            // Update user vote state - find the user's vote in the refreshed list
                            if let Ok(user_pubkey) = nostr_client::get_user_pubkey().await {
                                let user_pubkey_str = user_pubkey.to_string();
                                if let Some(user_vote_event) = vote_events.iter()
                                    .find(|v| v.pubkey.to_string() == user_pubkey_str) {
                                    user_vote.set(Some(user_vote_event.clone()));
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to refresh votes after voting: {}", e);
                        }
                    }

                    // Clear selection and keep results visible
                    selected_options.set(Vec::new());
                }
                Err(e) => {
                    log::error!("Failed to publish vote: {}", e);
                    // Revert optimistic changes on error
                    selected_options.set(previous_selected);
                    show_results.set(previous_show_results);
                }
            }
            is_voting.set(false);
        });
    };

    // Get display data
    let poll = poll_data.read().clone();
    let poll_title = poll.as_ref().map(|p| p.title.clone()).unwrap_or_default();
    let poll_type = poll.as_ref().map(|p| p.r#type.clone()).unwrap_or(PollType::SingleChoice);
    let poll_options = poll.as_ref().map(|p| p.options.clone()).unwrap_or_default();
    let poll_ends_at = poll.as_ref().and_then(|p| p.ends_at);

    let author_name = author_metadata.read().as_ref()
        .and_then(|m| m.display_name.clone().or_else(|| m.name.clone()))
        .unwrap_or_else(|| format!("{}...{}", &author_pubkey_for_display[..8], &author_pubkey_for_display[author_pubkey_for_display.len()-8..]));

    let time_ago = format_time_ago(created_at);
    let total_votes: usize = results().values().sum();
    let has_voted = user_vote.read().is_some();

    let is_expired = poll_ends_at
        .map(|ends_at| ends_at < Timestamp::now())
        .unwrap_or(false);

    let show_voting_ui = !*show_results.read() && !has_voted && !is_expired;

    rsx! {
        div {
            class: "p-4 hover:bg-accent/50 transition border-b border-border",

            // Author and timestamp
            div {
                class: "flex items-center gap-2 mb-3",
                Link {
                    to: Route::Profile { pubkey: author_pubkey_for_link.clone() },
                    class: "font-semibold hover:underline",
                    "{author_name}"
                }
                span { class: "text-muted-foreground text-sm", "· {time_ago}" }
            }

            // Poll question
            Link {
                to: Route::PollView { noteid: event_id_str.clone() },
                div {
                    class: "mb-3",
                    p { class: "text-lg font-medium mb-2", "{poll_title}" }

                    div {
                        class: "flex items-center gap-3 text-sm text-muted-foreground",
                        span {
                            class: "px-2 py-1 rounded bg-primary/10 text-primary text-xs",
                            {match poll_type {
                                PollType::SingleChoice => "Single Choice",
                                PollType::MultipleChoice => "Multiple Choice",
                            }}
                        }
                        if let Some(ends_at) = poll_ends_at {
                            PollTimer { ends_at }
                        }
                        span { "{total_votes} votes" }
                    }
                }
            }

            // Voting UI
            if show_voting_ui {
                div {
                    class: "space-y-2 mb-3",
                    for option in poll_options.iter() {
                        {
                            let opt_id = option.id.clone();
                            let opt_text = option.text.clone();
                            let is_selected = selected_options.read().contains(&opt_id);

                            rsx! {
                                button {
                                    key: "{opt_id}",
                                    class: format!(
                                        "w-full text-left p-3 rounded-lg border-2 transition {}",
                                        if is_selected { "border-primary bg-primary/5" } else { "border-border hover:border-primary/50" }
                                    ),
                                    onclick: move |_| {
                                        let poll = match poll_data.read().as_ref() {
                                            Some(p) => p.clone(),
                                            None => return,
                                        };

                                        let mut current = selected_options.read().clone();

                                        match poll.r#type {
                                            PollType::SingleChoice => {
                                                selected_options.set(vec![opt_id.clone()]);
                                            }
                                            PollType::MultipleChoice => {
                                                if current.contains(&opt_id) {
                                                    current.retain(|id| id != &opt_id);
                                                } else {
                                                    current.push(opt_id.clone());
                                                }
                                                selected_options.set(current);
                                            }
                                        }
                                    },
                                    "{opt_text}"
                                }
                            }
                        }
                    }

                    button {
                        class: "w-full mt-2 px-4 py-2 rounded-lg bg-primary text-primary-foreground font-medium hover:bg-primary/90 disabled:opacity-50",
                        disabled: selected_options.read().is_empty() || *is_voting.read(),
                        onclick: submit_vote,
                        if *is_voting.read() { "Submitting..." } else { "Submit Vote" }
                    }
                }
            }

            // Results view
            if *show_results.read() || has_voted || is_expired {
                div {
                    class: "space-y-2",
                    for option in poll_options.iter() {
                        {
                            let opt_id = option.id.clone();
                            let opt_text = option.text.clone();
                            let vote_count = *results().get(&opt_id).unwrap_or(&0);
                            let percentage = if total_votes > 0 {
                                (vote_count as f32 / total_votes as f32) * 100.0
                            } else {
                                0.0
                            };

                            rsx! {
                                div {
                                    key: "{opt_id}",
                                    class: "relative p-3 rounded-lg border overflow-hidden",

                                    div {
                                        class: "absolute inset-0 bg-primary/10",
                                        style: format!("width: {percentage}%")
                                    }

                                    div {
                                        class: "relative flex justify-between",
                                        span { "{opt_text}" }
                                        span { class: "font-medium", "{vote_count} ({percentage:.1}%)" }
                                    }
                                }
                            }
                        }
                    }

                    if !is_expired && !has_voted {
                        button {
                            class: "w-full mt-2 text-sm text-primary hover:underline",
                            onclick: move |_| show_results.set(false),
                            "Hide results and vote"
                        }
                    }
                }
            }
        }
    }
}

// Helper function to fetch poll votes
// NIP-88: Votes should be fetched from the relays specified in the poll
async fn fetch_poll_votes(
    poll_id: EventId,
    ends_at: Option<Timestamp>,
    poll_relays: Vec<nostr_sdk::RelayUrl>,
) -> Result<Vec<NostrEvent>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;

    // Ensure relays are ready before fetching
    nostr_client::ensure_relays_ready(&client).await;

    let mut filter = Filter::new()
        .kind(Kind::PollResponse)
        .event(poll_id);

    if let Some(until) = ends_at {
        filter = filter.until(until);
    }

    // If poll specifies relays, fetch from those relays specifically
    // Otherwise fall back to user's default relays
    let events = if !poll_relays.is_empty() {
        // Add poll relays temporarily if not already connected
        for relay_url in &poll_relays {
            if let Err(e) = client.add_relay(relay_url.as_str()).await {
                log::debug!("Could not add poll relay {}: {}", relay_url, e);
            }
        }

        // Fetch from poll-specified relays
        let relay_urls: Vec<nostr_sdk::Url> = poll_relays.iter()
            .filter_map(|r| nostr_sdk::Url::parse(r.as_str()).ok())
            .collect();

        if !relay_urls.is_empty() {
            client
                .fetch_events_from(relay_urls, filter.clone(), Duration::from_secs(10))
                .await
                .map_err(|e| format!("Failed to fetch votes from poll relays: {}", e))?
        } else {
            // Fallback if URL parsing failed
            client
                .fetch_events(filter, Duration::from_secs(10))
                .await
                .map_err(|e| format!("Failed to fetch votes: {}", e))?
        }
    } else {
        // No poll relays specified, use default relays
        client
            .fetch_events(filter, Duration::from_secs(10))
            .await
            .map_err(|e| format!("Failed to fetch votes: {}", e))?
    };

    Ok(deduplicate_votes(events.into_iter().collect()))
}

// Deduplicate votes: one vote per pubkey, latest timestamp wins
fn deduplicate_votes(events: Vec<NostrEvent>) -> Vec<NostrEvent> {
    let mut map: HashMap<String, NostrEvent> = HashMap::new();

    for event in events {
        let key = event.pubkey.to_string();
        if !map.contains_key(&key) || event.created_at > map[&key].created_at {
            map.insert(key, event);
        }
    }

    map.into_values().collect()
}

// Calculate poll results: option_id -> vote count
fn calculate_poll_results(poll: &Poll, vote_events: Vec<NostrEvent>) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();

    // Initialize all options with 0
    for option in &poll.options {
        counts.insert(option.id.clone(), 0);
    }

    // Count votes
    for vote_event in vote_events {
        for tag in vote_event.tags.iter() {
            if let Some(TagStandard::PollResponse(option_id)) = tag.as_standardized() {
                *counts.entry(option_id.clone()).or_insert(0) += 1;
            }
        }
    }

    counts
}

// Format timestamp as relative time
fn format_time_ago(timestamp: Timestamp) -> String {
    let now = Timestamp::now();
    let diff = now.as_secs() as i64 - timestamp.as_secs() as i64;

    if diff < 60 {
        format!("{}s", diff)
    } else if diff < 3600 {
        format!("{}m", diff / 60)
    } else if diff < 86400 {
        format!("{}h", diff / 3600)
    } else {
        format!("{}d", diff / 86400)
    }
}
