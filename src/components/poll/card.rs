use super::timer::PollTimer;
use crate::components::icons::{
    BookmarkIcon, MessageCircleIcon, Repeat2Icon, ShareIcon, ZapIcon,
};
use crate::components::{ConfirmModal, ReactionButton, ZapModal};
use crate::hooks::use_reaction;
use crate::routes::Route;
use crate::services::aggregation::InteractionCounts;
use crate::stores::bookmarks;
use crate::stores::nostr_client::{self, publish_repost, delete_repost, HAS_SIGNER};
use crate::stores::relay;
use crate::utils::format::{format_relative_time_or, format_sats_compact};
use crate::utils::truncate_pubkey;
use dioxus::prelude::*;
use nostr_sdk::{
    nips::nip19::Nip19Event,
    nips::nip88::{Poll, PollResponse, PollType},
    Event as NostrEvent, EventId, Filter, Kind, PublicKey, RelayPoolNotification, SubscriptionId,
    TagStandard, Timestamp, ToBech32,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
#[component]
pub fn PollCard(
    event: NostrEvent,
    #[props(default = None)]
    precomputed_counts: Option<InteractionCounts>,
) -> Element {
    let author_pubkey = event.pubkey.to_string();
    let author_pubkey_for_metadata = author_pubkey.clone();
    let author_pubkey_for_display = author_pubkey.clone();
    let author_pubkey_for_link = author_pubkey.clone();
    let event_id = event.id;
    let event_id_str = event_id.to_string();
    let event_id_repost = event_id_str.clone();
    let event_id_bookmark = event_id_str.clone();
    let event_clone = event.clone();
    let created_at = event.created_at;
    let mut author_metadata = use_signal(|| None::<nostr_sdk::Metadata>);
    let mut poll_data = use_signal(|| None::<Poll>);
    let mut votes = use_signal(Vec::<NostrEvent>::new);
    let mut loading_votes = use_signal(|| true);
    let mut user_vote = use_signal(|| None::<NostrEvent>);
    let mut selected_options = use_signal(Vec::<String>::new);
    let mut show_results = use_signal(|| false);
    let mut is_voting = use_signal(|| false);
    let mut vote_sub_id: Signal<Option<SubscriptionId>> = use_signal(|| None);
    let mut vote_gen = use_signal(|| 0u32);
    let mut poll_relay_urls: Signal<Vec<nostr_sdk::RelayUrl>> = use_signal(Vec::new);
    // Interaction bar state
    let mut is_reposting = use_signal(|| false);
    let mut is_reposted = use_signal(|| false);
    let mut user_repost_id = use_signal(|| None::<String>);
    let mut show_repost_menu = use_signal(|| false);
    let mut show_undo_repost_confirm = use_signal(|| false);
    let mut is_zapped = use_signal(|| false);
    let mut show_zap_modal = use_signal(|| false);
    let mut is_bookmarking = use_signal(|| false);
    let is_bookmarked = bookmarks::is_bookmarked(&event_id_str);
    let has_signer = *HAS_SIGNER.read();
    let mut reply_count = use_signal(|| 0usize);
    let mut repost_count = use_signal(|| 0usize);
    let mut zap_amount_sats = use_signal(|| 0u64);
    let reaction = use_reaction(
        event_id.to_hex(),
        event.pubkey.to_hex(),
        precomputed_counts.as_ref(),
    );
    // Sync precomputed interaction counts
    use_effect(
        use_reactive(
            &precomputed_counts,
            move |counts_opt| {
                if let Some(counts) = counts_opt {
                    let current_has_data = {
                        let reply = *reply_count.peek();
                        let repost = *repost_count.peek();
                        let zap = *zap_amount_sats.peek();
                        reply > 0 || repost > 0 || zap > 0
                    };
                    let new_has_data = counts.replies > 0 || counts.reposts > 0
                        || counts.zap_amount_sats > 0;
                    if new_has_data || !current_has_data {
                        reply_count.set(counts.replies.min(501));
                        repost_count.set(counts.reposts.min(501));
                        zap_amount_sats.set(counts.zap_amount_sats);
                    }
                    if counts.user_reposted.is_some() || !*is_reposted.peek() {
                        is_reposted.set(counts.user_reposted.unwrap_or(false));
                    }
                    if counts.user_repost_id.is_some() || user_repost_id.peek().is_none() {
                        user_repost_id.set(counts.user_repost_id.clone());
                    }
                    if counts.user_zapped.is_some() || !*is_zapped.peek() {
                        is_zapped.set(counts.user_zapped.unwrap_or(false));
                    }
                }
            },
        ),
    );
    let _metadata_task = use_future(move || {
        let pubkey_str = author_pubkey_for_metadata.clone();
        async move {
            match PublicKey::parse(&pubkey_str) {
                Ok(pk) => {
                    if let Some(client) = nostr_client::get_client() {
                        if let Ok(Some(metadata)) = client
                            .fetch_metadata(pk, Duration::from_secs(5))
                            .await
                        {
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
    let _poll_parse_task = use_future(move || {
        let evt = event_clone.clone();
        async move {
            match Poll::from_event(&evt) {
                Ok(poll) => poll_data.set(Some(poll)),
                Err(e) => log::error!("Failed to parse poll: {}", e),
            }
        }
    });
    // Cancellation flag for the notification loop; set synchronously in use_drop
    let cancelled = use_hook(|| Arc::new(AtomicBool::new(false)));
    let cancelled_for_loop = cancelled.clone();

    use_effect(use_reactive(&poll_data, move |pd| {
        let poll_id = event_id;
        let poll = pd.read().clone();
        let cancelled_for_loop = cancelled_for_loop.clone();
        // Increment generation counter before spawn to guard all mutations
        let current_vote_gen = vote_gen.peek().wrapping_add(1);
        vote_gen.set(current_vote_gen);
        spawn(async move {
            loading_votes.set(true);
            let (ends_at, poll_relays) = poll
                .map(|p| (p.ends_at, p.relays))
                .unwrap_or((None, Vec::new()));
            let poll_relays_for_sub = poll_relays.clone();
            // Capture timestamp before fetch so votes created during
            // the fetch window are not missed on the next poll.
            let pre_fetch = Timestamp::now();
            match fetch_poll_votes(poll_id, ends_at, poll_relays).await {
                Ok(vote_events) => {
                    if *vote_gen.peek() != current_vote_gen { return; }
                    votes.set(vote_events.clone());
                    if let Ok(user_pubkey) = nostr_client::get_cached_pubkey() {
                        let user_pubkey_str = user_pubkey.to_string();
                        if let Some(user_vote_event) = vote_events
                            .iter()
                            .find(|v| v.pubkey.to_string() == user_pubkey_str)
                        {
                            user_vote.set(Some(user_vote_event.clone()));
                            show_results.set(true);
                        }
                    }
                    // Subscribe for real-time vote updates
                    if let Some(client) = nostr_client::get_client() {
                        // Unsubscribe previous vote subscription before creating a new one
                        if let Some(old_sub_id) = vote_sub_id.peek().clone() {
                            client.unsubscribe(&old_sub_id).await;
                        }
                        // Re-add poll relays for subscription lifetime.
                        // Remove previously-added relays that are no longer in the new set
                        // to avoid leaking relay connections across poll_data changes.
                        if !poll_relays_for_sub.is_empty() {
                            let old_relays = poll_relay_urls.peek().clone();
                            let added = relay::add_relays(&client, &poll_relays_for_sub).await;
                            if !added.is_empty() {
                                nostr_client::ensure_relays_ready(&client).await;
                            }
                            // Build new_set from the desired set, not just newly-added relays.
                            // Already-connected relays won't appear in `added` but are still desired.
                            let new_set: std::collections::HashSet<&nostr_sdk::RelayUrl> = poll_relays_for_sub.iter().collect();
                            let removed: Vec<nostr_sdk::RelayUrl> = old_relays
                                .iter()
                                .filter(|r| !new_set.contains(r))
                                .cloned()
                                .collect();
                            if !removed.is_empty() {
                                relay::remove_relays(&client, &removed).await;
                            }
                            poll_relay_urls.set(poll_relays_for_sub.clone());
                        } else {
                            // No new poll relays; remove any previously-added ones
                            let old_relays = poll_relay_urls.peek().clone();
                            if !old_relays.is_empty() {
                                relay::remove_relays(&client, &old_relays).await;
                                poll_relay_urls.set(Vec::new());
                            }
                        }
                        let since_ts = votes.read().iter()
                            .map(|v| v.created_at)
                            .max()
                            .map(|t| Timestamp::from_secs(t.as_secs().saturating_sub(1)))
                            .unwrap_or(Timestamp::from_secs(pre_fetch.as_secs().saturating_sub(1)));
                        let mut vote_filter = Filter::new()
                            .kind(Kind::PollResponse)
                            .event(poll_id)
                            .since(since_ts);
                        if let Some(until) = ends_at {
                            vote_filter = vote_filter.until(until);
                        }
                        match client.subscribe(vote_filter, None).await {
                            Ok(output) => {
                                let subscription_id = output.val;
                                vote_sub_id.set(Some(subscription_id.clone()));
                                let cancelled_flag = cancelled_for_loop.clone();
                                spawn(async move {
                                    let mut notifications = client.notifications();
                                    while let Ok(notification) = notifications.recv().await {
                                        if cancelled_flag.load(Ordering::SeqCst) {
                                            // Component is being dropped; use_drop handles relay cleanup
                                            break;
                                        }
                                        if *vote_gen.peek() != current_vote_gen { break; }
                                        if let RelayPoolNotification::Event {
                                            subscription_id: sub_id,
                                            event: new_vote,
                                            ..
                                        } = notification
                                        {
                                            if sub_id == subscription_id {
                                                let already_exists = votes
                                                    .read()
                                                    .iter()
                                                    .any(|e| e.id == new_vote.id);
                                                if !already_exists {
                                                    votes.with_mut(|current| {
                                                        current.push((*new_vote).clone());
                                                        let deduped = deduplicate_votes(
                                                            std::mem::take(current),
                                                        );
                                                        *current = deduped;
                                                    });
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                            Err(e) => {
                                log::error!("Failed to subscribe for poll votes: {}", e);
                            }
                        }
                    }
                }
                Err(e) => log::error!("Failed to fetch votes: {}", e),
            }
            if *vote_gen.peek() == current_vote_gen {
                loading_votes.set(false);
            }
        });
    }));
    // Cleanup vote subscription and poll relays on unmount.
    // Set cancellation flag synchronously so the notification loop
    // can perform async cleanup reliably, rather than relying on
    // spawned tasks from a synchronous Drop context.
    use_drop(move || {
        cancelled.store(true, Ordering::SeqCst);
        let relays = poll_relay_urls.peek().clone();
        if let Some(sub_id) = vote_sub_id.peek().clone() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    client.unsubscribe(&sub_id).await;
                    relay::remove_relays(&client, &relays).await;
                }
            });
        } else if !relays.is_empty() {
            spawn(async move {
                if let Some(client) = nostr_client::get_client() {
                    relay::remove_relays(&client, &relays).await;
                }
            });
        }
    });
    let results = use_memo(move || {
        let poll = match poll_data.read().clone() {
            Some(p) => p,
            None => return HashMap::new(),
        };
        let vote_events = votes.read().clone();
        calculate_poll_results(&poll, vote_events)
    });
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
        let previous_selected = selected_options.read().clone();
        let previous_show_results = *show_results.read();
        show_results.set(true);
        is_voting.set(true);
        spawn(async move {
            let response = match poll.r#type {
                PollType::SingleChoice => {
                    PollResponse::SingleChoice {
                        poll_id,
                        response: options.first().unwrap().clone(),
                    }
                }
                PollType::MultipleChoice => {
                    PollResponse::MultipleChoice {
                        poll_id,
                        responses: options,
                    }
                }
            };
            let relays = poll.relays.clone();
            match nostr_client::publish_poll_vote(poll_id, response, relays.clone())
                .await
            {
                Ok(_event_id) => {
                    log::info!("Vote published successfully");
                    match fetch_poll_votes(poll_id, poll.ends_at, relays).await {
                        Ok(vote_events) => {
                            votes.set(vote_events.clone());
                            if let Ok(user_pubkey) = nostr_client::get_cached_pubkey() {
                                let user_pubkey_str = user_pubkey.to_string();
                                if let Some(user_vote_event) = vote_events
                                    .iter()
                                    .find(|v| v.pubkey.to_string() == user_pubkey_str)
                                {
                                    user_vote.set(Some(user_vote_event.clone()));
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Failed to refresh votes after voting: {}", e);
                        }
                    }
                    selected_options.set(Vec::new());
                }
                Err(e) => {
                    log::error!("Failed to publish vote: {}", e);
                    selected_options.set(previous_selected);
                    show_results.set(previous_show_results);
                }
            }
            is_voting.set(false);
        });
    };
    let poll = poll_data.read().clone();
    let poll_title = poll.as_ref().map(|p| p.title.clone()).unwrap_or_default();
    let poll_type = poll.as_ref().map(|p| p.r#type).unwrap_or(PollType::SingleChoice);
    let poll_options = poll.as_ref().map(|p| p.options.clone()).unwrap_or_default();
    let poll_ends_at = poll.as_ref().and_then(|p| p.ends_at);
    let author_name = author_metadata
        .read()
        .as_ref()
        .and_then(|m| m.display_name.clone().or_else(|| m.name.clone()))
        .unwrap_or_else(|| truncate_pubkey(&author_pubkey_for_display));
    let time_ago = format_relative_time_or(created_at.as_secs(), "now");
    let total_votes: usize = results().values().sum();
    let has_voted = user_vote.read().is_some();
    let is_expired = poll_ends_at
        .map(|ends_at| ends_at < Timestamp::now())
        .unwrap_or(false);
    let show_voting_ui = !*show_results.read() && !has_voted && !is_expired;
    let repost_button_class = if *is_reposted.read() {
        "flex items-center text-green-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-green-500 transition"
    };
    let zap_button_class = if *is_zapped.read() {
        "flex items-center gap-1 text-yellow-500 transition px-2 py-1.5 rounded"
    } else {
        "flex items-center gap-1 text-muted-foreground hover:text-yellow-500 hover:bg-yellow-500/10 transition px-2 py-1.5 rounded"
    };
    let bookmark_button_class = if is_bookmarked {
        "flex items-center text-blue-500 transition"
    } else {
        "flex items-center text-muted-foreground hover:text-blue-500 transition"
    };
    let nav = use_navigator();
    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition border-b border-border",
            div { class: "flex items-center gap-2 mb-3",
                Link {
                    to: Route::Profile {
                        pubkey: author_pubkey_for_link.clone(),
                    },
                    class: "font-semibold hover:underline",
                    "{author_name}"
                }
                span { class: "text-muted-foreground text-sm", "· {time_ago}" }
            }
            div { class: "mb-3",
                Link {
                    to: Route::PollView {
                        noteid: event_id_str.clone(),
                    },
                    class: "text-lg font-medium mb-2 block hover:underline",
                    "{poll_title}"
                }
                div { class: "flex items-center gap-3 text-sm text-muted-foreground",
                    span { class: "px-2 py-1 rounded bg-accent text-foreground text-xs",
                        {
                            match poll_type {
                                PollType::SingleChoice => "Single Choice",
                                PollType::MultipleChoice => "Multiple Choice",
                            }
                        }
                    }
                    if let Some(ends_at) = poll_ends_at {
                        PollTimer { ends_at }
                    }
                    span { "{total_votes} votes" }
                }
            }
            if show_voting_ui {
                div { class: "space-y-2 mb-3",
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
                                        if is_selected {
                                            "border-primary bg-primary/5"
                                        } else {
                                            "border-border hover:border-primary/50"
                                        },
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
                        if *is_voting.read() {
                            "Submitting..."
                        } else {
                            "Submit Vote"
                        }
                    }
                }
            }
            if *show_results.read() || has_voted || is_expired {
                div { class: "space-y-2",
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
                                div { key: "{opt_id}", class: "relative p-3 rounded-lg border overflow-hidden",
                                    div {
                                        class: "absolute inset-0 bg-primary/10",
                                        style: format!("width: {percentage}%"),
                                    }
                                    div { class: "relative flex justify-between",
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
            // Interaction bar
            div { class: "flex items-center justify-between max-w-md mt-2 -ml-2",
                Link {
                    to: Route::PollView {
                        noteid: event_id_str.clone(),
                    },
                    class: "flex items-center gap-1 hover:text-blue-500 hover:bg-blue-500/10 transition px-2 py-1.5 rounded text-muted-foreground",
                    MessageCircleIcon {
                        class: "h-4 w-4".to_string(),
                        filled: false,
                    }
                    span { class: "text-xs",
                        {
                            let count = *reply_count.read();
                            if count > 500 {
                                "500+".to_string()
                            } else if count > 0 {
                                count.to_string()
                            } else {
                                "".to_string()
                            }
                        }
                    }
                }
                div { class: "relative",
                    button {
                        class: "{repost_button_class} hover:bg-green-500/10 gap-1 px-2 py-1.5 rounded",
                        disabled: !has_signer || *is_reposting.read(),
                        onclick: move |e: MouseEvent| {
                            e.stop_propagation();
                            if has_signer && !*is_reposting.read() {
                                show_repost_menu.toggle();
                            }
                        },
                        Repeat2Icon {
                            class: "h-4 w-4".to_string(),
                            filled: false,
                        }
                        span { class: "text-xs",
                            {
                                let count = *repost_count.read();
                                if count > 500 {
                                    "500+".to_string()
                                } else if count > 0 {
                                    count.to_string()
                                } else {
                                    "".to_string()
                                }
                            }
                        }
                    }
                    if *show_repost_menu.read() {
                        div {
                            class: "fixed inset-0 z-40",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                show_repost_menu.set(false);
                            },
                        }
                        div {
                            class: "absolute bottom-full left-0 mb-1 bg-card border border-border rounded-lg shadow-lg py-1 min-w-[120px] z-50",
                            onclick: move |e: MouseEvent| e.stop_propagation(),
                            button {
                                class: "w-full px-3 py-2 text-left hover:bg-accent text-sm flex items-center gap-2",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_repost_menu.set(false);
                                    if *is_reposted.read() {
                                        show_undo_repost_confirm.set(true);
                                    } else {
                                        let event_id_clone = event_id_repost.clone();
                                        is_reposting.set(true);
                                        spawn(async move {
                                            match publish_repost(event_id_clone, None).await {
                                                Ok(repost_id) => {
                                                    is_reposted.set(true);
                                                    user_repost_id.set(Some(repost_id));
                                                    let current_count = *repost_count.peek();
                                                    repost_count.set(current_count + 1);
                                                    is_reposting.set(false);
                                                }
                                                Err(e) => {
                                                    log::error!("Failed to repost event: {}", e);
                                                    is_reposting.set(false);
                                                }
                                            }
                                        });
                                    }
                                },
                                Repeat2Icon {
                                    class: "h-4 w-4".to_string(),
                                    filled: false,
                                }
                                if *is_reposted.read() {
                                    "Undo Repost"
                                } else {
                                    "Repost"
                                }
                            }
                            button {
                                class: "w-full px-3 py-2 text-left hover:bg-accent text-sm flex items-center gap-2",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_repost_menu.set(false);
                                    let nevent = Nip19Event::new(event.id).author(event.pubkey);
                                    match nevent.to_bech32() {
                                        Ok(nevent_str) => {
                                            nav.push(Route::NoteNew {
                                                quote: Some(nevent_str),
                                            });
                                        }
                                        Err(e) => {
                                            log::warn!("Failed to encode nevent for quote: {}", e);
                                        }
                                    }
                                },
                                svg {
                                    xmlns: "http://www.w3.org/2000/svg",
                                    class: "h-4 w-4",
                                    fill: "none",
                                    view_box: "0 0 24 24",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    path {
                                        stroke_linecap: "round",
                                        stroke_linejoin: "round",
                                        d: "M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z",
                                    }
                                }
                                "Quote"
                            }
                        }
                    }
                }
                ReactionButton { reaction: reaction.clone(), has_signer }
                {
                    let has_lightning = author_metadata
                        .read()
                        .as_ref()
                        .and_then(|m| m.lud16.as_ref().or(m.lud06.as_ref()))
                        .is_some();
                    if has_lightning {
                        rsx! {
                            button {
                                class: "{zap_button_class}",
                                onclick: move |e: MouseEvent| {
                                    e.stop_propagation();
                                    show_zap_modal.set(true);
                                },
                                ZapIcon { class: "h-4 w-4".to_string(), filled: *is_zapped.read() }
                                span { class: "text-xs",
                                    {
                                        let amount = *zap_amount_sats.read();
                                        if amount > 0 { format_sats_compact(amount) } else { "".to_string() }
                                    }
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
                button {
                    class: "{bookmark_button_class} hover:bg-blue-500/10 px-2 py-1.5 rounded",
                    disabled: !has_signer || *is_bookmarking.read(),
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        if !has_signer || *is_bookmarking.read() {
                            return;
                        }
                        let event_id_clone = event_id_bookmark.clone();
                        let currently_bookmarked = bookmarks::is_bookmarked(&event_id_clone);
                        is_bookmarking.set(true);
                        spawn(async move {
                            let result = if currently_bookmarked {
                                bookmarks::unbookmark_event(event_id_clone).await
                            } else {
                                bookmarks::bookmark_event(event_id_clone).await
                            };
                            if let Err(e) = result {
                                log::error!("Failed to toggle bookmark: {}", e);
                            }
                            is_bookmarking.set(false);
                        });
                    },
                    BookmarkIcon {
                        class: "h-4 w-4".to_string(),
                        filled: is_bookmarked,
                    }
                }
                button {
                    class: "flex items-center text-muted-foreground hover:text-blue-500 hover:bg-blue-500/10 px-2 py-1.5 rounded transition",
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        let nevent = Nip19Event::new(event_id).author(event.pubkey);
                        if let Ok(nevent_str) = nevent.to_bech32() {
                            let share_url = format!("https://njump.me/{}", nevent_str);
                            spawn(async move {
                                if let Err(e) = crate::utils::clipboard::copy_to_clipboard(&share_url).await {
                                    log::warn!("Clipboard write failed: {:?}", e);
                                }
                            });
                        }
                    },
                    ShareIcon {
                        class: "h-4 w-4".to_string(),
                        filled: false,
                    }
                }
            }
        }
        if *show_zap_modal.read() {
            {
                let meta = author_metadata.read();
                let recipient_name = meta
                    .as_ref()
                    .and_then(|m| m.display_name.clone().or_else(|| m.name.clone()))
                    .unwrap_or_else(|| truncate_pubkey(&event.pubkey.to_string()));
                let lud16 = meta.as_ref().and_then(|m| m.lud16.clone());
                let lud06 = meta.as_ref().and_then(|m| m.lud06.clone());
                rsx! {
                    ZapModal {
                        recipient_pubkey: event.pubkey.to_hex(),
                        recipient_name: recipient_name,
                        lud16: lud16,
                        lud06: lud06,
                        event_id: Some(event_id.to_hex()),
                        on_close: move |_| show_zap_modal.set(false),
                    }
                }
            }
        }
        if *show_undo_repost_confirm.read() {
            ConfirmModal {
                title: "Undo Repost".to_string(),
                message: "Are you sure you want to undo this repost?".to_string(),
                confirm_text: Some("Undo".to_string()),
                cancel_text: None,
                on_confirm: move |_| {
                    show_undo_repost_confirm.set(false);
                    if let Some(repost_id) = user_repost_id.peek().clone() {
                        is_reposting.set(true);
                        spawn(async move {
                            match delete_repost(repost_id).await {
                                Ok(_) => {
                                    is_reposted.set(false);
                                    user_repost_id.set(None);
                                    let current_count = *repost_count.peek();
                                    if current_count > 0 {
                                        repost_count.set(current_count - 1);
                                    }
                                    is_reposting.set(false);
                                }
                                Err(e) => {
                                    log::error!("Failed to undo repost: {}", e);
                                    is_reposting.set(false);
                                }
                            }
                        });
                    }
                },
                on_cancel: move |_| {
                    show_undo_repost_confirm.set(false);
                },
            }
        }
    }
}
async fn fetch_poll_votes(
    poll_id: EventId,
    ends_at: Option<Timestamp>,
    poll_relays: Vec<nostr_sdk::RelayUrl>,
) -> Result<Vec<NostrEvent>, String> {
    let client = nostr_client::get_client().ok_or("Client not initialized")?;
    let mut filter = Filter::new().kind(Kind::PollResponse).event(poll_id);
    if let Some(until) = ends_at {
        filter = filter.until(until);
    }
    let events = if !poll_relays.is_empty() {
        let added_relays = relay::add_relays(&client, &poll_relays).await;
        if !added_relays.is_empty() {
            nostr_client::ensure_relays_ready(&client).await;
        }
        let relay_urls: Vec<nostr_sdk::Url> = poll_relays
            .iter()
            .filter_map(|r| nostr_sdk::Url::parse(r.as_str()).ok())
            .collect();
        let result = if !relay_urls.is_empty() {
            client
                .fetch_events_from(relay_urls, filter.clone(), Duration::from_secs(10))
                .await
                .map_err(|e| format!("Failed to fetch votes from poll relays: {}", e))
        } else {
            client
                .fetch_events(filter, Duration::from_secs(10))
                .await
                .map_err(|e| format!("Failed to fetch votes: {}", e))
        };
        relay::remove_relays(&client, &added_relays).await;
        result?
    } else {
        nostr_client::ensure_relays_ready(&client).await;
        client
            .fetch_events(filter, Duration::from_secs(10))
            .await
            .map_err(|e| format!("Failed to fetch votes: {}", e))?
    };
    Ok(deduplicate_votes(events.into_iter().collect()))
}
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
fn calculate_poll_results(poll: &Poll, vote_events: Vec<NostrEvent>) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let valid_ids: HashSet<&str> = poll.options.iter().map(|o| o.id.as_str()).collect();
    for option in &poll.options {
        counts.insert(option.id.clone(), 0);
    }
    let is_single_choice = matches!(poll.r#type, PollType::SingleChoice);
    for vote_event in vote_events {
        let mut seen_in_event: HashSet<&str> = HashSet::new();
        for tag in vote_event.tags.iter() {
            if let Some(TagStandard::PollResponse(option_id)) = tag.as_standardized() {
                if valid_ids.contains(option_id.as_str())
                    && seen_in_event.insert(option_id.as_str())
                {
                    // Lenient handling of multi-vote in single-choice polls: we count the first
                    // response option and silently drop subsequent ones, rather than rejecting
                    // the entire event. This allows tallying partial results from non-conforming clients.
                    if is_single_choice && seen_in_event.len() > 1 {
                        break;
                    }
                    *counts.get_mut(option_id.as_str()).unwrap() += 1;
                }
            }
        }
    }
    counts
}
