use dioxus::prelude::*;
use crate::stores::cashu_wallet;
use crate::utils::format_sats_with_separator;
use nostr_sdk::nips::nip60::TransactionDirection;

#[component]
pub fn TransactionHistory() -> Element {
    let history = cashu_wallet::WALLET_HISTORY.read();

    if history.is_empty() {
        return rsx! {
            div {
                class: "bg-card border border-border rounded-lg p-8 text-center",
                div {
                    class: "text-4xl mb-3",
                    "📜"
                }
                p {
                    class: "text-muted-foreground",
                    "No transactions yet"
                }
                p {
                    class: "text-sm text-muted-foreground mt-1",
                    "Your transaction history will appear here"
                }
            }
        };
    }

    rsx! {
        div {
            class: "bg-card border border-border rounded-lg overflow-hidden",

            // Timeline of transactions
            div {
                class: "divide-y divide-border",
                for (_i, item) in history.iter().enumerate() {
                    {
                        let is_incoming = matches!(item.direction, TransactionDirection::In);
                        let direction_icon = if is_incoming { "⬇️" } else { "⬆️" };
                        let direction_text = if is_incoming { "Received" } else { "Sent" };
                        let direction_color = if is_incoming { "text-green-500" } else { "text-orange-500" };
                        let amount_prefix = if is_incoming { "+" } else { "-" };

                        // Format timestamp
                        let timestamp_str = format_timestamp(item.created_at);

                        rsx! {
                            div {
                                key: "{item.event_id}",
                                class: "px-4 py-4 hover:bg-accent/50 transition",

                                div {
                                    class: "flex items-start justify-between",

                                    // Left side: icon and details
                                    div {
                                        class: "flex items-start gap-3 flex-1 min-w-0",

                                        // Direction icon
                                        div {
                                            class: "text-2xl flex-shrink-0 mt-1",
                                            "{direction_icon}"
                                        }

                                        // Transaction details
                                        div {
                                            class: "flex-1 min-w-0",
                                            div {
                                                class: "font-semibold {direction_color}",
                                                "{direction_text}"
                                            }
                                            div {
                                                class: "text-sm text-muted-foreground mt-1",
                                                "{timestamp_str}"
                                            }

                                            // Event references
                                            if !item.created_tokens.is_empty() || !item.destroyed_tokens.is_empty() || !item.redeemed_events.is_empty() {
                                                div {
                                                    class: "text-xs text-muted-foreground mt-2 space-y-1",

                                                    if !item.created_tokens.is_empty() {
                                                        div {
                                                            "✨ Created {item.created_tokens.len()} token event(s)"
                                                        }
                                                    }

                                                    if !item.destroyed_tokens.is_empty() {
                                                        div {
                                                            "🗑️ Destroyed {item.destroyed_tokens.len()} token event(s)"
                                                        }
                                                    }

                                                    if !item.redeemed_events.is_empty() {
                                                        div {
                                                            "⚡ Redeemed {item.redeemed_events.len()} event(s)"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Right side: amount
                                    div {
                                        class: "text-right flex-shrink-0",
                                        div {
                                            class: "font-bold text-lg {direction_color}",
                                            "{amount_prefix}{format_sats_with_separator(item.amount)}"
                                        }
                                        div {
                                            class: "text-sm text-muted-foreground",
                                            "{item.unit}"
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

/// Format Unix timestamp to human-readable string
fn format_timestamp(timestamp: u64) -> String {
    use chrono::{DateTime, Utc, Local, TimeZone};

    let datetime = Utc.timestamp_opt(timestamp as i64, 0)
        .single()
        .unwrap_or_else(|| Utc::now());

    let local_datetime: DateTime<Local> = datetime.into();
    let now = Local::now();

    let duration = now.signed_duration_since(local_datetime);

    if duration.num_seconds() < 60 {
        return "Just now".to_string();
    } else if duration.num_minutes() < 60 {
        let mins = duration.num_minutes();
        return format!("{}m ago", mins);
    } else if duration.num_hours() < 24 {
        let hours = duration.num_hours();
        return format!("{}h ago", hours);
    } else if duration.num_days() < 7 {
        let days = duration.num_days();
        return format!("{}d ago", days);
    } else {
        return local_datetime.format("%b %d, %Y").to_string();
    }
}
