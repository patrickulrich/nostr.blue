//! Repository Pull Requests Page
//!
//! View pull requests for a repository.

use dioxus::prelude::*;

/// Repository pull requests page component
#[component]
pub fn CodeRepoPulls(naddr: String) -> Element {
    rsx! {
        div {
            class: "p-4",
            h1 {
                class: "text-2xl font-bold mb-4",
                "Pull Requests"
            }
            p {
                class: "text-muted-foreground",
                "Pull requests for: {naddr}"
            }
        }
    }
}
