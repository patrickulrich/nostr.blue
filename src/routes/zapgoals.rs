use crate::components::{ClientInitializing, ZapGoalCard, ZapModal};
use crate::hooks::use_infinite_scroll;
use crate::routes::Route;
use crate::stores::nostr_client::{self, get_client};
use crate::stores::profiles;
use crate::stores::zap_goals_store::{
    self, fetch_global_goals, fetch_goal_progress_batch, fetch_goals_for_authors,
    fetch_project_goals, publish_zap_goal_tracked, ZapGoal, ZapGoalProgress, ZapGoalsFeedType,
};
use chrono::{Local, TimeZone, Utc};
use dioxus::prelude::*;
use dioxus_primitives::toast::{consume_toast, ToastOptions};
use nostr_sdk::PublicKey;
use std::collections::HashSet;
use url::Url;

const PAGE_SIZE: usize = 20;
const PROJECT_PIN_LIMIT: usize = 6;

fn sort_progress(items: &mut [ZapGoalProgress]) {
    items.sort_by(|left, right| {
        right
            .goal
            .is_project_goal
            .cmp(&left.goal.is_project_goal)
            .then_with(|| {
                left.goal
                    .closed_at
                    .unwrap_or(u64::MAX)
                    .cmp(&right.goal.closed_at.unwrap_or(u64::MAX))
            })
            .then_with(|| right.goal.created_at.cmp(&left.goal.created_at))
    });
}

fn merge_progress(
    existing: &[ZapGoalProgress],
    incoming: Vec<ZapGoalProgress>,
) -> Vec<ZapGoalProgress> {
    let mut by_id = std::collections::HashMap::new();
    for item in existing.iter().cloned() {
        by_id.insert(item.goal.event_id.clone(), item);
    }
    for item in incoming {
        by_id.insert(item.goal.event_id.clone(), item);
    }
    let mut merged: Vec<_> = by_id.into_values().collect();
    sort_progress(&mut merged);
    merged
}

async fn fetch_feed_page(
    feed_type: ZapGoalsFeedType,
    until: Option<u64>,
) -> Result<(Vec<ZapGoal>, Option<String>), String> {
    match feed_type {
        ZapGoalsFeedType::Following => {
            let Some(pubkey) = crate::stores::auth_store::get_pubkey() else {
                return Ok((
                    Vec::new(),
                    Some("Sign in to browse zap goals from people you follow.".to_string()),
                ));
            };

            let contacts = nostr_client::fetch_contacts(pubkey).await?;
            let authors: Vec<PublicKey> = contacts
                .into_iter()
                .filter_map(|contact| PublicKey::parse(&contact).ok())
                .collect();
            if authors.is_empty() {
                return Ok((
                    Vec::new(),
                    Some(
                        "Your following feed is empty. Follow a few people or switch to Global."
                            .to_string(),
                    ),
                ));
            }

            let goals = fetch_goals_for_authors(authors, PAGE_SIZE, until).await?;
            let message = if goals.is_empty() {
                Some("People you follow have not published active zap goals yet.".to_string())
            } else {
                None
            };
            Ok((goals, message))
        }
        ZapGoalsFeedType::Global => {
            let goals = fetch_global_goals(PAGE_SIZE, until).await?;
            let message = if goals.is_empty() {
                Some("No active zap goals found on your current relays.".to_string())
            } else {
                None
            };
            Ok((goals, message))
        }
    }
}

