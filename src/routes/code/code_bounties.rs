//! Bounty Marketplace
//!
//! Browse all open bounties across repositories.
#![allow(dead_code)]
use crate::components::icons;
use crate::routes::Route;
use crate::stores::nostr_client;
use crate::utils::format::{format_relative_time_or, format_sats_with_separator, truncate_pubkey};
use crate::utils::nip34::{Bounty, BountyStatus};
use dioxus::prelude::*;
use nostr_sdk::prelude::*;
use std::time::Duration;

type Result<T, E = String> = std::result::Result<T, E>;

const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, PartialEq)]
enum BountySort {
    Newest,
    HighestReward,
    LowestReward,
}

#[derive(Clone, Copy, PartialEq)]
enum StatusFilter {
    All,
    Pending,
    Claimed,
    Paid,
}

/// A bounty listing combines bounty data with display metadata.
#[derive(Debug, Clone, PartialEq)]
struct BountyListing {
    pub bounty: Bounty,
    pub issue_title: Option<String>,
    pub repo_name: Option<String>,
}

/// Fetch all bounties from relays (Kind 9806).
async fn fetch_all_bounties() -> Result<Vec<BountyListing>, String> {
    let filter = Filter::new()
        .kind(Kind::Custom(Bounty::KIND))
        .limit(200);
    let events = nostr_client::fetch_events_aggregated(filter, FETCH_TIMEOUT)
        .await
        .map_err(|e| format!("Failed to fetch bounties: {}", e))?;
    let mut listings = Vec::new();
    for event in &events {
        if let Some(bounty) = Bounty::from_event(event) {
            listings.push(BountyListing {
                bounty,
                issue_title: None,
                repo_name: None,
            });
        }
    }
    // Sort newest first by default
    listings.sort_by(|a, b| b.bounty.created_at.cmp(&a.bounty.created_at));
    Ok(listings)
}

