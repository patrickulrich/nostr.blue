//! Topic Badge Component
//! Clickable "#topic" chip that navigates to the topic feed
use crate::routes::Route;
use dioxus::prelude::*;

/// Clickable topic badge chip
#[component]
pub fn TopicBadge(topic: String) -> Element {
    rsx! {
        span {
            onclick: move |e: MouseEvent| e.stop_propagation(),
            Link {
                to: Route::TopicFeed { topic: topic.clone() },
                class: "inline-flex items-center gap-1 px-2 py-0.5 text-xs font-medium rounded-full bg-primary/10 text-primary hover:bg-primary/20 transition",
                span { "#{topic}" }
            }
        }
    }
}
