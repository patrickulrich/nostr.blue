use dioxus::prelude::*;
use nostr_sdk::Timestamp;
#[component]
pub fn PollTimer(ends_at: Timestamp) -> Element {
    let mut time_remaining = use_signal(|| calculate_time_remaining(ends_at));
    use_future(move || async move {
        loop {
            crate::platform::timer::sleep_ms(1000).await;
            time_remaining.set(calculate_time_remaining(ends_at));
        }
    });
    let is_expired = time_remaining() <= 0;
    rsx! {
        div { class: "text-sm text-gray-600 dark:text-gray-400",
            if is_expired {
                span { class: "flex items-center gap-1",
                    svg {
                        class: "w-4 h-4",
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z",
                        }
                    }
                    "Poll ended "
                    {format_ended_date(ends_at)}
                }
            } else {
                span { class: "flex items-center gap-1",
                    svg {
                        class: "w-4 h-4 text-green-600 dark:text-green-400",
                        xmlns: "http://www.w3.org/2000/svg",
                        fill: "none",
                        view_box: "0 0 24 24",
                        stroke: "currentColor",
                        path {
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            stroke_width: "2",
                            d: "M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z",
                        }
                    }
                    {format_time_remaining(time_remaining())}
                }
            }
        }
    }
}
fn calculate_time_remaining(ends_at: Timestamp) -> i64 {
    let now = Timestamp::now();
    let remaining = ends_at.as_secs() as i64 - now.as_secs() as i64;
    remaining.max(0)
}
fn format_time_remaining(seconds: i64) -> String {
    if seconds <= 0 {
        return "Poll ended".to_string();
    }
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if seconds >= 360000 {
        if days == 1 {
            format!("{} day left", days)
        } else {
            format!("{} days left", days)
        }
    } else if hours > 0 {
        format!("{}h {}m {}s left", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s left", minutes, secs)
    } else {
        format!("{}s left", secs)
    }
}
fn format_ended_date(ends_at: Timestamp) -> String {
    let now = Timestamp::now();
    let diff = now.as_secs() as i64 - ends_at.as_secs() as i64;
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        let minutes = diff / 60;
        format!("{} minute{} ago", minutes, if minutes == 1 { "" } else { "s" })
    } else if diff < 86400 {
        let hours = diff / 3600;
        format!("{} hour{} ago", hours, if hours == 1 { "" } else { "s" })
    } else if diff < 604800 {
        let days = diff / 86400;
        format!("{} day{} ago", days, if days == 1 { "" } else { "s" })
    } else {
        let date = chrono::DateTime::from_timestamp(ends_at.as_secs() as i64, 0)
            .unwrap_or_default();
        date.format("%b %d, %Y").to_string()
    }
}