#[component]
pub fn CodeBounties() -> Element {
    let mut bounties = use_signal(Vec::<BountyListing>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut sort = use_signal(|| BountySort::Newest);
    let mut status_filter = use_signal(|| StatusFilter::All);
    let mut min_amount = use_signal(|| 0u64);
    let mut search_query = use_signal(String::new);

    // Fetch all bounties
    use_effect(move || {
        let client_initialized = *nostr_client::CLIENT_INITIALIZED.read();
        if !client_initialized {
            return;
        }
        spawn(async move {
            loading.set(true);
            match fetch_all_bounties().await {
                Ok(b) => bounties.set(b),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    });

    // Apply filters and sorting
    let query = search_query.read().to_lowercase();
    let current_sort = *sort.read();
    let current_status = *status_filter.read();
    let current_min = *min_amount.read();

    let mut sorted_bounties: Vec<BountyListing> = bounties
        .read()
        .iter()
        .filter(|l| match current_status {
            StatusFilter::All => true,
            StatusFilter::Pending => l.bounty.status == BountyStatus::Pending,
            StatusFilter::Claimed => l.bounty.status == BountyStatus::Claimed,
            StatusFilter::Paid => l.bounty.status == BountyStatus::Paid,
        })
        .filter(|l| l.bounty.amount_sats >= current_min)
        .filter(|l| {
            if query.is_empty() {
                return true;
            }
            let title_match = l
                .issue_title
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&query);
            let repo_match = l
                .repo_name
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(&query);
            let pubkey_match = l.bounty.pubkey.to_lowercase().contains(&query);
            title_match || repo_match || pubkey_match
        })
        .cloned()
        .collect();

    match current_sort {
        BountySort::Newest => sorted_bounties.sort_by(|a, b| b.bounty.created_at.cmp(&a.bounty.created_at)),
        BountySort::HighestReward => sorted_bounties.sort_by(|a, b| b.bounty.amount_sats.cmp(&a.bounty.amount_sats)),
        BountySort::LowestReward => sorted_bounties.sort_by(|a, b| a.bounty.amount_sats.cmp(&b.bounty.amount_sats)),
    }

    // Stats (single read of bounties signal)
    let bounties_snapshot = bounties.read();
    let total_count = bounties_snapshot.len();
    let total_sats: u64 = bounties_snapshot.iter().map(|l| l.bounty.amount_sats).sum();
    let pending_count = bounties_snapshot
        .iter()
        .filter(|l| l.bounty.status == BountyStatus::Pending)
        .count();
    drop(bounties_snapshot);

    rsx! {
        div { class: "min-h-screen",
            // Header
            div { class: "sticky top-0 z-20 bg-background/80 backdrop-blur-sm border-b border-border",
                div { class: "p-4 flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        Link {
                            to: Route::CodeHome {},
                            class: "text-muted-foreground hover:text-foreground",
                            dangerous_inner_html: icons::ARROW_LEFT,
                        }
                        h1 { class: "text-xl font-bold flex items-center gap-2",
                            dangerous_inner_html: icons::BITCOIN,
                            "Bounty Marketplace"
                        }
                    }
                }
                // Search bar
                div { class: "px-4 pb-3",
                    input {
                        class: "w-full px-3 py-1.5 bg-muted rounded-lg text-sm focus:outline-none focus:ring-1 focus:ring-primary",
                        r#type: "text",
                        placeholder: "Search bounties...",
                        value: "{search_query}",
                        oninput: move |e| search_query.set(e.value()),
                    }
                }
                // Sort controls
                div { class: "px-4 pb-3 flex items-center gap-2 text-sm",
                    span { class: "text-muted-foreground", "Sort:" }
                    SortButton {
                        label: "Newest",
                        active: *sort.read() == BountySort::Newest,
                        onclick: move |_| sort.set(BountySort::Newest),
                    }
                    SortButton {
                        label: "Highest",
                        active: *sort.read() == BountySort::HighestReward,
                        onclick: move |_| sort.set(BountySort::HighestReward),
                    }
                    SortButton {
                        label: "Lowest",
                        active: *sort.read() == BountySort::LowestReward,
                        onclick: move |_| sort.set(BountySort::LowestReward),
                    }
                }
                // Status filter
                div { class: "px-4 pb-3 flex gap-2",
                    StatusChip {
                        label: "All",
                        active: *status_filter.read() == StatusFilter::All,
                        onclick: move |_| status_filter.set(StatusFilter::All),
                    }
                    StatusChip {
                        label: "Pending",
                        active: *status_filter.read() == StatusFilter::Pending,
                        onclick: move |_| status_filter.set(StatusFilter::Pending),
                    }
                    StatusChip {
                        label: "Claimed",
                        active: *status_filter.read() == StatusFilter::Claimed,
                        onclick: move |_| status_filter.set(StatusFilter::Claimed),
                    }
                    StatusChip {
                        label: "Paid",
                        active: *status_filter.read() == StatusFilter::Paid,
                        onclick: move |_| status_filter.set(StatusFilter::Paid),
                    }
                }
            }

            div { class: "p-4 max-w-4xl mx-auto space-y-4",
                // Stats bar
                if !*loading.read() && error.read().is_none() {
                    div { class: "grid grid-cols-3 gap-3",
                        StatCard {
                            label: "Total Bounties",
                            value: total_count.to_string(),
                        }
                        StatCard {
                            label: "Open Bounties",
                            value: pending_count.to_string(),
                        }
                        StatCard {
                            label: "Total Sats",
                            value: format_sats_with_separator(total_sats),
                        }
                    }
                }

                // Minimum amount filter
                div { class: "flex items-center gap-2 text-sm",
                    span { class: "text-muted-foreground", "Min amount:" }
                    input {
                        class: "w-28 px-2 py-1 bg-muted rounded text-sm focus:outline-none focus:ring-1 focus:ring-primary",
                        r#type: "number",
                        placeholder: "0",
                        value: if *min_amount.read() > 0 { min_amount.read().to_string() } else { String::new() },
                        oninput: move |e| {
                            let val = e.value().parse::<u64>().unwrap_or(0);
                            min_amount.set(val);
                        },
                    }
                    span { class: "text-muted-foreground", "sats" }
                }

                if *loading.read() {
                    LoadingSkeleton {}
                } else if let Some(err) = error.read().as_ref() {
                    div { class: "text-center py-12",
                        div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-destructive/10 flex items-center justify-center",
                            svg {
                                class: "w-8 h-8 text-destructive",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                circle { cx: "12", cy: "12", r: "10" }
                                line { x1: "15", y1: "9", x2: "9", y2: "15" }
                                line { x1: "9", y1: "9", x2: "15", y2: "15" }
                            }
                        }
                        h3 { class: "font-semibold text-lg mb-2", "Failed to load bounties" }
                        p { class: "text-muted-foreground text-sm", "{err}" }
                    }
                } else if sorted_bounties.is_empty() {
                    div { class: "text-center py-12",
                        div { class: "w-16 h-16 mx-auto mb-4 rounded-full bg-muted flex items-center justify-center",
                            svg {
                                class: "w-8 h-8 text-muted-foreground",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                circle { cx: "12", cy: "12", r: "10" }
                                path { d: "M12 16v-4" }
                                path { d: "M12 8h.01" }
                            }
                        }
                        h3 { class: "font-semibold text-lg mb-2", "No bounties found" }
                        p { class: "text-muted-foreground text-sm",
                            if total_count > 0 {
                                "Try adjusting your filters to see more results."
                            } else {
                                "No bounties have been posted yet. Be the first to fund an issue!"
                            }
                        }
                    }
                } else {
                    div { class: "text-sm text-muted-foreground mb-2",
                        "{sorted_bounties.len()} bounties"
                    }
                    div { class: "border border-border rounded-lg divide-y divide-border",
                        for listing in sorted_bounties.iter() {
                            BountyCard { key: "{listing.bounty.event_id}", listing: listing.clone() }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn BountyCard(listing: BountyListing) -> Element {
    let bounty = &listing.bounty;
    let status_class = match bounty.status {
        BountyStatus::Pending => "bg-yellow-500/20 text-yellow-400",
        BountyStatus::Claimed => "bg-blue-500/20 text-blue-400",
        BountyStatus::Paid => "bg-green-500/20 text-green-400",
    };
    let amount_display = format_sats_with_separator(bounty.amount_sats);
    let time_display = format_relative_time_or(bounty.created_at, "Unknown");
    let pubkey_display = truncate_pubkey(&bounty.pubkey);

    rsx! {
        div { class: "p-4 hover:bg-accent/50 transition",
            div { class: "flex items-start justify-between gap-3",
                div { class: "flex-1 min-w-0",
                    div { class: "flex items-center gap-2 mb-1",
                        // Amount badge
                        span { class: "inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-400 text-sm font-semibold",
                            svg {
                                class: "w-3.5 h-3.5",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M13 2L3 14h9l-1 8 10-12h-9l1-8z" }
                            }
                            "{amount_display} sats"
                        }
                        // Status badge
                        span { class: "px-2 py-0.5 rounded-full text-xs font-medium {status_class}",
                            "{bounty.status.label()}"
                        }
                    }
                    // Issue title or event ID
                    div { class: "text-sm mb-1",
                        if let Some(title) = &listing.issue_title {
                            span { class: "font-medium text-foreground", "{title}" }
                        } else {
                            span { class: "text-muted-foreground font-mono text-xs",
                                "Issue {truncate_pubkey(&bounty.issue_event_id)}"
                            }
                        }
                    }
                    // Repo name and metadata
                    div { class: "flex items-center gap-3 text-xs text-muted-foreground",
                        if let Some(repo) = &listing.repo_name {
                            span { class: "flex items-center gap-1",
                                svg {
                                    class: "w-3 h-3",
                                    xmlns: "http://www.w3.org/2000/svg",
                                    width: "24",
                                    height: "24",
                                    view_box: "0 0 24 24",
                                    fill: "none",
                                    stroke: "currentColor",
                                    stroke_width: "2",
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    line { x1: "6", y1: "3", x2: "6", y2: "15" }
                                    circle { cx: "18", cy: "6", r: "3" }
                                    circle { cx: "6", cy: "18", r: "3" }
                                    path { d: "M18 9a9 9 0 0 1-9 9" }
                                }
                                "{repo}"
                            }
                        }
                        span { class: "flex items-center gap-1",
                            svg {
                                class: "w-3 h-3",
                                xmlns: "http://www.w3.org/2000/svg",
                                width: "24",
                                height: "24",
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                path { d: "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" }
                                circle { cx: "12", cy: "7", r: "4" }
                            }
                            "{pubkey_display}"
                        }
                        span { "{time_display}" }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone, PartialEq)]
struct SortButtonProps {
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
}

#[component]
fn SortButton(props: SortButtonProps) -> Element {
    let class = if props.active {
        "text-primary font-medium"
    } else {
        "text-muted-foreground hover:text-foreground cursor-pointer"
    };
    rsx! {
        button { class: "{class}", onclick: move |e| props.onclick.call(e), "{props.label}" }
    }
}

#[derive(Props, Clone, PartialEq)]
struct StatusChipProps {
    label: &'static str,
    active: bool,
    onclick: EventHandler<MouseEvent>,
}

#[component]
fn StatusChip(props: StatusChipProps) -> Element {
    let class = if props.active {
        "px-3 py-1.5 text-sm rounded-full bg-primary text-primary-foreground"
    } else {
        "px-3 py-1.5 text-sm rounded-full bg-muted text-muted-foreground hover:bg-accent"
    };
    rsx! {
        button { class: "{class}", onclick: move |e| props.onclick.call(e), "{props.label}" }
    }
}

#[derive(Props, Clone, PartialEq)]
struct StatCardProps {
    label: &'static str,
    value: String,
}

#[component]
fn StatCard(props: StatCardProps) -> Element {
    rsx! {
        div { class: "bg-card border border-border rounded-lg p-3 text-center",
            div { class: "text-lg font-bold text-foreground", "{props.value}" }
            div { class: "text-xs text-muted-foreground", "{props.label}" }
        }
    }
}

#[component]
fn LoadingSkeleton() -> Element {
    rsx! {
        div { class: "space-y-2 animate-pulse",
            // Stats skeleton
            div { class: "grid grid-cols-3 gap-3 mb-4",
                for i in 0..3 {
                    div { key: "stat-{i}", class: "bg-card border border-border rounded-lg p-3",
                        div { class: "h-6 bg-muted rounded w-1/2 mx-auto mb-1" }
                        div { class: "h-3 bg-muted rounded w-2/3 mx-auto" }
                    }
                }
            }
            // Card skeletons
            for i in 0..5 {
                div { key: "card-{i}", class: "p-4 border border-border rounded-lg",
                    div { class: "flex items-center gap-2 mb-2",
                        div { class: "h-5 bg-muted rounded-full w-24" }
                        div { class: "h-4 bg-muted rounded-full w-16" }
                    }
                    div { class: "h-4 bg-muted rounded w-2/3 mb-2" }
                    div { class: "h-3 bg-muted rounded w-1/3" }
                }
            }
        }
    }
}
