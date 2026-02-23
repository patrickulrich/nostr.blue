//! Topic New Post Page
//! Full-page composer with topic autocomplete
use crate::components::{TopicPostComposer, TopicSidebar};
use crate::routes::Route;
use crate::stores::nostr_client::HAS_SIGNER;
use dioxus::prelude::*;

#[component]
pub fn TopicNewPost() -> Element {
    let has_signer = *HAS_SIGNER.read();
    let nav = navigator();

    rsx! {
        div {
            class: "flex gap-6 max-w-6xl mx-auto px-4 py-4",
            // Sidebar
            div {
                class: "hidden lg:block w-64 shrink-0",
                TopicSidebar {}
            }
            // Main
            div {
                class: "flex-1 min-w-0 max-w-2xl",
                h1 { class: "text-2xl font-bold text-foreground mb-4", "New Post" }
                if has_signer {
                    TopicPostComposer {
                        on_success: move |_event_id: String| {
                            // Navigate to the created post
                            // For now, go to topics home
                            nav.push(Route::TopicsHome {});
                        },
                    }
                } else {
                    div {
                        class: "bg-muted border border-border rounded-lg p-8 text-center",
                        p { class: "text-muted-foreground", "Sign in to create a topic post." }
                    }
                }
            }
        }
    }
}
