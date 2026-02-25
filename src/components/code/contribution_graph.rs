//! GitHub-style contribution heatmap component
//!
//! Renders a 52-column x 7-row grid showing code activity intensity
//! over the past year, with month labels, day labels, and hover tooltips.

use crate::services::git_hosting::activity::{fetch_contribution_graph, ContributionWeek};
use dioxus::prelude::*;
use nostr_sdk::PublicKey;

/// Month abbreviations for labeling the graph
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Convert unix timestamp to (year, month 1-12, day 1-31) using
/// Howard Hinnant's civil_from_days algorithm.
fn civil_from_ts(ts: u64) -> (i64, u32, u32) {
    // Shift from Unix epoch (1970-01-01) to the algorithm's epoch (0000-03-01)
    let z = ts as i64 / 86400 + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Format a unix timestamp as "Mon DD, YYYY"
fn format_date(ts: u64) -> String {
    let (y, m, d) = civil_from_ts(ts);
    let month_name = MONTHS.get(m as usize - 1).unwrap_or(&"???");
    format!("{} {}, {}", month_name, d, y)
}

/// Determine which month (1-12) a unix timestamp falls in
fn month_of(ts: u64) -> u32 {
    let (_, m, _) = civil_from_ts(ts);
    m
}

/// Return a Tailwind color class for a contribution count based on intensity level.
/// 5 levels: 0 = empty, 1-4 = increasing green intensity.
fn intensity_class(count: u32, max_count: u32) -> &'static str {
    if count == 0 || max_count == 0 {
        return "bg-muted";
    }
    let ratio = count as f64 / max_count as f64;
    if ratio <= 0.25 {
        "bg-green-200 dark:bg-green-900"
    } else if ratio <= 0.5 {
        "bg-green-400 dark:bg-green-700"
    } else if ratio <= 0.75 {
        "bg-green-600 dark:bg-green-500"
    } else {
        "bg-green-800 dark:bg-green-400"
    }
}

#[component]
pub fn ContributionGraph(pubkey: String) -> Element {
    let contribution_data = use_resource(use_reactive(&pubkey, |pk| {
        async move {
            let parsed = PublicKey::parse(&pk).map_err(|e| format!("Invalid pubkey: {e}"))?;
            fetch_contribution_graph(&parsed).await
        }
    }));

    rsx! {
        div { class: "bg-card border border-border rounded-lg p-4",
            match &*contribution_data.read() {
                Some(Ok(weeks)) => {
                    rsx! { ContributionGrid { weeks: weeks.clone() } }
                }
                Some(Err(e)) => {
                    rsx! {
                        div { class: "text-sm text-muted-foreground text-center py-4",
                            "Failed to load contributions: {e}"
                        }
                    }
                }
                None => {
                    rsx! {
                        div { class: "py-4",
                            // Loading skeleton: approximate graph shape
                            div { class: "h-4 w-48 bg-muted rounded animate-pulse mb-3" }
                            div { class: "h-24 bg-muted rounded animate-pulse" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ContributionGrid(weeks: Vec<ContributionWeek>) -> Element {
    // Compute total contributions and max daily count
    let total: u32 = weeks.iter().flat_map(|w| w.days.iter()).sum();
    let max_count: u32 = weeks.iter().flat_map(|w| w.days.iter()).copied().max().unwrap_or(0);

    // Build month labels: detect when the month changes across weeks.
    // Use mid-week date (week_start + 3 days) so the label reflects
    // whichever month owns the majority of the week's days.
    let mut month_labels: Vec<(usize, &str)> = Vec::new();
    let mut last_month: Option<u32> = None;
    for (col, week) in weeks.iter().enumerate() {
        let mid_week_ts = week.week_start + 3 * 86400;
        let m = month_of(mid_week_ts);
        if last_month != Some(m) {
            if let Some(name) = MONTHS.get(m as usize - 1) {
                month_labels.push((col, name));
            }
            last_month = Some(m);
        }
    }

    let day_labels = ["", "Mon", "", "Wed", "", "Fri", ""];

    rsx! {
        div {
            // Header
            div { class: "flex items-center justify-between mb-3",
                h3 { class: "text-sm font-medium text-foreground",
                    "{total} contributions in the last year"
                }
            }

            // Graph container with horizontal scroll on small screens
            div { class: "overflow-x-auto",
                div { class: "inline-flex gap-0",
                    // Day labels column
                    div { class: "flex flex-col mr-1",
                        // Spacer for month label row
                        div { class: "h-3 mb-0.5" }
                        for (i, label) in day_labels.iter().enumerate() {
                            div {
                                key: "day-label-{i}",
                                class: "h-3 flex items-center text-[10px] text-muted-foreground leading-none",
                                style: "margin-bottom: 1px;",
                                "{label}"
                            }
                        }
                    }

                    // Grid columns (one per week)
                    div {
                        // Month labels row
                        div { class: "flex",
                            style: "height: 14px; margin-bottom: 2px;",
                            for (col_idx, _week) in weeks.iter().enumerate() {
                                {
                                    let label = month_labels.iter().find(|(c, _)| *c == col_idx);
                                    rsx! {
                                        div {
                                            key: "month-{col_idx}",
                                            class: "text-[10px] text-muted-foreground leading-none",
                                            style: "width: 13px; flex-shrink: 0;",
                                            if let Some((_, name)) = label {
                                                "{name}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // Cells grid
                        div { class: "flex",
                            style: "gap: 1px;",
                            for (col_idx, week) in weeks.iter().enumerate() {
                                div {
                                    key: "week-{col_idx}",
                                    class: "flex flex-col",
                                    style: "gap: 1px;",
                                    for (day_idx, count) in week.days.iter().enumerate() {
                                        {
                                            let color = intensity_class(*count, max_count);
                                            let day_ts = week.week_start + day_idx as u64 * 86400;
                                            let date_str = format_date(day_ts);
                                            let c = *count;
                                            let contribution_text = if c == 1 {
                                                "contribution".to_string()
                                            } else {
                                                "contributions".to_string()
                                            };
                                            rsx! {
                                                div {
                                                    key: "cell-{col_idx}-{day_idx}",
                                                    class: "group relative",
                                                    div {
                                                        class: "w-3 h-3 rounded-sm {color}",
                                                    }
                                                    div {
                                                        class: "absolute bottom-full left-1/2 -translate-x-1/2 mb-1 px-2 py-1 bg-foreground text-background text-xs rounded opacity-0 group-hover:opacity-100 transition pointer-events-none whitespace-nowrap z-10",
                                                        "{c} {contribution_text} on {date_str}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Legend
            div { class: "flex items-center justify-end gap-1 mt-2 text-[10px] text-muted-foreground",
                span { "Less" }
                div { class: "w-3 h-3 rounded-sm bg-muted" }
                div { class: "w-3 h-3 rounded-sm bg-green-200 dark:bg-green-900" }
                div { class: "w-3 h-3 rounded-sm bg-green-400 dark:bg-green-700" }
                div { class: "w-3 h-3 rounded-sm bg-green-600 dark:bg-green-500" }
                div { class: "w-3 h-3 rounded-sm bg-green-800 dark:bg-green-400" }
                span { "More" }
            }
        }
    }
}
