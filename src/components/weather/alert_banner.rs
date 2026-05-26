use dioxus::prelude::*;
use crate::services::weather::types::WeatherAlert;

#[component]
pub fn AlertBanner(alerts: Vec<WeatherAlert>, on_expand: EventHandler<usize>) -> Element {
    if alerts.is_empty() {
        return rsx! { div {} };
    }

    rsx! {
        div { class: "space-y-2",
            for (i, alert) in alerts.iter().enumerate() {
                button {
                    key: "{alert.id}",
                    class: "w-full text-left border-l-4 {alert.severity.border_class()} bg-card rounded-r-lg p-3 hover:bg-accent transition",
                    onclick: move |_| on_expand.call(i),
                    div { class: "flex items-start gap-2",
                        span { class: "inline-block w-2 h-2 rounded-full mt-1.5 {alert.severity.color_class()}" }
                        span { class: "text-sm font-semibold {alert.severity.text_class()}",
                            "{alert.event}"
                        }
                    }
                    if let Some(headline) = &alert.headline {
                        p { class: "text-sm text-muted-foreground mt-1 line-clamp-2", "{headline}" }
                    }
                    if let Some(expires) = &alert.expires {
                        p { class: "text-xs text-muted-foreground mt-1", "Expires: {expires}" }
                    }
                }
            }
        }
    }
}
