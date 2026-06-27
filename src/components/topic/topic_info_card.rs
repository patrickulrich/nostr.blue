use crate::stores::topic_store::TopicMetadata;
use dioxus::prelude::*;

#[component]
pub fn TopicInfoCard(
    metadata: TopicMetadata,
    #[props(default = false)] is_creator: bool,
) -> Element {
    let mut expanded = use_signal(|| false);

    rsx! {
        div {
            class: "bg-card border border-border rounded-lg p-4 mb-4",
            div {
                class: "flex items-center justify-between",
                h2 { class: "text-lg font-bold text-foreground", "#{metadata.name}" }
                if is_creator {
                    span {
                        class: "text-xs bg-primary/10 text-primary px-2 py-0.5 rounded-full",
                        "Creator"
                    }
                }
            }
            if !metadata.description.is_empty() {
                p {
                    class: "text-sm text-muted-foreground mt-2",
                    "{metadata.description}"
                }
            }
            if !metadata.rules.is_empty() {
                div {
                    class: "mt-2",
                    button {
                        class: "text-xs text-primary hover:underline",
                        onclick: move |_| expanded.set(!expanded()),
                        if *expanded.read() { "Hide rules" } else { "Show rules" }
                    }
                    if *expanded.read() {
                        div {
                            class: "mt-2 p-3 bg-muted rounded-md text-sm text-muted-foreground whitespace-pre-wrap",
                            "{metadata.rules}"
                        }
                    }
                }
            }
        }
    }
}
