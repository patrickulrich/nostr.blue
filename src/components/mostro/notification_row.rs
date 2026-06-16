//! Single notification row for the Mostro notifications list.

use crate::stores::mostro::notification_store::MostroNotification;
use crate::utils::format;
use dioxus::prelude::*;

/// Icon glyph keyed on `action_str`. Keeps the notifications list scannable.
fn icon_for(action_str: &str) -> &'static str {
    match action_str {
        "pay-invoice" | "pay-bond-invoice" => "⚡",
        "add-invoice" | "add-bond-invoice" => "📥",
        "fiat-sent-ok" => "💸",
        "released" | "hold-invoice-payment-settled" => "🔓",
        "purchase-completed" => "✅",
        "canceled" | "hold-invoice-payment-canceled" | "admin-canceled" => "❌",
        "cooperative-cancel-initiated-by-you" | "cooperative-cancel-initiated-by-peer"
        | "cooperative-cancel-accepted" => "⏸",
        "dispute-initiated-by-you" | "dispute-initiated-by-peer" => "⚠️",
        "admin-took-dispute" => "🧑‍⚖️",
        "admin-settled" => "⚖️",
        "payment-failed" => "⚠️",
        "buyer-took-order" => "🤝",
        "hold-invoice-payment-accepted" => "🔒",
        "bond-slashed" => "🗡",
        "bond-invoice-accepted" | "bond-payout-completed" => "💰",
        "rate" | "rate-received" => "⭐",
        "chat-message" => "💬",
        "dispute-chat-message" => "⚖️",
        _ => "🔔",
    }
}

#[component]
pub fn NotificationRow(n: MostroNotification, on_click: EventHandler<String>) -> Element {
    let unread = n.read_at.is_none();
    let icon = icon_for(&n.action_str);
    let time_label = format::format_relative_time(n.created_at as u64)
        .unwrap_or_else(|| "unknown".to_string());
    let id_for_click = n.id.clone();

    rsx! {
        button {
            class: if unread {
                "w-full text-left p-3 rounded-lg bg-accent/60 hover:bg-accent transition flex gap-3 items-start border border-border"
            } else {
                "w-full text-left p-3 rounded-lg hover:bg-accent transition flex gap-3 items-start"
            },
            onclick: move |_| on_click.call(id_for_click.clone()),
            div { class: "text-2xl shrink-0 leading-none mt-0.5", "{icon}" }
            div { class: "flex-1 min-w-0",
                div { class: "flex items-baseline gap-2",
                    span {
                        class: if unread { "font-semibold text-sm" } else { "text-sm" },
                        "{n.title}"
                    }
                    if unread {
                        span { class: "w-2 h-2 rounded-full bg-primary shrink-0", title: "Unread" }
                    }
                }
                p { class: "text-sm text-muted-foreground line-clamp-2 mt-0.5", "{n.body}" }
                p { class: "text-xs text-muted-foreground/70 mt-1", "{time_label}" }
            }
        }
    }
}
