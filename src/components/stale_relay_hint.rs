use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct StaleRelayHintProps {
    pub last_check_timestamp: Option<u64>,
}

#[component]
pub fn StaleRelayHint(props: StaleRelayHintProps) -> Element {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let is_stale = match props.last_check_timestamp {
        Some(ts) => {
            let threshold = 14 * 24 * 60 * 60;
            now.saturating_sub(ts) > threshold
        }
        None => true,
    };

    if is_stale {
        rsx! {
            span {
                class: "inline-flex items-center gap-1 text-xs text-amber-500",
                title: "No recent monitor data available (may be stale)",
                "⚠ Stale"
            }
        }
    } else {
        rsx! {
            span {
                class: "inline-flex items-center gap-1 text-xs text-green-500",
                title: "Recently verified by relay monitor",
                "● Live"
            }
        }
    }
}