async fn enrich_progress(goals: Vec<ZapGoal>) -> Result<Vec<ZapGoalProgress>, String> {
    if goals.is_empty() {
        return Ok(Vec::new());
    }

    let author_pubkeys: Vec<String> = goals
        .iter()
        .map(|goal| goal.author_pubkey.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    profiles::prefetch_profiles(author_pubkeys).await;

    let progress = fetch_goal_progress_batch(&goals).await?;
    let contributor_pubkeys: Vec<String> = progress
        .iter()
        .flat_map(|item| {
            item.recent_contributors
                .iter()
                .map(|contributor| contributor.pubkey.clone())
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    profiles::prefetch_profiles(contributor_pubkeys).await;

    Ok(progress)
}

#[component]
pub fn ZapGoalsHome() -> Element {
    let toast = consume_toast();
    let navigator = use_navigator();
    let mut refresh_trigger = use_signal(|| 0u32);
    let default_feed = if crate::stores::auth_store::get_pubkey().is_some() {
        ZapGoalsFeedType::Following
    } else {
        ZapGoalsFeedType::Global
    };
    let mut feed_type = use_signal(|| default_feed);
    let mut search_query = use_signal(String::new);
    let mut goals = use_signal(Vec::<ZapGoalProgress>::new);
    let mut loading = use_signal(|| false);
    let mut pagination_loading = use_signal(|| false);
    let mut has_more = use_signal(|| false);
    let mut oldest_timestamp = use_signal(|| None::<u64>);
    let mut empty_message = use_signal(|| None::<String>);
    let mut error_message = use_signal(|| None::<String>);
    let mut selected_goal = use_signal(|| None::<ZapGoalProgress>);
    let mut open_goal_request = use_signal(|| 0u32);
    let mut request_generation = use_signal(|| 0u32);

    use_effect(move || {
        let _ = refresh_trigger.read();
        let current_feed_type = *feed_type.read();
        let initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !initialized {
            return;
        }
        let generation = request_generation.peek().wrapping_add(1);
        request_generation.set(generation);
        loading.set(true);
        pagination_loading.set(false);
        goals.set(Vec::new());
        error_message.set(None);
        empty_message.set(None);
        oldest_timestamp.set(None);
        has_more.set(false);
        spawn(async move {
            let project_goals = fetch_project_goals(PROJECT_PIN_LIMIT)
                .await
                .unwrap_or_default();
            if *request_generation.peek() != generation {
                return;
            }
            match fetch_feed_page(current_feed_type, None).await {
                Ok((feed_goals, message)) => {
                    if *request_generation.peek() != generation {
                        return;
                    }
                    empty_message.set(message);
                    let has_more_results = feed_goals.len() >= PAGE_SIZE;
                    oldest_timestamp.set(feed_goals.iter().map(|goal| goal.created_at).min());

                    let mut combined = project_goals;
                    combined.extend(feed_goals);
                    zap_goals_store::dedupe_goals(&mut combined);

                    match enrich_progress(combined).await {
                        Ok(mut progress) => {
                            if *request_generation.peek() != generation {
                                return;
                            }
                            sort_progress(&mut progress);
                            goals.set(progress);
                            has_more.set(has_more_results);
                        }
                        Err(error) => {
                            if *request_generation.peek() != generation {
                                return;
                            }
                            error_message.set(Some(error));
                        }
                    }
                }
                Err(error) => {
                    if *request_generation.peek() != generation {
                        return;
                    }
                    error_message.set(Some(error));
                }
            }
            if *request_generation.peek() == generation {
                loading.set(false);
            }
        });
    });

    let load_more = move || {
        if *loading.read() || *pagination_loading.read() || !*has_more.read() {
            return;
        }
        let current_feed_type = *feed_type.read();
        let until = *oldest_timestamp.read();
        let generation = request_generation.peek().wrapping_add(1);
        request_generation.set(generation);
        pagination_loading.set(true);
        error_message.set(None);
        spawn(async move {
            match fetch_feed_page(current_feed_type, until).await {
                Ok((next_goals, _)) => {
                    if *request_generation.peek() != generation {
                        return;
                    }
                    oldest_timestamp.set(next_goals.iter().map(|goal| goal.created_at).min());
                    has_more.set(next_goals.len() >= PAGE_SIZE);
                    match enrich_progress(next_goals).await {
                        Ok(progress) => {
                            if *request_generation.peek() != generation {
                                return;
                            }
                            error_message.set(None);
                            let existing = goals.read().clone();
                            goals.set(merge_progress(&existing, progress));
                        }
                        Err(error) => {
                            if *request_generation.peek() != generation {
                                return;
                            }
                            error_message.set(Some(error));
                        }
                    }
                }
                Err(error) => {
                    if *request_generation.peek() != generation {
                        return;
                    }
                    error_message.set(Some(error));
                }
            }
            if *request_generation.peek() == generation {
                pagination_loading.set(false);
            }
        });
    };
    let sentinel_id = use_infinite_scroll(load_more, has_more, pagination_loading);

    let filtered_goals =
        zap_goals_store::filter_goals_by_query(&goals.read(), &search_query.read());

    let mut open_goal_modal = {
        move |goal: ZapGoalProgress| {
            let goal_clone = goal.clone();
            let request_token = open_goal_request.peek().wrapping_add(1);
            open_goal_request.set(request_token);
            spawn(async move {
                if goal_clone.goal.is_project_goal {
                    if *open_goal_request.peek() != request_token {
                        return;
                    }
                    selected_goal.set(Some(goal_clone));
                    return;
                }

                if let Some(profile) = profiles::get_cached_profile(&goal_clone.goal.author_pubkey)
                {
                    if profile.lud16.is_some() || profile.lud06.is_some() {
                        if *open_goal_request.peek() != request_token {
                            return;
                        }
                        selected_goal.set(Some(goal_clone));
                        return;
                    }
                }

                match profiles::fetch_profile(goal_clone.goal.author_pubkey.clone()).await {
                    Ok(profile) => {
                        if *open_goal_request.peek() != request_token {
                            return;
                        }
                        if profile.lud16.is_some() || profile.lud06.is_some() {
                            selected_goal.set(Some(goal_clone));
                        } else {
                            toast.error(
                                "This author has not configured a Lightning address.".to_string(),
                                ToastOptions::new(),
                            );
                        }
                    }
                    Err(error) => {
                        if *open_goal_request.peek() != request_token {
                            return;
                        }
                        toast.error(
                            format!("Could not load author profile: {error}"),
                            ToastOptions::new(),
                        );
                    }
                }
            });
        }
    };

    rsx! {
        if !*nostr_client::CLIENT_INITIALIZED.read() {
            ClientInitializing {}
        } else {
            div { class: "min-h-screen",
                div { class: "sticky top-0 z-20 border-b border-border bg-background/90 backdrop-blur-sm",
                    div { class: "mx-auto max-w-5xl px-4 py-4",
                        div { class: "flex flex-wrap items-center justify-between gap-3",
                            div {
                                h1 { class: "flex items-center gap-2 text-2xl font-bold text-foreground",
                                    crate::components::icons::ZapIcon { class: "h-6 w-6 text-sky-500".to_string() }
                                    "Zap Goals"
                                }
                                p { class: "mt-1 text-sm text-muted-foreground",
                                    "Fund people and projects on Nostr with transparent progress tracking."
                                }
                            }
                            div { class: "flex flex-wrap items-center gap-2",
                                button {
                                    class: "rounded-lg border border-border px-3 py-2 text-sm transition hover:bg-accent",
                                    onclick: move |_| {
                                        let next = refresh_trigger.read().wrapping_add(1);
                                        refresh_trigger.set(next);
                                    },
                                    "Refresh"
                                }
                                select {
                                    class: "rounded-lg border border-border bg-background px-3 py-2 text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                    value: "{feed_type.read().label()}",
                                    onchange: move |evt| {
                                        let next = match evt.value().as_str() {
                                            "Global" => ZapGoalsFeedType::Global,
                                            _ => ZapGoalsFeedType::Following,
                                        };
                                        feed_type.set(next);
                                    },
                                    option { value: "Following", "Following" }
                                    option { value: "Global", "Global" }
                                }
                                if *crate::stores::nostr_client::HAS_SIGNER.read() {
                                    Link {
                                        to: Route::ZapGoalsNew {},
                                        class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90",
                                        "New Zap Goal"
                                    }
                                }
                            }
                        }
                        div { class: "mt-3",
                            input {
                                class: "w-full rounded-xl border border-border bg-background px-4 py-2.5 text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                                r#type: "text",
                                placeholder: "Search goal summaries, descriptions, or authors...",
                                value: "{search_query}",
                                oninput: move |evt| search_query.set(evt.value()),
                            }
                        }
                    }
                }

                div { class: "mx-auto max-w-5xl px-4 py-6",
                    if let Some(error) = error_message.read().as_ref() {
                        div { class: "mb-4 rounded-xl border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive",
                            "{error}"
                        }
                    }

                    if *loading.read() {
                        div { class: "space-y-4",
                            for idx in 0..3 {
                                div { key: "goal-skeleton-{idx}", class: "rounded-2xl border border-border bg-card p-4 animate-pulse",
                                    div { class: "mb-4 h-6 w-40 rounded bg-muted" }
                                    div { class: "mb-2 h-4 w-full rounded bg-muted" }
                                    div { class: "mb-2 h-4 w-3/4 rounded bg-muted" }
                                    div { class: "mt-4 h-3 w-full rounded bg-muted" }
                                }
                            }
                        }
                    } else if filtered_goals.is_empty() {
                        div { class: "rounded-2xl border border-dashed border-border bg-card px-6 py-12 text-center",
                            h2 { class: "text-lg font-semibold text-foreground", "No zap goals to show" }
                            p { class: "mt-2 text-sm text-muted-foreground",
                                "{empty_message.read().clone().unwrap_or_else(|| \"Try switching feeds or publish the first goal.\".to_string())}"
                            }
                            if *crate::stores::nostr_client::HAS_SIGNER.read() {
                                button {
                                    class: "mt-5 rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90",
                                    onclick: move |_| {
                                        navigator.push(Route::ZapGoalsNew {});
                                    },
                                    "Create your first zap goal"
                                }
                            }
                        }
                    } else {
                        div { class: "space-y-4",
                            for goal in filtered_goals.iter().cloned() {
                                {
                                    let goal_for_click = goal.clone();
                                    rsx! {
                                        ZapGoalCard {
                                            key: "{goal.goal.event_id}",
                                            progress: goal,
                                            on_contribute: move |_| open_goal_modal(goal_for_click.clone()),
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if *has_more.read() {
                        div {
                            id: "{sentinel_id}",
                            class: "py-8 text-center text-sm text-muted-foreground",
                            if *pagination_loading.read() {
                                "Loading more goals…"
                            }
                        }
                    }
                }

                if let Some(goal) = selected_goal.read().clone() {
                    {
                        let project_profile = profiles::get_cached_profile(&goal.goal.author_pubkey);
                        let recipient_name = if goal.goal.is_project_goal {
                            "nostr.blue".to_string()
                        } else {
                            project_profile
                                .as_ref()
                                .map(|profile| profile.get_display_name())
                                .unwrap_or_else(|| crate::utils::format::truncate_pubkey(&goal.goal.author_pubkey))
                        };
                        let lud16 = if goal.goal.is_project_goal {
                            Some(zap_goals_store::PROJECT_DONATION_LUD16.to_string())
                        } else {
                            project_profile.as_ref().and_then(|profile| profile.lud16.clone())
                        };
                        let lud06 = if goal.goal.is_project_goal {
                            None
                        } else {
                            project_profile.as_ref().and_then(|profile| profile.lud06.clone())
                        };
                        rsx! {
                            ZapModal {
                                recipient_pubkey: if goal.goal.is_project_goal {
                                    PublicKey::parse(zap_goals_store::PROJECT_DONATION_NPUB)
                                        .expect("PROJECT_DONATION_NPUB must be a valid pubkey")
                                        .to_hex()
                                } else {
                                    goal.goal.author_pubkey.clone()
                                },
                                recipient_name,
                                lud16,
                                lud06,
                                event_id: Some(goal.goal.event_id.clone()),
                                initial_amount: Some(21),
                                relay_hints: Some(goal.goal.relays.clone()),
                                on_close: move |_| {
                                    selected_goal.set(None);
                                    let next = refresh_trigger.read().wrapping_add(1);
                                    refresh_trigger.set(next);
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn ZapGoalsNew() -> Element {
    let navigator = use_navigator();
    let toast = consume_toast();
    let signed_in = *crate::stores::nostr_client::HAS_SIGNER.read();
    let client_initialized = *crate::stores::nostr_client::CLIENT_INITIALIZED.read();
    let mut amount_sats = use_signal(|| "21000".to_string());
    let mut summary = use_signal(String::new);
    let mut content = use_signal(String::new);
    let mut image = use_signal(String::new);
    let mut goal_url = use_signal(String::new);
    let mut closed_at = use_signal(String::new);
    let mut relays_text = use_signal(String::new);
    let mut relays_prefilled = use_signal(|| false);
    let mut relays_prefill_generation = use_signal(|| 0u32);
    let mut publishing = use_signal(|| false);
    let mut error_message = use_signal(|| None::<String>);

    use_effect(move || {
        let initialized = *crate::stores::nostr_client::CLIENT_INITIALIZED.read();
        if !initialized || *relays_prefilled.read() {
            return;
        }
        let generation = relays_prefill_generation.peek().wrapping_add(1);
        relays_prefill_generation.set(generation);
        spawn(async move {
            let relay_lines = if let Some(client) = get_client() {
                client
                    .relays()
                    .await
                    .into_keys()
                    .take(8)
                    .map(|relay| relay.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                String::new()
            };
            if *relays_prefill_generation.peek() != generation
                || *relays_prefilled.peek()
                || !relays_text.peek().is_empty()
                || relay_lines.is_empty()
            {
                return;
            }
            relays_text.set(relay_lines);
            relays_prefilled.set(true);
        });
    });

    let handle_publish = move |_| {
        let amount_value = amount_sats.read().trim().parse::<u64>();
        let summary_value = summary.read().trim().to_string();
        let content_value = content.read().trim().to_string();
        let image_value = image.read().trim().to_string();
        let url_value = goal_url.read().trim().to_string();
        let closed_at_value = closed_at.read().trim().to_string();
        let relays_value = relays_text.read().clone();

        let amount_value = match amount_value {
            Ok(amount) if amount > 0 => amount,
            _ => {
                error_message.set(Some("Enter a valid target amount in sats.".to_string()));
                return;
            }
        };
        if content_value.is_empty() {
            error_message.set(Some("Add a description for your zap goal.".to_string()));
            return;
        }

        let relays: Vec<String> = relays_value
            .split(|ch: char| ch == '\n' || ch == ',' || ch.is_whitespace())
            .map(str::trim)
            .filter_map(|relay| {
                let parsed = Url::parse(relay).ok()?;
                matches!(parsed.scheme(), "ws" | "wss")
                    .then_some(parsed.host_str().is_some())
                    .filter(|valid| *valid)
                    .map(|_| parsed.to_string())
            })
            .collect();
        if relays.is_empty() {
            error_message.set(Some("Add at least one valid relay URL.".to_string()));
            return;
        }

        let close_timestamp = if closed_at_value.is_empty() {
            None
        } else {
            match chrono::NaiveDateTime::parse_from_str(&closed_at_value, "%Y-%m-%dT%H:%M") {
                Ok(value) => {
                    let Some(local_dt) = Local.from_local_datetime(&value).earliest() else {
                        error_message.set(Some("Use a valid close date/time.".to_string()));
                        return;
                    };
                    let timestamp = local_dt.with_timezone(&Utc).timestamp();
                    if timestamp <= Utc::now().timestamp()
                        || !(0..=253_402_300_799).contains(&timestamp)
                    {
                        error_message.set(Some("Use a valid close date/time.".to_string()));
                        return;
                    }
                    Some(timestamp as u64)
                }
                Err(_) => {
                    error_message.set(Some("Use a valid close date/time.".to_string()));
                    return;
                }
            }
        };

        error_message.set(None);
        publishing.set(true);
        spawn(async move {
            match publish_zap_goal_tracked(
                amount_value,
                if summary_value.is_empty() {
                    None
                } else {
                    Some(summary_value)
                },
                content_value,
                if image_value.is_empty() {
                    None
                } else {
                    Some(image_value)
                },
                close_timestamp,
                relays,
                if url_value.is_empty() {
                    None
                } else {
                    Some(url_value)
                },
            )
            .await
            {
                Ok(result) => {
                    if result.success_count() > 0 {
                        toast.success(
                            format!(
                                "Zap goal published to {}/{} relays",
                                result.success_count(),
                                result.total_attempted()
                            ),
                            ToastOptions::new(),
                        );
                        navigator.push(Route::ZapGoalsHome {});
                    } else {
                        toast.error(
                            format!(
                                "Failed to publish to any relays (0/{})",
                                result.total_attempted()
                            ),
                            ToastOptions::new(),
                        );
                        publishing.set(false);
                    }
                }
                Err(error) => {
                    error_message.set(Some(error));
                    publishing.set(false);
                }
            }
        });
    };

    if !signed_in {
        return rsx! {
            div { class: "mx-auto max-w-3xl px-4 py-16",
                div { class: "rounded-2xl border border-border bg-card p-8 text-center",
                    h1 { class: "text-2xl font-bold text-foreground", "New Zap Goal" }
                    p { class: "mt-3 text-sm text-muted-foreground",
                        "Sign in with your Nostr account to publish a goal."
                    }
                    Link {
                        to: Route::Settings {},
                        class: "mt-6 inline-flex rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90",
                        "Open Settings"
                    }
                }
            }
        };
    }

    if !client_initialized {
        return rsx! { ClientInitializing {} };
    }

    rsx! {
        div { class: "min-h-screen",
            div { class: "sticky top-0 z-20 border-b border-border bg-background/90 backdrop-blur-sm",
                div { class: "mx-auto flex max-w-4xl items-center justify-between gap-3 px-4 py-3",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::ZapGoalsHome {},
                            class: "rounded-lg px-3 py-2 text-sm transition hover:bg-accent",
                            "Back"
                        }
                        h1 { class: "text-xl font-bold text-foreground", "New Zap Goal" }
                    }
                    button {
                        class: "rounded-lg bg-primary px-4 py-2 text-sm font-medium text-primary-foreground transition hover:bg-primary/90 disabled:cursor-not-allowed disabled:opacity-50",
                        disabled: *publishing.read(),
                        onclick: handle_publish,
                        if *publishing.read() { "Publishing..." } else { "Publish" }
                    }
                }
            }

            div { class: "mx-auto max-w-4xl space-y-6 px-4 py-6",
                if let Some(error) = error_message.read().as_ref() {
                    div { class: "rounded-xl border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive",
                        "{error}"
                    }
                }

                div { class: "grid gap-6 md:grid-cols-2",
                    div { class: "space-y-2",
                        label { class: "text-sm font-medium text-foreground", "Target amount (sats)" }
                        input {
                            class: "w-full rounded-xl border border-border bg-background px-4 py-3 text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "number",
                            min: "1",
                            value: "{amount_sats}",
                            oninput: move |evt| amount_sats.set(evt.value()),
                        }
                    }
                    div { class: "space-y-2",
                        label { class: "text-sm font-medium text-foreground", "Summary" }
                        input {
                            class: "w-full rounded-xl border border-border bg-background px-4 py-3 text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "text",
                            placeholder: "Monthly development goal",
                            value: "{summary}",
                            oninput: move |evt| summary.set(evt.value()),
                        }
                    }
                }

                div { class: "space-y-2",
                    label { class: "text-sm font-medium text-foreground", "Description" }
                    textarea {
                        class: "min-h-44 w-full rounded-xl border border-border bg-background px-4 py-3 text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        placeholder: "Explain what this goal funds and why it matters.",
                        value: "{content}",
                        oninput: move |evt| content.set(evt.value()),
                    }
                }

                div { class: "grid gap-6 md:grid-cols-2",
                    div { class: "space-y-2",
                        label { class: "text-sm font-medium text-foreground", "Image URL" }
                        input {
                            class: "w-full rounded-xl border border-border bg-background px-4 py-3 text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "url",
                            placeholder: "https://example.com/goal.png",
                            value: "{image}",
                            oninput: move |evt| image.set(evt.value()),
                        }
                    }
                    div { class: "space-y-2",
                        label { class: "text-sm font-medium text-foreground", "Related URL" }
                        input {
                            class: "w-full rounded-xl border border-border bg-background px-4 py-3 text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                            r#type: "url",
                            placeholder: "https://github.com/...",
                            value: "{goal_url}",
                            oninput: move |evt| goal_url.set(evt.value()),
                        }
                    }
                }

                div { class: "space-y-2",
                    label { class: "text-sm font-medium text-foreground", "Close at (optional)" }
                    input {
                        class: "w-full rounded-xl border border-border bg-background px-4 py-3 text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        r#type: "datetime-local",
                        value: "{closed_at}",
                        oninput: move |evt| closed_at.set(evt.value()),
                    }
                }

                div { class: "space-y-2",
                    label { class: "text-sm font-medium text-foreground", "Relay list" }
                    textarea {
                        class: "min-h-36 w-full rounded-xl border border-border bg-background px-4 py-3 font-mono text-sm focus:outline-hidden focus:ring-2 focus:ring-primary",
                        placeholder: "One relay per line",
                        value: "{relays_text}",
                        oninput: move |evt| relays_text.set(evt.value()),
                    }
                    p { class: "text-xs text-muted-foreground",
                        "These relays are stored in the goal event and reused when people zap it."
                    }
                }
            }
        }
    }
}
