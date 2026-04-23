use dioxus::prelude::*;

#[component]
pub fn PublishQueueIndicator() -> Element {
    let count = use_memo(crate::stores::publish_queue::get_pending_count);
    if *count.read() == 0 {
        return rsx! {};
    }
    rsx! {
        Link {
            to: crate::routes::Route::PublishQueue {},
            class: "relative p-2 hover:bg-accent rounded-lg transition",
            div { class: "w-5 h-5 flex items-center justify-center",
                span { class: "text-sm font-medium text-foreground", "{count}" }
            }
        }
    }
}
