//! Repository Issues Page
//!
//! View issues for a repository.

use dioxus::prelude::*;

/// Repository issues page component
#[component]
pub fn CodeRepoIssues(naddr: String) -> Element {
    rsx! {
        div {
            class: "p-4",
            h1 {
                class: "text-2xl font-bold mb-4",
                "Issues"
            }
            p {
                class: "text-muted-foreground",
                "Issues for: {naddr}"
            }
        }
    }
}
