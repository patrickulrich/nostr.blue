use dioxus::prelude::*;

#[component]
pub fn SearchInput() -> Element {
    rsx! {
        div {
            class: "relative",

            input {
                r#type: "text",
                placeholder: "Search Nostr...",
                disabled: true,
                class: "w-full px-4 py-2 pr-10 bg-muted border border-border rounded-full text-sm opacity-50 cursor-not-allowed"
            }

            div {
                class: "absolute right-2 top-1/2 -translate-y-1/2 p-1.5 opacity-50",
                "🔍"
            }
        }
    }
}
