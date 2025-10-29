use dioxus::prelude::*;
use crate::routes::Route;

#[component]
pub fn SearchInput() -> Element {
    let nav = use_navigator();
    let mut search_query = use_signal(|| String::new());

    let handle_submit = move |evt: Event<FormData>| {
        evt.prevent_default();
        let query = search_query.read().trim().to_string();
        if !query.is_empty() {
            nav.push(Route::Search {});
            // TODO: Pass search query as URL parameter or state
        }
    };

    rsx! {
        form {
            onsubmit: handle_submit,
            class: "relative",

            input {
                r#type: "text",
                placeholder: "Search Nostr...",
                value: "{search_query}",
                oninput: move |evt| search_query.set(evt.value()),
                class: "w-full px-4 py-2 pr-10 bg-muted border border-border rounded-full focus:outline-none focus:ring-2 focus:ring-blue-500 text-sm"
            }

            button {
                r#type: "submit",
                class: "absolute right-2 top-1/2 -translate-y-1/2 p-1.5 hover:bg-accent rounded-full transition",
                "🔍"
            }
        }
    }
}
