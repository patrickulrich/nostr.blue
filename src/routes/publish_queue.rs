use crate::stores::publish_queue;
use crate::stores::publish_queue::types::{
    PublishQueueStoreStoreExt, QueueEventStatus, QueueEventType,
};
use dioxus::prelude::*;

#[component]
pub fn PublishQueue() -> Element {
    let events = {
        let queue = publish_queue::PUBLISH_QUEUE.read();
        queue.events().read().clone()
    };

    let failed: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e.status,
                QueueEventStatus::Failed { .. }
                    | QueueEventStatus::MaxRetriesExceeded { .. }
                    | QueueEventStatus::PartialFailure
            )
        })
        .collect();
    let pending: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.status, QueueEventStatus::Pending))
        .collect();
    let publishing: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.status, QueueEventStatus::Publishing))
        .collect();
    let success: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e.status,
                QueueEventStatus::Success | QueueEventStatus::Aborted
            )
        })
        .collect();

    rsx! {
        div { class: "max-w-2xl mx-auto p-4",
            h1 { class: "text-xl font-bold mb-4", "Publish Queue" }

            div { class: "flex gap-2 mb-4",
                if !failed.is_empty() {
                    button {
                        class: "px-3 py-1.5 bg-accent text-accent-foreground rounded-lg text-sm hover:bg-accent/80 transition",
                        onclick: move |_| {
                            let _ = spawn(async move {
                                publish_queue::retry_all_failed().await;
                            });
                        },
                        "Retry All Failed ({failed.len()})"
                    }
                }
                if !success.is_empty() {
                    button {
                        class: "px-3 py-1.5 bg-muted text-foreground rounded-lg text-sm hover:bg-accent transition",
                        onclick: move |_| {
                            let _ = spawn(async move {
                                publish_queue::clear_completed().await;
                            });
                        },
                        "Clear Completed"
                    }
                }
            }

            {render_section("Publishing", &publishing)}
            {render_section("Pending", &pending)}
            {render_section("Failed", &failed)}
            {render_section("Completed", &success)}

            if events.is_empty() {
                p { class: "text-muted-foreground text-center py-8", "No events in queue" }
            }
        }
    }
}

fn render_section(title: &str, events: &[&crate::stores::publish_queue::types::QueuedEvent]) -> Element {
    if events.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "mb-6",
            h2 { class: "text-sm font-semibold text-muted-foreground uppercase mb-2",
                "{title} ({events.len()})"
            }
            div { class: "space-y-2",
                {events.iter().map(|e| {
                    let display_id: String = if e.event_id.len() > 16 {
                        format!("{}...", &e.event_id[..16])
                    } else {
                        e.event_id.clone()
                    };
                    let id_retry = e.id.clone();
                    let id_abort = e.id.clone();
                    let is_failed = matches!(
                        &e.status,
                        QueueEventStatus::Failed { .. } | QueueEventStatus::MaxRetriesExceeded { .. }
                    );
                    rsx! {
                        div {
                            key: "{e.id}",
                            class: "bg-card border border-border rounded-lg p-3 flex items-center justify-between",
                            div { class: "flex-1 min-w-0",
                                p { class: "text-sm font-medium truncate", "{format_event_type(&e.event_type)}" }
                                p { class: "text-xs text-muted-foreground", "{display_id}" }
                            }
                            div { class: "flex items-center gap-2 ml-2",
                                if e.retry_count > 0 {
                                    span { class: "text-xs text-muted-foreground", "retry {e.retry_count}" }
                                }
                                if is_failed {
                                    {
                                        let id_r = id_retry.clone();
                                        rsx! {
                                            button {
                                                class: "px-2 py-1 text-xs bg-accent text-accent-foreground rounded hover:bg-accent/80 transition",
                                                onclick: move |_| {
                                                    let id = id_r.clone();
                                                    let _ = spawn(async move {
                                                        publish_queue::retry(&id).await;
                                                    });
                                                },
                                                "Retry"
                                            }
                                        }
                                    }
                                    {
                                        let id_a = id_abort.clone();
                                        rsx! {
                                            button {
                                                class: "px-2 py-1 text-xs bg-muted text-foreground rounded hover:bg-accent transition",
                                                onclick: move |_| {
                                                    let id = id_a.clone();
                                                    let _ = spawn(async move {
                                                        publish_queue::abort(&id).await;
                                                    });
                                                },
                                                "Abort"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                })}
            }
        }
    }
}

fn format_event_type(t: &QueueEventType) -> String {
    match t {
        QueueEventType::Note => "Note".to_string(),
        QueueEventType::Reaction => "Reaction".to_string(),
        QueueEventType::Repost => "Repost".to_string(),
        QueueEventType::Article => "Article".to_string(),
        QueueEventType::Profile => "Profile".to_string(),
        QueueEventType::Contacts => "Contacts".to_string(),
        QueueEventType::Media => "Media".to_string(),
        QueueEventType::Edit => "Edit".to_string(),
        QueueEventType::DirectMessage => "Direct Message".to_string(),
        QueueEventType::Calendar => "Calendar Event".to_string(),
        QueueEventType::Shop => "Shop".to_string(),
        QueueEventType::Cashu => "Wallet".to_string(),
        QueueEventType::Community => "Community".to_string(),
        QueueEventType::Channel => "Channel".to_string(),
        QueueEventType::PinBoard => "Pin Board".to_string(),
        QueueEventType::Topic => "Topic".to_string(),
        QueueEventType::Pack => "Pack".to_string(),
        QueueEventType::Mute => "Mute/Block".to_string(),
        QueueEventType::Poll => "Poll".to_string(),
        QueueEventType::Bookmark => "Bookmark".to_string(),
        QueueEventType::GitHosting => "Git".to_string(),
        QueueEventType::Nsite => "Static Pages".to_string(),
        QueueEventType::RelayList => "Relay List".to_string(),
        QueueEventType::Other(s) => s.clone(),
    }
}
