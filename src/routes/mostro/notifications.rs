//! Mostro notification history list.
//!
//! Shows all persisted notifications (`NOTIFICATIONS` global signal),
//! grouped by day. Tap a row to mark read and navigate to the source
//! trade or dispute. Clear-all button wipes the list (also publishes an
//! empty NIP-78 record so the state syncs across devices).

use crate::components::mostro::notification_row::NotificationRow;
use crate::components::ClientInitializing;
use crate::routes::Route;
use crate::stores::mostro::notification_store;
use crate::utils::format;
use dioxus::prelude::*;

#[component]
pub fn MostroNotifications() -> Element {
    // Read the signal so we re-render on push/mark_read/clear_all.
    let notifications: Vec<notification_store::MostroNotification> =
        notification_store::NOTIFICATIONS.read().clone();
    let unread = notifications.iter().filter(|n| n.read_at.is_none()).count();

    // Group by day (Today / Yesterday / YYYY-MM-DD). Sort newest-first.
    let mut grouped: Vec<(String, Vec<notification_store::MostroNotification>)> = Vec::new();
    let mut sorted = notifications.clone();
    sorted.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    for n in sorted {
        let label = day_label(n.created_at);
        match grouped.iter_mut().find(|(l, _)| *l == label) {
            Some(entry) => entry.1.push(n),
            None => grouped.push((label, vec![n])),
        }
    }

    rsx! {
        div { class: "min-h-screen p-4 max-w-3xl mx-auto",
            if !*crate::stores::nostr_client::CLIENT_INITIALIZED.read() {
                ClientInitializing {}
            } else {
                div { class: "space-y-4",
                    div { class: "flex items-center justify-between",
                        div { class: "flex items-center gap-3",
                            button {
                                class: "p-2 hover:bg-accent rounded-lg",
                                title: "Back to P2P",
                                onclick: move |_| {
                                    let _ = navigator().push(Route::MostroHome {});
                                },
                                crate::components::icons::ArrowLeftIcon { class: "w-5 h-5".to_string() }
                            }
                            h1 { class: "text-xl font-bold", "Notifications" }
                            if unread > 0 {
                                span {
                                    class: "text-xs font-medium px-2 py-0.5 rounded-full bg-primary text-primary-foreground",
                                    "{unread} unread"
                                }
                            }
                        }
                        if !notifications.is_empty() {
                            button {
                                class: "p-2 hover:bg-accent rounded-lg text-sm text-muted-foreground",
                                title: "Mark all as read",
                                onclick: move |_| {
                                    notification_store::mark_all_read();
                                },
                                "Mark all read"
                            }
                            button {
                                class: "p-2 hover:bg-accent rounded-lg text-sm text-muted-foreground",
                                title: "Clear all notifications",
                                onclick: move |_| {
                                    notification_store::clear_all();
                                },
                                "Clear"
                            }
                        }
                    }

                    if notifications.is_empty() {
                        div { class: "p-8 text-center",
                            div { class: "text-4xl mb-4", "🔔" }
                            h3 { class: "text-lg font-medium mb-2", "No notifications yet" }
                            p { class: "text-muted-foreground mb-4",
                                "Trade updates, chat messages, and dispute events will appear here."
                            }
                            button {
                                class: "px-4 py-2 bg-primary text-primary-foreground rounded-lg",
                                onclick: move |_| {
                                    let _ = navigator().push(Route::MostroHome {});
                                },
                                "Browse Orders"
                            }
                        }
                    } else {
                        for (label, group) in grouped {
                            div { class: "space-y-1",
                                div { class: "text-xs font-medium text-muted-foreground uppercase tracking-wide mt-3 mb-1",
                                    "{label}"
                                }
                                for n in group {
                                    NotificationRow {
                                        key: "{n.id}",
                                        n: n.clone(),
                                        on_click: move |id: String| {
                                            notification_store::mark_read(&id);
                                            // Navigate to the source trade if we know it.
                                            if let Some(order_id) = notification_for_id(&id)
                                                .and_then(|n| n.order_id)
                                            {
                                                let _ = navigator().push(Route::MostroTradeDetail {
                                                    order_id,
                                                });
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
}

/// Look up a notification by id in the current signal state.
fn notification_for_id(id: &str) -> Option<notification_store::MostroNotification> {
    notification_store::NOTIFICATIONS
        .read()
        .iter()
        .find(|n| n.id == id)
        .cloned()
}

/// Human-friendly day label for grouping.
fn day_label(created_at: i64) -> String {
    let now = crate::platform::timestamp::now_secs() as i64;
    let day_secs = 24 * 60 * 60;
    let today = now / day_secs;
    let that_day = created_at.max(0) / day_secs;
    match today.saturating_sub(that_day) {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        n if n < 7 => format!("{n} days ago"),
        _ => format::format_relative_time(created_at as u64)
            .unwrap_or_else(|| format!("{} days ago", today.saturating_sub(that_day))),
    }
}
